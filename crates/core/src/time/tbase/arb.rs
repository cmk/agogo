//! Proptest strategies for [`TBase`](super::TBase).
//!
//! Gated on `cfg(any(test, feature = "testkit"))` so downstream
//! crates can pull these strategies into their own proptest blocks
//! without forcing the rest of the workspace to compile proptest.

use proptest::prelude::*;

use crate::time::tbase::TBase;

/// Binary subdivision strategy (9 variants). Used wherever a
/// `TBase`-typed value is required — most prominently
/// [`SwingConfig::resolution`](crate::time::swing::SwingConfig).
pub fn arb_tbase() -> impl Strategy<Value = TBase> {
    prop_oneof![
        1 => Just(TBase::T1),
        1 => Just(TBase::T256),
        4 => prop::sample::select(TBase::ALL.as_slice()),
    ]
}
