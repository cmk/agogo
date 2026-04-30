#![forbid(unsafe_code)]

//! stdio-core adapter support for agogo.
//!
//! This crate starts with agogo-owned real-time control primitives.
//! The actual `stdio_core::driver::StudioMcpServer` impl is kept as
//! the next slice because the sibling stdio-core checkout currently
//! pins a newer Rust toolchain than this workspace. Keeping the RT
//! bridge independent preserves `cargo test --workspace` on agogo's
//! pinned toolchain while giving the adapter a tested core.

pub mod driver;
pub mod rt_bridge;
pub mod snapshot;

pub use driver::{AgogoDriver, AgogoDriverConfig, Tool};
pub use rt_bridge::{
    BridgeError, ControlCommand, ControlProducer, RtControlConsumer, RtParams, spsc,
};
pub use snapshot::{AgogoSnapshot, SnapshotSlot};
