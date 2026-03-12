mod util;

use crate::*;
use alloc::vec::Vec;
use core::{
    cell::Cell,
    ffi::{CStr, c_void},
    time::Duration,
};
use glow::HasContext;
use util::*;

pub struct Backend {
    gl: glow::Context,
    features: Features,
}

pub struct Context<'a> {
    gl: &'a glow::Context,
    features: &'a Features,

    last_pipeline: Cell<Option<glow::Program>>,
    last_viewport: Cell<Option<TextureBounds>>,
    last_scissor: Cell<Option<Option<TextureBounds>>>,
    last_framebuffer: Cell<Option<Option<glow::Framebuffer>>>,
}

#[derive(Debug)]
pub struct Buffer {
    buffer: glow::Buffer,
    capacity: u32,
    is_dynamic: bool,
    role: BufferRole,
}

#[derive(Debug)]
pub struct Pipeline {
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
}

#[derive(Debug)]
pub struct Texture {
    texture: glow::Texture,
    width: u32,
    height: u32,
}

#[derive(Debug)]
pub struct Framebuffer {
    framebuffer: Option<glow::Framebuffer>,
    color_texture: Option<glow::Texture>,
    color_renderbuffer: Option<glow::Renderbuffer>,
    depth_texture: Option<glow::Texture>,
    depth_renderbuffer: Option<glow::Renderbuffer>,
}

#[derive(Debug)]
pub struct Profiler {
    // 0 - idle, 1 - started, 2 - waiting for result
    state: Cell<u8>,
    query: glow::Query,
}

impl Backend {
    /// # Safety
    ///
    /// The caller must ensure that the provided loader function correctly loads OpenGL function
    /// pointers and that the OpenGL context is properly initialized before calling this function.
    pub unsafe fn new(loader: &mut dyn FnMut(&CStr) -> *const c_void) -> Result<Self, Error> {
        unsafe {
            if !is_context_valid(loader) {
                return Err(Error::InvalidContext);
            }

            let gl = glow::Context::from_loader_function_cstr(|c| loader(c));
            let features = Features::from_context(&gl);

            Ok(Self { features, gl })
        }
    }

    /// # Safety
    ///
    /// The caller must ensure that the OpenGL context is current and valid for the duration of the
    /// returned `Context` instance.
    #[inline]
    pub unsafe fn begin(&self) -> Context<'_> {
        Context {
            gl: &self.gl,
            features: &self.features,

            last_pipeline: Cell::new(None),
            last_scissor: Cell::new(None),
            last_viewport: Cell::new(None),
            last_framebuffer: Cell::new(None),
        }
    }
}

impl Context<'_> {
    /// A framebuffer handle representing the screen, used for drawing to the screen and reading
    /// pixels from it.
    pub fn screen(&self) -> &Framebuffer {
        const SCREEN: Framebuffer = Framebuffer {
            framebuffer: None,
            color_texture: None,
            color_renderbuffer: None,
            depth_texture: None,
            depth_renderbuffer: None,
        };

        &SCREEN
    }
}

impl crate::Context for Context<'_> {
    type Buffer = Buffer;
    type Texture = Texture;
    type Pipeline = Pipeline;
    type Profiler = Profiler;
    type Framebuffer = Framebuffer;

    #[inline]
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            shader_format: ShaderFormat::Glsl,
            supports_profiler: self.features.query_time_elapsed,
            texture_size: self.features.max_texture_size,
            texture_bindings: self.features.max_texture_image_units,
            framebuffer_size: self.features.max_framebuffer_size,
            framebuffer_msaa: self.features.max_framebuffer_msaa,
            uniform_buffer_size: self.features.max_uniform_buffer_size,
            storage_buffer_size: self.features.max_storage_buffer_size,
            uniform_buffer_alignment: self.features.uniform_buffer_offset_alignment,
            storage_buffer_alignment: self.features.storage_buffer_offset_alignment,
            uniform_buffer_bindings: self.features.max_uniform_buffer_bindings,
            storage_buffer_bindings: self.features.max_storage_buffer_bindings,
        }
    }

    fn create_buffer(&self, layout: BufferLayout) -> Result<Self::Buffer, Error> {
        unsafe {
            if layout.capacity > self.features.max_buffer_size(layout.role) {
                return Err(Error::UnsupportedSize);
            }

            let buffer = self.gl.create_buffer().map_err(Error::Internal)?;

            self.gl
                .bind_buffer(buffer_target(layout.role), Some(buffer));

            self.gl.buffer_data_size(
                buffer_target(layout.role),
                layout.capacity as i32,
                if layout.dynamic {
                    glow::DYNAMIC_DRAW
                } else {
                    glow::STATIC_DRAW
                },
            );

            Ok(Buffer {
                buffer,
                capacity: layout.capacity,
                is_dynamic: layout.dynamic,
                role: layout.role,
            })
        }
    }

    fn create_texture(&self, layout: TextureLayout) -> Result<Self::Texture, Error> {
        unsafe {
            if layout.width.max(layout.height) > self.features.max_texture_size {
                return Err(Error::UnsupportedSize);
            }

            let (format, data_type, internal_format) = color_format(layout.format);

            let texture = self.gl.create_texture().map_err(Error::Internal)?;

            self.gl.bind_texture(glow::TEXTURE_2D, Some(texture));

            self.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                match layout.filter_min {
                    TextureFilter::Nearest => glow::NEAREST as i32,
                    TextureFilter::Linear => glow::LINEAR as i32,
                },
            );

            self.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                match layout.filter_mag {
                    TextureFilter::Nearest => glow::NEAREST as i32,
                    TextureFilter::Linear => glow::LINEAR as i32,
                },
            );

            self.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                match layout.wrap_x {
                    TextureWrap::Mirror => glow::MIRRORED_REPEAT as i32,
                    TextureWrap::Repeat => glow::REPEAT as i32,
                    TextureWrap::Clamp => glow::CLAMP_TO_EDGE as i32,
                },
            );

            self.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                match layout.wrap_y {
                    TextureWrap::Mirror => glow::MIRRORED_REPEAT as i32,
                    TextureWrap::Repeat => glow::REPEAT as i32,
                    TextureWrap::Clamp => glow::CLAMP_TO_EDGE as i32,
                },
            );

            self.gl.tex_image_2d(
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
                texture,
                width: layout.width,
                height: layout.height,
            })
        }
    }

    fn create_pipeline(&self, layout: PipelineLayout) -> Result<Self::Pipeline, Error> {
        unsafe {
            let shader = match layout.shader {
                Shader::Glsl(shader) => shader,
                _ => return Err(Error::UnsupportedFormat),
                // todo
            };

            let program =
                DisposeOnDrop::new(self.gl.create_program().map_err(Error::Internal)?, |obj| {
                    self.gl.delete_program(obj)
                });

            let vertex_array = DisposeOnDrop::new(
                self.gl.create_vertex_array().map_err(Error::Internal)?,
                |obj| self.gl.delete_vertex_array(obj),
            );

            let [vertex, fragment] = [false, true].map(|is_fragment| {
                let source = if is_fragment {
                    shader.fragment
                } else {
                    shader.vertex
                };

                let shader = DisposeOnDrop::new(
                    self.gl
                        .create_shader(if is_fragment {
                            glow::FRAGMENT_SHADER
                        } else {
                            glow::VERTEX_SHADER
                        })
                        .map_err(Error::Internal)?,
                    |obj| self.gl.delete_shader(obj),
                );

                self.gl.shader_source(*shader, source);
                self.gl.compile_shader(*shader);

                if !self.gl.get_shader_compile_status(*shader) {
                    let log = self.gl.get_shader_info_log(*shader);
                    return Err(Error::Compile(
                        if is_fragment {
                            CompileStage::Fragment
                        } else {
                            CompileStage::Vertex
                        },
                        log,
                    ));
                }

                self.gl.attach_shader(*program, *shader);
                Ok(shader)
            });

            let vertex = vertex?;
            let fragment = fragment?;

            self.gl.link_program(*program);
            if !self.gl.get_program_link_status(*program) {
                let log = self.gl.get_program_info_log(*program);
                return Err(Error::Compile(CompileStage::Linking, log));
            }

            self.gl.detach_shader(*program, *vertex);
            self.gl.detach_shader(*program, *fragment);

            let bindings =
                prepare_pipeline_bindings(self.gl, self.features, *program, shader.bindings)?;

            Ok(Pipeline {
                bindings,
                program: program.take(),
                vertex_array: vertex_array.take(),
                color_blend: layout.color_blend,
                depth_test: layout.depth_test,
                depth_write: layout.depth_write,
                stencil_ccw: layout.stencil_ccw,
                stencil_cw: layout.stencil_cw,
                cull_ccw: layout.cull_ccw,
                cull_cw: layout.cull_cw,
            })
        }
    }

    fn create_framebuffer(&self, layout: FramebufferLayout) -> Result<Self::Framebuffer, Error> {
        unsafe {
            let gl = &self.gl;

            if layout.width.max(layout.height) > self.features.max_framebuffer_size {
                return Err(Error::UnsupportedSize);
            }

            if layout.msaa_samples > self.features.max_framebuffer_msaa {
                return Err(Error::UnsupportedSampleCount);
            }

            let framebuffer =
                DisposeOnDrop::new(gl.create_framebuffer().map_err(Error::Internal)?, |obj| {
                    gl.delete_framebuffer(obj)
                });

            self.gl
                .bind_framebuffer(glow::FRAMEBUFFER, Some(*framebuffer));

            // this is a cursed try block
            let (color_texture, color_renderbuffer) = if let Some(format) = layout.color {
                let (format, data_type, internal_format) = color_format(format);

                if layout.is_color_bindable {
                    let texture = DisposeOnDrop::new(
                        self.gl.create_texture().map_err(Error::Internal)?,
                        |obj| self.gl.delete_texture(obj),
                    );

                    self.gl.bind_texture(glow::TEXTURE_2D, Some(*texture));

                    if layout.msaa_samples > 0 {
                        gl.tex_image_2d_multisample(
                            glow::TEXTURE_2D,
                            layout.msaa_samples as i32,
                            internal_format as i32,
                            layout.width as i32,
                            layout.height as i32,
                            false,
                        );
                    } else {
                        gl.tex_image_2d(
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

                    gl.tex_parameter_i32(
                        glow::TEXTURE_2D,
                        glow::TEXTURE_MIN_FILTER,
                        glow::NEAREST as i32,
                    );

                    gl.tex_parameter_i32(
                        glow::TEXTURE_2D,
                        glow::TEXTURE_MAG_FILTER,
                        glow::NEAREST as i32,
                    );

                    gl.framebuffer_texture_2d(
                        glow::FRAMEBUFFER,
                        glow::COLOR_ATTACHMENT0,
                        glow::TEXTURE_2D,
                        Some(*texture),
                        0,
                    );

                    (Some(texture), None)
                } else {
                    let buffer = DisposeOnDrop::new(
                        self.gl.create_renderbuffer().map_err(Error::Internal)?,
                        |obj| self.gl.delete_renderbuffer(obj),
                    );

                    self.gl.bind_renderbuffer(glow::RENDERBUFFER, Some(*buffer));

                    if layout.msaa_samples > 0 {
                        self.gl.renderbuffer_storage_multisample(
                            glow::RENDERBUFFER,
                            layout.msaa_samples as i32,
                            internal_format,
                            layout.width as i32,
                            layout.height as i32,
                        );
                    } else {
                        self.gl.renderbuffer_storage(
                            glow::RENDERBUFFER,
                            internal_format,
                            layout.width as i32,
                            layout.height as i32,
                        );
                    }

                    self.gl.framebuffer_renderbuffer(
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

            let (depth_texture, depth_renderbuffer) = if let Some(format) = layout.depth {
                let (format, attachment) = depth_format(format);

                if layout.is_depth_bindable {
                    let texture = DisposeOnDrop::new(
                        self.gl.create_texture().map_err(Error::Internal)?,
                        |obj| self.gl.delete_texture(obj),
                    );

                    self.gl.bind_texture(glow::TEXTURE_2D, Some(*texture));

                    if layout.msaa_samples > 0 {
                        gl.tex_image_2d_multisample(
                            glow::TEXTURE_2D,
                            layout.msaa_samples as i32,
                            format as i32,
                            layout.width as i32,
                            layout.height as i32,
                            false,
                        );
                    } else {
                        gl.tex_image_2d(
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

                    gl.framebuffer_texture_2d(
                        glow::FRAMEBUFFER,
                        attachment,
                        glow::TEXTURE_2D,
                        Some(*texture),
                        0,
                    );

                    (Some(texture), None)
                } else {
                    let buffer = DisposeOnDrop::new(
                        self.gl.create_renderbuffer().map_err(Error::Internal)?,
                        |obj| self.gl.delete_renderbuffer(obj),
                    );

                    self.gl.bind_renderbuffer(glow::RENDERBUFFER, Some(*buffer));

                    if layout.msaa_samples > 0 {
                        self.gl.renderbuffer_storage_multisample(
                            glow::RENDERBUFFER,
                            layout.msaa_samples as i32,
                            format,
                            layout.width as i32,
                            layout.height as i32,
                        );
                    } else {
                        self.gl.renderbuffer_storage(
                            glow::RENDERBUFFER,
                            format,
                            layout.width as i32,
                            layout.height as i32,
                        );
                    }

                    self.gl.framebuffer_renderbuffer(
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
                framebuffer: Some(framebuffer.take()),
                color_texture: color_texture.map(|t| t.take()),
                depth_texture: depth_texture.map(|t| t.take()),
                color_renderbuffer: color_renderbuffer.map(|b| b.take()),
                depth_renderbuffer: depth_renderbuffer.map(|b| b.take()),
            })
        }
    }

    fn create_profiler(&self) -> Result<Self::Profiler, Error> {
        unsafe {
            if !self.features.query_time_elapsed {
                return Err(Error::UnsupportedFeature);
            }

            Ok(Profiler {
                query: self.gl.create_query().map_err(Error::Internal)?,
                state: Cell::new(0),
            })
        }
    }

    fn delete_buffer(&self, buffer: Self::Buffer) {
        unsafe { self.gl.delete_buffer(buffer.buffer) };
    }

    fn delete_texture(&self, texture: Self::Texture) {
        unsafe { self.gl.delete_texture(texture.texture) };
    }

    fn delete_pipeline(&self, pipeline: Self::Pipeline) {
        unsafe {
            self.gl.delete_program(pipeline.program);
            self.gl.delete_vertex_array(pipeline.vertex_array);
        };
    }

    fn delete_framebuffer(&self, framebuffer: Self::Framebuffer) {
        unsafe {
            if let Some(framebuffer) = framebuffer.framebuffer {
                self.gl.delete_framebuffer(framebuffer);
            }

            if let Some(texture) = framebuffer.color_texture {
                self.gl.delete_texture(texture);
            }

            if let Some(texture) = framebuffer.depth_texture {
                self.gl.delete_texture(texture);
            }

            if let Some(renderbuffer) = framebuffer.color_renderbuffer {
                self.gl.delete_renderbuffer(renderbuffer);
            }

            if let Some(renderbuffer) = framebuffer.depth_renderbuffer {
                self.gl.delete_renderbuffer(renderbuffer);
            }
        };
    }

    fn delete_profiler(&self, profiler: Self::Profiler) {
        unsafe { self.gl.delete_query(profiler.query) };
    }

    fn invalidate_buffer(
        &self,
        buffer: &Self::Buffer,
        offset: u32,
        size: u32,
    ) -> Result<(), Error> {
        unsafe {
            if offset.saturating_add(size) > buffer.capacity
                || !offset.is_multiple_of(self.features.buffer_alignment(buffer.role))
            {
                return Err(Error::InvalidBounds);
            }

            self.gl
                .bind_buffer(buffer_target(buffer.role), Some(buffer.buffer));

            if offset == 0 && size == buffer.capacity {
                self.gl.buffer_data_size(
                    buffer_target(buffer.role),
                    buffer.capacity as i32,
                    if buffer.is_dynamic {
                        glow::DYNAMIC_DRAW
                    } else {
                        glow::STATIC_DRAW
                    },
                );
            } else if self.features.invalidate_buffer_sub_data {
                self.gl.invalidate_buffer_sub_data(
                    buffer_target(buffer.role),
                    offset as i32,
                    size as i32,
                );
            }

            Ok(())
        }
    }

    fn copy_buffer(
        &self,
        buffer: &Self::Buffer,
        source_buffer: &Self::Buffer,
        offset: u32,
        source_offset: u32,
        size: u32,
    ) -> Result<(), Error> {
        unsafe {
            if !offset.is_multiple_of(self.features.buffer_alignment(buffer.role))
                || !source_offset.is_multiple_of(self.features.buffer_alignment(source_buffer.role))
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

            self.gl
                .bind_buffer(glow::COPY_READ_BUFFER, Some(source_buffer.buffer));
            self.gl
                .bind_buffer(glow::COPY_WRITE_BUFFER, Some(buffer.buffer));

            self.gl.copy_buffer_sub_data(
                glow::COPY_READ_BUFFER,
                glow::COPY_WRITE_BUFFER,
                source_offset as i32,
                offset as i32,
                size as i32,
            );

            Ok(())
        }
    }

    fn upload_buffer(&self, buffer: &Self::Buffer, offset: u32, data: &[u8]) -> Result<(), Error> {
        unsafe {
            if offset.saturating_add(data.len() as u32) > buffer.capacity
                || !offset.is_multiple_of(self.features.buffer_alignment(buffer.role))
            {
                return Err(Error::InvalidBounds);
            }

            self.gl
                .bind_buffer(buffer_target(buffer.role), Some(buffer.buffer));

            if offset == 0 && data.len() as u32 == buffer.capacity {
                self.gl.buffer_data_u8_slice(
                    buffer_target(buffer.role),
                    data,
                    if buffer.is_dynamic {
                        glow::DYNAMIC_DRAW
                    } else {
                        glow::STATIC_DRAW
                    },
                );
            } else {
                self.gl
                    .buffer_sub_data_u8_slice(buffer_target(buffer.role), offset as i32, data);
            }

            Ok(())
        }
    }

    fn upload_texture(
        &self,
        texture: &Self::Texture,
        bounds: TextureBounds,
        format: TextureFormat,
        data: &[u8],
    ) -> Result<(), Error> {
        unsafe {
            if bounds.x.saturating_add(bounds.width) > texture.width
                || bounds.y.saturating_add(bounds.height) > texture.height
            {
                return Err(Error::InvalidBounds);
            }

            if data.len() != (bounds.width * bounds.height * format.bytes_per_pixel()) as usize {
                return Err(Error::InvalidData);
            }

            let (format, data_type, _) = color_format(format);

            self.gl
                .bind_texture(glow::TEXTURE_2D, Some(texture.texture));

            self.gl.tex_sub_image_2d(
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
        }
    }

    fn read_framebuffer(
        &self,
        target: &Self::Framebuffer,
        bounds: TextureBounds,
        format: TextureFormat,
        data: &mut [u8],
    ) -> Result<(), Error> {
        unsafe {
            if bounds.x.saturating_add(bounds.width) > self.features.max_framebuffer_size
                || bounds.y.saturating_add(bounds.height) > self.features.max_framebuffer_size
            {
                return Err(Error::InvalidBounds);
            }

            if data.len() != (bounds.width * bounds.height * format.bytes_per_pixel()) as usize {
                return Err(Error::InvalidData);
            }

            let (format, data_type, _) = color_format(format);

            self.gl
                .bind_framebuffer(glow::READ_FRAMEBUFFER, target.framebuffer);

            self.gl.read_pixels(
                bounds.x as i32,
                bounds.y as i32,
                bounds.width as i32,
                bounds.height as i32,
                format,
                data_type,
                glow::PixelPackData::Slice(Some(data)),
            );

            Ok(())
        }
    }

    fn begin_profiler(&self, profiler: &Self::Profiler) {
        unsafe {
            if profiler.state.get() == 0 {
                self.gl.begin_query(glow::TIME_ELAPSED, profiler.query);
                profiler.state.set(1);
            }
        }
    }

    fn end_profiler(&self, profiler: &Self::Profiler) -> Option<Duration> {
        unsafe {
            if profiler.state.get() == 1 {
                self.gl.end_query(glow::TIME_ELAPSED);
                profiler.state.set(2);
            }

            if profiler.state.get() == 2 {
                let available = self
                    .gl
                    .get_query_parameter_u32(profiler.query, glow::QUERY_RESULT_AVAILABLE);

                if available != 0 {
                    let result = self
                        .gl
                        .get_query_parameter_u32(profiler.query, glow::QUERY_RESULT);

                    profiler.state.set(0);
                    return Some(Duration::from_nanos(result as u64));
                }
            }
        }

        None
    }

    fn draw(&self, draw: DrawRequest<Self>) -> Result<(), Error> {
        unsafe {
            if self.last_framebuffer.replace(Some(draw.target.framebuffer))
                != Some(draw.target.framebuffer)
            {
                self.gl
                    .bind_framebuffer(glow::FRAMEBUFFER, draw.target.framebuffer);
            }

            if self.last_pipeline.replace(Some(draw.pipeline.program))
                != Some(draw.pipeline.program)
            {
                util::apply_pipeline(self.gl, draw.pipeline);
            }

            if self.last_viewport.replace(Some(draw.viewport)) != Some(draw.viewport) {
                self.gl.viewport(
                    draw.viewport.x as i32,
                    draw.viewport.y as i32,
                    draw.viewport.width as i32,
                    draw.viewport.height as i32,
                );
            }

            if self.last_scissor.replace(Some(draw.scissor)) != Some(draw.scissor) {
                match draw.scissor {
                    None => self.gl.disable(glow::SCISSOR_TEST),
                    Some(scissor) => {
                        self.gl.enable(glow::SCISSOR_TEST);
                        self.gl.scissor(
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
                            Some(BindingData::Buffer {
                                buffer,
                                offset,
                                size,
                            }) => (buffer, offset, size),
                            _ => return Err(Error::InvalidBinding(i)),
                        };

                        if buffer.role != *role || data_size < size {
                            return Err(Error::InvalidBinding(i));
                        }

                        self.gl
                            .bind_buffer(buffer_target(*role), Some(buffer.buffer));
                        self.gl.bind_buffer_range(
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
                            Some(BindingData::FramebufferColor { framebuffer }) => {
                                match framebuffer.color_texture {
                                    Some(texture) => texture,
                                    None => return Err(Error::InvalidBinding(i)),
                                }
                            }
                            _ => {
                                return Err(Error::InvalidBinding(i));
                            }
                        };

                        self.gl.active_texture(glow::TEXTURE0 + *index);
                        self.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                    }
                }
            }

            self.gl
                .draw_arrays(glow::TRIANGLES, 0, draw.triangles as i32 * 3);

            Ok(())
        }
    }
}
