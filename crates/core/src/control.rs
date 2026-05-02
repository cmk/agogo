//! layer: control
//! depends-on: sink, channel, time, conn
//!
//! Control-plane facade for block scheduling, sync/phase tracking,
//! and the transport playhead runtime.
//!
//! Submodules (Plan 2026-04-29-01 T6 reorganised the control-plane):
//! - [`event`] — block-level `tick_stream` event emission (was
//!   `channel/scheduler.rs`).
//! - [`sync`]  — PLL / detector / phase source (was top-level
//!   `sync/` with `pulse_train.rs` renamed to `pulse.rs`).
//! - [`transport`] — N-channel [`Playhead`] runtime and transport
//!   policy/state.

pub mod event;
pub mod sync;
pub mod transport;

pub use event::tick_stream;
pub use sync::{
    DetectorConfig, Peak, PeakDetector, PhaseSource, PhaseSourceImpl, Pll, PllOutput, PllSettings,
    PllState,
};
pub use transport::{Playhead, PlayheadStopHandle, TransportPolicy, TransportState};
