use std::error;
use std::fmt::{self, Debug};
use std::time::Duration;

pub use buffer::*;
pub use draw::*;
pub use framebuffer::*;
pub use pipeline::*;
pub use shader::*;
pub use texture::*;

/// The main interface for interacting with a backend, used for creating and managing resources
/// (buffers, textures, pipelines, framebuffers, profilers) and issuing draw calls.
pub trait Context: Sized {
    /// A buffer object, used for storing arbitrary data on the GPU
    type Buffer: Debug;
    /// A texture object, used for storing image data on the GPU
    type Texture: Debug;
    /// A pipeline object, represents a shader program and associated draw state (blend mode,
    /// stencil, depth test, etc.)
    type Pipeline: Debug;
    /// A framebuffer object, used as a texture you can draw to and sample from.
    type Framebuffer: Debug;
    /// A fence object, used for synchronizing the CPU command queue with the GPU execution.
    type Fence: Debug;
    /// A query object, used for asynchronous queries to the GPU (e.g. for profiling).
    type Query: Debug;

    /// The capabilities of the GPU, used for determining what features are supported and the limits
    /// of various resources (max texture size, max buffer size, etc.)
    fn capabilities(&self) -> Capabilities;

    /// Creates a new buffer with the given layout, and returns a handle to it.
    ///
    /// # Errors
    /// - [`Error::UnsupportedSize`] if the requested buffer size is larger than what is supported.
    ///   Note that the alignment requirement does not apply here (a backend could allocate more
    ///   memory than requested if needed).
    /// - [`Error::Internal`] if an internal error occurs while creating the buffer.
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
    /// - [`Error::UnsupportedFormat`] if the requested color/depth/stencil format is not supported,
    ///   if the requested MSAA sample count is not supported, if the requested number of
    ///   attachments is not supported, or if too many attachments were requested.
    /// - [`Error::Internal`] if an internal error occurs while creating the framebuffer.
    fn create_framebuffer(&self, layout: FramebufferLayout) -> Result<Self::Framebuffer, Error>;

    /// Uploads data to a buffer, replacing the contents of the buffer at the given offset.
    ///
    /// # Errors
    /// - [`Error::InvalidResource`] if the buffer does not belong to this context.
    /// - [`Error::InvalidOperation`] if the buffer was not created with the
    ///   [`BufferLayout::can_upload`] flag.
    /// - [`Error::InvalidBounds`] if the offset and data size exceed the buffer capacity, or if the
    ///   offset does not match alignment requirements for the buffer layout.
    /// - [`Error::Internal`] if an internal error occurs while uploading the data.
    fn upload_buffer(&self, buffer: &Self::Buffer, offset: u64, data: &[u8]) -> Result<(), Error>;

    /// Downloads data from a buffer, returning a readback object that can be used to read the data
    /// back from the GPU in an asynchronous manner.
    ///
    /// # Synchronization
    /// Calling this method has to wait until all previous operations that write to this buffer are
    /// finished, which could cause a stall if the buffer is still in use by the GPU.
    ///
    /// To read the data in an asynchronous manner, consider using a fence
    /// ([`Context::insert_fence`]).
    ///
    /// # Errors
    /// - [`Error::InvalidResource`] if the buffer does not belong to this context.
    /// - [`Error::InvalidOperation`] if the buffer was not created with the
    ///   [`BufferLayout::can_download`] flag.
    /// - [`Error::InvalidBounds`] if the offset exceeds the buffer capacity, or if the offset does
    ///   not match alignment requirements for the buffer layout.
    /// - [`Error::Internal`] if an internal error occurs while downloading the data.
    fn download_buffer(&self, buffer: &Self::Buffer, offset: u64, data: &mut [u8]) -> Result<(), Error>;

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
    fn copy_buffer_to_buffer(
        &self,
        dst_buffer: &Self::Buffer,
        dst_offset: u64,
        src_buffer: &Self::Buffer,
        src_offset: u64,
        size: u64,
    ) -> Result<(), Error>;

    /// Copy data from a buffer to a texture, replacing the contents of the texture at the given
    /// bounds. The data must match the layout specified by the (width, height, format) triple.
    ///
    /// Texture data must conform to the native format of the destination texture (as specified in
    /// [`Context::create_texture`])
    ///
    /// # Errors
    /// - [`Error::InvalidResource`] if the texture does not belong to this context.
    /// - [`Error::InvalidBounds`] if the destination bounds exceed the texture dimensions, the
    ///   source bounds exceed the buffer capacity, or if the offset does not match alignment
    ///   requirements for the buffer layout.
    /// - [`Error::Internal`] if an internal error occurs while copying the data.
    fn copy_buffer_to_texture(
        &self,
        dst_texture: &Self::Texture,
        dst_bounds: TextureBounds,
        src_buffer: &Self::Buffer,
        src_offset: u64,
    ) -> Result<(), Error>;

    /// Copy data from a framebuffer to a buffer, replacing the contents of the buffer at the given
    /// offset. The data will be read from the specified attachment of the framebuffer, and must
    /// match the layout specified by the (width, height, format) triple.
    ///
    /// # Errors
    /// - [`Error::InvalidResource`] if the framebuffer does not belong to this context.
    /// - [`Error::InvalidBounds`] if the destination bounds exceed the buffer capacity, the source
    ///   bounds exceed the framebuffer dimensions, or if the offset does not match alignment
    ///   requirements for the buffer layout.
    /// - [`Error::Internal`] if an internal error occurs while copying the data.
    fn copy_framebuffer_to_buffer(
        &self,
        dst_buffer: &Self::Buffer,
        dst_offset: u64,
        src_framebuffer: &Self::Framebuffer,
        src_attachment: FramebufferAttachment,
        src_bounds: TextureBounds,
    ) -> Result<(), Error>;

    /// Begins a profiling section, which will measure the requested [`QueryData`]
    /// until [`Context::end_query`] is called with the same query.
    ///
    /// # Errors
    /// - [`Error::UnsupportedFeature`] if the backend does not support profiling queries.
    /// - [`Error::InvalidOperation`] if a query with the same type is already active.
    /// - [`Error::Internal`] if an internal error occurs while beginning the query.
    fn begin_query(&self, query: QueryType) -> Result<Self::Query, Error>;

    /// Ends a profiling section for the given query.
    ///
    /// # Errors
    /// - [`Error::InvalidResource`] if the query does not belong to this context.
    /// - [`Error::InvalidOperation`] if [`Context::end_query`] was called for this query before.
    /// - [`Error::Internal`] if an internal error occurs while ending the query.
    fn end_query(&self, query: &Self::Query) -> Result<(), Error>;

    /// Reads the result of a query section, if available.
    ///
    /// # Errors
    /// - [`Error::InvalidResource`] if the query does not belong to this context.
    /// - [`Error::InvalidOperation`] if [`Context::end_query`] has not been called for this query.
    /// - [`Error::Internal`] if an internal error occurs while reading the query.
    fn read_query(&self, query: &Self::Query) -> Result<Option<u64>, Error>;

    /// Waits until the GPU has reached the given fence, which means that all commands issued before
    /// the fence have been completed by the GPU. Fences are created by calling
    /// [`Context::present`].
    ///
    /// Returns Ok(true) if the fence was signalled, Ok(false) if the timeout was reached first.
    ///
    /// Use timeout of zero to check the status of the fence without blocking. Please note that the
    /// implementation is allowed to wake up spuriously and return `Ok(false)` even if the timeout
    /// has not been reached.
    ///
    /// # Errors
    /// - [`Error::InvalidResource`] if the fence does not belong to this context.
    /// - [`Error::UnsupportedFeature`] if the backend does not support waiting on fences.
    /// - [`Error::Internal`] if an internal error occurs while waiting for the fence.
    fn wait_fence(&self, fence: &Self::Fence, timeout: Duration) -> Result<bool, Error>;

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
    /// - [`Error::BindingMismatch`] if the provided bindings do not match the bindings declared
    ///   when the pipeline was created (e.g. wrong number of bindings, or wrong types of bindings).
    /// - [`Error::FramebufferInUse`] if the target framebuffer is also used as a binding resource
    ///   at the same time.
    /// - [`Error::Internal`] if an internal error occurs while issuing the draw call.
    fn draw(&self, draw: DrawRequest<Self>) -> Result<(), Error>;

    /// Submits all pending commands to the GPU for execution. This should be called at the end of
    /// each frame.
    ///
    /// Returns a fence object that can be used to synchronize with the GPU execution of the
    /// submitted commands by calling [`Context::wait_fence`].
    ///
    /// # Errors
    /// - [`Error::InvalidContext`] if the current backend-defined context is no longer valid (e.g.
    ///   lost OpenGL context).
    /// - [`Error::Internal`] if an internal error occurs while submitting the commands.
    fn present(&self) -> Result<Self::Fence, Error>;
}

/// The capabilities of the GPU, used for determining what features are supported and the limits of
/// various resources (max texture size, max buffer size, etc.)
#[derive(Debug, Clone, Copy)]
pub struct Capabilities {
    /// The shader format that is used by the backend
    pub shader_format: ShaderFormat,

    /// Maximum supported width/height of a framebuffer
    pub framebuffer_size: u32,
    /// Maximum supported MSAA sample count for framebuffers, 0 if MSAA is not supported.
    pub framebuffer_msaa: u32,
    /// Maximum supported number of color attachments for a framebuffer.
    pub framebuffer_outputs: u32,

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
    pub storage_buffer_size: u64,
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
    /// The requested feature is not supported by the GPU
    UnsupportedFeature,
    /// The pipeline binding requested is not supported by the GPU (e.g. more texture bindings than
    /// supported)
    UnsupportedBinding(usize),

    /// Trying to update a texture/buffer for a region that is out of bounds for a resource,
    /// or the bounds overlap when copying from/to the same buffer.
    InvalidBounds,
    /// Trying to perform an operation that is not valid in the current state (e.g. attempt to
    /// upload to a non-writable buffer, or attempt to read from an open query)
    InvalidOperation,
    /// Trying to use a resource that does not belong to this context
    InvalidResource,
    /// Attempt to create a context with an invalid backend state (i.e. a non-current or
    /// non-existing OpenGL context)
    InvalidContext,

    /// The binding provided does not match the pipeline description
    BindingMismatch(usize, &'static str),
    /// Attempt to draw to a framebuffer that is also used as a binding resource at the same time.
    FramebufferInUse,

    /// An error occurred during fragment shader compilation.
    Compile(CompileStage, String),

    /// An internal error has occurred, this is a catch-all for any error that does not fit into the
    /// other categories, and is not the fault of the user (e.g. a driver bug, or an unexpected
    /// failure in the graphics API)
    Internal(String),
}

impl error::Error for Error {}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::UnsupportedSize => write!(f, "requested size is not supported by the backend"),
            Error::UnsupportedFormat => {
                write!(f, "requested format is not supported by the backend")
            }
            Error::UnsupportedFeature => {
                write!(f, "requested feature is not supported by the backend")
            }
            Error::UnsupportedBinding(i) => {
                write!(f, "unsupported binding in pipeline (index {})", i)
            }
            Error::InvalidContext => write!(f, "invalid context"),
            Error::InvalidBounds => write!(f, "invalid bounds for a resource update"),
            Error::InvalidResource => write!(f, "resource does not belong to this context"),
            Error::InvalidOperation => write!(f, "operation is not valid in the current state"),
            Error::BindingMismatch(i, msg) => {
                write!(f, "binding does not match the pipeline (index {}): {}", i, msg)
            }
            Error::FramebufferInUse => write!(f, "framebuffer already in use"),
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
    #[allow(unused)] //docs
    use crate::*;

    /// The role of a buffer, which determines how it is expected to be used
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum BufferRole {
        /// A uniform buffer
        Uniform,
        /// A storage buffer
        Storage,
        /// A staging buffer (not expected to be used directly in a draw call, but can be used as an
        /// intermediate buffer for copying data to/from the GPU)
        Staging,
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

        /// Whether the buffer is expected to be read back from the GPU (e.g. with
        /// [`Context::download_buffer`]).
        pub can_download: bool,

        /// Whether the buffer is expected to be updated from the CPU (e.g. with
        /// [`Context::upload_buffer`]).
        ///
        /// Setting this to `false` can allow the backend to optimize the buffer for GPU-only usage,
        /// which can improve performance if the buffer is only used as a resource for draw
        /// calls and never updated from the CPU after creation (for updating such a buffer,
        /// consider using a separate staging buffer and copying the data to the GPU with
        /// [`Context::copy_buffer_to_buffer`])
        pub can_upload: bool,
    }

    impl BufferLayout {
        /// Creates a new buffer layout with the given role and capacity, and default usage flags
        /// (not readable, not writable).
        pub fn new(role: BufferRole, capacity: u64) -> Self {
            Self {
                role,
                capacity,
                can_download: false,
                can_upload: false,
            }
        }

        /// Sets the buffer to be downloadable, which means that it can be read back from the GPU
        /// with [`Context::download_buffer`].
        pub fn with_can_download(mut self) -> Self {
            self.can_download = true;
            self
        }

        /// Sets the buffer to be uploadable, which means that it can be updated from the CPU with
        /// [`Context::upload_buffer`].
        pub fn with_can_upload(mut self) -> Self {
            self.can_upload = true;
            self
        }
    }
}

mod texture {
    /// Texture format, which determines how the color pixel data is stored and/or interpreted.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[non_exhaustive]
    pub enum TextureFormat {
        /// 8-bit unsigned normalized red channel (0..1)
        R8,
        /// 8-bit unsigned normalized red, green, blue, alpha channels (0..1)
        RGBA8,
        /// 8-bit unsigned normalized blue, green, red, alpha channels (0..1)
        BGRA8,

        /// 8-bit signed normalized red channel (-1..1)
        R8S,
        /// 16-bit signed normalized red channel (-1..1)
        R16S,
        /// 32-bit floating point red channel
        R32F,
        /// 32-bit floating point red and green channels
        RG32F,
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

    /// The border color used when the wrapping mode is [`TextureWrap::Border`]. This determines the
    /// color returned when sampling a texture with out-of-bounds texture coordinates.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum TextureBorder {
        /// Transparent (0.0, 0.0, 0.0, 0.0)
        Transparent,
        /// Opaque black (0.0, 0.0, 0.0, 1.0)
        Black,
        /// Opaque white (1.0, 1.0, 1.0, 1.0)
        White,
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
        pub wrap_border: TextureBorder,
    }

    impl TextureFormat {
        /// Returns the number of bytes per pixel for this texture format.
        pub const fn bytes_per_pixel(&self) -> u32 {
            match self {
                TextureFormat::R8 => 1,
                TextureFormat::RGBA8 => 4,
                TextureFormat::BGRA8 => 4,
                TextureFormat::R8S => 1,
                TextureFormat::R16S => 2,
                TextureFormat::R32F => 4,
                TextureFormat::RG32F => 8,
            }
        }
    }

    impl TextureLayout {
        /// Creates a new texture layout with the given width, height, and format, and default
        /// sampling parameters (nearest filtering, border wrapping with transparent border color).
        pub fn new(width: u32, height: u32, format: TextureFormat) -> Self {
            Self {
                width,
                height,
                format,
                filter_min: TextureFilter::Nearest,
                filter_mag: TextureFilter::Nearest,
                wrap_x: TextureWrap::Border,
                wrap_y: TextureWrap::Border,
                wrap_border: TextureBorder::Transparent,
            }
        }

        /// Sets the filtering mode for the texture.
        pub fn with_filter(mut self, filter_min: TextureFilter, filter_mag: TextureFilter) -> Self {
            self.filter_min = filter_min;
            self.filter_mag = filter_mag;
            self
        }

        /// Sets the wrapping mode for the texture.
        pub fn with_wrap(mut self, wrap_x: TextureWrap, wrap_y: TextureWrap) -> Self {
            self.wrap_x = wrap_x;
            self.wrap_y = wrap_y;
            self
        }

        /// Sets the border color for the texture, used when the wrapping mode is set to
        /// [`TextureWrap::Border`].
        pub fn with_border(mut self, wrap_border: TextureBorder) -> Self {
            self.wrap_border = wrap_border;
            self
        }
    }
}

mod framebuffer {
    use crate::*;

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
        Color(u8),
        /// Depth attachment
        Depth,
        /// Stencil attachment
        Stencil,
    }

    /// The layout of a framebuffer, used for creating a framebuffer with the desired
    /// specifications.
    #[derive(Debug, Clone, Copy)]
    pub struct FramebufferLayout<'a> {
        /// The width of the framebuffer in pixels.
        pub width: u32,
        /// The height of the framebuffer in pixels.
        pub height: u32,

        /// The format of the color attachment, if any.
        pub color: &'a [TextureFormat],
        /// The format of the depth attachment, if any.
        pub depth: Option<DepthFormat>,
        /// The format of the stencil attachment, if any.
        pub stencil: Option<StencilFormat>,

        /// The number of samples for MSAA, 0 if MSAA is not used.
        pub msaa_samples: u32,

        /// Whether the framebuffer is expected to be used as a persistent render target (i.e.
        /// contents are preserved across frames and can be drawn to multiple times).
        pub is_persistent: bool,
        /// Whether any color attachment is expected to be used as a texture that is sampled from.
        pub is_color_bindable: bool,
        /// Whether the depth attachment is expected to be used as a texture that is sampled from.
        pub is_depth_bindable: bool,
    }

    impl DepthFormat {
        /// Returns the number of bytes per pixel for this texture format.
        pub const fn bytes_per_pixel(&self) -> u32 {
            match self {
                DepthFormat::D24 => 3,
                DepthFormat::D32F => 4,
            }
        }
    }

    impl StencilFormat {
        /// Returns the number of bytes per pixel for this texture format.
        pub const fn bytes_per_pixel(&self) -> u32 {
            match self {
                StencilFormat::S8 => 1,
            }
        }
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
        /// GLSL shader format of a specified GLSL version (120, 330, etc.)
        Glsl(u32),

        /// GLSL ES shader format of a specified GLSL ES version (100, 300, etc.)
        Gles(u32),

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

    /// The GLSL/GLSL ES shader representation.
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
    use crate::*;

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

        /// The color outputs of this pipeline.
        pub color_outputs: &'a [TextureFormat],

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
        /// Saturating increment (clamp to max value)   
        IncrementClamp,
        /// Saturating decrement (clamp to min value)
        DecrementClamp,
        /// Wrapping increment (wrap to min value when exceeding max value)
        IncrementWrap,
        /// Wrapping decrement (wrap to max value when exceeding min value)
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
    /// ```ignore
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

    impl BlendMode {
        /// Opaque blending mode (overwrite)
        pub const OVERWRITE: Self = Self {
            color_src: BlendFactor::One,
            color_dst: BlendFactor::Zero,
            color_op: BlendOp::Add,

            alpha_src: BlendFactor::One,
            alpha_dst: BlendFactor::Zero,
            alpha_op: BlendOp::Add,
        };

        /// Discard blending mode
        pub const DISCARD: Self = Self {
            color_src: BlendFactor::Zero,
            color_dst: BlendFactor::One,
            color_op: BlendOp::Add,

            alpha_src: BlendFactor::Zero,
            alpha_dst: BlendFactor::One,
            alpha_op: BlendOp::Add,
        };

        /// Additive blending mode
        pub const ADDITIVE: Self = Self {
            color_src: BlendFactor::One,
            color_dst: BlendFactor::One,
            color_op: BlendOp::Add,

            alpha_src: BlendFactor::One,
            alpha_dst: BlendFactor::One,
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

    impl<'a> PipelineLayout<'a> {
        /// Creates a new pipeline layout.
        pub fn new(shader: Shader<'a>) -> Self {
            Self {
                shader,
                color_outputs: &[],
                color_blend: BlendMode::OVERWRITE,
                depth_test: CompareFn::Always,
                depth_write: false,
                stencil_ccw: StencilFace::default(),
                stencil_cw: StencilFace::default(),
                cull_ccw: false,
                cull_cw: false,
                topology: PrimitiveTopology::TriangleList,
            }
        }

        /// Sets the color output formats for this pipeline.
        pub fn with_color_outputs(mut self, color_outputs: &'a [TextureFormat]) -> Self {
            self.color_outputs = color_outputs;
            self
        }

        /// Sets the color blending mode for this pipeline. By default, no blending is performed.
        pub fn with_color_blend(mut self, blend: BlendMode) -> Self {
            self.color_blend = blend;
            self
        }

        /// Sets the depth test & write mask for this pipeline. By default, depth testing is
        /// disabled and depth writing is disabled as well.
        pub fn with_depth(mut self, compare: CompareFn, write: bool) -> Self {
            self.depth_test = compare;
            self.depth_write = write;
            self
        }

        /// Sets the stencil test and operations for this pipeline. By default, stencil
        /// testing is disabled.
        pub fn with_stencil(mut self, stencil_cw: StencilFace, stencil_ccw: StencilFace) -> Self {
            self.stencil_cw = stencil_cw;
            self.stencil_ccw = stencil_ccw;
            self
        }

        /// Sets the culling for each face winding. By default, culling is disabled.
        pub fn with_culling(mut self, cull_ccw: bool, cull_cw: bool) -> Self {
            self.cull_ccw = cull_ccw;
            self.cull_cw = cull_cw;
            self
        }

        /// Sets the primitives topology for this pipeline. The default topology is
        /// [`PrimitiveTopology::TriangleList`].
        pub fn with_topology(mut self, topology: PrimitiveTopology) -> Self {
            self.topology = topology;
            self
        }
    }
}

mod draw {
    use crate::*;

    /// A query request type.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum QueryType {
        /// Query the time elapsed between the start and the end of the query, in nanoseconds.
        Elapsed,

        /// Query the number of primitives (triangles) generated.
        Primitives,

        /// Query if any samples passed the depth and stencil tests.
        Occlusion(OcclusionTest),
    }

    /// The type of occlusion query to perform.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum OcclusionTest {
        /// Query the exact number of samples that passed the depth and stencil tests. This is the
        /// most accurate, but also the most expensive occlusion query type.
        Samples,
        /// Query if _any_ samples passed the depth and stencil tests (returns 0 if no samples
        /// passed).
        Exact,
        /// Query if _any_ samples passed the depth and stencil tests (returns 0 if no samples
        /// passed). Fastest, but may return false positives (it may return non-zero even if no
        /// samples passed) due to the conservative nature of the query.
        Conservative,
    }

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
        /// The pipeline to use for this draw call.
        pub pipeline: &'a C::Pipeline,

        /// The framebuffer to draw onto.
        pub target: &'a C::Framebuffer,

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
            offset: u64,
            /// The size of the bound region in bytes.
            size: u64,
        },
    }

    impl<'a, C: Context> ClearRequest<'a, C> {
        /// Creates a new clear request with the specified target framebuffer and clear parameters.
        pub fn new(target: &'a C::Framebuffer) -> Self {
            Self {
                target,
                scissor: None,
                color: None,
                depth: None,
                stencil: None,
            }
        }

        /// Sets the scissor rectangle for this clear request. Set to `None` by default.
        pub fn with_scissor(mut self, scissor: TextureBounds) -> Self {
            self.scissor = Some(scissor);
            self
        }

        /// Sets the clear color for this clear request. Set to `None` by default.
        pub fn with_color(mut self, color: [f32; 4]) -> Self {
            self.color = Some(color);
            self
        }

        /// Sets the clear depth for this clear request. Set to `None` by default.
        pub fn with_depth(mut self, depth: f32) -> Self {
            self.depth = Some(depth);
            self
        }

        /// Sets the clear stencil value for this clear request. Set to `None` by default.
        pub fn with_stencil(mut self, stencil: u8) -> Self {
            self.stencil = Some(stencil);
            self
        }
    }

    impl<'a, C: Context> DrawRequest<'a, C> {
        /// Creates a new draw request with the specified parameters.
        pub fn new(target: &'a C::Framebuffer, pipeline: &'a C::Pipeline) -> Self {
            Self {
                pipeline,
                target,
                bindings: &[],
                vertices: 0,
                scissor: None,
                viewport: TextureBounds {
                    x: 0,
                    y: 0,
                    width: 0,
                    height: 0,
                },
            }
        }

        /// Sets the bindings for this draw request.
        pub fn with_bindings(mut self, bindings: &'a [BindingData<'a, C>]) -> Self {
            self.bindings = bindings;
            self
        }

        /// Sets the viewport for this draw request.
        pub fn with_viewport(mut self, viewport: TextureBounds) -> Self {
            self.viewport = viewport;
            self
        }

        /// Sets the scissor rectangle for this draw request. Set to `None` by default.
        pub fn with_scissor(mut self, scissor: TextureBounds) -> Self {
            self.scissor = Some(scissor);
            self
        }

        /// Sets the number of vertices to dispatch for this draw request.
        pub fn with_vertices(mut self, vertices: u32) -> Self {
            self.vertices = vertices;
            self
        }
    }
}
