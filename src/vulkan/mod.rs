// work in progress, currently this is a mess
#![allow(missing_docs)]

mod util;

use crate::*;
use alloc::{ffi::CString, format};
use ash::vk;
use util::*;

pub struct Backend {
    instance: ash::Instance,
    device: ash::Device,
    queue: vk::Queue,

    device_memory_properties: vk::PhysicalDeviceMemoryProperties,
    device_limits: vk::PhysicalDeviceLimits,

    command_pool: vk::CommandPool,
    query_pool: vk::QueryPool,
}

pub struct Context<'a> {
    backend: &'a Backend,
    commands: vk::CommandBuffer,
}

#[derive(Debug)]
pub struct Buffer {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    dynamic: bool,
}

#[derive(Debug)]
pub struct Texture {
    image: vk::Image,
    sampler: vk::Sampler,
}

#[derive(Debug)]
pub struct Profiler {}

#[derive(Debug)]
pub struct Pipeline {
    pipeline: vk::Pipeline,
}

#[derive(Debug)]
pub enum VulkanError {
    Loading(ash::LoadingError),
    Error(vk::Result),
}

impl From<ash::LoadingError> for VulkanError {
    fn from(err: ash::LoadingError) -> Self {
        VulkanError::Loading(err)
    }
}

impl From<vk::Result> for VulkanError {
    fn from(err: vk::Result) -> Self {
        VulkanError::Error(err)
    }
}

impl Backend {
    pub fn new() -> Result<Self, VulkanError> {
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

impl Context<'_> {
    pub fn submit(self) -> Result<(), VulkanError> {
        unsafe {
            self.backend.device.end_command_buffer(self.commands)?;
            self.backend
                .device
                .queue_submit(self.backend.queue, &[vk::SubmitInfo::default()], vk::Fence::null())?;
            self.backend
                .device
                .free_command_buffers(self.backend.command_pool, &[self.commands]);
            Ok(())
        }
    }
}

impl crate::Context for Context<'_> {
    type Buffer = Buffer;
    type Texture = Texture;
    type Pipeline = Pipeline;
    type Profiler = Profiler;
    type Framebuffer = vk::Framebuffer;

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            shader_format: ShaderFormat::SpirV,
            supports_profiler: self.backend.device_limits.timestamp_period > 0.0,
            ..todo!()
        }
    }

    fn create_buffer(&self, layout: BufferLayout) -> Result<Self::Buffer, Error> {
        unsafe {
            let buffer_info = vk::BufferCreateInfo::default().size(layout.capacity).usage(
                vk::BufferUsageFlags::TRANSFER_DST
                    | vk::BufferUsageFlags::TRANSFER_SRC
                    | match layout.role {
                        BufferRole::Vertex => vk::BufferUsageFlags::VERTEX_BUFFER,
                        BufferRole::Index => vk::BufferUsageFlags::INDEX_BUFFER,
                        BufferRole::Uniform => vk::BufferUsageFlags::UNIFORM_BUFFER,
                        BufferRole::Storage => vk::BufferUsageFlags::STORAGE_BUFFER,
                    },
            );

            let buffer = self.backend.device.create_buffer(&buffer_info, None).unwrap();

            let memory_reqs = self.backend.device.get_buffer_memory_requirements(buffer);
            let memory_type = find_memorytype_index(
                &memory_reqs,
                &self.backend.device_memory_properties,
                if layout.dynamic {
                    vk::MemoryPropertyFlags::HOST_VISIBLE
                } else {
                    vk::MemoryPropertyFlags::DEVICE_LOCAL
                },
            )
            .unwrap();

            let memory_info = vk::MemoryAllocateInfo::default()
                .allocation_size(memory_reqs.size)
                .memory_type_index(memory_type);

            let memory = self.backend.device.allocate_memory(&memory_info, None).unwrap();

            self.backend.device.bind_buffer_memory(buffer, memory, 0).unwrap();

            Ok(Buffer {
                buffer,
                memory,
                dynamic: layout.dynamic,
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

            let image = self.backend.device.create_image(&image_info, None).unwrap();
            let sampler = self.backend.device.create_sampler(&sampler_info, None).unwrap();

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
                .backend
                .device
                .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(shader.vertex_module), None)
                .unwrap();

            let fragment_module = self
                .backend
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
                blend_enable: (layout.color_blend != BlendMode::OPAQUE) as vk::Bool32,
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
                .backend
                .device
                .create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
                .map_err(|(_, err)| Error::Internal(format!("Failed to create graphics pipeline: {:?}", err)))?[0];

            Ok(Pipeline { pipeline })
        }
    }

    fn create_framebuffer(&self, layout: FramebufferLayout) -> Result<Self::Framebuffer, Error> {
        todo!()
    }

    fn create_profiler(&self) -> Result<Self::Profiler, Error> {
        todo!()
    }

    fn delete_buffer(&self, buffer: Self::Buffer) {
        unsafe {
            self.backend.device.free_memory(buffer.memory, None);
            self.backend.device.destroy_buffer(buffer.buffer, None);
        }
    }

    fn delete_texture(&self, texture: Self::Texture) {
        unsafe {
            self.backend.device.destroy_sampler(texture.sampler, None);
            self.backend.device.destroy_image(texture.image, None);
        }
    }

    fn delete_pipeline(&self, pipeline: Self::Pipeline) {
        unsafe {
            self.backend.device.destroy_pipeline(pipeline.pipeline, None);
        }
    }

    fn delete_framebuffer(&self, framebuffer: Self::Framebuffer) {
        todo!()
    }

    fn delete_profiler(&self, profiler: Self::Profiler) {
        todo!()
    }

    fn upload_texture(
        &self,
        texture: &Self::Texture,
        bounds: TextureBounds,
        format: TextureFormat,
        data: &[u8],
    ) -> Result<(), Error> {
        todo!()
    }

    fn upload_buffer(&self, buffer: &Self::Buffer, offset: u64, data: &[u8]) -> Result<(), Error> {
        unsafe {
            if buffer.dynamic {
                let mapped = self
                    .backend
                    .device
                    .map_memory(buffer.memory, offset, data.len() as u64, vk::MemoryMapFlags::empty())
                    .unwrap();

                core::ptr::copy_nonoverlapping(data.as_ptr(), mapped as *mut u8, data.len());
                self.backend.device.unmap_memory(buffer.memory);
            } else {
                // TODO: staging buffer upload for non-dynamic buffers
            }

            Ok(())
        }
    }

    fn copy_buffer(
        &self,
        dst_buffer: &Self::Buffer,
        src_buffer: &Self::Buffer,
        dst_offset: u64,
        src_offset: u64,
        size: u64,
    ) -> Result<(), Error> {
        unsafe {
            self.backend.device.cmd_copy_buffer(
                self.commands,
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

    fn invalidate_buffer(&self, buffer: &Self::Buffer, offset: u64, size: u64) -> Result<(), Error> {
        todo!()
    }

    fn read_framebuffer(
        &self,
        target: &Self::Framebuffer,
        bounds: TextureBounds,
        format: TextureFormat,
        data: &mut [u8],
    ) -> Result<(), Error> {
        todo!()
    }

    fn begin_profiler(&self, profiler: &Self::Profiler) {
        todo!()
    }

    fn end_profiler(&self, profiler: &Self::Profiler) -> Option<core::time::Duration> {
        todo!()
    }

    fn clear(&self, clear: ClearRequest<Self>) -> Result<(), Error> {
        todo!()
    }

    fn draw(&self, draw: DrawRequest<Self>) -> Result<(), Error> {
        todo!()
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().ok();
            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_device(None);
            self.instance.destroy_instance(None);
        }
    }
}
