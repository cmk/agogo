//! Cirklon grid-and-tick algebra: pure, tempo-independent.
//!
//! Port of `Control.Cirklon.Type.Time` from the sibling `recologic`
//! Haskell client. The module delivers:
//!
//! - `TBase` — the 14-constructor musical time-base enum, ordered by
//!   divisibility of tick counts at 192 PPQN.
//! - Lattice / biheyting operations on `TBase`.
//! - Five Galois connections built on `connections::Conn<A, B>`:
//!   `quantize_at`, `ticks`, `rat_tick`, `time`, `tbase`.
//! - `Tick` (192 PPQN master counter) and `Time { beats, base }`.
//! - Integer-valued swing with alignment helpers.
//! - Linear and Hermite-smoothstep envelopes.
//!
//! No I/O, no audio, no tempo coupling. `Tick ↔ Samples` is deferred
//! to a later integration sprint.

pub mod conn;
pub mod envelope;
pub mod swing;
pub mod tbase;
pub mod tick;
