//! layer: control
//! depends-on: sink, channel, time, conn
//!
//! Pure control-loop facade for sync/phase tracking.
//!
//! Submodules:
//! - [`detect`] — peak detector with parabolic sub-sample interpolation.
//! - [`pll`]    — Type-II (PI) second-order phase-locked loop.
//! - [`pulse`]  — synthetic Hann-bell signal generator for tests and traces.
//! - [`source`] — `PhaseSource` enum unifying internal-clock vs.
//!   external-PLL phase queries.

pub mod detect;
pub mod pll;
pub mod pulse;
pub mod source;

pub use detect::{DetectorConfig, Peak, PeakDetector};
pub use pll::{Pll, PllOutput, PllSettings, PllState};
pub use source::{PhaseSource, PhaseSourceImpl};
