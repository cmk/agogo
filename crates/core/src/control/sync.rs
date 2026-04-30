//! Audio-clock synchronization primitives.
//!
//! Pure DSP: caller-provided `&[f32]` sample blocks in, detected pulse
//! positions and a smoothed BPM/phase estimate out. No audio I/O, no
//! `Tick` conversion (Sprint 3 integration work).
//!
//! Submodules:
//! - [`detect`] — peak detector with parabolic sub-sample interpolation.
//! - [`pll`]    — Type-II (PI) second-order phase-locked loop.
//! - [`pulse`]  — synthetic Hann-bell signal generator for tests
//!                and `cli/sync_trace` (was `pulse_train.rs` before
//!                Plan 2026-04-29-01 T6).
//! - [`source`] — `PhaseSource` enum unifying internal-clock vs.
//!                external-PLL phase queries.

pub mod detect;
pub mod pll;
pub mod pulse;
pub mod source;

pub use detect::{DetectorConfig, Peak, PeakDetector};
pub use pll::{Pll, PllOutput, PllSettings, PllState};
pub use source::{PhaseSource, PhaseSourceImpl};
