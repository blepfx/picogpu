mod util;

use crate::{
    BlendMode, PrimitiveTopology, TextureBorder, TextureFilter,
    context::{
        BufferLayout, Capabilities, ClearRequest, Context, DrawRequest, Error, FramebufferLayout, PipelineLayout,
        TextureBounds, TextureFormat, TextureLayout,
    },
    metal::util::{blend_factor, blend_op, texture_wrap},
};
use alloc::{string::ToString, vec::Vec};
use core::time::Duration;
use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_metal::*;

pub struct MetalContext {
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

impl MetalContext {
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

impl Context for MetalContext {
    type Buffer = Buffer;
    type Texture = Texture;
    type Pipeline = Pipeline;
    type Profiler = ();
    type Framebuffer = ();

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

        match layout.format {
            TextureFormat::R8 => descriptor.setPixelFormat(MTLPixelFormat::R8Unorm),
            TextureFormat::R8S => descriptor.setPixelFormat(MTLPixelFormat::R8Snorm),
            TextureFormat::R16S => descriptor.setPixelFormat(MTLPixelFormat::R16Snorm),
            TextureFormat::R32F => descriptor.setPixelFormat(MTLPixelFormat::R32Float),
            TextureFormat::RG32F => descriptor.setPixelFormat(MTLPixelFormat::RG32Float),
            TextureFormat::RGBA8 => descriptor.setPixelFormat(MTLPixelFormat::BGRA8Unorm),
            TextureFormat::RGB8 => {
                descriptor.setPixelFormat(MTLPixelFormat::RGBA8Unorm);
                descriptor.setSwizzle(MTLTextureSwizzleChannels {
                    red: MTLTextureSwizzle::Red,
                    green: MTLTextureSwizzle::Green,
                    blue: MTLTextureSwizzle::Blue,
                    alpha: MTLTextureSwizzle::One,
                });
            }
        }

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
            attachment.setPixelFormat(match *output {
                TextureFormat::R8 => MTLPixelFormat::R8Unorm,
                TextureFormat::RGB8 => MTLPixelFormat::RGBA8Unorm, // No RGB8 format, use RGBA8 with swizzle
                TextureFormat::RGBA8 => MTLPixelFormat::BGRA8Unorm,
                TextureFormat::R8S => MTLPixelFormat::R8Snorm,
                TextureFormat::R16S => MTLPixelFormat::R16Snorm,
                TextureFormat::R32F => MTLPixelFormat::R32Float,
                TextureFormat::RG32F => MTLPixelFormat::RG32Float,
            });

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

            attachment.setWriteMask({
                let mut mask = MTLColorWriteMask::empty();

                if layout.color_mask.red {
                    mask |= MTLColorWriteMask::Red;
                }

                if layout.color_mask.green {
                    mask |= MTLColorWriteMask::Green;
                }

                if layout.color_mask.blue {
                    mask |= MTLColorWriteMask::Blue;
                }

                if layout.color_mask.alpha {
                    mask |= MTLColorWriteMask::Alpha;
                }

                mask
            });
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

    fn create_profiler(&self) -> Result<Self::Profiler, Error> {
        Err(Error::UnsupportedFeature)
    }

    fn upload_texture(
        &self,
        texture: &Self::Texture,
        bounds: TextureBounds,
        format: TextureFormat,
        data: &[u8],
    ) -> Result<(), Error> {
        Err(Error::UnsupportedFeature)
    }

    fn upload_buffer(&self, buffer: &Self::Buffer, offset: u64, data: &[u8]) -> Result<(), Error> {
        Err(Error::UnsupportedFeature)
    }

    fn copy_buffer(
        &self,
        dst_buffer: &Self::Buffer,
        src_buffer: &Self::Buffer,
        dst_offset: u64,
        src_offset: u64,
        size: u64,
    ) -> Result<(), Error> {
        Err(Error::UnsupportedFeature)
    }

    fn invalidate_buffer(&self, buffer: &Self::Buffer, offset: u64, size: u64) -> Result<(), Error> {
        Err(Error::UnsupportedFeature)
    }

    fn read_framebuffer(
        &self,
        target: &Self::Framebuffer,
        bounds: TextureBounds,
        format: TextureFormat,
        data: &mut [u8],
    ) -> Result<(), Error> {
        Err(Error::UnsupportedFeature)
    }

    fn begin_profiler(&self, profiler: &Self::Profiler) -> Result<(), Error> {
        Err(Error::UnsupportedFeature)
    }

    fn end_profiler(&self, profiler: &Self::Profiler) -> Result<Option<Duration>, Error> {
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
}
