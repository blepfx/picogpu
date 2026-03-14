use crate::{TextureFilter, TextureFormat, TextureWrap};
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
