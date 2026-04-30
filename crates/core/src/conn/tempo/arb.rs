//! Proptest strategies for [`Tempo`](super::Tempo).
//!
//! Gated on `cfg(any(test, feature = "testkit"))` so downstream
//! crates can pull these strategies into their own proptest blocks
//! without forcing the rest of the workspace to compile proptest.

use proptest::prelude::*;

use crate::conn::tempo::Tempo;

/// BPM strategy as `Tempo` (BPM × 10⁶). Biased toward common
/// musical tempos with some boundary spice.
pub fn arb_bpm() -> impl Strategy<Value = Tempo> {
    prop_oneof![
        1 => Just(Tempo::from_bpm_integer(60)),
        1 => Just(Tempo::from_bpm_integer(120)),
        1 => Just(Tempo::from_bpm_integer(200)),
        5 => (60_000_000u32..200_000_000).prop_map(Tempo),
        1 => (30_000_000u32..400_000_000).prop_map(Tempo),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    proptest! {
        #[test]
        fn arb_bpm_in_range(bpm in arb_bpm()) {
            prop_assert!((30_000_000..=400_000_000).contains(&bpm.0));
        }
    }
}
