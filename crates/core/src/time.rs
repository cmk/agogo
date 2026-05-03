//! layer: time
//! depends-on: conn
//!
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
//! - Four Galois connections built on `connections::Conn<A, B>`:
//!   `ticks`, `rat_tick`, `time`, `grid`.
//! - `Tick` (960 PPQN master counter) and `Time::At { beats, base }`
//!   / `Time::End`.
//! - Integer-valued swing on a binary resolution with alignment
//!   helpers.
//! - Linear and Hermite-smoothstep envelopes.
//!
//! No I/O, no audio. `Tick ↔ Samples` is
//! [`crate::time::conn::SampleTickConn`] — a Conn-shaped struct that
//! captures runtime `(sr, bpm, ppqn)`. Plan 2026-04-29-01 T3 merged
//! it in from the deleted `sync/sample_tick.rs`; the "no tempo
//! coupling" prose convention was relaxed when `Tempo` itself moved
//! to `conn::tempo` and the layering rule began enforcing the
//! partial order more strictly.

pub mod conn;
pub mod envelope;
pub mod grid;
pub mod swing;
pub mod tbase;
pub mod tick;

#[cfg(any(test, feature = "testkit"))]
pub mod arb;
