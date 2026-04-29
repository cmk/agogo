//! Proptest strategies for sample-rate values.
//!
//! Gated on `cfg(any(test, feature = "testkit"))` so downstream
//! crates can pull these strategies into their own proptest blocks
//! without forcing the rest of the workspace to compile proptest.

use proptest::prelude::*;

/// Sample rate strategy: standard audio rates only. (`u32` so it can
/// be used by callers that pick a rate type at the callsite; the
/// typed variants `S044` / `S048` / … expose the same values via
/// [`SampleRate::HZ`](super::SampleRate::HZ).)
pub fn arb_sample_rate() -> impl Strategy<Value = u32> {
    prop_oneof![
        Just(44_100u32),
        Just(48_000u32),
        Just(96_000u32),
        Just(192_000u32),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    proptest! {
        #[test]
        fn arb_sample_rate_is_standard(sr in arb_sample_rate()) {
            prop_assert!(matches!(sr, 44_100 | 48_000 | 96_000 | 192_000));
        }
    }
}
