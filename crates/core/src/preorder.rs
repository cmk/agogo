//! Local preorder trait — vendored from `connections @ d1ac1ead`'s
//! `lattice::Ple`, removed upstream by `connections @ 9aa426b`.
//!
//! Upstream's rationale for removal: the lawful framework now
//! consumes `Eq + PartialOrd` directly, and `Ple` was a redundant
//! single-method trait whose impls were mostly trivial (`<=`
//! delegations).
//!
//! agogo keeps `Ple` because its load-bearing impl — `Grid`'s
//! divisibility preorder — is **not** the natural order on its
//! component fields. Two `Grid` values can compare as `Equal` under
//! divisibility yet differ structurally; the `Ple` impl in
//! [`crate::time::grid`] expresses the divisibility relation
//! directly, and `PartialOrd` for `Grid` is then derived from `Ple`
//! (not the other way around). Replacing those 73 call-site
//! `.ple(&x)` invocations with `<=` would either lose that semantic
//! distinction or require a wrapper-newtype-per-comparison shim.
//!
//! `U7` and `U4` (in [`crate::midi`]) get a thin `Ple` impl that's
//! just `<=` on the inner `u8` — it exists for trait-bound
//! uniformity in the property-test laws, not because the
//! divisibility/order distinction matters for those types.

/// Reflexive, transitive preorder. `a.ple(&b)` reads "a is below b".
///
/// Distinct from `PartialOrd::le` because some agogo types (notably
/// [`crate::time::grid::Grid`]) define a custom preorder that does
/// *not* coincide with the natural order on their underlying fields.
pub trait Ple {
    fn ple(&self, other: &Self) -> bool;
}
