// work in progress, currently this is a mess

mod util;

use crate::{
    BufferLayout, BufferRole, Capabilities, DrawRequest, Error, FramebufferLayout, PipelineLayout, ShaderFormat,
    TextureBounds, TextureFormat, TextureLayout,
    vulkan::util::{find_memorytype_index, texture_filter, texture_format, texture_wrap},
};
use ash::vk;

pub struct Backend {
    device: ash::Device,
    device_memory_properties: vk::PhysicalDeviceMemoryProperties,
    device_limits: vk::PhysicalDeviceLimits,

    queue: vk::Queue,
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
    pub fn from_device(device: ash::Device, queue: vk::Queue) -> Result<Self, VulkanError> {
        unsafe {
            let command_pool = device.create_command_pool(&vk::CommandPoolCreateInfo::default(), None)?;

            // Initialize Vulkan resources such as command pools, queues, etc. here
            Ok(Self {
                device,
                queue,
                command_pool,
                ..todo!()
            })
        }
    }

    pub fn new() -> Result<Self, VulkanError> {
        unsafe {
            let entry = ash::Entry::load()?;
            let instance = entry.create_instance(&vk::InstanceCreateInfo::default(), None)?;

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
    type Pipeline = vk::Pipeline;
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
        todo!()
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
        todo!()
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
        }
    }
}
