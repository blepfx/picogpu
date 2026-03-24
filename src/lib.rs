#![doc = include_str!("../README.md")]
//#![warn(missing_docs, missing_debug_implementations)]
//#![deny(clippy::unwrap_used, clippy::expect_used)]
#![no_std]

extern crate alloc;

mod context;
#[cfg(feature = "opengl")]
pub mod opengl;
// #[cfg(feature = "vulkan")]
// pub mod vulkan;

pub use context::*;
