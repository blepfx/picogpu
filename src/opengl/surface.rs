use crate::Error;
use alloc::string::String;
use core::ffi::{CStr, c_void};

/// A trait representing an OpenGL surface. This must be implemented by the user of the OpenGL
/// backend/window provider, etc.
///
/// # Safety
///
/// Implementors must ensure that the provided OpenGL context is valid:
/// - get_proc_address must return valid function pointers for all OpenGL functions used by the
///   backend, or a `null`-like pointer for unsupported functions.
/// - make_current must correctly set the current OpenGL context for the calling thread
pub unsafe trait Surface {
    /// Get the address of an OpenGL function by name. The returned pointer must be valid for all
    /// OpenGL functions used by the backend, or a `null`-like pointer for unsupported functions.
    fn get_proc_address(&self, name: &CStr) -> *const c_void;

    /// Swap the front and back buffers of the surface, presenting the rendered image to the screen.
    fn swap_buffers(&self) -> Result<(), SurfaceError>;

    /// Make the OpenGL context associated with this surface current on the calling thread.
    /// This must ensure that subsequent OpenGL calls on the calling thread will affect this
    /// surface's context.
    ///
    /// The implementation should check if the context is already current and do nothing in
    /// that case (for better performance).
    fn make_current(&self) -> Result<(), SurfaceError>;
}

/// An error that has occurred in the OpenGL surface, such as an invalid context, lost context, etc.
#[derive(Debug)]
pub enum SurfaceError {
    /// The provided surface does not correspond to a valid OpenGL context, or the context has been
    /// lost, etc.
    InvalidContext,
    /// Internal error occurred
    Internal(String),
}

impl core::error::Error for SurfaceError {}
impl core::fmt::Display for SurfaceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SurfaceError::InvalidContext => write!(f, "invalid opengl context"),
            SurfaceError::Internal(msg) => write!(f, "{}", msg),
        }
    }
}

impl From<SurfaceError> for Error {
    fn from(value: SurfaceError) -> Self {
        match value {
            SurfaceError::InvalidContext => Error::InvalidContext,
            SurfaceError::Internal(msg) => Error::Internal(msg),
        }
    }
}
