//! layer: time
//! depends-on: conn
//!
//! Recologic grid-and-tick algebra: pure, tempo-independent.
//!
//! Port of `Control.Recologic.Type.Time` from the sibling `recologic`
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
//! - Time conversion helpers: `TICKTIME`, `TIMETIME`, `GRIDGRID`,
//!   plus `Tick` ↔ sample frames.
//! - `Tick` (960 PPQN master counter) and `Time::At { beats, base }`
//!   / `Time::End`.
//! - Integer-valued swing on a binary resolution with alignment
//!   helpers.
//! - Linear and Hermite-smoothstep envelopes.
//!
//! No I/O, no audio. Tick-to-sample scheduling uses fixed `PPQN`,
//! runtime `Tempo`, and rate-specific helpers in [`crate::time::conn`]
//! that dispatch through the static sample-rate connection types.

pub mod conn;
pub mod envelope;
pub mod grid;
pub mod swing;
pub mod tbase;
pub mod tick;

#[cfg(any(test, feature = "testkit"))]
pub mod arb;
