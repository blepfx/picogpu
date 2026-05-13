// work in progress, currently this is a mess
#![allow(missing_docs)]

mod util;

use crate::*;
use ash::vk;
use std::ffi::CString;
use std::fmt::Debug;
use std::ptr::null_mut;
use std::sync::{Arc, Mutex};
use util::*;

#[derive(Clone)]
pub struct Context(Arc<ContextInner>);

struct ContextInner {
    instance: ash::Instance,
    device: ash::Device,
    queue: vk::Queue,

    device_memory_properties: vk::PhysicalDeviceMemoryProperties,
    device_limits: vk::PhysicalDeviceLimits,

    query_pool: vk::QueryPool,
    fence_pool: Mutex<Vec<vk::Fence>>,

    command_pool: Mutex<vk::CommandPool>,
    command_buffer: Mutex<vk::CommandBuffer>,
}

#[derive(Debug)]
pub struct Buffer {
    buffer: vk::Buffer,

    gpu_memory: vk::DeviceMemory,
    cpu_memory: *mut u8,

    role: BufferRole,
    capacity: u64,
    can_upload: bool,
    can_download: bool,
}

#[derive(Debug)]
pub struct Texture {
    image: vk::Image,
    sampler: vk::Sampler,
}

#[derive(Debug)]
pub struct Framebuffer {
    image: vk::Image,
    sampler: vk::Sampler,
}

#[derive(Debug)]
pub struct Pipeline {
    pipeline: vk::Pipeline,
}

#[derive(Debug)]
pub struct Fence {
    context: Context,
    fence: vk::Fence,
}

impl From<ash::LoadingError> for crate::Error {
    fn from(err: ash::LoadingError) -> Self {
        Error::Internal(err.to_string())
    }
}

impl From<vk::Result> for crate::Error {
    fn from(err: vk::Result) -> Self {
        match err {
            vk::Result::ERROR_DEVICE_LOST => Error::InvalidContext,
            err => Error::Internal(err.to_string()),
        }
    }
}

impl Context {
    pub fn new() -> Result<Self, Error> {
        unsafe {
            let entry = ash::Entry::load()?;
            let instance = entry.create_instance(
                &vk::InstanceCreateInfo::default().enabled_layer_names(&[c"VK_LAYER_KHRONOS_validation".as_ptr()]),
                None,
            )?;

            todo!()
        }
    }
}

impl crate::Context for Context {
    type Buffer = Buffer;
    type Texture = Texture;
    type Pipeline = Pipeline;
    type Framebuffer = Framebuffer;
    type Fence = Fence;
    type Query = ();

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            shader_format: ShaderFormat::SpirV,
            ..todo!()
        }
    }

    fn create_buffer(&self, layout: BufferLayout) -> Result<Self::Buffer, Error> {
        unsafe {
            let buffer_info = vk::BufferCreateInfo::default().size(layout.capacity).usage(
                vk::BufferUsageFlags::TRANSFER_DST
                    | vk::BufferUsageFlags::TRANSFER_SRC
                    | match layout.role {
                        BufferRole::Uniform => vk::BufferUsageFlags::UNIFORM_BUFFER,
                        BufferRole::Storage => vk::BufferUsageFlags::STORAGE_BUFFER,
                        BufferRole::Staging => vk::BufferUsageFlags::empty(),
                    },
            );

            let buffer = self.0.device.create_buffer(&buffer_info, None).unwrap();

            let memory_reqs = self.0.device.get_buffer_memory_requirements(buffer);
            let memory_type = find_memorytype_index(
                &memory_reqs,
                &self.0.device_memory_properties,
                if layout.can_upload || layout.can_download {
                    vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_CACHED
                } else {
                    vk::MemoryPropertyFlags::DEVICE_LOCAL
                },
            )
            .unwrap();

            let memory_info = vk::MemoryAllocateInfo::default()
                .allocation_size(memory_reqs.size)
                .memory_type_index(memory_type);

            let gpu_memory = self.0.device.allocate_memory(&memory_info, None).unwrap();
            self.0.device.bind_buffer_memory(buffer, gpu_memory, 0).unwrap();

            let cpu_memory = if layout.can_upload || layout.can_download {
                self.0
                    .device
                    .map_memory(gpu_memory, 0, layout.capacity, vk::MemoryMapFlags::empty())
                    .unwrap() as *mut u8
            } else {
                null_mut()
            };

            Ok(Buffer {
                buffer,
                gpu_memory,
                cpu_memory,

                role: layout.role,
                capacity: layout.capacity,
                can_download: layout.can_download,
                can_upload: layout.can_upload,
            })
        }
    }

    fn create_texture(&self, layout: TextureLayout) -> Result<Self::Texture, Error> {
        unsafe {
            let image_info = vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(texture_format(layout.format))
                .extent(vk::Extent3D {
                    width: layout.width,
                    height: layout.height,
                    depth: 1,
                })
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(vk::ImageUsageFlags::SAMPLED);

            let sampler_info = vk::SamplerCreateInfo::default()
                .mag_filter(texture_filter(layout.filter_mag))
                .min_filter(texture_filter(layout.filter_min))
                .address_mode_u(texture_wrap(layout.wrap_x))
                .address_mode_v(texture_wrap(layout.wrap_y));

            let image = self.0.device.create_image(&image_info, None).unwrap();
            let sampler = self.0.device.create_sampler(&sampler_info, None).unwrap();

            Ok(Texture { image, sampler })
        }
    }

    fn create_pipeline(&self, layout: PipelineLayout) -> Result<Self::Pipeline, Error> {
        unsafe {
            let shader = match layout.shader {
                Shader::SpirV(shader) => shader,
                _ => return Err(Error::UnsupportedFormat),
            };

            let vertex_module = self
                .0
                .device
                .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(shader.vertex_module), None)
                .unwrap();

            let fragment_module = self
                .0
                .device
                .create_shader_module(
                    &vk::ShaderModuleCreateInfo::default().code(shader.fragment_module),
                    None,
                )
                .unwrap();

            let vertex_entry = CString::new(shader.vertex_entry).unwrap();
            let vertex_stage = vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(vertex_module)
                .name(vertex_entry.as_c_str());

            let fragment_entry = CString::new(shader.fragment_entry).unwrap();
            let fragment_stage = vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(fragment_module)
                .name(fragment_entry.as_c_str());

            let dynamic_state = vk::PipelineDynamicStateCreateInfo::default()
                .dynamic_states(&[vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR]);

            let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
                .vertex_binding_descriptions(&[])
                .vertex_attribute_descriptions(&[]);

            let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
                .topology(match layout.topology {
                    PrimitiveTopology::TriangleList => vk::PrimitiveTopology::TRIANGLE_LIST,
                    PrimitiveTopology::TriangleStrip => vk::PrimitiveTopology::TRIANGLE_STRIP,
                    PrimitiveTopology::TriangleFan => vk::PrimitiveTopology::TRIANGLE_FAN,
                })
                .primitive_restart_enable(false);

            let viewport_state = vk::PipelineViewportStateCreateInfo::default()
                .viewport_count(1)
                .scissor_count(1);

            let rasterization_state = vk::PipelineRasterizationStateCreateInfo::default()
                .depth_clamp_enable(false)
                .rasterizer_discard_enable(false)
                .polygon_mode(vk::PolygonMode::FILL)
                .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
                .cull_mode(match (layout.cull_ccw, layout.cull_cw) {
                    (false, false) => vk::CullModeFlags::NONE,
                    (true, false) => vk::CullModeFlags::BACK,
                    (false, true) => vk::CullModeFlags::FRONT,
                    (true, true) => vk::CullModeFlags::FRONT_AND_BACK,
                });

            let multisample_state =
                vk::PipelineMultisampleStateCreateInfo::default().rasterization_samples(vk::SampleCountFlags::TYPE_1);

            let depth_stencil_state = vk::PipelineDepthStencilStateCreateInfo::default()
                .depth_test_enable(layout.depth_test != CompareFn::Always)
                .depth_write_enable(layout.depth_write)
                .depth_compare_op(compare_op(layout.depth_test))
                .stencil_test_enable(
                    layout.stencil_ccw != StencilFace::default() || layout.stencil_cw != StencilFace::default(),
                )
                .front(vk::StencilOpState {
                    fail_op: stencil_op(layout.stencil_ccw.fail_op),
                    pass_op: stencil_op(layout.stencil_ccw.pass_op),
                    depth_fail_op: stencil_op(layout.stencil_ccw.depth_fail_op),
                    compare_op: compare_op(layout.stencil_ccw.compare),
                    compare_mask: layout.stencil_ccw.mask as u32,
                    write_mask: layout.stencil_ccw.mask as u32,
                    reference: layout.stencil_ccw.reference as u32,
                })
                .back(vk::StencilOpState {
                    fail_op: stencil_op(layout.stencil_cw.fail_op),
                    pass_op: stencil_op(layout.stencil_cw.pass_op),
                    depth_fail_op: stencil_op(layout.stencil_cw.depth_fail_op),
                    compare_op: compare_op(layout.stencil_cw.compare),
                    compare_mask: layout.stencil_cw.mask as u32,
                    write_mask: layout.stencil_cw.mask as u32,
                    reference: layout.stencil_cw.reference as u32,
                });

            let color_blend_attachments = [vk::PipelineColorBlendAttachmentState {
                blend_enable: (layout.color_blend != BlendMode::OVERWRITE) as vk::Bool32,
                src_color_blend_factor: blend_factor(layout.color_blend.color_src),
                dst_color_blend_factor: blend_factor(layout.color_blend.color_dst),
                src_alpha_blend_factor: blend_factor(layout.color_blend.alpha_src),
                dst_alpha_blend_factor: blend_factor(layout.color_blend.alpha_dst),
                color_blend_op: blend_op(layout.color_blend.color_op),
                alpha_blend_op: blend_op(layout.color_blend.alpha_op),
                color_write_mask: vk::ColorComponentFlags::RGBA,
            }];

            let color_blend_state = vk::PipelineColorBlendStateCreateInfo::default()
                .logic_op_enable(false)
                .attachments(&color_blend_attachments);

            let pipeline_stages = [vertex_stage, fragment_stage];
            let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
                .stages(&pipeline_stages)
                .vertex_input_state(&vertex_input)
                .input_assembly_state(&input_assembly)
                .viewport_state(&viewport_state)
                .rasterization_state(&rasterization_state)
                .multisample_state(&multisample_state)
                .depth_stencil_state(&depth_stencil_state)
                .color_blend_state(&color_blend_state)
                .dynamic_state(&dynamic_state);

            let pipeline = self
                .0
                .device
                .create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
                .map_err(|(_, err)| Error::Internal(format!("Failed to create graphics pipeline: {:?}", err)))?[0];

            Ok(Pipeline { pipeline })
        }
    }

    fn create_framebuffer(&self, layout: FramebufferLayout) -> Result<Self::Framebuffer, Error> {
        todo!()
    }

    fn upload_buffer(&self, buffer: &Self::Buffer, offset: u64, data: &[u8]) -> Result<(), Error> {
        unsafe {
            if !buffer.can_upload {
                return Err(Error::InvalidOperation);
            }

            if offset.saturating_add(data.len() as u64) > buffer.capacity {
                return Err(Error::InvalidBounds);
            }

            core::ptr::copy_nonoverlapping(data.as_ptr(), buffer.cpu_memory.add(offset as usize), data.len());

            self.0
                .device
                .flush_mapped_memory_ranges(&[vk::MappedMemoryRange::default()
                    .memory(buffer.gpu_memory)
                    .offset(offset)
                    .size(data.len() as u64)])?;

            // TODO: synchronization

            Ok(())
        }
    }

    fn download_buffer(&self, buffer: &Self::Buffer, offset: u64, data: &mut [u8]) -> Result<(), Error> {
        unsafe {
            if !buffer.can_download {
                return Err(Error::InvalidOperation);
            }

            if offset.saturating_add(data.len() as u64) > buffer.capacity {
                return Err(Error::InvalidBounds);
            }

            self.0
                .device
                .invalidate_mapped_memory_ranges(&[vk::MappedMemoryRange::default()
                    .memory(buffer.gpu_memory)
                    .offset(offset)
                    .size(data.len() as u64)])?;

            core::ptr::copy_nonoverlapping(buffer.cpu_memory.add(offset as usize), data.as_mut_ptr(), data.len());

            // TODO: synchronization

            Ok(())
        }
    }

    fn copy_buffer_to_buffer(
        &self,
        dst_buffer: &Self::Buffer,
        dst_offset: u64,
        src_buffer: &Self::Buffer,
        src_offset: u64,
        size: u64,
    ) -> Result<(), Error> {
        unsafe {
            let buffer = self.0.command_buffer.lock().expect("poisoned");

            self.0.device.cmd_copy_buffer(
                *buffer,
                src_buffer.buffer,
                dst_buffer.buffer,
                &[vk::BufferCopy {
                    src_offset,
                    dst_offset,
                    size,
                }],
            );

            Ok(())
        }
    }

    fn copy_buffer_to_texture(
        &self,
        dst_texture: &Self::Texture,
        dst_bounds: TextureBounds,
        src_buffer: &Self::Buffer,
        src_offset: u64,
    ) -> Result<(), Error> {
        unsafe {
            let buffer = self.0.command_buffer.lock().expect("poisoned");

            self.0.device.cmd_copy_buffer_to_image(
                *buffer,
                src_buffer.buffer,
                dst_texture.image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[vk::BufferImageCopy {
                    buffer_offset: src_offset,
                    buffer_row_length: dst_bounds.width,
                    buffer_image_height: dst_bounds.height,
                    image_subresource: vk::ImageSubresourceLayers {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        mip_level: 0,
                        base_array_layer: 0,
                        layer_count: 1,
                    },
                    image_offset: vk::Offset3D {
                        x: dst_bounds.x as i32,
                        y: dst_bounds.y as i32,
                        z: 0,
                    },
                    image_extent: vk::Extent3D {
                        width: dst_bounds.width,
                        height: dst_bounds.height,
                        depth: 1,
                    },
                }],
            );

            Ok(())
        }
    }

    fn copy_framebuffer_to_buffer(
        &self,
        dst_buffer: &Self::Buffer,
        dst_offset: u64,
        src_framebuffer: &Self::Framebuffer,
        src_attachment: FramebufferAttachment,
        src_bounds: TextureBounds,
    ) -> Result<(), Error> {
        unsafe {
            let buffer = self.0.command_buffer.lock().expect("poisoned");

            self.0.device.cmd_copy_image_to_buffer(
                *buffer,
                src_framebuffer.image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                dst_buffer.buffer,
                &[vk::BufferImageCopy {
                    buffer_offset: dst_offset,
                    buffer_row_length: 0,
                    buffer_image_height: 0,
                    image_subresource: vk::ImageSubresourceLayers {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        mip_level: 0,
                        base_array_layer: 0,
                        layer_count: 1,
                    },
                    image_offset: vk::Offset3D {
                        x: src_bounds.x as i32,
                        y: src_bounds.y as i32,
                        z: 0,
                    },
                    image_extent: vk::Extent3D {
                        width: src_bounds.width,
                        height: src_bounds.height,
                        depth: 1,
                    },
                }],
            );

            Ok(())
        }
    }

    fn wait_fence(&self, fence: &Self::Fence, timeout: std::time::Duration) -> Result<bool, Error> {
        unsafe {
            let timeout = timeout.as_nanos().try_into().unwrap_or(u64::MAX);
            let result = self.0.device.wait_for_fences(&[fence.fence], true, timeout);
            match result {
                Ok(()) => Ok(true),
                Err(vk::Result::TIMEOUT) => Ok(false),
                Err(err) => Err(err.into()),
            }
        }
    }

    fn begin_query(&self, query: QueryType) -> Result<Self::Query, Error> {
        todo!()
    }

    fn end_query(&self, query: &Self::Query) -> Result<(), Error> {
        todo!()
    }

    fn read_query(&self, query: &Self::Query) -> Result<Option<u64>, Error> {
        todo!()
    }

    fn clear(&self, clear: ClearRequest<Self>) -> Result<(), Error> {
        todo!()
    }

    fn draw(&self, draw: DrawRequest<Self>) -> Result<(), Error> {
        todo!()
    }

    fn present(&self) -> Result<Self::Fence, Error> {
        unsafe {
            let buffer = self.0.command_buffer.lock().expect("poisoned");
            let fence = match self.0.fence_pool.lock().expect("poisoned").pop() {
                Some(fence) => {
                    self.0.device.reset_fences(&[fence])?;
                    fence
                }

                None => self
                    .0
                    .device
                    .create_fence(&vk::FenceCreateInfo::default(), None)
                    .unwrap(),
            };

            self.0.device.end_command_buffer(*buffer)?;
            self.0
                .device
                .queue_submit(self.0.queue, &[vk::SubmitInfo::default()], fence)?;
            self.0.device.free_command_buffers(self.0.command_pool, &[*buffer]);

            Ok(Fence {
                context: self.clone(),
                fence,
            })
        }
    }
}

impl Debug for Context {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("vulkan::Context").finish_non_exhaustive()
    }
}

impl Drop for Fence {
    fn drop(&mut self) {
        self.context.0.fence_pool.lock().expect("poisoned").push(self.fence);
    }
}

impl Drop for ContextInner {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().ok();

            // drop all fences
            for fence in self.fence_pool.lock().expect("poisoned").drain(..) {
                self.device.destroy_fence(fence, None);
            }

            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_device(None);
            self.instance.destroy_instance(None);
        }
    }
}
