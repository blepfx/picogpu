use picogpu::*;

fn main() {
    picoview::WindowBuilder::new(|window| {
        let opengl = window.opengl().unwrap();
        let context = unsafe {
            assert!(opengl.make_current(true));
            opengl::Backend::new(&mut |x| opengl.get_proc_address(x) as *const _).unwrap()
        };

        let buffer = unsafe {
            context
                .begin()
                .create_buffer(BufferLayout {
                    capacity: 32,
                    dynamic: true,
                    role: BufferRole::Uniform,
                })
                .unwrap()
        };

        let texture = unsafe {
            let texture = context
                .begin()
                .create_texture(TextureLayout {
                    width: 8,
                    height: 8,
                    format: TextureFormat::RGBA8,
                    filter_mag: TextureFilter::Linear,
                    filter_min: TextureFilter::Linear,
                    wrap_x: TextureWrap::Repeat,
                    wrap_y: TextureWrap::Repeat,
                })
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

            context
                .begin()
                .upload_texture(
                    &texture,
                    TextureBounds {
                        x: 0,
                        y: 0,
                        width: 8,
                        height: 8,
                    },
                    TextureFormat::RGBA8,
                    &data,
                )
                .unwrap();

            texture
        };

        let framebuffer  = unsafe {
            context
                .begin()
                .create_framebuffer(FramebufferLayout {
                    color: Some(TextureFormat::RGBA8),
                    depth: None,
                    msaa_samples: 0,
                    is_color_bindable: false,
                    is_depth_bindable: false,
                    is_persistent: false,
                    width: 200,
                    height: 200,
                })
                .unwrap()
        };

        let profiler_1 = unsafe {
            context.begin().create_profiler().unwrap()
        };

        unsafe {
            let caps = context.begin().capabilities();
            dbg!(caps);
        }

        let pipeline = unsafe {
            context
                .begin()
                .create_pipeline(PipelineLayout {
                    shader: ShaderGlsl {
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

                            void main() {
                                vec4 texture = texture2D(Texture, gl_FragCoord.xy / vec2(8.0, 8.0) + offset);
                                gl_FragColor = texture * color;
                            }
                        "#,
                        bindings: &["Uniforms", "Texture"],
                    }
                    .into(),
                    color_format: TextureFormat::RGBA8,
                    color_blend: BlendMode::ALPHA,
                    depth_test: CompareFn::Always,
                    depth_write: false,
                    stencil_ccw: StencilFace::default(),
                    stencil_cw: StencilFace::default(),
                    cull_ccw: false,
                    cull_cw: false,
                })
                .unwrap()
        };

        let mut frames = 0;

        Box::new(move |event| match event {
            picoview::Event::WindowFrame => {
                frames += 1;

                let context = unsafe {
                    assert!(opengl.make_current(true));
                    context.begin()
                };
                
                context.begin_profiler(&profiler_1);
               
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
                    .draw(DrawRequest {
                        target: &framebuffer,
                        pipeline: &pipeline,

                        color_op: MemoryOp {
                            load: LoadOp::Clear([0.0, 0.0, 0.0, 1.0]),
                            store: StoreOp::Store,
                        },
                        depth_op: MemoryOp {
                            load: LoadOp::Clear(1.0),
                            store: StoreOp::Store,
                        },
                        stencil_op: MemoryOp {
                            load: LoadOp::Clear(0),
                            store: StoreOp::Store,
                        },

                        viewport: TextureBounds {
                            x: 0,
                            y: 0,
                            width: 200,
                            height: 200,
                        },

                        scissor: None,
                        triangles: 2,

                        bindings: &[
                            BindingData::Buffer {
                                buffer: &buffer,
                                offset: 0,
                                size: 32,
                            },
                            BindingData::Texture { texture: &texture },
                        ],
                    })
                    .unwrap();
                }

                dbg!(context.end_profiler(&profiler_1));

                unsafe {
                    opengl.swap_buffers();
                }
            }

            picoview::Event::WindowClose => {
                window.close();
            }
            _ => {}
        })
    })
    .with_opengl(picoview::GlConfig {
        version: picoview::GlVersion::Compat(1, 1),
        ..Default::default()
    })
    .open_blocking()
    .unwrap();
}
