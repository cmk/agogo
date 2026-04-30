//! Galois-connection layer — value types and the lawful conversions
//! between numeric realms.
//!
//! Members:
//!
//! - [`fixed`] — agogo's decimal fixed-point ladder
//!   (`Pico`, `Micro`, `FD06`, `FD12`, …) and the Galois conns
//!   between adjacent resolutions.
//! - [`float`] — `F064FDxx` Conns (vendored from `connections`)
//!   that bridge `f64` ↔ fxp at the SI-unit boundary.
//! - [`sample`] — `SampleRate` ladder and the `FD12Sxxx` Conns
//!   between Pico and per-rate sample counts.
//! - [`midi`] — MIDI domain newtypes (`U7`, `U4`) and their
//!   `Cast 'L`-shaped Conns into `u8`.
//! - [`tempo`] — `Tempo` (u32 µBPM wrapper). Lives at conn rather
//!   than time because it is a value type over a fixed-point
//!   representation, not a temporal coordinate.
//! - [`phase`] — `Phase` (Q0.32 wrapping torus quotient). Same
//!   shape rationale as `Tempo`.
//! - [`boundary`] — argv parsers, FFI parity helpers, and the
//!   PI-controller f64 ↔ fxp seam. All sites that knowingly
//!   touch `f64` route through here so the rest of the workspace
//!   stays float-free.

pub mod boundary;
pub mod fixed;
pub mod float;
pub mod midi;
pub mod phase;
pub mod sample;
pub mod tempo;

#[cfg(any(test, feature = "testkit"))]
pub mod arb;
