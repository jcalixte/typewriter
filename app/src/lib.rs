//! Application layer — the host-testable orchestration tier.
//!
//! Holds the code that drives the pure domain (`editor`) against the hardware
//! frontier (`hal`) without ever naming esp-idf: currently the panel render
//! engine ([`Panel`], generic over [`hal::Screen`]). The run-loop `Runtime` and
//! its ports (Storage, NetService, Clock, System, FileIndex) land here as the
//! migration proceeds. Depends only on inner layers (`editor`, `display`,
//! `hal`), so it builds and is tested on the host, off the xtensa target.
//!
//! Mirrors the composition/application tier of the C `../typing-machine`
//! reference — the layer that drives the domain through injected interfaces.

mod ports;
mod render;
mod runtime;

pub use ports::*;
pub use render::*;
pub use runtime::{file_stem, Runtime};
