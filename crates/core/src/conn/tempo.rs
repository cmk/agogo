//! `Tempo` — BPM × 10⁶ stored as u32.
//!
//! Range 0..≈4295 BPM (plenty for music). Resolution 10⁻⁶ BPM, well
//! below any human perceptual threshold.
//!
//! Moved here from `crate::fxp` (Plan 2026-04-28-03 T4): `Tempo` is a
//! musical-time noun (BPM is a rate over time), pairs naturally with
//! `Tick` / `Time` / `Sxxx`. The `time/` module's "no tempo coupling"
//! invariant is about *operations*; defining the *type* here is fine.

/// Beats per minute × 10⁶.
#[repr(transparent)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Tempo(pub u32);

impl Tempo {
    pub const ZERO: Self = Self(0);

    /// Construct from an integer BPM. Panics if `n > 4294` (`n × 10⁶`
    /// overflows `u32`). `checked_mul` avoids the silent release-build
    /// wrap that plain `n * 1_000_000` would produce.
    pub const fn from_bpm_integer(n: u32) -> Self {
        match n.checked_mul(1_000_000) {
            Some(v) => Self(v),
            None => panic!("Tempo::from_bpm_integer: n must be ≤ 4294"),
        }
    }

    /// `|self - other|` as `u32` via `u32::abs_diff` — no
    /// sign-flipping through i64. Replaces five open-coded
    /// `(a.0 as i64 - b.0 as i64).unsigned_abs()` sites in
    /// `sync::pll`.
    pub const fn abs_diff(self, other: Tempo) -> u32 {
        self.0.abs_diff(other.0)
    }
}

#[cfg(any(test, feature = "testkit"))]
pub mod arb;
