use alloc::string::String;
use core::{fmt::Debug, time::Duration};

pub use buffer::*;
pub use draw::*;
pub use framebuffer::*;
pub use pipeline::*;
pub use shader::*;
pub use texture::*;

/// The main interface for interacting with a backend, used for creating and managing resources
/// (buffers, textures, pipelines, framebuffers, profilers) and issuing draw calls.
pub trait Context {
    /// A buffer object, used for storing arbitrary data on the GPU
    type Buffer: 'static + Debug + Send;
    /// A texture object, used for storing image data on the GPU
    type Texture: 'static + Debug + Send;
    /// A pipeline object, represents a shader program and associated draw state (blend mode,
    /// stencil, depth test, etc.)
    type Pipeline: 'static + Debug + Send;
    /// A profiler object, used for measuring GPU execution time of draw calls.
    type Profiler: 'static + Debug + Send;
    /// A framebuffer object, used as a texture you can draw to and sample from.
    type Framebuffer: 'static + Debug + Send;

    /// The capabilities of the GPU, used for determining what features are supported and the limits
    /// of various resources (max texture size, max buffer size, etc.)
    fn capabilities(&self) -> Capabilities;

    /// Creates a new buffer with the given layout, and returns a handle to it.
    ///
    /// # Errors
    /// - [Error::UnsupportedSize] if the requested buffer size is larger than what is supported.
    /// - [Error::Internal] if an internal error occurs while creating the buffer.
    fn create_buffer(&self, layout: BufferLayout) -> Result<Self::Buffer, Error>;
    /// Creates a new texture with the given layout, and returns a handle to it.
    ///
    /// # Errors
    /// - [Error::UnsupportedSize] if the requested texture dimensions are larger than what is
    ///   supported.
    /// - [Error::UnsupportedFormat] if the requested texture format is not supported.
    /// - [Error::Internal] if an internal error occurs while creating the texture.
    fn create_texture(&self, layout: TextureLayout) -> Result<Self::Texture, Error>;
    /// Creates a new pipeline with the given layout, and returns a handle to it.
    ///
    /// # Errors
    /// - [Error::UnsupportedFormat] if the shader format used in the pipeline is not supported.
    /// - [Error::UnsupportedBinding] if the pipeline layout requires more bindings than what is
    ///   supported.
    /// - [Error::Compile] if an error occurs while compiling the shader for the pipeline.
    /// - [Error::Internal] if an internal error occurs while creating the pipeline.
    fn create_pipeline(&self, layout: PipelineLayout) -> Result<Self::Pipeline, Error>;
    /// Creates a new framebuffer with the given layout, and returns a handle to it.
    ///
    /// # Errors
    /// - [Error::UnsupportedSize] if the requested framebuffer dimensions are larger than what is
    ///   supported.
    /// - [Error::UnsupportedFormat] if the requested color/depth/stencil format is not supported.
    /// - [Error::UnsupportedSampleCount] if the requested MSAA sample count is not supported.
    /// - [Error::Internal] if an internal error occurs while creating the framebuffer.
    fn create_framebuffer(&self, layout: FramebufferLayout) -> Result<Self::Framebuffer, Error>;
    /// Creates a new profiler, and returns a handle to it.
    ///
    /// # Errors
    /// - [Error::UnsupportedFeature] if the GPU does not support profiling.
    /// - [Error::Internal] if an internal error occurs while creating the profiler.
    fn create_profiler(&self) -> Result<Self::Profiler, Error>;

    /// Deletes a buffer, freeing its memory on the GPU.
    fn delete_buffer(&self, buffer: Self::Buffer);

    /// Deletes a texture, freeing its memory on the GPU.
    fn delete_texture(&self, texture: Self::Texture);

    /// Deletes a pipeline, freeing its resources on the GPU.
    fn delete_pipeline(&self, pipeline: Self::Pipeline);

    /// Deletes a framebuffer, freeing its resources on the GPU.
    fn delete_framebuffer(&self, framebuffer: Self::Framebuffer);

    /// Deletes a profiler, freeing its resources on the GPU.
    fn delete_profiler(&self, profiler: Self::Profiler);

    /// Uploads data to a texture, replacing the contents of the texture at the given bounds.
    /// The data must match the layout specified by the (width, height, format) triple.
    ///
    /// # Errors
    /// - [Error::InvalidResource] if the texture does not belong to this context.
    /// - [Error::InvalidBounds] if the bounds exceed the texture dimensions.
    /// - [Error::InvalidData] if the data size does not match the expected size for the given
    ///   bounds and format.
    /// - [Error::Internal] if an internal error occurs while uploading the data.
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
    /// - [Error::InvalidResource] if the buffer does not belong to this context.
    /// - [Error::InvalidBounds] if the offset and data size exceed the buffer capacity, or if the
    ///   offset does not match alignment requirements for the buffer layout.
    /// - [Error::Internal] if an internal error occurs while uploading the data.
    fn upload_buffer(&self, buffer: &Self::Buffer, offset: u32, data: &[u8]) -> Result<(), Error>;

    /// Copies data from one buffer to another, replacing the contents of the destination buffer at
    /// the given offset. The source and destination regions must not overlap if the source and
    /// destination buffers are the same.
    ///
    /// # Errors
    /// - [Error::InvalidResource] if either buffer does not belong to this context.
    /// - [Error::InvalidBounds] if the source or destination offset and size exceed the respective
    ///   buffer sizes, if the source and destination regions overlap when the source and
    ///   destination buffers are the same, or if the offset does not match alignment requirements
    ///   for the buffer layout.
    /// - [Error::Internal] if an internal error occurs while copying the data.
    fn copy_buffer(
        &self,
        buffer: &Self::Buffer,
        source_buffer: &Self::Buffer,
        offset: u32,
        source_offset: u32,
        size: u32,
    ) -> Result<(), Error>;

    /// Invalidates a region of a buffer, indicating that the contents of that region are no longer
    /// needed and can be discarded by the GPU. This is a hint to the backend that can help with
    /// avoiding unnecessary synchronization for future drawcalls.
    ///
    /// # Errors
    /// - [Error::InvalidResource] if the buffer does not belong to this context.
    /// - [Error::InvalidBounds] if the offset and size exceed the buffer capacity, or if the offset
    ///   does not match alignment requirements for the buffer layout.
    /// - [Error::Internal] if an internal error occurs while invalidating the buffer.
    fn invalidate_buffer(&self, buffer: &Self::Buffer, offset: u32, size: u32)
    -> Result<(), Error>;

    /// Reads data from a framebuffer, copying the contents of the specified bounds into the
    /// provided data buffer. This is a slow operation, and should be avoided if possible.
    /// Common usecases include readback for screenshots and draw tests.
    ///
    /// # Errors
    /// - [Error::InvalidResource] if the framebuffer does not belong to this context.
    /// - [Error::InvalidBounds] if the bounds exceed the framebuffer dimensions.
    /// - [Error::InvalidData] if the data buffer size does not match the expected size for the
    ///   given bounds and format.
    /// - [Error::Internal] if an internal error occurs while reading the data.
    fn read_framebuffer(
        &self,
        target: &Self::Framebuffer,
        bounds: TextureBounds,
        format: TextureFormat,
        data: &mut [u8],
    ) -> Result<(), Error>;

    /// Begins a profiling section for the given profiler, which will measure the GPU execution time
    /// of subsequent draw calls until [`Context::end_profiler`] is called with the same profiler.
    fn begin_profiler(&self, profiler: &Self::Profiler);

    /// Ends a profiling section for the given profiler, returning the measured GPU execution time
    /// of the draw calls that were executed since [`Context::begin_profiler`] was called on the
    /// profiler.
    ///
    /// Profiling is asynchronous, the returned execution time may not be available immediately,
    /// and can be `None` if the GPU has not finished executing the draw calls yet. Once the
    /// execution time is available, it will be returned by this method on subsequent calls; the
    /// profiler will do nothing until a result is available
    fn end_profiler(&self, profiler: &Self::Profiler) -> Option<Duration>;

    /// Issue a draw call with the given target, pipeline, bindings, and other draw parameters.
    ///
    /// # Errors
    /// - [Error::InvalidResource] if the framebuffer, pipeline, or any resources used in the
    ///   bindings do not belong to this context.
    /// - [Error::InvalidBinding] if the provided bindings do not match the bindings declared when
    ///   the pipeline was created (e.g. wrong number of bindings, or wrong types of bindings).
    /// - [Error::Internal] if an internal error occurs while issuing the draw call.
    fn draw(&self, draw: DrawRequest<Self>) -> Result<(), Error>;
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
    pub uniform_buffer_size: u32,
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
/// This is a "god" error type. While it is not often desirable to have a catch-all error type,
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
    #[derive(Debug, Clone, Copy)]
    pub struct BufferLayout {
        /// The role of the buffer, which determines how it is expected to be used
        pub role: BufferRole,
        /// The capacity of the buffer in bytes.
        pub capacity: u32,
        /// Whether the buffer is expected to be updated frequently with new data. This is a hint
        /// that can result in better performance if used correctly.
        pub dynamic: bool,
    }
}

mod texture {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[non_exhaustive]
    pub enum TextureFormat {
        R8,
        RGB8,
        RGBA8,

        R8Snorm,
        R16Snorm,
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
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum TextureWrap {
        /// Clamp texture coordinates to the range [0, 1].
        Clamp,
        /// Repeat and wrap around the texture coordinates
        Repeat,
        /// Mirror the texture coordinates and repeat
        Mirror,
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
    }

    impl TextureFormat {
        /// Returns the number of bytes per pixel for this texture format.
        pub const fn bytes_per_pixel(&self) -> u32 {
            match self {
                TextureFormat::R8 => 1,
                TextureFormat::RGB8 => 3,
                TextureFormat::RGBA8 => 4,
                TextureFormat::R8Snorm => 1,
                TextureFormat::R16Snorm => 2,
                TextureFormat::R32F => 4,
            }
        }
    }
}

mod framebuffer {
    use crate::TextureFormat;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[non_exhaustive]
    pub enum DepthStencilFormat {
        Depth24Stencil8,
        Depth32FStencil8,
        Depth32F,
        Stencil8,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum FramebufferAttachment {
        Color,
        Depth,
        Stencil,
    }

    #[derive(Debug, Clone, Copy)]
    pub struct FramebufferLayout {
        pub width: u32,
        pub height: u32,

        pub color: Option<TextureFormat>,
        pub depth: Option<DepthStencilFormat>,
        pub msaa_samples: u32,

        pub is_persistent: bool,
        pub is_color_bindable: bool,
        pub is_depth_bindable: bool,
    }
}

mod shader {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum CompileStage {
        Vertex,
        Fragment,
        Linking,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[non_exhaustive]
    pub enum ShaderFormat {
        SpirV,
        Glsl,
    }

    #[derive(Debug, Clone, Copy)]
    #[non_exhaustive]
    pub enum Shader<'a> {
        Glsl(ShaderGlsl<'a>),
        SpirV(&'a [u8]),
    }

    #[derive(Debug, Clone, Copy)]
    pub struct ShaderGlsl<'a> {
        /// The vertex shader source code.
        pub vertex: &'a str,

        /// The fragment shader source code.
        pub fragment: &'a str,

        /// The names of the shader binding variable names, in order of their binding index.
        pub bindings: &'a [&'a str],
    }

    impl Shader<'_> {
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
}

mod pipeline {
    use crate::{Shader, TextureFormat};

    #[derive(Debug, Clone)]
    pub struct PipelineLayout<'a> {
        /// The shader (vertex and fragment) used by this pipeline
        pub shader: Shader<'a>,

        /// The format of the output color buffer
        pub color_format: TextureFormat,
        /// Blend mode used for color blending (how the resulting color is calculated from the
        /// fragment shader output and the existing color in the framebuffer)
        pub color_blend: BlendMode,
        /// Depth test function (discards fragments if it fails the depth test against the existing
        /// depth in the framebuffer)
        pub depth_test: CompareFn,
        /// Whether to write the depth output of the fragment shader to the depth buffer
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
    }

    /// A comparison function used for depth and stencil tests
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum CompareFn {
        Never,
        Less,
        Equal,
        LessEqual,
        Greater,
        NotEqual,
        GreaterEqual,
        Always,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum StencilOp {
        Keep,
        Zero,
        Replace,
        Invert,
        IncrementClamp,
        DecrementClamp,
        IncrementWrap,
        DecrementWrap,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[repr(align(8))]
    pub struct StencilFace {
        pub mask: u8,
        pub reference: u8,
        pub compare: CompareFn,
        pub pass_op: StencilOp,
        pub fail_op: StencilOp,
        pub depth_fail_op: StencilOp,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum BlendFactor {
        Zero,
        One,
        SrcColor,
        OneMinusSrcColor,
        DstColor,
        OneMinusDstColor,
        SrcAlpha,
        OneMinusSrcAlpha,
        DstAlpha,
        OneMinusDstAlpha,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum BlendOp {
        Add,
        Subtract,
        ReverseSubtract,
        Min,
        Max,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[repr(align(8))]
    pub struct BlendMode {
        pub color_src: BlendFactor,
        pub color_dst: BlendFactor,
        pub color_op: BlendOp,

        pub alpha_src: BlendFactor,
        pub alpha_dst: BlendFactor,
        pub alpha_op: BlendOp,
    }

    impl BlendMode {
        pub const OPAQUE: Self = Self {
            color_src: BlendFactor::One,
            color_dst: BlendFactor::Zero,
            color_op: BlendOp::Add,

            alpha_src: BlendFactor::One,
            alpha_dst: BlendFactor::Zero,
            alpha_op: BlendOp::Add,
        };

        pub const ALPHA: Self = Self {
            color_src: BlendFactor::SrcAlpha,
            color_dst: BlendFactor::OneMinusSrcAlpha,
            color_op: BlendOp::Add,

            alpha_src: BlendFactor::One,
            alpha_dst: BlendFactor::OneMinusSrcAlpha,
            alpha_op: BlendOp::Add,
        };

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
    use crate::{Context, TextureBounds};

    #[derive(Debug)]
    pub struct DrawRequest<'a, C: Context + ?Sized> {
        pub target: &'a C::Framebuffer,

        pub color_op: MemoryOp<[f32; 4]>,
        pub depth_op: MemoryOp<f32>,
        pub stencil_op: MemoryOp<u8>,

        pub pipeline: &'a C::Pipeline,
        pub bindings: &'a [BindingData<'a, C>],

        pub viewport: TextureBounds,
        pub scissor: Option<TextureBounds>,

        pub triangles: u32,
    }

    #[derive(Debug, Clone, Copy)]
    pub struct MemoryOp<T> {
        pub load: LoadOp<T>,
        pub store: StoreOp,
    }

    #[derive(Debug, Clone, Copy)]
    pub enum LoadOp<T> {
        Clear(T),
        Discard,
        Load,
    }

    #[derive(Debug, Clone, Copy)]
    pub enum StoreOp {
        Store,
        Discard,
    }

    #[derive(Debug, Clone, Copy)]
    pub enum BindingData<'a, C: Context + ?Sized> {
        Texture {
            texture: &'a C::Texture,
        },
        FramebufferColor {
            framebuffer: &'a C::Framebuffer,
        },
        Buffer {
            buffer: &'a C::Buffer,
            offset: u32,
            size: u32,
        },
    }
}
