use core::{fmt::Debug, time::Duration};

use alloc::string::String;
pub use buffer::*;
pub use draw::*;
pub use framebuffer::*;
pub use pipeline::*;
pub use shader::*;
pub use texture::*;

/// The main interface for interacting with a backend, used for creating and managing resources
/// (buffers, textures, pipelines, framebuffers, profilers) and issuing draw calls.
pub trait Context: Clone {
    /// A buffer object, used for storing arbitrary data on the GPU
    type Buffer: Debug;
    /// A texture object, used for storing image data on the GPU
    type Texture: Debug;
    /// A pipeline object, represents a shader program and associated draw state (blend mode,
    /// stencil, depth test, etc.)
    type Pipeline: Debug;
    /// A profiler object, used for measuring GPU execution time of draw calls.
    type Profiler: Debug;
    /// A framebuffer object, used as a texture you can draw to and sample from.
    type Framebuffer: Debug;

    /// The capabilities of the GPU, used for determining what features are supported and the limits
    /// of various resources (max texture size, max buffer size, etc.)
    fn capabilities(&self) -> Capabilities;

    /// Creates a new buffer with the given layout, and returns a handle to it.
    ///
    /// # Errors
    /// - [`Error::UnsupportedSize`] if the requested buffer size is larger than what is supported.
    ///   Note that the alignment requirement does not apply here (a backend could allocate more
    ///   memory than requested if needed).
    /// - [`Error::Internal] if an internal error occurs while creating the buffer.
    fn create_buffer(&self, layout: BufferLayout) -> Result<Self::Buffer, Error>;

    /// Creates a new texture with the given layout, and returns a handle to it.
    ///
    /// Please note that the backend can "promote" the pixel format if needed (for example, if RGB8
    /// is not supported, it can be promoted to RGBA8), so the returned texture may be of a larger
    /// size than requested.
    ///
    /// # Errors
    /// - [`Error::UnsupportedSize`] if the requested texture dimensions are larger than what is
    ///   supported.
    /// - [`Error::UnsupportedFormat`] if the requested texture format is not supported.
    /// - [`Error::Internal`] if an internal error occurs while creating the texture.
    fn create_texture(&self, layout: TextureLayout) -> Result<Self::Texture, Error>;

    /// Creates a new pipeline with the given layout, and returns a handle to it.
    ///
    /// # Errors
    /// - [`Error::UnsupportedFormat`] if the shader format used in the pipeline is not supported.
    /// - [`Error::UnsupportedBinding`] if the pipeline layout requires more bindings than what is
    ///   supported.
    /// - [`Error::Compile`] if an error occurs while compiling the shader for the pipeline.
    /// - [`Error::Internal`] if an internal error occurs while creating the pipeline.
    fn create_pipeline(&self, layout: PipelineLayout) -> Result<Self::Pipeline, Error>;

    /// Creates a new framebuffer with the given layout, and returns a handle to it.
    ///
    /// Please note that the backend can "promote" the framebuffer layout if needed (for example, if
    /// RGB8 color format is not supported, it can be promoted to RGBA8), so the returned
    /// framebuffer may have a different layout than requested. The returned framebuffer will
    /// always have at least the features specified in the requested layout, but may have additional
    /// features (e.g. more attachments, or larger size).
    ///
    /// # Errors
    /// - [`Error::UnsupportedSize`] if the requested framebuffer dimensions are larger than what is
    ///   supported.
    /// - [`Error::UnsupportedFormat`] if the requested color/depth/stencil format is not supported.
    /// - [`Error::UnsupportedSampleCount`] if the requested MSAA sample count is not supported.
    /// - [`Error::Internal`] if an internal error occurs while creating the framebuffer.
    fn create_framebuffer(&self, layout: FramebufferLayout) -> Result<Self::Framebuffer, Error>;

    /// Creates a new profiler, and returns a handle to it.
    ///
    /// # Errors
    /// - [`Error::UnsupportedFeature`] if the GPU does not support profiling.
    /// - [`Error::Internal`] if an internal error occurs while creating the profiler.
    fn create_profiler(&self) -> Result<Self::Profiler, Error>;

    /// Uploads data to a texture, replacing the contents of the texture at the given bounds.
    /// The data must match the layout specified by the (width, height, format) triple.
    ///
    /// # Errors
    /// - [`Error::InvalidResource`] if the texture does not belong to this context.
    /// - [`Error::InvalidBounds`] if the bounds exceed the texture dimensions.
    /// - [`Error::InvalidData`] if the data size does not match the expected size for the given
    ///   bounds and format.
    /// - [`Error::Internal`] if an internal error occurs while uploading the data.
    fn upload_texture(
        &self,
        texture: &Self::Texture,
        bounds: TextureBounds,
        format: TextureFormat,
        data: &[u8],
    ) -> Result<(), Error>;

    /// Uploads data to a buffer, replacing the contents of the buffer at the given offset.
    ///
    /// # Errors
    /// - [`Error::InvalidResource`] if the buffer does not belong to this context.
    /// - [`Error::InvalidBounds`] if the offset and data size exceed the buffer capacity, or if the
    ///   offset does not match alignment requirements for the buffer layout.
    /// - [`Error::Internal`] if an internal error occurs while uploading the data.
    fn upload_buffer(&self, buffer: &Self::Buffer, offset: u64, data: &[u8]) -> Result<(), Error>;

    /// Copies data from one buffer to another, replacing the contents of the destination buffer at
    /// the given offset. The source and destination regions must not overlap if the source and
    /// destination buffers are the same.
    ///
    /// # Errors
    /// - [`Error::InvalidResource`] if either buffer does not belong to this context.
    /// - [`Error::InvalidBounds`] if the source or destination offset and size exceed the
    ///   respective buffer sizes, if the source and destination regions overlap when the source and
    ///   destination buffers are the same, or if the offset does not match alignment requirements
    ///   for the buffer layout.
    /// - [`Error::Internal`] if an internal error occurs while copying the data.
    fn copy_buffer(
        &self,
        dst_buffer: &Self::Buffer,
        src_buffer: &Self::Buffer,
        dst_offset: u64,
        src_offset: u64,
        size: u64,
    ) -> Result<(), Error>;

    /// Invalidates a region of a buffer, indicating that the contents of that region are no longer
    /// needed and can be discarded by the GPU. This is a hint to the backend that can help with
    /// avoiding unnecessary synchronization for future drawcalls.
    ///
    /// # Errors
    /// - [`Error::InvalidResource`] if the buffer does not belong to this context.
    /// - [`Error::InvalidBounds`] if the offset and size exceed the buffer capacity, or if the
    ///   offset does not match alignment requirements for the buffer layout.
    /// - [`Error::Internal`] if an internal error occurs while invalidating the buffer.
    fn invalidate_buffer(&self, buffer: &Self::Buffer, offset: u64, size: u64) -> Result<(), Error>;

    /// Reads data from a framebuffer, copying the contents of the specified bounds into the
    /// provided data buffer. This is a slow operation, and should be avoided if possible.
    /// Common usecases include readback for screenshots and draw tests.
    ///
    /// # Errors
    /// - [`Error::InvalidResource`] if the framebuffer does not belong to this context.
    /// - [`Error::InvalidBounds`] if the bounds exceed the framebuffer dimensions.
    /// - [`Error::InvalidData`] if the data buffer size does not match the expected size for the
    ///   given bounds and format.
    /// - [`Error::Internal`] if an internal error occurs while reading the data.
    fn read_framebuffer(
        &self,
        target: &Self::Framebuffer,
        bounds: TextureBounds,
        format: TextureFormat,
        data: &mut [u8],
    ) -> Result<(), Error>;

    /// Begins a profiling section for the given profiler, which will measure the GPU execution time
    /// of subsequent draw calls until [`Context::end_profiler`] is called with the same profiler.
    fn begin_profiler(&self, profiler: &Self::Profiler) -> Result<(), Error>;

    /// Ends a profiling section for the given profiler, returning the measured GPU execution time
    /// of the draw calls that were executed since [`Context::begin_profiler`] was called on the
    /// profiler.
    ///
    /// Profiling is asynchronous, the returned execution time may not be available immediately,
    /// and can be `None` if the GPU has not finished executing the draw calls yet. Once the
    /// execution time is available, it will be returned by this method on subsequent calls; the
    /// profiler will do nothing until a result is available
    fn end_profiler(&self, profiler: &Self::Profiler) -> Result<Option<Duration>, Error>;

    /// Clears a framebuffer with the given clear parameters, which can specify clearing the color,
    /// depth, and stencil buffers in a specified region (or the whole framebuffer if the scissor is
    /// None).
    ///
    /// # Errors
    /// - [`Error::InvalidResource`] if the framebuffer does not belong to this context.
    /// - [`Error::Internal`] if an internal error occurs while issuing the clear command.
    fn clear(&self, clear: ClearRequest<Self>) -> Result<(), Error>;

    /// Issue a draw call with the given target, pipeline, bindings, and other draw parameters.
    ///
    /// # Errors
    /// - [`Error::InvalidResource`] if the framebuffer, pipeline, or any resources used in the
    ///   bindings do not belong to this context.
    /// - [`Error::InvalidBinding`] if the provided bindings do not match the bindings declared when
    ///   the pipeline was created (e.g. wrong number of bindings, or wrong types of bindings).
    /// - [`Error::InvalidFramebuffer`] if the target framebuffer is also used as a binding resource
    ///   at the same time.
    /// - [`Error::Internal`] if an internal error occurs while issuing the draw call.
    fn draw(&self, draw: DrawRequest<Self>) -> Result<(), Error>;

    /// Submits all pending commands to the GPU for execution. This should be called at the end of
    /// each frame.
    ///
    /// # Errors
    /// - [`Error::Internal`] if an internal error occurs while submitting the commands.
    fn submit(&self) -> Result<(), Error>;
}

/// The capabilities of the GPU, used for determining what features are supported and the limits of
/// various resources (max texture size, max buffer size, etc.)
#[derive(Debug, Clone, Copy)]
pub struct Capabilities {
    /// The shader format that is used by the backend
    pub shader_format: ShaderFormat,
    /// Whether the backend supports creating and using [`Context::Profiler`] objects.
    pub supports_profiler: bool,

    /// Maximum supported width/height of a framebuffer
    pub framebuffer_size: u32,
    /// Maximum supported MSAA sample count for framebuffers, 0 if MSAA is not supported.
    pub framebuffer_msaa: u32,

    /// Maximum supported width/height of a texture.
    pub texture_size: u32,
    /// Maximum supported number of texture/framebuffer bindings per drawcall.
    pub texture_bindings: u32,

    /// Maximum supported size of a buffer with role [`BufferRole::Uniform`], in bytes.
    pub uniform_buffer_size: u64,
    /// Alignment requirement for uniform buffer offsets, in bytes.
    pub uniform_buffer_alignment: u32,
    /// Maximum supported number of uniform buffer bindings per drawcall.
    pub uniform_buffer_bindings: u32,

    /// Maximum supported size of a buffer with role [`BufferRole::Storage`], in bytes.
    pub storage_buffer_size: u32,
    /// Alignment requirement for storage buffer offsets, in bytes.
    pub storage_buffer_alignment: u32,
    /// Maximum supported number of storage buffer bindings per drawcall.
    pub storage_buffer_bindings: u32,
}

/// A generic error type for all possible operations in the context.
///
/// This is a catch-all error type. While it is not often desirable to have a catch-all error type,
/// it simplifies maintenance and API by a lot, and you should not be doing exhaustive matching on
/// it anyway, since it is marked as non-exhaustive and can have new variants added in the future
/// without a major version bump.
///
/// Please refer to documentation for individual methods to learn more about what errors they can
/// return and how to handle them.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Error {
    /// The requested size (buffer size, texture dimensions, framebuffer dimensions) is larger than
    /// what is supported
    UnsupportedSize,
    /// The requested format (texture format, shader format, depth/stencil format) is not supported
    /// by the GPU.
    UnsupportedFormat,
    /// The requested sample count (MSAA) is not supported by the GPU.
    UnsupportedSampleCount,
    /// The requested feature is not supported by the GPU
    UnsupportedFeature,
    /// The pipeline binding requested is not supported by the GPU (e.g. more texture bindings than
    /// supported)
    UnsupportedBinding(usize),

    /// Trying to update a texture/buffer for a region that is out of bounds for a resource,
    /// or the bounds overlap when copying from/to the same buffer.
    InvalidBounds,
    /// Trying to use a resource that does not belong to this context
    InvalidResource,
    /// The data provided for a resource update does not match the requested specification
    InvalidData,
    /// The binding provided does not match the pipeline description
    InvalidBinding(usize),
    /// Attempt to create a context with an invalid backend state (i.e. a non-current or
    /// non-existing OpenGL context)
    InvalidContext,
    /// Attempt to draw to a framebuffer that is also used as a binding resource at the same time.
    InvalidFramebuffer,

    /// An error occurred during fragment shader compilation.
    Compile(CompileStage, String),

    /// An internal error has occurred, this is a catch-all for any error that does not fit into the
    /// other categories, and is not the fault of the user (e.g. a driver bug, or an unexpected
    /// failure in the graphics API)
    Internal(String),
}

impl core::error::Error for Error {}
impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::UnsupportedSize => write!(f, "requested size is not supported by the backend"),
            Error::UnsupportedFormat => {
                write!(f, "requested format is not supported by the backend")
            }
            Error::UnsupportedSampleCount => {
                write!(f, "requested sample count is not supported by the backend")
            }
            Error::UnsupportedFeature => {
                write!(f, "requested feature is not supported by the backend")
            }
            Error::UnsupportedBinding(i) => {
                write!(f, "unsupported binding in pipeline (index {})", i)
            }
            Error::InvalidContext => write!(f, "invalid context"),
            Error::InvalidBounds => write!(f, "invalid bounds for a resource update"),
            Error::InvalidData => write!(f, "data does not match the requested format"),
            Error::InvalidResource => write!(f, "resource does not belong to this context"),
            Error::InvalidBinding(i) => {
                write!(f, "binding does not match the pipeline (index {})", i)
            }
            Error::InvalidFramebuffer => write!(f, "framebuffer already in use"),
            Error::Compile(CompileStage::Fragment, msg) => {
                write!(f, "fragment shader compilation error: {}", msg)
            }
            Error::Compile(CompileStage::Vertex, msg) => {
                write!(f, "vertex shader compilation error: {}", msg)
            }
            Error::Compile(CompileStage::Linking, msg) => {
                write!(f, "shader linking error: {}", msg)
            }
            Error::Internal(msg) => write!(f, "internal error: {}", msg),
        }
    }
}

mod buffer {
    /// The role of a buffer, which determines how it is expected to be used
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum BufferRole {
        /// A uniform buffer
        Uniform,
        /// A storage buffer
        Storage,
        /// A vertex buffer
        Vertex,
        /// An index buffer
        Index,
    }

    /// The layout of a buffer, used for creating a buffer with the desired specifications.
    ///
    /// ## Buffer
    /// A buffer is a contiguous region of memory allocated on the GPU. It is used for storing
    /// arbitrary data that can be read by a pipeline, or written to by the CPU.
    #[derive(Debug, Clone, Copy)]
    pub struct BufferLayout {
        /// The role of the buffer, which determines how it is expected to be used.
        pub role: BufferRole,
        /// The capacity of the buffer in bytes. Must be less than or equal to the maximum buffer
        /// size supported by the backend for the given role.
        pub capacity: u64,
        /// Whether the buffer is expected to be updated frequently with new data.
        ///
        /// As a rule of thumb, if you plan to update it once or twice over the lifetime of the
        /// buffer, you can set this to false, but if you plan to update it every frame or so, you
        /// should set this to true. This is a hint to the backend that can help with choosing the
        /// right memory type and usage for the buffer.
        pub dynamic: bool,
    }
}

mod texture {
    /// Texture format, which determines how the color pixel data is stored and/or interpreted.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[non_exhaustive]
    pub enum TextureFormat {
        /// 8-bit unsigned normalized red channel (0..1)
        R8,
        /// 8-bit unsigned normalized red, green, blue channels (0..1)
        RGB8,
        /// 8-bit unsigned normalized red, green, blue, alpha channels (0..1)
        RGBA8,

        /// 8-bit signed normalized red channel (-1..1)
        R8S,
        /// 16-bit signed normalized red channel (-1..1)
        R16S,
        /// 32-bit floating point red channel
        R32F,
    }

    /// The filtering mode used when sampling from a texture, which determines how the texture is
    /// sampled when the texture coordinates do not map 1:1 with texels (e.g. when minifying or
    /// magnifying the texture)
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum TextureFilter {
        /// Nearest neighbor filtering
        Nearest,
        /// Bilinear filtering
        Linear,
    }

    /// The wrapping mode used when sampling from a texture, which determines how texture
    /// coordinates outside the range [0, 1] are handled
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub enum TextureWrap {
        /// Clamp texture coordinates to the range [0, 1].
        Clamp,
        /// Repeat and wrap around the texture coordinates
        Repeat,
        /// Mirror the texture coordinates and repeat
        Mirror,
        /// Constant border color
        Border,
    }

    /// A region of pixels
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct TextureBounds {
        /// The X coordinate of the top-left corner of the region
        pub x: u32,
        /// The Y coordinate of the top-left corner of the region
        pub y: u32,
        /// The width of the region in pixels
        pub width: u32,
        /// The height of the region in pixels
        pub height: u32,
    }

    /// The layout of a texture, used for creating a texture with the desired specifications.
    #[derive(Debug, Clone, Copy)]
    pub struct TextureLayout {
        /// The width of the texture in pixels.
        pub width: u32,
        /// The height of the texture in pixels.
        pub height: u32,
        /// The format of each pixel
        pub format: TextureFormat,

        /// Filtering mode for minifying sampling
        pub filter_min: TextureFilter,
        /// Filtering mode for magnifying sampling
        pub filter_mag: TextureFilter,
        /// Wrapping mode for the X axis
        pub wrap_x: TextureWrap,
        /// Wrapping mode for the Y axis
        pub wrap_y: TextureWrap,
        /// Border color used when the wrapping mode is [`TextureWrap::Border`]
        pub wrap_border: [f32; 4],
    }

    impl TextureFormat {
        /// Returns the number of bytes per pixel for this texture format.
        pub const fn bytes_per_pixel(&self) -> u32 {
            match self {
                TextureFormat::R8 => 1,
                TextureFormat::RGB8 => 3,
                TextureFormat::RGBA8 => 4,
                TextureFormat::R8S => 1,
                TextureFormat::R16S => 2,
                TextureFormat::R32F => 4,
            }
        }
    }
}

mod framebuffer {
    use crate::TextureFormat;

    /// The format of the depth attachment.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum DepthFormat {
        /// 24-bit unsigned normalized depth.
        D24,
        /// 32-bit floating point depth.
        D32F,
    }

    /// The format of the stencil attachment.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum StencilFormat {
        /// 8-bit unsigned stencil component.
        S8,
    }

    /// The type of attachment of a framebuffer.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum FramebufferAttachment {
        /// Color attachment
        Color,
        /// Depth attachment
        Depth,
        /// Stencil attachment
        Stencil,
    }

    /// The layout of a framebuffer, used for creating a framebuffer with the desired
    /// specifications.
    #[derive(Debug, Clone, Copy)]
    pub struct FramebufferLayout {
        /// The width of the framebuffer in pixels.
        pub width: u32,
        /// The height of the framebuffer in pixels.
        pub height: u32,

        /// The format of the color attachment, if any.
        pub color: Option<TextureFormat>,
        /// The format of the depth attachment, if any.
        pub depth: Option<DepthFormat>,
        /// The format of the stencil attachment, if any.
        pub stencil: Option<StencilFormat>,

        /// The number of samples for MSAA, 0 if MSAA is not used.
        pub msaa_samples: u32,

        /// Whether the framebuffer is expected to be used as a persistent render target (i.e.
        /// contents are preserved across frames and can be drawn to multiple times).
        pub is_persistent: bool,
        /// Whether the color attachment is expected to be used as a texture that is sampled from.
        pub is_color_bindable: bool,
        /// Whether the depth attachment is expected to be used as a texture that is sampled from.
        pub is_depth_bindable: bool,
    }
}

mod shader {
    /// The stage of shader compilation that an error occurred in, used for error reporting.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum CompileStage {
        /// An error occurred while compiling the vertex shader.
        Vertex,
        /// An error occurred while compiling the fragment shader.
        Fragment,
        /// An error occurred while linking the shader program.
        Linking,
    }

    /// The shader format used by the backend.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[non_exhaustive]
    pub enum ShaderFormat {
        /// GLSL shader format.
        Glsl,
        /// SPIR-V shader format.
        SpirV,
    }

    /// A backend-specific shader representation. `picogpu` does not use a common shader
    /// representation and requires you to pre-compile shaders for each backend.
    #[derive(Debug, Clone, Copy)]
    #[non_exhaustive]
    pub enum Shader<'a> {
        /// GLSL shader representation.
        Glsl(ShaderGlsl<'a>),
        /// SPIR-V shader representation.
        SpirV(ShaderSpirV<'a>),
    }

    /// The GLSL shader representation.
    #[derive(Debug, Clone, Copy)]
    pub struct ShaderGlsl<'a> {
        /// The vertex shader source code.
        pub vertex: &'a str,

        /// The fragment shader source code.
        pub fragment: &'a str,

        /// The names of the shader binding variable names, in order of their binding index.
        pub bindings: &'a [&'a str],
    }

    /// The SPIR-V shader representation.   
    #[derive(Debug, Clone, Copy)]
    pub struct ShaderSpirV<'a> {
        /// The SPIR-V module bytecode for the vertex shader
        pub vertex_module: &'a [u32],
        /// The entry point name for the vertex shader
        pub vertex_entry: &'a str,
        /// The SPIR-V module bytecode for the fragment shader
        pub fragment_module: &'a [u32],
        /// The entry point name for the fragment shader
        pub fragment_entry: &'a str,
    }

    impl Shader<'_> {
        /// Returns the shader format of this shader.
        pub fn format(&self) -> ShaderFormat {
            match self {
                Shader::Glsl(_) => ShaderFormat::Glsl,
                Shader::SpirV(_) => ShaderFormat::SpirV,
            }
        }
    }

    impl<'a> From<ShaderGlsl<'a>> for Shader<'a> {
        fn from(value: ShaderGlsl<'a>) -> Self {
            Self::Glsl(value)
        }
    }

    impl<'a> From<ShaderSpirV<'a>> for Shader<'a> {
        fn from(value: ShaderSpirV<'a>) -> Self {
            Self::SpirV(value)
        }
    }
}

mod pipeline {
    use crate::{Shader, TextureFormat};

    /// The layout of a pipeline, used for creating a pipeline with the desired specifications.
    ///
    /// ## Pipeline
    /// A graphics pipeline represents an object that determines how draw calls are executed and
    /// rendered. Think of a graphics pipeline as an "assembly line": vertices go in, pixels come
    /// out.
    ///
    /// A pipeline determines (in order of execution):
    /// - How the vertices are constructed and how input data is interpreted (vertex shader)
    /// - How the vertices are interpreted into primitives and rasterized into fragments (topology)
    /// - How the fragments are clipped and discarded (cull mode, depth test, stencil test)
    /// - How the fragments are shaded (fragment shader)
    /// - How the output color is determined (blending)
    #[derive(Debug, Clone)]
    pub struct PipelineLayout<'a> {
        /// The shader (vertex and fragment) used by this pipeline
        pub shader: Shader<'a>,

        /// The format of the output color buffer
        pub color_format: TextureFormat,

        /// Blend mode used for color blending.
        ///
        /// This determines how the output color from the fragment shader (source) is blended with
        /// the existing color in the framebuffer (destination). The blend mode specifies how to
        /// compute the final color based on the source and destination colors, using the
        /// specified blend factors and operations for both color and alpha channels.
        pub color_blend: BlendMode,

        /// Depth test function.
        ///
        /// This is used to determine whether a fragment should be discarded based on its depth
        /// value compared to the existing depth value in the framebuffer. If the test
        /// fails, the fragment is discarded and does not update the color or depth buffers.
        pub depth_test: CompareFn,

        /// Whether to write the depth output of the fragment shader to the depth buffer.
        /// If this is false, the depth output will be discarded, but depth testing can still be
        /// performed.
        pub depth_write: bool,

        /// Stencil test and operations for clockwise wound triangles
        pub stencil_cw: StencilFace,
        /// Stencil test and operations for counter-clockwise wound triangles
        pub stencil_ccw: StencilFace,

        /// Whether to discard fragments from counter-clockwise wound triangles relative to the
        /// screen
        pub cull_ccw: bool,
        /// Whether to discard fragments from clockwise wound triangles relative to the screen
        pub cull_cw: bool,

        /// The primitive topology used for drawing, which determines how the vertex ordering is
        /// interpreted as primitives.
        pub topology: PrimitiveTopology,
    }

    /// A comparison function used for depth and stencil tests
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum CompareFn {
        /// Always false
        Never,
        /// Less than
        Less,
        /// Equal to
        Equal,
        /// Less than or equal to
        LessEqual,
        /// Greater than
        Greater,
        /// Not equal to
        NotEqual,
        /// Greater than or equal to
        GreaterEqual,
        /// Always true
        Always,
    }

    /// A stencil operation, which determines how the stencil is updated after a test.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum StencilOp {
        /// Do nothing.
        Keep,
        /// Set to zero.
        Zero,
        /// Set to reference.
        Replace,
        /// Bitwise inverse.
        Invert,
        /// Increment (saturate)
        IncrementClamp,
        /// Decrement (saturate)
        DecrementClamp,
        /// Increment (wrap)
        IncrementWrap,
        /// Decrement (wrap)
        DecrementWrap,
    }

    /// Stencil test and operations for a face (clockwise or counter-clockwise wound triangles)
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[repr(align(8))]
    pub struct StencilFace {
        /// A bitmask that determines which bits of the stencil buffer are used for the stencil test
        pub mask: u8,
        /// The reference value for the stencil test, used as a comparison value and in case
        /// [`StencilOp::Replace`] is used.
        pub reference: u8,
        /// The comparison function used for the stencil test. The fragment is discarded if the test
        /// fails.
        pub compare: CompareFn,
        /// What happens when the test passes
        pub pass_op: StencilOp,
        /// What happens when the test fails
        pub fail_op: StencilOp,
        /// What happens when the depth test fails (if depth testing is enabled)
        pub depth_fail_op: StencilOp,
    }

    /// Blend mode multiplier
    ///
    /// See [`BlendMode`] for how these are used in blending.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum BlendFactor {
        /// Zero
        Zero,
        /// One
        One,
        /// Source color.
        SrcColor,
        /// Inverse of source color (1 - src_color).
        OneMinusSrcColor,
        /// Destination color.
        DstColor,
        /// Inverse of destination color (1 - dst_color).
        OneMinusDstColor,
        /// Source alpha.
        SrcAlpha,
        /// Inverse of source alpha (1 - src_alpha).
        OneMinusSrcAlpha,
        /// Destination alpha.
        DstAlpha,
        /// Inverse of destination alpha (1 - dst_alpha).
        OneMinusDstAlpha,
    }

    /// Blend mode operation
    ///
    /// See [`BlendMode`] for how these are used in blending.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum BlendOp {
        /// Add source and destination.
        Add,
        /// Subtract destination from source.
        Subtract,
        /// Subtract source from destination.
        ReverseSubtract,
        /// Component-wise minimum.
        Min,
        /// Component-wise maximum.
        Max,
    }

    /// Blend mode, which determines how the source and destination colors are blended together
    ///
    /// The blend formula is:
    /// ```
    /// result = (src_value * src_factor) <color_op> (dst_value * dst_factor)
    /// ```
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[repr(align(8))]
    pub struct BlendMode {
        /// Blend factor for the source color (output of the fragment shader)
        pub color_src: BlendFactor,
        /// Blend factor for the destination color (existing color in the framebuffer)
        pub color_dst: BlendFactor,
        /// Blend operation for the color channels.
        pub color_op: BlendOp,

        /// Blend factor for the source alpha (output of the fragment shader)
        pub alpha_src: BlendFactor,
        /// Blend factor for the destination alpha (existing alpha in the framebuffer)
        pub alpha_dst: BlendFactor,
        /// Blend operation for the alpha channel.
        pub alpha_op: BlendOp,
    }

    /// The primitive topology used for drawing, which determines how the vertex data is interpreted
    /// as triangles.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum PrimitiveTopology {
        /// Each group of 3 vertices forms a separate triangle.
        TriangleList,
        /// Each vertex forms a triangle together with the previous 2 vertices, reusing vertices to
        /// form connected strips of triangles.
        TriangleStrip,
        /// A fan of triangles sharing a common vertex (the first vertex)
        TriangleFan,
    }

    impl BlendMode {
        /// Opaque blending mode (overwrite)
        pub const OPAQUE: Self = Self {
            color_src: BlendFactor::One,
            color_dst: BlendFactor::Zero,
            color_op: BlendOp::Add,

            alpha_src: BlendFactor::One,
            alpha_dst: BlendFactor::Zero,
            alpha_op: BlendOp::Add,
        };

        /// Simple alpha blending mode
        pub const ALPHA: Self = Self {
            color_src: BlendFactor::SrcAlpha,
            color_dst: BlendFactor::OneMinusSrcAlpha,
            color_op: BlendOp::Add,

            alpha_src: BlendFactor::One,
            alpha_dst: BlendFactor::OneMinusSrcAlpha,
            alpha_op: BlendOp::Add,
        };

        /// Premultiplied alpha blending mode
        pub const PREMUL: Self = Self {
            color_src: BlendFactor::One,
            color_dst: BlendFactor::OneMinusSrcAlpha,
            color_op: BlendOp::Add,

            alpha_src: BlendFactor::One,
            alpha_dst: BlendFactor::OneMinusSrcAlpha,
            alpha_op: BlendOp::Add,
        };
    }

    impl Default for StencilFace {
        fn default() -> Self {
            Self {
                mask: 0x0,
                reference: 0,
                compare: CompareFn::Always,
                pass_op: StencilOp::Keep,
                fail_op: StencilOp::Keep,
                depth_fail_op: StencilOp::Keep,
            }
        }
    }
}

mod draw {
    use crate::{Context, FramebufferAttachment, TextureBounds};

    /// A request to the backend to clear a region of a framebuffer with the specified clear
    /// parameters.
    #[derive(Debug, Clone, Copy)]
    pub struct ClearRequest<'a, C: Context> {
        /// The framebuffer to clear.
        pub target: &'a C::Framebuffer,
        /// The subregion of the framebuffer to clear, or `None` to clear the whole framebuffer.
        pub scissor: Option<TextureBounds>,
        /// Target value for the color attachment, does nothing if `None`
        pub color: Option<[f32; 4]>,
        /// Target value for the depth attachment, does nothing if `None`
        pub depth: Option<f32>,
        /// Target value for the stencil attachment, does nothing if `None`
        pub stencil: Option<u8>,
    }

    /// A request to the backend to draw a batch of primitives.
    #[derive(Debug, Clone, Copy)]
    pub struct DrawRequest<'a, C: Context> {
        /// The framebuffer to draw onto.
        pub target: &'a C::Framebuffer,

        /// The graphics pipeline to use for drawing.
        pub pipeline: &'a C::Pipeline,
        /// The resources to bind to the pipeline for this draw call. The binding order is
        /// determined by the specificed pipeline layout and shader data.
        pub bindings: &'a [BindingData<'a, C>],

        /// The viewport to use for drawing. Determines the transformation from normalized device
        /// coordinates to screen coordinates.
        pub viewport: TextureBounds,
        /// The scissor rectangle to use for drawing. Limits the drawing area to a specific region.
        /// If `None`, no scissor test is applied and drawing can affect the whole framebuffer.
        pub scissor: Option<TextureBounds>,

        /// The number of vertices to dispatch.
        pub vertices: u32,
    }

    /// A resource binding for a draw call, which can be a texture, framebuffer, or a buffer region.
    #[derive(Debug, Clone, Copy)]
    pub enum BindingData<'a, C: Context> {
        /// A texture/sampler binding.
        Texture {
            /// The texture to bind.
            texture: &'a C::Texture,
        },
        /// A framebuffer binding.
        Framebuffer {
            /// The framebuffer to bind.
            ///
            /// This cannot be the same framebuffer as the draw target, otherwise this will result
            /// in an error.
            framebuffer: &'a C::Framebuffer,
            /// Which attachment of the framebuffer to bind.
            ///
            /// Please note that the framebuffer must have been created with the corresponding
            /// attachment as bindable (e.g. `is_color_bindable` for `Color` attachment), otherwise
            /// this will result in an error.
            attachment: FramebufferAttachment,
        },
        /// A buffer binding.
        Buffer {
            /// The buffer to bind.
            buffer: &'a C::Buffer,
            /// The start of the bound region.
            offset: u32,
            /// The size of the bound region in bytes.
            size: u32,
        },
    }
}
