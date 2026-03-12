# picogpu
*picogpu* is a small and lightweight GPU abstraction layer for Rust

## Goals

- Small API surface GPU abstraction layer that contains only necessary features for basic drawing
- Small and lightweight, no heavy and unnecessary dependencies
- Support for multiple backends (Vulkan, OpenGL, Metal)
- Support for multiple platforms (Windows, Linux, macOS)
- Support old hardware (at least OpenGL 3.1+)
- Be as low overhead as possible

## Features

- Static texture management (create, destroy, upload, bind)
- Buffer (uniform/storage) management (copy, invalidate, upload)
- Pipeline management (shader + draw state)
- Framebuffer management
- Draw requests

## License

Licensed under either of <a href="LICENSE-APACHE">Apache License, Version
2.0</a> or <a href="LICENSE-MIT">MIT license</a> at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in `picoview` by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.