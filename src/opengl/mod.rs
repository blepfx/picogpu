//! An OpenGL implementation of the `picogpu` rendering backend.
//!
//! This implementation and provides a high-level API for
//! creating and managing GPU resources, handling errors, and issuing draw calls.
//!
//! The OpenGL target is 3.0+, but some features may require higher versions or extensions, which
//! are checked for at runtime and will return an error if not supported by the current context.
//!
//! OpenGL backend is single-threaded, meaning that all resources are !Send and !Sync.

mod surface;
mod util;

use crate::*;
use alloc::{boxed::Box, rc::Rc, vec::Vec};
use core::{
    cell::{Cell, RefCell},
    fmt::Debug,
    time::Duration,
};
use glow::HasContext;
use util::*;

pub use surface::{Surface, SurfaceError};

/// OpenGL implementation of the `picogpu` rendering backend.
#[derive(Clone)]
pub struct Context<'a>(Rc<RefCell<ContextThread<'a>>>);

struct ContextThread<'a> {
    gl: glow::Context,
    features: Features,
    surface: Box<dyn Surface + 'a>,

    last_pipeline: Option<glow::Program>,
    last_viewport: Option<TextureBounds>,
    last_scissor: Option<Option<TextureBounds>>,
    last_framebuffer: Option<Option<glow::Framebuffer>>,
}

/// An OpenGL buffer object.
#[derive(Debug)]
pub struct Buffer<'a> {
    context: Context<'a>,
    buffer: glow::Buffer,

    is_dynamic: bool,
    capacity: u32,
    role: BufferRole,
}

/// An OpenGL pipeline object.
#[derive(Debug)]
pub struct Pipeline<'a> {
    context: Context<'a>,
    program: glow::Program,

    vertex_array: glow::VertexArray,
    bindings: Vec<ProgramBinding>,

    topology: PrimitiveTopology,
    color_blend: BlendMode,
    depth_test: CompareFn,
    depth_write: bool,
    stencil_ccw: StencilFace,
    stencil_cw: StencilFace,
    cull_ccw: bool,
    cull_cw: bool,
}

/// An OpenGL texture object.
#[derive(Debug)]
pub struct Texture<'a> {
    context: Context<'a>,
    texture: glow::Texture,

    width: u32,
    height: u32,
}

/// An OpenGL framebuffer object.
#[derive(Debug)]
pub struct Framebuffer<'a> {
    context: Context<'a>,
    framebuffer: Option<glow::Framebuffer>,

    color_texture: Option<glow::Texture>,
    depth_texture: Option<glow::Texture>,
    color_buffer: Option<glow::Renderbuffer>,
    depth_buffer: Option<glow::Renderbuffer>,
}

/// An OpenGL profiler object.
#[derive(Debug)]
pub struct Profiler<'a> {
    context: Context<'a>,
    query: glow::Query,
    state: Cell<u8>, // 0 - idle, 1 - started, 2 - waiting for result
}

/// The type of OpenGL debug message sent to the debug callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DebugMessage {
    /// An error message.
    Error,

    /// Deprecated functionality was used.
    Deprecated,

    /// Undefined behavior was detected.
    UndefinedBehavior,

    /// Performance issues were detected.   
    Performance,

    /// A portability issue was detected.
    Portability,

    /// Marker?
    Marker,

    ///  Other/unknown message.
    Other,
}

impl<'a> Context<'a> {
    /// Create a new OpenGL backend with the provided OpenGL surface.
    ///
    /// # Errors
    /// - [`Error::InvalidContext`] if the provided loader function does not provide a valid OpenGL
    ///   context.
    /// - [`Error::Internal`] if an internal error occurs while creating the OpenGL context.
    pub fn new(surface: impl Surface + 'a) -> Result<Self, Error> {
        unsafe {
            surface.make_current()?;

            if !is_context_valid(&mut |f| surface.get_proc_address(f)) {
                return Err(Error::InvalidContext);
            }

            let gl = glow::Context::from_loader_function_cstr(|f| surface.get_proc_address(f));
            let features = Features::from_context(&gl);

            Ok(Self(Rc::new(RefCell::new(ContextThread {
                gl,
                features,
                surface: Box::new(surface),

                last_pipeline: None,
                last_viewport: None,
                last_scissor: None,
                last_framebuffer: None,
            }))))
        }
    }

    fn with_current<R>(&self, f: impl FnOnce(&mut ContextThread) -> Result<R, Error>) -> Result<R, Error> {
        let mut context = self.0.borrow_mut();
        context.surface.make_current()?;
        f(&mut context)
    }
}

impl<'a> Context<'a> {
    /// Attach a debug callback to the OpenGL context that will be called whenever a debug message
    /// is generated.
    ///
    /// Can only be called once.
    pub fn attach_debug_callback(&self, callback: impl Fn(DebugMessage, &str) + Send + Sync + 'static) {
        self.with_current(|thread| unsafe {
            thread.gl.enable(glow::DEBUG_OUTPUT);
            thread.gl.enable(glow::DEBUG_OUTPUT_SYNCHRONOUS);
            thread.gl.debug_message_callback(move |_, type_, _, _, message| {
                let type_ = match type_ {
                    glow::DEBUG_TYPE_ERROR => DebugMessage::Error,
                    glow::DEBUG_TYPE_DEPRECATED_BEHAVIOR => DebugMessage::Deprecated,
                    glow::DEBUG_TYPE_UNDEFINED_BEHAVIOR => DebugMessage::UndefinedBehavior,
                    glow::DEBUG_TYPE_PORTABILITY => DebugMessage::Portability,
                    glow::DEBUG_TYPE_PERFORMANCE => DebugMessage::Performance,
                    glow::DEBUG_TYPE_MARKER => DebugMessage::Marker,
                    _ => DebugMessage::Other,
                };

                callback(type_, message);
            });

            Ok(())
        })
        .ok();
    }

    /// A framebuffer handle representing the screen, used for drawing to the screen and reading
    /// pixels from it.
    pub fn screen(&self) -> Framebuffer<'a> {
        Framebuffer {
            context: self.clone(),
            framebuffer: None,
            color_texture: None,
            depth_texture: None,
            color_buffer: None,
            depth_buffer: None,
        }
    }
}

impl<'a> crate::Context for Context<'a> {
    type Buffer = Buffer<'a>;
    type Texture = Texture<'a>;
    type Pipeline = Pipeline<'a>;
    type Profiler = Profiler<'a>;
    type Framebuffer = Framebuffer<'a>;

    #[inline]
    fn capabilities(&self) -> Capabilities {
        let thread = self.0.borrow();

        Capabilities {
            shader_format: thread.features.glsl_version(),
            supports_profiler: thread.features.query_time_elapsed,
            texture_size: thread.features.max_texture_size,
            texture_bindings: thread.features.max_texture_image_units,
            framebuffer_size: thread.features.max_framebuffer_size,
            framebuffer_msaa: thread.features.max_framebuffer_msaa,
            uniform_buffer_size: thread.features.max_uniform_buffer_size as u64,
            storage_buffer_size: thread.features.max_storage_buffer_size,
            uniform_buffer_alignment: thread.features.uniform_buffer_offset_alignment,
            storage_buffer_alignment: thread.features.storage_buffer_offset_alignment,
            uniform_buffer_bindings: thread.features.max_uniform_buffer_bindings,
            storage_buffer_bindings: thread.features.max_storage_buffer_bindings,
        }
    }

    fn create_buffer(&self, layout: BufferLayout) -> Result<Self::Buffer, Error> {
        self.with_current(|thread| unsafe {
            if thread.features.max_buffer_size(layout.role) == 0 {
                return Err(Error::UnsupportedFeature);
            }

            let capacity: u32 = match layout.capacity.try_into() {
                Ok(capacity) => capacity,
                Err(_) => return Err(Error::UnsupportedSize),
            };

            if capacity > thread.features.max_buffer_size(layout.role) {
                return Err(Error::UnsupportedSize);
            }

            let buffer = thread.gl.create_buffer().map_err(Error::Internal)?;
            thread.gl.bind_buffer(buffer_target(layout.role), Some(buffer));
            thread.gl.buffer_data_size(
                buffer_target(layout.role),
                capacity as i32,
                if layout.dynamic {
                    glow::DYNAMIC_DRAW
                } else {
                    glow::STATIC_DRAW
                },
            );

            Ok(Buffer {
                context: self.clone(),
                buffer,
                capacity,
                is_dynamic: layout.dynamic,
                role: layout.role,
            })
        })
    }

    fn create_texture(&self, layout: TextureLayout) -> Result<Self::Texture, Error> {
        self.with_current(|thread| unsafe {
            if layout.width.max(layout.height) > thread.features.max_texture_size {
                return Err(Error::UnsupportedSize);
            }

            let (format, data_type, internal_format) = color_format(layout.format);
            let texture = thread.gl.create_texture().map_err(Error::Internal)?;

            thread.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            thread.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                match layout.filter_min {
                    TextureFilter::Nearest => glow::NEAREST as i32,
                    TextureFilter::Linear => glow::LINEAR as i32,
                },
            );

            thread.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                match layout.filter_mag {
                    TextureFilter::Nearest => glow::NEAREST as i32,
                    TextureFilter::Linear => glow::LINEAR as i32,
                },
            );

            thread.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                match layout.wrap_x {
                    TextureWrap::Mirror => glow::MIRRORED_REPEAT as i32,
                    TextureWrap::Repeat => glow::REPEAT as i32,
                    TextureWrap::Clamp => glow::CLAMP_TO_EDGE as i32,
                    TextureWrap::Border => glow::CLAMP_TO_BORDER as i32,
                },
            );

            thread.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                match layout.wrap_y {
                    TextureWrap::Mirror => glow::MIRRORED_REPEAT as i32,
                    TextureWrap::Repeat => glow::REPEAT as i32,
                    TextureWrap::Clamp => glow::CLAMP_TO_EDGE as i32,
                    TextureWrap::Border => glow::CLAMP_TO_BORDER as i32,
                },
            );

            thread
                .gl
                .tex_parameter_f32_slice(glow::TEXTURE_2D, glow::TEXTURE_BORDER_COLOR, &layout.wrap_border);

            thread.gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                internal_format as i32,
                layout.width as i32,
                layout.height as i32,
                0,
                format,
                data_type,
                glow::PixelUnpackData::Slice(None),
            );

            Ok(Texture {
                context: self.clone(),
                texture,
                width: layout.width,
                height: layout.height,
            })
        })
    }

    fn create_pipeline(&self, layout: PipelineLayout) -> Result<Self::Pipeline, Error> {
        self.with_current(|thread| unsafe {
            let shader = match layout.shader {
                Shader::Glsl(shader) => shader,
                _ => return Err(Error::UnsupportedFormat),
                // todo
            };

            let program = DisposeOnDrop::new(thread.gl.create_program().map_err(Error::Internal)?, |obj| {
                thread.gl.delete_program(obj)
            });

            let vertex_array = DisposeOnDrop::new(thread.gl.create_vertex_array().map_err(Error::Internal)?, |obj| {
                thread.gl.delete_vertex_array(obj)
            });

            let [vertex, fragment] = [false, true].map(|is_fragment| {
                let source = if is_fragment { shader.fragment } else { shader.vertex };

                let shader = DisposeOnDrop::new(
                    thread
                        .gl
                        .create_shader(if is_fragment {
                            glow::FRAGMENT_SHADER
                        } else {
                            glow::VERTEX_SHADER
                        })
                        .map_err(Error::Internal)?,
                    |obj| thread.gl.delete_shader(obj),
                );

                thread.gl.shader_source(*shader, source);
                thread.gl.compile_shader(*shader);

                if !thread.gl.get_shader_compile_status(*shader) {
                    let log = thread.gl.get_shader_info_log(*shader);
                    return Err(Error::Compile(
                        if is_fragment {
                            CompileStage::Fragment
                        } else {
                            CompileStage::Vertex
                        },
                        log,
                    ));
                }

                thread.gl.attach_shader(*program, *shader);
                Ok(shader)
            });

            let vertex = vertex?;
            let fragment = fragment?;

            thread.gl.link_program(*program);
            if !thread.gl.get_program_link_status(*program) {
                let log = thread.gl.get_program_info_log(*program);
                return Err(Error::Compile(CompileStage::Linking, log));
            }

            thread.gl.detach_shader(*program, *vertex);
            thread.gl.detach_shader(*program, *fragment);

            let bindings = prepare_pipeline_bindings(&thread.gl, &thread.features, *program, shader.bindings)?;

            Ok(Pipeline {
                context: self.clone(),
                program: program.take(),
                vertex_array: vertex_array.take(),
                bindings,
                topology: layout.topology,
                color_blend: layout.color_blend,
                depth_test: layout.depth_test,
                depth_write: layout.depth_write,
                stencil_ccw: layout.stencil_ccw,
                stencil_cw: layout.stencil_cw,
                cull_ccw: layout.cull_ccw,
                cull_cw: layout.cull_cw,
            })
        })
    }

    fn create_framebuffer(&self, layout: FramebufferLayout) -> Result<Self::Framebuffer, Error> {
        self.with_current(|thread| unsafe {
            if layout.width.max(layout.height) > thread.features.max_framebuffer_size {
                return Err(Error::UnsupportedSize);
            }

            if layout.msaa_samples > thread.features.max_framebuffer_msaa {
                return Err(Error::UnsupportedSampleCount);
            }

            let framebuffer = DisposeOnDrop::new(thread.gl.create_framebuffer().map_err(Error::Internal)?, |obj| {
                thread.gl.delete_framebuffer(obj)
            });

            thread.gl.bind_framebuffer(glow::FRAMEBUFFER, Some(*framebuffer));

            let (color_texture, color_buffer) = if let Some(format) = layout.color {
                let (format, data_type, internal_format) = color_format(format);

                if layout.is_color_bindable {
                    let texture = DisposeOnDrop::new(thread.gl.create_texture().map_err(Error::Internal)?, |obj| {
                        thread.gl.delete_texture(obj)
                    });

                    thread.gl.bind_texture(glow::TEXTURE_2D, Some(*texture));

                    if layout.msaa_samples > 0 {
                        thread.gl.tex_image_2d_multisample(
                            glow::TEXTURE_2D,
                            layout.msaa_samples as i32,
                            internal_format as i32,
                            layout.width as i32,
                            layout.height as i32,
                            false,
                        );
                    } else {
                        thread.gl.tex_image_2d(
                            glow::TEXTURE_2D,
                            0,
                            internal_format as i32,
                            layout.width as i32,
                            layout.height as i32,
                            0,
                            format,
                            data_type,
                            glow::PixelUnpackData::Slice(None),
                        );
                    }

                    thread
                        .gl
                        .tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
                    thread
                        .gl
                        .tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
                    thread.gl.framebuffer_texture_2d(
                        glow::FRAMEBUFFER,
                        glow::COLOR_ATTACHMENT0,
                        glow::TEXTURE_2D,
                        Some(*texture),
                        0,
                    );

                    (Some(texture), None)
                } else {
                    let buffer = DisposeOnDrop::new(thread.gl.create_renderbuffer().map_err(Error::Internal)?, |obj| {
                        thread.gl.delete_renderbuffer(obj)
                    });

                    thread.gl.bind_renderbuffer(glow::RENDERBUFFER, Some(*buffer));

                    if layout.msaa_samples > 0 {
                        thread.gl.renderbuffer_storage_multisample(
                            glow::RENDERBUFFER,
                            layout.msaa_samples as i32,
                            internal_format,
                            layout.width as i32,
                            layout.height as i32,
                        );
                    } else {
                        thread.gl.renderbuffer_storage(
                            glow::RENDERBUFFER,
                            internal_format,
                            layout.width as i32,
                            layout.height as i32,
                        );
                    }

                    thread.gl.framebuffer_renderbuffer(
                        glow::FRAMEBUFFER,
                        glow::COLOR_ATTACHMENT0,
                        glow::RENDERBUFFER,
                        Some(*buffer),
                    );

                    (None, Some(buffer))
                }
            } else {
                (None, None)
            };

            let (depth_texture, depth_buffer) = if let Some((format, attachment)) =
                depth_stencil_format(layout.depth, layout.stencil)
            {
                if layout.is_depth_bindable {
                    let texture = DisposeOnDrop::new(thread.gl.create_texture().map_err(Error::Internal)?, |obj| {
                        thread.gl.delete_texture(obj)
                    });

                    thread.gl.bind_texture(glow::TEXTURE_2D, Some(*texture));

                    if layout.msaa_samples > 0 {
                        thread.gl.tex_image_2d_multisample(
                            glow::TEXTURE_2D,
                            layout.msaa_samples as i32,
                            format as i32,
                            layout.width as i32,
                            layout.height as i32,
                            false,
                        );
                    } else {
                        thread.gl.tex_image_2d(
                            glow::TEXTURE_2D,
                            0,
                            format as i32,
                            layout.width as i32,
                            layout.height as i32,
                            0,
                            glow::DEPTH_COMPONENT32F,
                            glow::FLOAT,
                            glow::PixelUnpackData::Slice(None),
                        );
                    }

                    thread.gl.framebuffer_texture_2d(
                        glow::FRAMEBUFFER,
                        attachment,
                        glow::TEXTURE_2D,
                        Some(*texture),
                        0,
                    );

                    (Some(texture), None)
                } else {
                    let buffer = DisposeOnDrop::new(thread.gl.create_renderbuffer().map_err(Error::Internal)?, |obj| {
                        thread.gl.delete_renderbuffer(obj)
                    });

                    thread.gl.bind_renderbuffer(glow::RENDERBUFFER, Some(*buffer));

                    if layout.msaa_samples > 0 {
                        thread.gl.renderbuffer_storage_multisample(
                            glow::RENDERBUFFER,
                            layout.msaa_samples as i32,
                            format,
                            layout.width as i32,
                            layout.height as i32,
                        );
                    } else {
                        thread.gl.renderbuffer_storage(
                            glow::RENDERBUFFER,
                            format,
                            layout.width as i32,
                            layout.height as i32,
                        );
                    }

                    thread.gl.framebuffer_renderbuffer(
                        glow::FRAMEBUFFER,
                        attachment,
                        glow::RENDERBUFFER,
                        Some(*buffer),
                    );

                    (None, Some(buffer))
                }
            } else {
                (None, None)
            };

            Ok(Framebuffer {
                context: self.clone(),
                framebuffer: Some(framebuffer.take()),
                color_texture: color_texture.map(|t| t.take()),
                depth_texture: depth_texture.map(|t| t.take()),
                color_buffer: color_buffer.map(|b| b.take()),
                depth_buffer: depth_buffer.map(|b| b.take()),
            })
        })
    }

    fn create_profiler(&self) -> Result<Self::Profiler, Error> {
        self.with_current(|thread| unsafe {
            if !thread.features.query_time_elapsed {
                return Err(Error::UnsupportedFeature);
            }

            Ok(Profiler {
                query: thread.gl.create_query().map_err(Error::Internal)?,
                context: self.clone(),
                state: Cell::new(0),
            })
        })
    }

    fn invalidate_buffer(&self, buffer: &Self::Buffer, offset: u64, size: u64) -> Result<(), Error> {
        self.with_current(|thread| unsafe {
            let size: u32 = size.try_into().map_err(|_| Error::InvalidBounds)?;
            let offset: u32 = offset.try_into().map_err(|_| Error::InvalidBounds)?;

            if offset.saturating_add(size) > buffer.capacity
                || !offset.is_multiple_of(thread.features.buffer_alignment(buffer.role))
            {
                return Err(Error::InvalidBounds);
            }

            thread.gl.bind_buffer(buffer_target(buffer.role), Some(buffer.buffer));

            if offset == 0 && size == buffer.capacity {
                thread.gl.buffer_data_size(
                    buffer_target(buffer.role),
                    buffer.capacity as i32,
                    if buffer.is_dynamic {
                        glow::DYNAMIC_DRAW
                    } else {
                        glow::STATIC_DRAW
                    },
                );
            } else if thread.features.invalidate_buffer_sub_data {
                thread
                    .gl
                    .invalidate_buffer_sub_data(buffer_target(buffer.role), offset as i32, size as i32);
            }

            Ok(())
        })
    }

    fn copy_buffer(
        &self,
        buffer: &Self::Buffer,
        source_buffer: &Self::Buffer,
        offset: u64,
        source_offset: u64,
        size: u64,
    ) -> Result<(), Error> {
        self.with_current(|thread| unsafe {
            let size: u32 = size.try_into().map_err(|_| Error::InvalidBounds)?;
            let offset: u32 = offset.try_into().map_err(|_| Error::InvalidBounds)?;
            let source_offset: u32 = source_offset.try_into().map_err(|_| Error::InvalidBounds)?;

            if !offset.is_multiple_of(thread.features.buffer_alignment(buffer.role))
                || !source_offset.is_multiple_of(thread.features.buffer_alignment(source_buffer.role))
            {
                return Err(Error::InvalidBounds);
            }

            if offset.saturating_add(size) > buffer.capacity
                || source_offset.saturating_add(size) > source_buffer.capacity
            {
                return Err(Error::InvalidBounds);
            }

            if buffer.buffer == source_buffer.buffer
                && offset.min(source_offset).saturating_add(size) > offset.max(source_offset)
            {
                return Err(Error::InvalidBounds);
            }

            thread
                .gl
                .bind_buffer(glow::COPY_READ_BUFFER, Some(source_buffer.buffer));
            thread.gl.bind_buffer(glow::COPY_WRITE_BUFFER, Some(buffer.buffer));

            thread.gl.copy_buffer_sub_data(
                glow::COPY_READ_BUFFER,
                glow::COPY_WRITE_BUFFER,
                source_offset as i32,
                offset as i32,
                size as i32,
            );

            Ok(())
        })
    }

    fn upload_buffer(&self, buffer: &Self::Buffer, offset: u64, data: &[u8]) -> Result<(), Error> {
        self.with_current(|thread| unsafe {
            let offset: u32 = offset.try_into().map_err(|_| Error::InvalidBounds)?;
            let size: u32 = data.len().try_into().map_err(|_| Error::InvalidBounds)?;

            if offset.saturating_add(size) > buffer.capacity
                || !offset.is_multiple_of(thread.features.buffer_alignment(buffer.role))
            {
                return Err(Error::InvalidBounds);
            }

            thread.gl.bind_buffer(buffer_target(buffer.role), Some(buffer.buffer));

            if offset == 0 && size == buffer.capacity {
                thread.gl.buffer_data_u8_slice(
                    buffer_target(buffer.role),
                    data,
                    if buffer.is_dynamic {
                        glow::DYNAMIC_DRAW
                    } else {
                        glow::STATIC_DRAW
                    },
                );
            } else {
                thread
                    .gl
                    .buffer_sub_data_u8_slice(buffer_target(buffer.role), offset as i32, data);
            }

            Ok(())
        })
    }

    fn upload_texture(
        &self,
        texture: &Self::Texture,
        bounds: TextureBounds,
        format: TextureFormat,
        data: &[u8],
    ) -> Result<(), Error> {
        self.with_current(|thread| unsafe {
            if bounds.x.saturating_add(bounds.width) > texture.width
                || bounds.y.saturating_add(bounds.height) > texture.height
            {
                return Err(Error::InvalidBounds);
            }

            if data.len() != (bounds.width * bounds.height * format.bytes_per_pixel()) as usize {
                return Err(Error::InvalidData);
            }

            let (format, data_type, _) = color_format(format);

            thread.gl.bind_texture(glow::TEXTURE_2D, Some(texture.texture));

            thread.gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                bounds.x as i32,
                bounds.y as i32,
                bounds.width as i32,
                bounds.height as i32,
                format,
                data_type,
                glow::PixelUnpackData::Slice(Some(data)),
            );

            Ok(())
        })
    }

    fn read_framebuffer(
        &self,
        target: &Self::Framebuffer,
        bounds: TextureBounds,
        format: TextureFormat,
        data: &mut [u8],
    ) -> Result<(), Error> {
        self.with_current(|thread| unsafe {
            if bounds.x.saturating_add(bounds.width) > thread.features.max_framebuffer_size
                || bounds.y.saturating_add(bounds.height) > thread.features.max_framebuffer_size
            {
                return Err(Error::InvalidBounds);
            }

            if data.len() != (bounds.width * bounds.height * format.bytes_per_pixel()) as usize {
                return Err(Error::InvalidData);
            }

            let (format, data_type, _) = color_format(format);

            thread.gl.bind_framebuffer(glow::READ_FRAMEBUFFER, target.framebuffer);

            thread.gl.read_pixels(
                bounds.x as i32,
                bounds.y as i32,
                bounds.width as i32,
                bounds.height as i32,
                format,
                data_type,
                glow::PixelPackData::Slice(Some(data)),
            );

            Ok(())
        })
    }

    fn begin_profiler(&self, profiler: &Self::Profiler) -> Result<(), Error> {
        self.with_current(|thread| unsafe {
            if profiler.state.get() == 0 {
                thread.gl.begin_query(glow::TIME_ELAPSED, profiler.query);
                profiler.state.set(1);
            }

            Ok(())
        })
    }

    fn end_profiler(&self, profiler: &Self::Profiler) -> Result<Option<Duration>, Error> {
        self.with_current(|thread| unsafe {
            if profiler.state.get() == 1 {
                thread.gl.end_query(glow::TIME_ELAPSED);
                profiler.state.set(2);
            }

            if profiler.state.get() == 2 {
                let available = thread
                    .gl
                    .get_query_parameter_u32(profiler.query, glow::QUERY_RESULT_AVAILABLE);

                if available != 0 {
                    let result = thread.gl.get_query_parameter_u32(profiler.query, glow::QUERY_RESULT);

                    profiler.state.set(0);
                    return Ok(Some(Duration::from_nanos(result as u64)));
                }
            }

            Ok(None)
        })
    }

    fn clear(&self, clear: ClearRequest<Self>) -> Result<(), Error> {
        self.with_current(|thread| unsafe {
            if thread.last_framebuffer != Some(clear.target.framebuffer) {
                thread.last_framebuffer = Some(clear.target.framebuffer);
                thread.gl.bind_framebuffer(glow::FRAMEBUFFER, clear.target.framebuffer);
            }

            if let Some(color) = clear.color {
                thread.gl.clear_color(color[0], color[1], color[2], color[3]);
            }

            if let Some(depth) = clear.depth {
                thread.gl.clear_depth_f32(depth);
            }

            if let Some(stencil) = clear.stencil {
                thread.gl.clear_stencil(stencil as i32);
            }

            if thread.last_scissor != Some(clear.scissor) {
                thread.last_scissor = Some(clear.scissor);

                match clear.scissor {
                    None => thread.gl.disable(glow::SCISSOR_TEST),
                    Some(scissor) => {
                        thread.gl.enable(glow::SCISSOR_TEST);
                        thread.gl.scissor(
                            scissor.x as i32,
                            scissor.y as i32,
                            scissor.width as i32,
                            scissor.height as i32,
                        );
                    }
                }
            }

            let mut mask = 0;

            if clear.color.is_some() {
                mask |= glow::COLOR_BUFFER_BIT;
            }

            if clear.depth.is_some() {
                mask |= glow::DEPTH_BUFFER_BIT;
            }

            if clear.stencil.is_some() {
                mask |= glow::STENCIL_BUFFER_BIT;
            }

            thread.gl.clear(mask);

            Ok(())
        })
    }

    fn draw(&self, draw: DrawRequest<Self>) -> Result<(), Error> {
        self.with_current(|thread| unsafe {
            if thread.last_framebuffer != Some(draw.target.framebuffer) {
                thread.last_framebuffer = Some(draw.target.framebuffer);
                thread.gl.bind_framebuffer(glow::FRAMEBUFFER, draw.target.framebuffer);
            }

            if thread.last_pipeline != Some(draw.pipeline.program) {
                thread.last_pipeline = Some(draw.pipeline.program);
                util::apply_pipeline(&thread.gl, draw.pipeline);
            }

            if thread.last_viewport != Some(draw.viewport) {
                thread.last_viewport = Some(draw.viewport);
                thread.gl.viewport(
                    draw.viewport.x as i32,
                    draw.viewport.y as i32,
                    draw.viewport.width as i32,
                    draw.viewport.height as i32,
                );
            }

            if thread.last_scissor != Some(draw.scissor) {
                thread.last_scissor = Some(draw.scissor);
                match draw.scissor {
                    None => thread.gl.disable(glow::SCISSOR_TEST),
                    Some(scissor) => {
                        thread.gl.enable(glow::SCISSOR_TEST);
                        thread.gl.scissor(
                            scissor.x as i32,
                            scissor.y as i32,
                            scissor.width as i32,
                            scissor.height as i32,
                        );
                    }
                }
            }

            for (i, binding) in draw.pipeline.bindings.iter().enumerate() {
                match binding {
                    ProgramBinding::Unbound => {}

                    ProgramBinding::Buffer { index, size, role } => {
                        let (buffer, offset, data_size) = match draw.bindings.get(i) {
                            Some(BindingData::Buffer { buffer, offset, size }) => (buffer, offset, size),
                            _ => return Err(Error::InvalidBinding(i)),
                        };

                        if buffer.role != *role || data_size < size {
                            return Err(Error::InvalidBinding(i));
                        }

                        thread.gl.bind_buffer(buffer_target(*role), Some(buffer.buffer));
                        thread.gl.bind_buffer_range(
                            buffer_target(buffer.role),
                            *index,
                            Some(buffer.buffer),
                            *offset as i32,
                            *data_size as i32,
                        );
                    }

                    ProgramBinding::Texture2D { index } => {
                        let texture = match draw.bindings.get(i) {
                            Some(BindingData::Texture { texture }) => texture.texture,
                            Some(BindingData::Framebuffer {
                                framebuffer,
                                attachment,
                            }) => {
                                if framebuffer.framebuffer == draw.target.framebuffer {
                                    return Err(Error::InvalidFramebuffer);
                                }

                                let texture = match attachment {
                                    FramebufferAttachment::Color => framebuffer.color_texture,
                                    FramebufferAttachment::Depth => framebuffer.depth_texture,
                                    FramebufferAttachment::Stencil => None,
                                };

                                match texture {
                                    Some(texture) => texture,
                                    None => return Err(Error::InvalidBinding(i)),
                                }
                            }

                            _ => {
                                return Err(Error::InvalidBinding(i));
                            }
                        };

                        thread.gl.active_texture(glow::TEXTURE0 + *index);
                        thread.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                    }
                }
            }

            thread.gl.draw_arrays(
                match draw.pipeline.topology {
                    PrimitiveTopology::TriangleList => glow::TRIANGLES,
                    PrimitiveTopology::TriangleStrip => glow::TRIANGLE_STRIP,
                    PrimitiveTopology::TriangleFan => glow::TRIANGLE_FAN,
                },
                0,
                draw.vertices as i32,
            );

            Ok(())
        })
    }

    fn present(&self) -> Result<(), Error> {
        self.with_current(|thread| {
            thread.surface.swap_buffers()?;
            Ok(())
        })
    }
}

impl Drop for Buffer<'_> {
    fn drop(&mut self) {
        let _ = self.context.with_current(|thread| unsafe {
            thread.gl.delete_buffer(self.buffer);
            Ok(())
        });
    }
}

impl Drop for Texture<'_> {
    fn drop(&mut self) {
        let _ = self.context.with_current(|thread| unsafe {
            thread.gl.delete_texture(self.texture);
            Ok(())
        });
    }
}

impl Drop for Pipeline<'_> {
    fn drop(&mut self) {
        let _ = self.context.with_current(|thread| unsafe {
            thread.gl.delete_program(self.program);
            thread.gl.delete_vertex_array(self.vertex_array);
            Ok(())
        });
    }
}

impl Drop for Profiler<'_> {
    fn drop(&mut self) {
        let _ = self.context.with_current(|thread| unsafe {
            thread.gl.delete_query(self.query);
            Ok(())
        });
    }
}

impl Drop for Framebuffer<'_> {
    fn drop(&mut self) {
        let _ = self.context.with_current(|thread| unsafe {
            if let Some(framebuffer) = self.framebuffer {
                thread.gl.delete_framebuffer(framebuffer);
            }

            if let Some(texture) = self.color_texture {
                thread.gl.delete_texture(texture);
            }

            if let Some(texture) = self.depth_texture {
                thread.gl.delete_texture(texture);
            }

            if let Some(renderbuffer) = self.color_buffer {
                thread.gl.delete_renderbuffer(renderbuffer);
            }

            if let Some(renderbuffer) = self.depth_buffer {
                thread.gl.delete_renderbuffer(renderbuffer);
            }

            Ok(())
        });
    }
}

impl Debug for Context<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Context").finish_non_exhaustive()
    }
}
