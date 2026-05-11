mod util;

use crate::metal::util::*;
use crate::{
    BlendMode, BufferLayout, Capabilities, ClearRequest, DrawRequest, Error, FramebufferLayout, PipelineLayout,
    PrimitiveTopology, TextureBorder, TextureBounds, TextureFilter, TextureFormat, TextureLayout,
};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::*;

pub struct Context {
    device: Retained<ProtocolObject<dyn MTLDevice>>,
    queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    buffer: Option<Retained<ProtocolObject<dyn MTLCommandBuffer>>>,
}

#[derive(Debug)]
pub struct Buffer {
    buffer: Retained<ProtocolObject<dyn MTLBuffer>>,
}

#[derive(Debug)]
pub struct Pipeline {
    pipeline: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,

    cull_cw: bool,
    cull_ccw: bool,
    topology: PrimitiveTopology,
}

#[derive(Debug)]
pub struct Texture {
    texture: Retained<ProtocolObject<dyn MTLTexture>>,
    sampler: Retained<ProtocolObject<dyn MTLSamplerState>>,
}

pub struct Framebuffer {
    color: Vec<Retained<ProtocolObject<dyn MTLTexture>>>,
    depth: Option<Retained<ProtocolObject<dyn MTLTexture>>>,
}

impl Context {
    /// Creates a new [`MetalContext`] from an existing [`MTLDevice`]
    pub fn from_device(device: Retained<ProtocolObject<dyn MTLDevice>>) -> Result<Self, Error> {
        let queue = device
            .newCommandQueue()
            .ok_or_else(|| Error::Internal("Failed to create command queue".into()))?;

        Ok(Self {
            device,
            queue,
            buffer: None,
        })
    }

    pub fn new() -> Result<Self, Error> {
        let device = MTLCreateSystemDefaultDevice().ok_or(Error::InvalidContext)?;
        Self::from_device(device)
    }
}

impl crate::Context for Context {
    type Buffer = Buffer;
    type Texture = Texture;
    type Pipeline = Pipeline;
    type Framebuffer = ();
    type Fence = ();
    type Query = ();

    fn capabilities(&self) -> Capabilities {
        todo!()
    }

    fn create_buffer(&self, layout: BufferLayout) -> Result<Self::Buffer, Error> {
        let buffer = self
            .device
            .newBufferWithLength_options(
                layout.capacity as usize,
                MTLResourceOptions::CPUCacheModeDefaultCache | MTLResourceOptions::StorageModeManaged,
            )
            .ok_or_else(|| Error::Internal("Failed to create buffer".into()))?;

        Ok(Buffer { buffer })
    }

    fn create_texture(&self, layout: TextureLayout) -> Result<Self::Texture, Error> {
        let descriptor = MTLTextureDescriptor::new();

        unsafe {
            descriptor.setWidth(layout.width as usize);
            descriptor.setHeight(layout.height as usize);
        }

        descriptor.setUsage(MTLTextureUsage::ShaderRead);
        descriptor.setTextureType(MTLTextureType::Type2D);
        descriptor.setStorageMode(MTLStorageMode::Private);
        descriptor.setPixelFormat(texture_format(layout.format));

        let texture = self
            .device
            .newTextureWithDescriptor(&descriptor)
            .ok_or_else(|| Error::Internal("Failed to create texture".into()))?;

        let descriptor = MTLSamplerDescriptor::new();

        descriptor.setMagFilter(match layout.filter_mag {
            TextureFilter::Nearest => MTLSamplerMinMagFilter::Nearest,
            TextureFilter::Linear => MTLSamplerMinMagFilter::Linear,
        });

        descriptor.setMinFilter(match layout.filter_min {
            TextureFilter::Nearest => MTLSamplerMinMagFilter::Nearest,
            TextureFilter::Linear => MTLSamplerMinMagFilter::Linear,
        });

        descriptor.setSAddressMode(texture_wrap(layout.wrap_x));
        descriptor.setTAddressMode(texture_wrap(layout.wrap_y));
        descriptor.setBorderColor(match layout.wrap_border {
            TextureBorder::Transparent => MTLSamplerBorderColor::TransparentBlack,
            TextureBorder::Black => MTLSamplerBorderColor::OpaqueBlack,
            TextureBorder::White => MTLSamplerBorderColor::OpaqueWhite,
        });

        let sampler = self
            .device
            .newSamplerStateWithDescriptor(&descriptor)
            .ok_or_else(|| Error::Internal("Failed to create sampler".into()))?;

        Ok(Texture { texture, sampler })
    }

    fn create_pipeline(&self, layout: PipelineLayout) -> Result<Self::Pipeline, Error> {
        let descriptor = MTLRenderPipelineDescriptor::new();
        descriptor.setShaderValidation(MTLShaderValidation::Enabled);

        for (index, output) in layout.color_outputs.iter().enumerate() {
            let attachment = unsafe { descriptor.colorAttachments().objectAtIndexedSubscript(index) };
            attachment.setPixelFormat(texture_format(*output));

            if layout.color_blend == BlendMode::OPAQUE {
                attachment.setBlendingEnabled(false);
            } else {
                attachment.setBlendingEnabled(true);
                attachment.setAlphaBlendOperation(blend_op(layout.color_blend.alpha_op));
                attachment.setRgbBlendOperation(blend_op(layout.color_blend.color_op));
                attachment.setSourceRGBBlendFactor(blend_factor(layout.color_blend.color_src));
                attachment.setSourceAlphaBlendFactor(blend_factor(layout.color_blend.alpha_src));
                attachment.setDestinationRGBBlendFactor(blend_factor(layout.color_blend.color_dst));
                attachment.setDestinationAlphaBlendFactor(blend_factor(layout.color_blend.alpha_dst));
            }
        }

        let pipeline = self
            .device
            .newRenderPipelineStateWithDescriptor_error(&descriptor)
            .map_err(|e| Error::Internal(e.to_string()))?;

        Ok(Pipeline {
            pipeline,
            topology: layout.topology,
            cull_cw: layout.cull_cw,
            cull_ccw: layout.cull_ccw,
        })
    }

    fn create_framebuffer(&self, layout: FramebufferLayout) -> Result<Self::Framebuffer, Error> {
        Err(Error::UnsupportedFeature)
    }

    fn upload_buffer(&self, buffer: &Self::Buffer, offset: u64, data: &[u8]) -> Result<(), Error> {
        Err(Error::UnsupportedFeature)
    }

    fn download_buffer(&self, buffer: &Self::Buffer, offset: u64, data: &mut [u8]) -> Result<(), Error> {
        Err(Error::UnsupportedFeature)
    }

    fn invalidate_buffer(&self, buffer: &Self::Buffer, offset: u64, size: u64) -> Result<(), Error> {
        Err(Error::UnsupportedFeature)
    }

    fn clear(&self, _clear: ClearRequest<Self>) -> Result<(), Error> {
        Ok(())
    }

    fn draw(&self, _draw: DrawRequest<Self>) -> Result<(), Error> {
        Ok(())
    }

    fn present(&self) -> Result<(), Error> {
        Err(Error::UnsupportedFeature)
    }

    fn copy_buffer_to_buffer(
        &self,
        dst_buffer: &Self::Buffer,
        dst_offset: u64,
        src_buffer: &Self::Buffer,
        src_offset: u64,
        size: u64,
    ) -> Result<(), Error> {
        todo!()
    }

    fn copy_buffer_to_texture(
        &self,
        dst_texture: &Self::Texture,
        dst_bounds: TextureBounds,
        src_buffer: &Self::Buffer,
        src_offset: u64,
    ) -> Result<(), Error> {
        todo!()
    }

    fn copy_framebuffer_to_buffer(
        &self,
        dst_buffer: &Self::Buffer,
        dst_format: TextureFormat,
        dst_offset: u64,
        src_framebuffer: &Self::Framebuffer,
        src_attachment: crate::FramebufferAttachment,
        src_bounds: TextureBounds,
    ) -> Result<(), Error> {
        todo!()
    }

    fn begin_query(&self, query: crate::QueryType) -> Result<Self::Query, Error> {
        todo!()
    }

    fn end_query(&self, query: &Self::Query) -> Result<(), Error> {
        todo!()
    }

    fn read_query(&self, query: &Self::Query) -> Result<Option<u64>, Error> {
        todo!()
    }

    fn insert_fence(&self) -> Result<Self::Fence, Error> {
        todo!()
    }

    fn wait_fence(&self, fence: &Self::Fence, timeout: std::time::Duration) -> Result<bool, Error> {
        todo!()
    }
}
