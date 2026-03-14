use crate::{
    BlendFactor, BlendMode, BlendOp, BufferRole, CompareFn, DepthStencilFormat, Error, StencilOp, TextureFormat,
};
use alloc::vec::Vec;
use core::{
    ffi::{CStr, c_void},
    mem::ManuallyDrop,
    ops::Deref,
};
use glow::HasContext;

pub struct Features {
    pub uniform_buffers: bool,
    pub storage_buffers: bool,
    pub query_time_elapsed: bool,
    pub invalidate_buffer_sub_data: bool,

    pub max_texture_size: u32,
    pub max_framebuffer_size: u32,
    pub max_framebuffer_msaa: u32,
    pub max_uniform_buffer_size: u32,
    pub max_storage_buffer_size: u32,
    pub max_texture_image_units: u32,
    pub max_uniform_buffer_bindings: u32,
    pub max_storage_buffer_bindings: u32,
    pub uniform_buffer_offset_alignment: u32,
    pub storage_buffer_offset_alignment: u32,
}

#[derive(Debug, Clone, Copy)]
pub enum ProgramBinding {
    Unbound,
    Buffer { index: u32, size: u32, role: BufferRole },
    Texture2D { index: u32 },
}

pub unsafe fn is_context_valid(loader: &mut dyn FnMut(&CStr) -> *const c_void) -> bool {
    unsafe {
        let gl_get_string = loader(c"glGetString");
        if gl_get_string.addr() < 8 || gl_get_string.addr() == usize::MAX {
            return false;
        }

        let gl_get_string: extern "system" fn(u32) -> *const i8 = core::mem::transmute(gl_get_string.addr());
        let version_string = gl_get_string(glow::VERSION);
        if version_string.addr() < 8 || version_string.addr() == usize::MAX {
            return false;
        }

        true
    }
}

impl Features {
    pub unsafe fn from_context(gl: &glow::Context) -> Self {
        unsafe {
            let version = gl.version();
            let extensions = gl.supported_extensions();

            let uniform_buffers = !version.is_embedded && (version.major, version.minor) >= (3, 1)
                || version.is_embedded && (version.major, version.minor) >= (3, 0)
                || extensions.contains("GL_ARB_uniform_buffer_object");
            let storage_buffers = !version.is_embedded && (version.major, version.minor) >= (4, 3)
                || extensions.contains("GL_ARB_shader_storage_buffer_object");
            let query_time_elapsed = !version.is_embedded && (version.major, version.minor) >= (3, 3)
                || extensions.contains("GL_ARB_timer_query");
            let framebuffer_msaa =
                (version.major, version.minor) >= (3, 0) || extensions.contains("GL_EXT_framebuffer_multisample");

            let max_texture_size = gl.get_parameter_i32(glow::MAX_TEXTURE_SIZE) as u32;
            let max_renderbuffer_size = gl.get_parameter_i32(glow::MAX_RENDERBUFFER_SIZE) as u32;

            Features {
                uniform_buffers,
                storage_buffers,
                query_time_elapsed,

                invalidate_buffer_sub_data: !version.is_embedded && (version.major, version.minor) >= (4, 3)
                    || extensions.contains("GL_ARB_invalidate_subdata"),

                max_texture_size,
                max_texture_image_units: gl.get_parameter_i32(glow::MAX_TEXTURE_IMAGE_UNITS) as u32,

                max_framebuffer_size: max_texture_size.min(max_renderbuffer_size),
                max_framebuffer_msaa: if framebuffer_msaa {
                    gl.get_parameter_i32(glow::MAX_SAMPLES) as u32
                } else {
                    0
                },

                max_uniform_buffer_size: if uniform_buffers {
                    gl.get_parameter_i32(glow::MAX_UNIFORM_BLOCK_SIZE) as u32
                } else {
                    0
                },

                max_storage_buffer_size: if storage_buffers {
                    gl.get_parameter_i32(glow::MAX_SHADER_STORAGE_BLOCK_SIZE) as u32
                } else {
                    0
                },

                uniform_buffer_offset_alignment: if uniform_buffers {
                    gl.get_parameter_i32(glow::UNIFORM_BUFFER_OFFSET_ALIGNMENT) as u32
                } else {
                    1
                },

                storage_buffer_offset_alignment: if storage_buffers {
                    gl.get_parameter_i32(glow::SHADER_STORAGE_BUFFER_OFFSET_ALIGNMENT) as u32
                } else {
                    1
                },

                max_storage_buffer_bindings: if storage_buffers {
                    let vertex = gl.get_parameter_i32(glow::MAX_VERTEX_SHADER_STORAGE_BLOCKS) as u32;
                    let fragment = gl.get_parameter_i32(glow::MAX_FRAGMENT_SHADER_STORAGE_BLOCKS) as u32;
                    vertex.min(fragment)
                } else {
                    0
                },

                max_uniform_buffer_bindings: if uniform_buffers {
                    let vertex = gl.get_parameter_i32(glow::MAX_VERTEX_UNIFORM_BLOCKS) as u32;
                    let fragment = gl.get_parameter_i32(glow::MAX_FRAGMENT_UNIFORM_BLOCKS) as u32;
                    vertex.min(fragment)
                } else {
                    0
                },
            }
        }
    }

    pub fn max_buffer_size(&self, role: BufferRole) -> u32 {
        match role {
            BufferRole::Uniform => self.max_uniform_buffer_size,
            BufferRole::Storage => self.max_storage_buffer_size,
            BufferRole::Vertex | BufferRole::Index => 0,
        }
    }

    pub fn buffer_alignment(&self, role: BufferRole) -> u32 {
        match role {
            BufferRole::Uniform => self.uniform_buffer_offset_alignment,
            BufferRole::Storage => self.storage_buffer_offset_alignment,
            BufferRole::Vertex | BufferRole::Index => 1,
        }
    }
}

pub unsafe fn prepare_pipeline_bindings(
    gl: &glow::Context,
    features: &Features,
    program: glow::Program,
    bindings: &[&str],
) -> Result<Vec<ProgramBinding>, Error> {
    let mut texture_index = 0;
    let mut uniform_index = 0;
    let mut storage_index = 0;

    // needed so we can set uniforms for texture bindings
    unsafe {
        gl.use_program(Some(program));
    }

    bindings
        .iter()
        .enumerate()
        .map(|(i, binding)| unsafe {
            if features.uniform_buffers
                && let Some(index) = gl.get_uniform_block_index(program, binding)
            {
                if uniform_index >= features.max_uniform_buffer_bindings {
                    return Err(Error::UnsupportedBinding(i));
                }

                uniform_index += 1;

                let size =
                    gl.get_active_uniform_block_parameter_i32(program, index, glow::UNIFORM_BLOCK_DATA_SIZE) as u32;

                return Ok(ProgramBinding::Buffer {
                    index,
                    size,
                    role: BufferRole::Uniform,
                });
            }

            if features.storage_buffers
                && let Some(index) = gl.get_shader_storage_block_index(program, binding)
            {
                if storage_index >= features.max_storage_buffer_bindings {
                    return Err(Error::UnsupportedBinding(i));
                }

                storage_index += 1;

                let data =
                    gl.get_program_resource_i32(program, glow::SHADER_STORAGE_BLOCK, index, &[glow::BUFFER_DATA_SIZE]);

                return Ok(ProgramBinding::Buffer {
                    index,
                    size: data.first().copied().unwrap_or(0) as u32,
                    role: BufferRole::Storage,
                });
            }

            if let Some(Some(index)) = gl.get_uniform_indices(program, &[binding]).first()
                && let Some(info) = gl.get_active_uniform(program, *index)
                && let Some(location) = gl.get_uniform_location(program, binding)
                && info.utype == glow::SAMPLER_2D
            {
                let index = {
                    let index = texture_index;
                    texture_index += 1;
                    index
                };

                if index >= features.max_texture_image_units {
                    return Err(Error::UnsupportedBinding(i));
                }

                gl.uniform_1_i32(Some(&location), index as i32);
                return Ok(ProgramBinding::Texture2D { index });
            }

            Ok(ProgramBinding::Unbound)
        })
        .collect()
}

pub unsafe fn apply_pipeline(gl: &glow::Context, pipeline: &super::Pipeline) {
    unsafe {
        gl.use_program(Some(pipeline.program));
        gl.bind_vertex_array(Some(pipeline.vertex_array));

        // stencil
        if (pipeline.stencil_cw, pipeline.stencil_ccw) == Default::default() {
            gl.disable(glow::STENCIL_TEST);
        } else {
            gl.enable(glow::STENCIL_TEST);
            gl.front_face(glow::CCW);

            gl.stencil_func_separate(
                glow::FRONT,
                compare_fn(pipeline.stencil_ccw.compare),
                pipeline.stencil_ccw.reference as i32,
                pipeline.stencil_ccw.mask as u32,
            );
            gl.stencil_op_separate(
                glow::FRONT,
                stencil_op(pipeline.stencil_ccw.fail_op),
                stencil_op(pipeline.stencil_ccw.depth_fail_op),
                stencil_op(pipeline.stencil_ccw.pass_op),
            );
            gl.stencil_func_separate(
                glow::BACK,
                compare_fn(pipeline.stencil_cw.compare),
                pipeline.stencil_cw.reference as i32,
                pipeline.stencil_cw.mask as u32,
            );
            gl.stencil_op_separate(
                glow::BACK,
                stencil_op(pipeline.stencil_cw.fail_op),
                stencil_op(pipeline.stencil_cw.depth_fail_op),
                stencil_op(pipeline.stencil_cw.pass_op),
            );
        }

        // cull
        gl.front_face(glow::CCW);
        match (pipeline.cull_ccw, pipeline.cull_cw) {
            (true, true) => {
                gl.enable(glow::CULL_FACE);
                gl.cull_face(glow::FRONT_AND_BACK);
            }
            (true, false) => {
                gl.enable(glow::CULL_FACE);
                gl.cull_face(glow::BACK);
            }
            (false, true) => {
                gl.enable(glow::CULL_FACE);
                gl.cull_face(glow::FRONT);
            }
            (false, false) => gl.disable(glow::CULL_FACE),
        }

        // blend
        if pipeline.color_blend == BlendMode::OPAQUE {
            gl.disable(glow::BLEND);
        } else {
            gl.enable(glow::BLEND);
            gl.blend_func_separate(
                blend_factor(pipeline.color_blend.color_src),
                blend_factor(pipeline.color_blend.color_dst),
                blend_factor(pipeline.color_blend.alpha_src),
                blend_factor(pipeline.color_blend.alpha_dst),
            );
            gl.blend_equation_separate(
                blend_equation(pipeline.color_blend.color_op),
                blend_equation(pipeline.color_blend.alpha_op),
            );
        }

        // depth
        gl.depth_mask(pipeline.depth_write);
        if pipeline.depth_test == CompareFn::Always {
            gl.disable(glow::DEPTH_TEST);
        } else {
            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(compare_fn(pipeline.depth_test));
        }
    }
}

pub fn blend_factor(blend: BlendFactor) -> u32 {
    match blend {
        BlendFactor::Zero => glow::ZERO,
        BlendFactor::One => glow::ONE,
        BlendFactor::SrcColor => glow::SRC_COLOR,
        BlendFactor::OneMinusSrcColor => glow::ONE_MINUS_SRC_COLOR,
        BlendFactor::DstColor => glow::DST_COLOR,
        BlendFactor::OneMinusDstColor => glow::ONE_MINUS_DST_COLOR,
        BlendFactor::SrcAlpha => glow::SRC_ALPHA,
        BlendFactor::OneMinusSrcAlpha => glow::ONE_MINUS_SRC_ALPHA,
        BlendFactor::DstAlpha => glow::DST_ALPHA,
        BlendFactor::OneMinusDstAlpha => glow::ONE_MINUS_DST_ALPHA,
    }
}

pub fn blend_equation(op: BlendOp) -> u32 {
    match op {
        BlendOp::Add => glow::FUNC_ADD,
        BlendOp::Subtract => glow::FUNC_SUBTRACT,
        BlendOp::ReverseSubtract => glow::FUNC_REVERSE_SUBTRACT,
        BlendOp::Min => glow::MIN,
        BlendOp::Max => glow::MAX,
    }
}

pub fn stencil_op(op: StencilOp) -> u32 {
    match op {
        StencilOp::Keep => glow::KEEP,
        StencilOp::Zero => glow::ZERO,
        StencilOp::Replace => glow::REPLACE,
        StencilOp::IncrementClamp => glow::INCR,
        StencilOp::DecrementClamp => glow::DECR,
        StencilOp::Invert => glow::INVERT,
        StencilOp::IncrementWrap => glow::INCR_WRAP,
        StencilOp::DecrementWrap => glow::DECR_WRAP,
    }
}

pub fn compare_fn(compare: CompareFn) -> u32 {
    match compare {
        CompareFn::Never => glow::NEVER,
        CompareFn::Less => glow::LESS,
        CompareFn::Equal => glow::EQUAL,
        CompareFn::LessEqual => glow::LEQUAL,
        CompareFn::Greater => glow::GREATER,
        CompareFn::NotEqual => glow::NOTEQUAL,
        CompareFn::GreaterEqual => glow::GEQUAL,
        CompareFn::Always => glow::ALWAYS,
    }
}

pub fn color_format(format: TextureFormat) -> (u32, u32, u32) {
    match format {
        TextureFormat::R8 => (glow::RED, glow::UNSIGNED_BYTE, glow::R8),
        TextureFormat::RGB8 => (glow::RGB, glow::UNSIGNED_BYTE, glow::RGB8),
        TextureFormat::RGBA8 => (glow::RGBA, glow::UNSIGNED_BYTE, glow::RGBA8),
        TextureFormat::R8S => (glow::RED, glow::BYTE, glow::R8_SNORM),
        TextureFormat::R16S => (glow::RED, glow::SHORT, glow::R16_SNORM),
        TextureFormat::R32F => (glow::RED, glow::FLOAT, glow::R32F),
    }
}

pub fn depth_format(format: DepthStencilFormat) -> (u32, u32) {
    match format {
        DepthStencilFormat::Depth24Stencil8 => (glow::DEPTH24_STENCIL8, glow::DEPTH_STENCIL),
        DepthStencilFormat::Depth32FStencil8 => (glow::DEPTH32F_STENCIL8, glow::DEPTH_STENCIL),
        DepthStencilFormat::Depth32F => (glow::DEPTH_COMPONENT32F, glow::DEPTH_COMPONENT),
        DepthStencilFormat::Stencil8 => (glow::STENCIL_INDEX8, glow::STENCIL_INDEX),
    }
}

pub fn buffer_target(role: BufferRole) -> u32 {
    match role {
        BufferRole::Uniform => glow::UNIFORM_BUFFER,
        BufferRole::Storage => glow::SHADER_STORAGE_BUFFER,
        BufferRole::Vertex => glow::ARRAY_BUFFER,
        BufferRole::Index => glow::ELEMENT_ARRAY_BUFFER,
    }
}

pub struct DisposeOnDrop<T, F: FnOnce(T)>(ManuallyDrop<(T, F)>);

impl<T, F: FnOnce(T)> DisposeOnDrop<T, F> {
    pub fn new(value: T, dispose_fn: F) -> Self {
        Self(ManuallyDrop::new((value, dispose_fn)))
    }

    pub fn take(mut self) -> T {
        let value = unsafe { ManuallyDrop::take(&mut self.0).0 };
        core::mem::forget(self);
        value
    }
}

impl<T, F: FnOnce(T)> Deref for DisposeOnDrop<T, F> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0.0
    }
}

impl<T, F: FnOnce(T)> Drop for DisposeOnDrop<T, F> {
    fn drop(&mut self) {
        unsafe {
            let (value, dispose) = ManuallyDrop::take(&mut self.0);
            dispose(value);
        }
    }
}
