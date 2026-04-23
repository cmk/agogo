//! Audio-clock synchronization primitives.
//!
//! Pure DSP: caller-provided `&[f32]` sample blocks in, detected pulse
//! positions and a smoothed BPM/phase estimate out. No audio I/O, no
//! `Tick` conversion (Sprint 3 integration work).
//!
//! Submodules:
//! - [`detect`] — peak detector with parabolic sub-sample interpolation.
//! - [`pll`]    — Type-II (PI) second-order phase-locked loop.
//! - [`source`] — `PhaseSource` enum unifying internal-clock vs.
//!                external-PLL phase queries.

pub mod detect;
pub mod pll;
pub mod source;
