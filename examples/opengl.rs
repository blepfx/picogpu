use picogpu::opengl::{Surface, SurfaceError};
use picogpu::*;
use std::time::Duration;

struct GlSurfaceAdapter<'a>(picoview::GlContext<'a>);

unsafe impl Surface for GlSurfaceAdapter<'_> {
    fn swap_buffers(&self) -> Result<(), SurfaceError> {
        self.0.swap_buffers().map_err(|_| SurfaceError::InvalidContext)
    }

    fn make_current(&self) -> Result<(), SurfaceError> {
        self.0.make_current(true).map_err(|_| SurfaceError::InvalidContext)
    }

    fn get_proc_address(&self, name: &core::ffi::CStr) -> *const core::ffi::c_void {
        self.0.get_proc_address(name)
    }
}

fn main() {
    picoview::WindowBuilder::new(|window| {
        let context = opengl::Context::new(GlSurfaceAdapter(window.opengl().unwrap())).unwrap();

        context.attach_debug_callback(|kind, message| {
            println!("[{:?}] {}", kind, message);
        });

        let buffer = context
            .create_buffer(BufferLayout::new(BufferRole::Uniform, 32).with_can_upload())
            .unwrap();

        let texture = {
            let texture = context
                .create_texture(
                    TextureLayout::new(8, 8, TextureFormat::RGBA8)
                        .with_filter(TextureFilter::Linear, TextureFilter::Linear)
                        .with_wrap(TextureWrap::Repeat, TextureWrap::Repeat),
                )
                .unwrap();

            let mut data = vec![255u8; 8 * 8 * 4];
            for i in 0..8 {
                for j in 0..8 {
                    let offset = (i * 8 + j) * 4;
                    let pixel = (i / 4 + j / 4) % 2 == 0;
                    data[offset] = if pixel { 255 } else { 0 };
                    data[offset + 1] = if pixel { 255 } else { 0 };
                    data[offset + 2] = 0;
                    data[offset + 3] = 255;
                }
            }

            let buffer = context
                .create_buffer(BufferLayout::new(BufferRole::Staging, data.len() as u64).with_can_upload())
                .unwrap();

            context.upload_buffer(&buffer, 0, &data).unwrap();
            context
                .copy_buffer_to_texture(
                    &texture,
                    TextureBounds {
                        x: 0,
                        y: 0,
                        width: 8,
                        height: 8,
                    },
                    &buffer,
                    0,
                )
                .unwrap();

            texture
        };

        {
            let caps = context.capabilities();
            dbg!(caps);
        }

        let pipeline = {
            let shader = ShaderGlsl {
                vertex: r#"
                            #version 330

                            void main() {
                                vec2 position = vec2(0.0, 0.0);
                                if (gl_VertexID == 0 || gl_VertexID == 3) {
                                    position = vec2(-0.25, -0.25);
                                } else if (gl_VertexID == 1) {
                                    position = vec2(0.5, -0.5);
                                } else if (gl_VertexID == 2 || gl_VertexID == 4) {
                                    position = vec2(0.5, 0.5);
                                } else if (gl_VertexID == 5) {
                                    position = vec2(-0.5, 0.5);
                                }

                                gl_Position = vec4(position, 0.0, 1.0);
                            }
                        "#,
                fragment: r#"
                            #version 330

                            uniform sampler2D Texture;
                            layout(std140) uniform Uniforms {
                                vec4 color;
                                vec2 offset;
                            };

                            out vec4 fragColor;

                            void main() {
                                vec4 tex = texture(Texture, gl_FragCoord.xy / vec2(8.0, 8.0) + offset);
                                fragColor = tex * color;
                            }
                        "#,
                bindings: &["Uniforms", "Texture"],
            };

            context
                .create_pipeline(
                    PipelineLayout::new(shader.into())
                        .with_color_outputs(&[TextureFormat::RGBA8])
                        .with_color_blend(BlendMode::ALPHA),
                )
                .unwrap()
        };

        let mut frames = 0;
        let mut width = 200;
        let mut height = 200;

        let mut current_query = None;

        Box::new(move |event| match event {
            picoview::Event::WindowResize { size } => {
                width = size.width;
                height = size.height;
            }

            picoview::Event::WindowFrame => {
                frames += 1;

                if let Some(query) = &current_query
                    && let Some(result) = context.read_query(query).unwrap()
                {
                    println!(
                        "Time elapsed: {:.2}ms",
                        Duration::from_nanos(result).as_secs_f64() * 1000.0
                    );
                    current_query = None;
                }

                let query = current_query
                    .is_none()
                    .then(|| context.begin_query(QueryType::Elapsed).unwrap());

                context
                    .clear(
                        ClearRequest::new(&context.screen())
                            .with_color([0.1, 0.1, 0.1, 1.0])
                            .with_depth(1.0),
                    )
                    .unwrap();

                for i in 0..100 {
                    {
                        let x = 0.5 + 5.0 * (frames as f32 * 0.01 + i as f32 * 0.0001).cos();
                        let y = 0.5 + 5.0 * (frames as f32 * 0.01 + i as f32 * 0.0001).sin();

                        let mut data = [0; 24];
                        data[0..4].copy_from_slice(&f32::to_ne_bytes(0.5));
                        data[4..8].copy_from_slice(&f32::to_ne_bytes(0.2));
                        data[8..12].copy_from_slice(&f32::to_ne_bytes(0.3));
                        data[12..16].copy_from_slice(&f32::to_ne_bytes(0.01));
                        data[16..20].copy_from_slice(&f32::to_ne_bytes(x));
                        data[20..24].copy_from_slice(&f32::to_ne_bytes(y));
                        context.upload_buffer(&buffer, 0, &data).unwrap();
                    }

                    context
                        .draw(
                            DrawRequest::new(&context.screen(), &pipeline)
                                .with_vertices(2 * 3)
                                .with_viewport(TextureBounds {
                                    x: 0,
                                    y: 0,
                                    width,
                                    height,
                                })
                                .with_bindings(&[
                                    BindingData::Buffer {
                                        buffer: &buffer,
                                        offset: 0,
                                        size: 32,
                                    },
                                    BindingData::Texture { texture: &texture },
                                ]),
                        )
                        .unwrap();
                }

                if let Some(query) = query {
                    context.end_query(&query).unwrap();
                    current_query = Some(query);
                }

                let fence = context.present().unwrap();

                let is_signalled = context.wait_fence(&fence, Duration::ZERO).unwrap();
                dbg!(is_signalled);
            }

            picoview::Event::WindowClose => {
                window.close();
            }
            _ => {}
        })
    })
    .with_resizable((0, 0), (1000, 1000))
    .with_opengl(picoview::GlConfig {
        version: picoview::GlVersion::Core(3, 3),
        msaa_count: 4,
        ..Default::default()
    })
    .open_blocking()
    .unwrap();
}
