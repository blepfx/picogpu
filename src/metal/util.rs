use crate::{BlendFactor, BlendOp, TextureFormat, TextureWrap};
use objc2_metal::{MTLBlendFactor, MTLBlendOperation, MTLPixelFormat, MTLSamplerAddressMode};

pub fn texture_wrap(wrap: TextureWrap) -> MTLSamplerAddressMode {
    match wrap {
        TextureWrap::Repeat => MTLSamplerAddressMode::Repeat,
        TextureWrap::Mirror => MTLSamplerAddressMode::MirrorRepeat,
        TextureWrap::Clamp => MTLSamplerAddressMode::ClampToEdge,
        TextureWrap::Border => MTLSamplerAddressMode::ClampToBorderColor,
    }
}

pub fn texture_format(format: TextureFormat) -> MTLPixelFormat {
    match format {
        TextureFormat::RGBA8 => MTLPixelFormat::RGBA8Unorm,
        TextureFormat::BGRA8 => MTLPixelFormat::BGRA8Unorm,
        TextureFormat::R8 => MTLPixelFormat::R8Unorm,
        TextureFormat::R8S => MTLPixelFormat::R8Snorm,
        TextureFormat::R16S => MTLPixelFormat::R16Snorm,
        TextureFormat::R32F => MTLPixelFormat::R32Float,
        TextureFormat::RG32F => MTLPixelFormat::RG32Float,
    }
}

pub fn blend_factor(factor: BlendFactor) -> MTLBlendFactor {
    match factor {
        BlendFactor::Zero => MTLBlendFactor::Zero,
        BlendFactor::One => MTLBlendFactor::One,
        BlendFactor::SrcColor => MTLBlendFactor::SourceColor,
        BlendFactor::OneMinusSrcColor => MTLBlendFactor::OneMinusSourceColor,
        BlendFactor::DstColor => MTLBlendFactor::DestinationColor,
        BlendFactor::OneMinusDstColor => MTLBlendFactor::OneMinusDestinationColor,
        BlendFactor::SrcAlpha => MTLBlendFactor::SourceAlpha,
        BlendFactor::OneMinusSrcAlpha => MTLBlendFactor::OneMinusSourceAlpha,
        BlendFactor::DstAlpha => MTLBlendFactor::DestinationAlpha,
        BlendFactor::OneMinusDstAlpha => MTLBlendFactor::OneMinusDestinationAlpha,
    }
}

pub fn blend_op(operation: BlendOp) -> MTLBlendOperation {
    match operation {
        BlendOp::Add => MTLBlendOperation::Add,
        BlendOp::Subtract => MTLBlendOperation::Subtract,
        BlendOp::ReverseSubtract => MTLBlendOperation::ReverseSubtract,
        BlendOp::Min => MTLBlendOperation::Min,
        BlendOp::Max => MTLBlendOperation::Max,
    }
}
