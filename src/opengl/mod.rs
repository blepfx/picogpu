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
use glow::HasContext;
use std::cell::{Cell, RefCell};
use std::fmt::Debug;
use std::mem::replace;
use std::rc::Rc;
use std::time::Duration;
use util::*;

pub use surface::{Surface, SurfaceError};

/// OpenGL implementation of the `picogpu` rendering backend.
#[derive(Clone)]
pub struct Context<'a>(Rc<RefCell<ContextInner<'a>>>);

struct ContextInner<'a> {
    gl: glow::Context,
    features: Features,
    surface: Box<dyn Surface + 'a>,

    last_pipeline: Option<glow::Program>,
    last_viewport: Option<TextureBounds>,
    last_scissor: Option<Option<TextureBounds>>,
    last_framebuffer: Option<Option<glow::Framebuffer>>,

    query_pool: Vec<glow::Query>,
    query_timestamp: bool,
    query_primitives: bool,
    query_occlusion: bool,
}

/// An OpenGL buffer object.
///
/// See [`Context::Buffer`](crate::Context::Buffer) for more details.
#[derive(Debug)]
pub struct Buffer<'a> {
    context: Context<'a>,
    buffer: glow::Buffer,

    role: BufferRole,
    capacity: u32,
    can_upload: bool,
    can_download: bool,
}

/// An OpenGL pipeline object.
///
/// See [`Context::Pipeline`](crate::Context::Pipeline) for more details.
#[derive(Debug)]
pub struct Pipeline<'a> {
    context: Context<'a>,
    program: glow::Program,

    vertex_array: glow::VertexArray,
    bindings: Vec<ProgramBinding>,

    color_blend: BlendMode,
    depth_test: CompareFn,
    depth_write: bool,
    stencil_ccw: StencilFace,
    stencil_cw: StencilFace,
    cull_ccw: bool,
    cull_cw: bool,
    topology: PrimitiveTopology,
}

/// An OpenGL texture object.
///
/// See [`Context::Texture`](crate::Context::Texture) for more details.
#[derive(Debug)]
pub struct Texture<'a> {
    context: Context<'a>,
    texture: glow::Texture,

    width: u32,
    height: u32,
    format: TextureFormat,
}

/// An OpenGL framebuffer object.
///
/// See [`Context::Framebuffer`](crate::Context::Framebuffer) for more details.
#[derive(Debug)]
pub struct Framebuffer<'a> {
    context: Context<'a>,
    framebuffer: Option<glow::Framebuffer>,

    color: Vec<FramebufferColor>,
    depth: Option<FramebufferDepthStencil>,
}

#[derive(Debug)]
struct FramebufferColor {
    storage: FramebufferStorage,
    format: TextureFormat,
}

#[derive(Debug)]
struct FramebufferDepthStencil {
    storage: FramebufferStorage,
    depth: Option<DepthFormat>,
    stencil: Option<StencilFormat>,
}

/// An OpenGL fence object.
///
/// See [`Context::Fence`](crate::Context::Fence) for more details.
#[derive(Debug)]
pub struct Fence<'a> {
    context: Context<'a>,
    fence: Option<glow::Fence>,
}

/// An OpenGL query object.
#[derive(Debug)]
pub struct Query<'a> {
    context: Context<'a>,
    state: Cell<QueryState>,
    type_: QueryType,
}

#[derive(Debug, Clone, Copy)]
enum QueryState {
    Begun(glow::Query),
    Ended(glow::Query),
    Available(u64),
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
            let Some(features) = Features::from_context(&gl) else {
                return Err(Error::Internal(format!("opengl version too old: {:?}", gl.version())));
            };

            Ok(Self(Rc::new(RefCell::new(ContextInner {
                gl,
                features,
                surface: Box::new(surface),

                last_pipeline: None,
                last_viewport: None,
                last_scissor: None,
                last_framebuffer: None,

                query_pool: Vec::new(),
                query_occlusion: false,
                query_primitives: false,
                query_timestamp: false,
            }))))
        }
    }

    /// Attach a debug callback to the OpenGL context that will be called whenever a debug message
    /// is generated.
    ///
    /// Can only be called once.
    pub fn attach_debug_callback(&self, callback: impl Fn(DebugMessage, &str) + Send + Sync + 'static) {
        self.with_current(|thread| unsafe {
            if !thread.features.debug_callback {
                return Ok(());
            }

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
            color: Vec::new(),
            depth: None,
        }
    }

    fn with_current<R>(&self, f: impl FnOnce(&mut ContextInner) -> Result<R, Error>) -> Result<R, Error> {
        let mut context = self.0.borrow_mut();
        context.surface.make_current()?;
        f(&mut context)
    }
}

impl<'a> crate::Context for Context<'a> {
    type Buffer = Buffer<'a>;
    type Texture = Texture<'a>;
    type Pipeline = Pipeline<'a>;
    type Framebuffer = Framebuffer<'a>;
    type Fence = Fence<'a>;
    type Query = Query<'a>;

    #[inline]
    fn capabilities(&self) -> Capabilities {
        let thread = self.0.borrow();

        Capabilities {
            shader_format: thread.features.glsl_version(),
            texture_size: thread.features.max_texture_size,
            texture_bindings: thread.features.max_texture_image_units,
            framebuffer_size: thread.features.max_framebuffer_size,
            framebuffer_msaa: thread.features.max_framebuffer_msaa,
            framebuffer_outputs: thread.features.max_framebuffer_outputs,
            uniform_buffer_size: thread.features.max_uniform_buffer_size as u64,
            storage_buffer_size: thread.features.max_storage_buffer_size as u64,
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
                buffer_hint(layout.can_upload, layout.can_download),
            );

            Ok(Buffer {
                context: self.clone(),
                buffer,
                capacity,
                can_upload: layout.can_upload,
                can_download: layout.can_download,
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
                texture_wrap(layout.wrap_x) as i32,
            );

            thread.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                texture_wrap(layout.wrap_y) as i32,
            );

            thread.gl.tex_parameter_f32_slice(
                glow::TEXTURE_2D,
                glow::TEXTURE_BORDER_COLOR,
                match layout.wrap_border {
                    TextureBorder::Transparent => &[0.0, 0.0, 0.0, 0.0],
                    TextureBorder::Black => &[0.0, 0.0, 0.0, 1.0],
                    TextureBorder::White => &[1.0, 1.0, 1.0, 1.0],
                },
            );

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
                format: layout.format,
            })
        })
    }

    fn create_pipeline(&self, layout: PipelineLayout) -> Result<Self::Pipeline, Error> {
        self.with_current(|thread| unsafe {
            let shader = match layout.shader {
                Shader::Glsl(shader) => shader,
                _ => return Err(Error::UnsupportedFormat),
            };

            let program = DisposeOnDrop::new(thread.gl.create_program().map_err(Error::Internal)?, |obj| {
                thread.gl.delete_program(obj)
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

            let vertex_array = DisposeOnDrop::new(thread.gl.create_vertex_array().map_err(Error::Internal)?, |obj| {
                thread.gl.delete_vertex_array(obj)
            });

            Ok(Pipeline {
                context: self.clone(),
                program: program.take(),
                vertex_array: vertex_array.take(),
                bindings,
                color_blend: layout.color_blend,
                topology: layout.topology,
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
                return Err(Error::UnsupportedFormat);
            }

            if layout.color.len() > thread.features.max_framebuffer_outputs as usize {
                return Err(Error::UnsupportedFormat);
            }

            let framebuffer = DisposeOnDrop::new(thread.gl.create_framebuffer().map_err(Error::Internal)?, |obj| {
                thread.gl.delete_framebuffer(obj)
            });

            thread.gl.bind_framebuffer(glow::FRAMEBUFFER, Some(*framebuffer));

            let color = layout
                .color
                .iter()
                .enumerate()
                .map(|(index, format)| {
                    let (gl_format, data_type, internal_format) = color_format(*format);
                    let storage = FramebufferStorage::create(
                        &thread.gl,
                        glow::COLOR_ATTACHMENT0 + index as u32,
                        gl_format,
                        data_type,
                        internal_format,
                        layout.width,
                        layout.height,
                        layout.msaa_samples,
                        layout.is_color_bindable,
                    )?;

                    Ok(DisposeOnDrop::new(
                        FramebufferColor {
                            storage,
                            format: *format,
                        },
                        |obj| obj.storage.delete(&thread.gl),
                    ))
                })
                .collect::<Result<Vec<_>, Error>>()?;

            let depth = if let Some((format, attachment)) = depth_stencil_format(layout.depth, layout.stencil) {
                let storage = FramebufferStorage::create(
                    &thread.gl,
                    attachment,
                    format,
                    glow::FLOAT,
                    glow::DEPTH_COMPONENT32F,
                    layout.width,
                    layout.height,
                    layout.msaa_samples,
                    layout.is_color_bindable,
                )?;

                Some(DisposeOnDrop::new(
                    FramebufferDepthStencil {
                        storage,
                        depth: layout.depth,
                        stencil: layout.stencil,
                    },
                    |obj| obj.storage.delete(&thread.gl),
                ))
            } else {
                None
            };

            if layout.color.len() > 1 {
                let draw_buffers = (0..layout.color.len() as u32)
                    .map(|i| glow::COLOR_ATTACHMENT0 + i)
                    .collect::<Vec<_>>();
                thread.gl.draw_buffers(&draw_buffers);
            }

            Ok(Framebuffer {
                context: self.clone(),
                framebuffer: Some(framebuffer.take()),
                color: color.into_iter().map(|storage| storage.take()).collect(),
                depth: depth.map(|storage| storage.take()),
            })
        })
    }

    fn copy_buffer_to_buffer(
        &self,
        dst_buffer: &Self::Buffer,
        dst_offset: u64,
        src_buffer: &Self::Buffer,
        src_offset: u64,
        size: u64,
    ) -> Result<(), Error> {
        self.with_current(|thread| unsafe {
            let size: u32 = size.try_into().map_err(|_| Error::InvalidBounds)?;
            let offset: u32 = dst_offset.try_into().map_err(|_| Error::InvalidBounds)?;
            let source_offset: u32 = src_offset.try_into().map_err(|_| Error::InvalidBounds)?;

            if !offset.is_multiple_of(thread.features.buffer_alignment(dst_buffer.role))
                || !source_offset.is_multiple_of(thread.features.buffer_alignment(src_buffer.role))
            {
                return Err(Error::InvalidBounds);
            }

            if offset.saturating_add(size) > dst_buffer.capacity
                || source_offset.saturating_add(size) > src_buffer.capacity
            {
                return Err(Error::InvalidBounds);
            }

            if dst_buffer.buffer == src_buffer.buffer
                && offset.min(source_offset).saturating_add(size) > offset.max(source_offset)
            {
                return Err(Error::InvalidBounds);
            }

            thread.gl.bind_buffer(glow::COPY_READ_BUFFER, Some(src_buffer.buffer));
            thread.gl.bind_buffer(glow::COPY_WRITE_BUFFER, Some(dst_buffer.buffer));
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

    fn copy_buffer_to_texture(
        &self,
        dst_texture: &Self::Texture,
        dst_bounds: TextureBounds,
        src_buffer: &Self::Buffer,
        src_offset: u64,
    ) -> Result<(), Error> {
        self.with_current(|thread| unsafe {
            let size = dst_bounds.width * dst_bounds.height * dst_texture.format.bytes_per_pixel();
            let offset: u32 = src_offset.try_into().map_err(|_| Error::InvalidBounds)?;
            let (format, data_type, _) = color_format(dst_texture.format);

            if dst_bounds.x.saturating_add(dst_bounds.width) > dst_texture.width
                || dst_bounds.y.saturating_add(dst_bounds.height) > dst_texture.height
            {
                return Err(Error::InvalidBounds);
            }

            if offset.saturating_add(size) > src_buffer.capacity
                || !offset.is_multiple_of(thread.features.buffer_alignment(src_buffer.role))
            {
                return Err(Error::InvalidBounds);
            }

            thread
                .gl
                .bind_buffer(glow::PIXEL_UNPACK_BUFFER, Some(src_buffer.buffer));
            thread.gl.bind_texture(glow::TEXTURE_2D, Some(dst_texture.texture));
            thread.gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            thread.gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                dst_bounds.x as i32,
                dst_bounds.y as i32,
                dst_bounds.width as i32,
                dst_bounds.height as i32,
                format,
                data_type,
                glow::PixelUnpackData::BufferOffset(offset),
            );
            // dont forget to unbind it!
            thread.gl.bind_buffer(glow::PIXEL_UNPACK_BUFFER, None);

            Ok(())
        })
    }

    fn copy_framebuffer_to_buffer(
        &self,
        dst_buffer: &Self::Buffer,
        dst_offset: u64,
        src_framebuffer: &Self::Framebuffer,
        src_attachment: FramebufferAttachment,
        src_bounds: TextureBounds,
    ) -> Result<(), Error> {
        self.with_current(|thread| unsafe {
            let (format, gltype, pixel_size) = match src_attachment {
                FramebufferAttachment::Stencil => {
                    if src_framebuffer.depth.as_ref().is_none_or(|d| d.stencil.is_none()) {
                        return Err(Error::InvalidOperation);
                    }

                    (glow::STENCIL_INDEX, glow::UNSIGNED_BYTE, 1)
                }

                FramebufferAttachment::Depth => {
                    if src_framebuffer.depth.as_ref().is_none_or(|d| d.depth.is_none()) {
                        return Err(Error::InvalidOperation);
                    }

                    (glow::DEPTH_COMPONENT, glow::FLOAT, 4)
                }

                FramebufferAttachment::Color(index) => {
                    let color = src_framebuffer
                        .color
                        .get(index as usize)
                        .ok_or(Error::InvalidOperation)?;

                    thread.gl.read_buffer(glow::COLOR_ATTACHMENT0 + index as u32);
                    let (format, gl_type, _) = color_format(color.format);
                    (format, gl_type, color.format.bytes_per_pixel())
                }
            };

            let size = src_bounds.width * src_bounds.height * pixel_size;
            let offset: u32 = dst_offset.try_into().map_err(|_| Error::InvalidBounds)?;

            if src_bounds.x.saturating_add(src_bounds.width) > thread.features.max_framebuffer_size
                || src_bounds.y.saturating_add(src_bounds.height) > thread.features.max_framebuffer_size
            {
                return Err(Error::InvalidBounds);
            }

            if offset.saturating_add(size) > dst_buffer.capacity
                || !offset.is_multiple_of(thread.features.buffer_alignment(dst_buffer.role))
            {
                return Err(Error::InvalidBounds);
            }

            thread
                .gl
                .bind_framebuffer(glow::READ_FRAMEBUFFER_BINDING, src_framebuffer.framebuffer);
            thread.gl.bind_buffer(glow::PIXEL_PACK_BUFFER, Some(dst_buffer.buffer));
            thread.gl.read_pixels(
                src_bounds.x as i32,
                src_bounds.y as i32,
                src_bounds.width as i32,
                src_bounds.height as i32,
                format,
                gltype,
                glow::PixelPackData::BufferOffset(offset),
            );
            // dont forget to unbind it!
            thread.gl.bind_buffer(glow::PIXEL_PACK_BUFFER, None);

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

            if !buffer.can_upload {
                return Err(Error::InvalidOperation);
            }

            thread.gl.bind_buffer(buffer_target(buffer.role), Some(buffer.buffer));

            if offset == 0 && size == buffer.capacity {
                thread.gl.buffer_data_u8_slice(
                    buffer_target(buffer.role),
                    data,
                    buffer_hint(buffer.can_upload, buffer.can_download),
                );
            } else {
                thread
                    .gl
                    .buffer_sub_data_u8_slice(buffer_target(buffer.role), offset as i32, data);
            }

            Ok(())
        })
    }

    fn download_buffer(&self, buffer: &Self::Buffer, offset: u64, data: &mut [u8]) -> Result<(), Error> {
        self.with_current(|thread| unsafe {
            let offset: u32 = offset.try_into().map_err(|_| Error::InvalidBounds)?;
            let size: u32 = data.len().try_into().map_err(|_| Error::InvalidBounds)?;

            if offset.saturating_add(size) > buffer.capacity
                || !offset.is_multiple_of(thread.features.buffer_alignment(buffer.role))
            {
                return Err(Error::InvalidBounds);
            }

            if !buffer.can_download {
                return Err(Error::InvalidOperation);
            }

            thread.gl.bind_buffer(glow::COPY_READ_BUFFER, Some(buffer.buffer));
            thread
                .gl
                .get_buffer_sub_data(glow::COPY_READ_BUFFER, offset as i32, data);

            Ok(())
        })
    }

    fn begin_query(&self, type_: QueryType) -> Result<Self::Query, Error> {
        self.with_current(|thread| unsafe {
            let supported = match type_ {
                QueryType::Primitives => thread.features.query_samples_primitives,
                QueryType::Elapsed => thread.features.query_time_elapsed,
                QueryType::Occlusion(OcclusionTest::Samples) => thread.features.query_samples_primitives,
                QueryType::Occlusion(OcclusionTest::Exact) => thread.features.query_occlusion,
                QueryType::Occlusion(OcclusionTest::Conservative) => thread.features.query_occlusion_conservative,
            };

            if !supported {
                return Err(Error::UnsupportedFeature);
            }

            let active = match type_ {
                QueryType::Primitives => replace(&mut thread.query_primitives, true),
                QueryType::Elapsed => replace(&mut thread.query_timestamp, true),
                QueryType::Occlusion(_) => replace(&mut thread.query_occlusion, true),
            };

            if active {
                return Err(Error::InvalidOperation);
            }

            let query = match thread.query_pool.pop() {
                Some(query) => query,
                None => thread.gl.create_query().map_err(Error::Internal)?,
            };

            thread.gl.begin_query(query_target(type_), query);

            Ok(Query {
                context: self.clone(),
                state: Cell::new(QueryState::Begun(query)),
                type_,
            })
        })
    }

    fn end_query(&self, query: &Self::Query) -> Result<(), Error> {
        self.with_current(|thread| unsafe {
            let QueryState::Begun(gl_query) = query.state.get() else {
                return Err(Error::InvalidOperation);
            };

            match query.type_ {
                QueryType::Primitives => thread.query_primitives = false,
                QueryType::Elapsed => thread.query_timestamp = false,
                QueryType::Occlusion(_) => thread.query_occlusion = false,
            };

            thread.gl.end_query(query_target(query.type_));
            query.state.set(QueryState::Ended(gl_query));
            Ok(())
        })
    }

    fn read_query(&self, query: &Self::Query) -> Result<Option<u64>, Error> {
        self.with_current(|thread| unsafe {
            match query.state.get() {
                QueryState::Available(result) => Ok(Some(result)),
                QueryState::Begun(_) => Err(Error::InvalidOperation),
                QueryState::Ended(gl_query) => {
                    let available = thread
                        .gl
                        .get_query_parameter_u32(gl_query, glow::QUERY_RESULT_AVAILABLE);

                    if available == 0 {
                        return Ok(None);
                    }

                    let result = if thread.features.query_u64_result {
                        thread.gl.get_query_parameter_u64(gl_query, glow::QUERY_RESULT)
                    } else {
                        thread.gl.get_query_parameter_u32(gl_query, glow::QUERY_RESULT) as u64
                    };

                    thread.query_pool.push(gl_query);
                    query.state.set(QueryState::Available(result));
                    Ok(Some(result))
                }
            }
        })
    }

    fn wait_fence(&self, fence: &Self::Fence, timeout: Duration) -> Result<bool, Error> {
        self.with_current(|thread| unsafe {
            let Some(fence) = fence.fence else {
                return Err(Error::UnsupportedFeature);
            };

            if timeout == Duration::ZERO {
                let result = thread.gl.get_sync_status(fence);
                return Ok(result == glow::SIGNALED);
            }

            let result = thread
                .gl
                .client_wait_sync(fence, 0, timeout.as_nanos().try_into().unwrap_or(i32::MAX));

            if result == glow::WAIT_FAILED {
                return Err(Error::Internal("Waiting on a fence failed".into()));
            }

            Ok(result != glow::TIMEOUT_EXPIRED)
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
            if draw.vertices == 0 {
                return Ok(());
            }

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
                            _ => return Err(Error::BindingMismatch(i, "not a buffer")),
                        };

                        if buffer.role != *role {
                            return Err(Error::BindingMismatch(i, "invalid role"));
                        }

                        if *data_size < *size as u64 {
                            return Err(Error::BindingMismatch(i, "invalid size"));
                        }

                        if offset.saturating_add(*data_size) > buffer.capacity as u64 {
                            return Err(Error::BindingMismatch(i, "invalid offset"));
                        }

                        if !offset.is_multiple_of(thread.features.buffer_alignment(buffer.role) as u64) {
                            return Err(Error::BindingMismatch(i, "invalid alignment"));
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
                                if draw.target.framebuffer == framebuffer.framebuffer {
                                    return Err(Error::FramebufferInUse);
                                }

                                let texture = match attachment {
                                    FramebufferAttachment::Stencil => None,
                                    FramebufferAttachment::Depth => {
                                        framebuffer.depth.as_ref().and_then(|depth| depth.storage.texture())
                                    }
                                    FramebufferAttachment::Color(index) => framebuffer
                                        .color
                                        .get(*index as usize)
                                        .and_then(|color| color.storage.texture()),
                                };

                                match texture {
                                    Some(texture) => texture,
                                    None => return Err(Error::BindingMismatch(i, "invalid attachment")),
                                }
                            }

                            _ => {
                                return Err(Error::BindingMismatch(i, "not a texture"));
                            }
                        };

                        thread.gl.active_texture(glow::TEXTURE0 + *index);
                        thread.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                    }
                }
            }

            let topology = match draw.pipeline.topology {
                PrimitiveTopology::TriangleList => glow::TRIANGLES,
                PrimitiveTopology::TriangleStrip => glow::TRIANGLE_STRIP,
                PrimitiveTopology::TriangleFan => glow::TRIANGLE_FAN,
            };

            thread.gl.draw_arrays(topology, 0, draw.vertices as i32);

            Ok(())
        })
    }

    fn present(&self) -> Result<Self::Fence, Error> {
        self.with_current(|thread| {
            let fence = thread
                .features
                .fence_sync_objects
                .then(|| unsafe { thread.gl.fence_sync(glow::SYNC_GPU_COMMANDS_COMPLETE, 0).ok() })
                .flatten();

            thread.surface.swap_buffers()?;

            Ok(Fence {
                context: self.clone(),
                fence,
            })
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

impl Drop for Framebuffer<'_> {
    fn drop(&mut self) {
        let _ = self.context.with_current(|thread| unsafe {
            if let Some(framebuffer) = self.framebuffer {
                thread.gl.delete_framebuffer(framebuffer);
            }

            if let Some(depth) = self.depth.take() {
                depth.storage.delete(&thread.gl);
            }

            for color in self.color.drain(..) {
                color.storage.delete(&thread.gl);
            }

            Ok(())
        });
    }
}

impl Drop for Fence<'_> {
    fn drop(&mut self) {
        let _ = self.context.with_current(|thread| unsafe {
            if let Some(fence) = self.fence {
                thread.gl.delete_sync(fence);
            }

            Ok(())
        });
    }
}

impl Drop for Query<'_> {
    fn drop(&mut self) {
        let _ = self.context.with_current(|thread| {
            if let QueryState::Begun(gl_query) | QueryState::Ended(gl_query) = self.state.get() {
                thread.query_pool.push(gl_query);
            }

            Ok(())
        });
    }
}

impl Drop for ContextInner<'_> {
    fn drop(&mut self) {
        if self.surface.make_current().is_err() {
            return; // nothing to do, the context is already lost
        }

        for query in self.query_pool.drain(..) {
            unsafe {
                self.gl.delete_query(query);
            }
        }
    }
}

impl Debug for Context<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("opengl::Context").finish_non_exhaustive()
    }
}
