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

    /// Maximum representable BPM as `f64`: `u32::MAX as f64 / 10⁶`
    /// ≈ 4294.967295. Used as the upper bound for argv parsers
    /// that want to reject "out of range" BPM rather than silently
    /// saturate.
    ///
    /// Computed as a plain `u32 as f64 / 1.0e6` because
    /// `tempo_to_f64_bpm(Tempo(u32::MAX))` returns a much larger
    /// value (the `I064U032.inner` saturating-widen step lifts
    /// `u32::MAX` to `i64::MAX` before the F-ladder inverse, so
    /// the result is `i64::MAX / 10⁶` ≈ 9.22 × 10¹²). The `× 10⁻⁶`
    /// here is a one-off domain-boundary constant, not a
    /// per-input scale shift; documented inline so a future
    /// reviewer doesn't try to "Conn-discipline" it away.
    pub const MAX_BPM_F64: f64 = (u32::MAX as f64) / 1_000_000.0;

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
