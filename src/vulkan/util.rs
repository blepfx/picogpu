use crate::*;
use ash::vk;

pub fn texture_format(format: TextureFormat) -> vk::Format {
    match format {
        TextureFormat::RGBA8 => vk::Format::R8G8B8A8_UNORM,
        TextureFormat::R8 => vk::Format::R8_UNORM,
        TextureFormat::RGB8 => vk::Format::R8G8B8_UNORM,
        TextureFormat::R8S => vk::Format::R8_SNORM,
        TextureFormat::R16S => vk::Format::R16_SNORM,
        TextureFormat::R32F => vk::Format::R32_SFLOAT,
    }
}

pub fn texture_filter(filter: TextureFilter) -> vk::Filter {
    match filter {
        TextureFilter::Nearest => vk::Filter::NEAREST,
        TextureFilter::Linear => vk::Filter::LINEAR,
    }
}

pub fn texture_wrap(wrap: TextureWrap) -> vk::SamplerAddressMode {
    match wrap {
        TextureWrap::Repeat => vk::SamplerAddressMode::REPEAT,
        TextureWrap::Mirror => vk::SamplerAddressMode::MIRRORED_REPEAT,
        TextureWrap::Clamp => vk::SamplerAddressMode::CLAMP_TO_EDGE,
    }
}

pub fn compare_op(compare_fn: CompareFn) -> vk::CompareOp {
    match compare_fn {
        CompareFn::Never => vk::CompareOp::NEVER,
        CompareFn::Less => vk::CompareOp::LESS,
        CompareFn::Equal => vk::CompareOp::EQUAL,
        CompareFn::LessEqual => vk::CompareOp::LESS_OR_EQUAL,
        CompareFn::Greater => vk::CompareOp::GREATER,
        CompareFn::NotEqual => vk::CompareOp::NOT_EQUAL,
        CompareFn::GreaterEqual => vk::CompareOp::GREATER_OR_EQUAL,
        CompareFn::Always => vk::CompareOp::ALWAYS,
    }
}

pub fn stencil_op(stencil_op: StencilOp) -> vk::StencilOp {
    match stencil_op {
        StencilOp::Keep => vk::StencilOp::KEEP,
        StencilOp::Zero => vk::StencilOp::ZERO,
        StencilOp::Replace => vk::StencilOp::REPLACE,
        StencilOp::IncrementClamp => vk::StencilOp::INCREMENT_AND_CLAMP,
        StencilOp::DecrementClamp => vk::StencilOp::DECREMENT_AND_CLAMP,
        StencilOp::IncrementWrap => vk::StencilOp::INCREMENT_AND_WRAP,
        StencilOp::DecrementWrap => vk::StencilOp::DECREMENT_AND_WRAP,
        StencilOp::Invert => vk::StencilOp::INVERT,
    }
}

pub fn blend_factor(blend_factor: BlendFactor) -> vk::BlendFactor {
    match blend_factor {
        BlendFactor::Zero => vk::BlendFactor::ZERO,
        BlendFactor::One => vk::BlendFactor::ONE,
        BlendFactor::SrcColor => vk::BlendFactor::SRC_COLOR,
        BlendFactor::OneMinusSrcColor => vk::BlendFactor::ONE_MINUS_SRC_COLOR,
        BlendFactor::DstColor => vk::BlendFactor::DST_COLOR,
        BlendFactor::OneMinusDstColor => vk::BlendFactor::ONE_MINUS_DST_COLOR,
        BlendFactor::SrcAlpha => vk::BlendFactor::SRC_ALPHA,
        BlendFactor::OneMinusSrcAlpha => vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
        BlendFactor::DstAlpha => vk::BlendFactor::DST_ALPHA,
        BlendFactor::OneMinusDstAlpha => vk::BlendFactor::ONE_MINUS_DST_ALPHA,
    }
}

pub fn blend_op(blend_op: BlendOp) -> vk::BlendOp {
    match blend_op {
        BlendOp::Add => vk::BlendOp::ADD,
        BlendOp::Subtract => vk::BlendOp::SUBTRACT,
        BlendOp::ReverseSubtract => vk::BlendOp::REVERSE_SUBTRACT,
        BlendOp::Min => vk::BlendOp::MIN,
        BlendOp::Max => vk::BlendOp::MAX,
    }
}

pub fn find_memorytype_index(
    memory_req: &vk::MemoryRequirements,
    memory_prop: &vk::PhysicalDeviceMemoryProperties,
    flags: vk::MemoryPropertyFlags,
) -> Option<u32> {
    memory_prop.memory_types[..memory_prop.memory_type_count as _]
        .iter()
        .enumerate()
        .find(|(index, memory_type)| {
            (1 << index) & memory_req.memory_type_bits != 0 && memory_type.property_flags & flags == flags
        })
        .map(|(index, _memory_type)| index as _)
}
