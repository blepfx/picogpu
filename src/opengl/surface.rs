use core::ffi::{CStr, c_void};

use alloc::string::String;

use crate::Error;

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
    fn get_proc_address(&self, name: &CStr) -> *const c_void;
    fn make_current(&self, current: bool) -> Result<(), SurfaceError>;
    fn swap_buffers(&self) -> Result<(), SurfaceError>;
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
