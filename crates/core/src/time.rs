//! Cirklon grid-and-tick algebra: pure, tempo-independent.
//!
//! Port of `Control.Cirklon.Type.Time` from the sibling `recologic`
//! Haskell client, extended at v0.2 to a 36-element bounded
//! distributive Heyting lattice. The module delivers:
//!
//! - `TBase` — 9-variant binary subdivision axis (T1, T2, …, T256).
//!   Used wherever a binary grid is required specifically (swing
//!   resolution, the `n` field of `Grid`).
//! - `Grid` — the full 36-element lattice as a product
//!   `{ n: TBase, t: bool, q: bool }`, with named consts (`T16`,
//!   `T16T`, `T8Q`, `T2P`, …) and component-wise meet / join /
//!   Heyting implication.
//! - Five Galois connections built on `connections::Conn<A, B>`:
//!   `quantize_at(g: Grid)`, `ticks`, `rat_tick`, `time`, `grid`.
//! - `Tick` (960 PPQN master counter) and `Time { beats, base: Grid }`.
//! - Integer-valued swing on a binary resolution with alignment
//!   helpers.
//! - Linear and Hermite-smoothstep envelopes.
//!
//! No I/O, no audio, no tempo coupling. `Tick ↔ Samples` is
//! `crate::sync::sample_tick::SampleTickConn` (it reads `Tempo`, so
//! it lives under `sync`, not `time`).

pub mod conn;
pub mod decimal;
pub mod envelope;
pub mod float;
pub mod grid;
pub mod sample;
pub mod swing;
pub mod tbase;
pub mod tick;

#[cfg(test)]
pub mod arb;
