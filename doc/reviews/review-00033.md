# PR #33 — Bump connections to HEAD + migrate import paths and Conn names

## Summary

Advances the `connections` pin from `6c888626` to HEAD (`33ec628`) and
migrates all agogo code that broke under the intervening breaking API changes.

**Breaking changes addressed:**

1. **Namespace flatten** — `conn::float` and `conn::std` were promoted to
   top-level `float` and `int` modules. Three production import sites updated
   in `fxp.rs` and `time/decimal.rs`.

2. **Module rename** — `property` → `prop`, with `laws` split into `prop::conn`
   (Galois-connection predicates) and `prop::lattice` (lattice predicates).
   Five test import sites updated in `decimal.rs` and `sample.rs`. Import
   aliased `as laws` to keep all `laws::conn_*` call sites unchanged — the
   predicate renames (`cast_*` → `conn_*`) had already been applied in a
   prior sprint.

3. **Local Conn name conformance** — grep audit found two sub-8-char names in
   `midi.rs`: `U7U8` → `U007U008` and `U4U8` → `U004U008`, conforming to the
   upstream 8-char zero-padded convention.

All 943 tests pass on the new rev; clippy and `check-floats.sh` are clean.
