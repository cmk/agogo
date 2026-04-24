# Review Calibration Examples

These examples demonstrate the review style we want: **cite the contract (docs,
plan, or naming), show how the code violates it, and name the consequence.**
Each example is a real review comment from an independent reviewer on a Rust
codebase. They are injected into the code-reviewer subagent prompt as few-shot
calibration.

---

## Pattern 1: Doc says X, code does Y

> **File:** `crates/foo-api/src/lib.rs`
>
> **Diff context:**
> ```
> +/// Cloneable handle for consumers to receive events.
> +///
> +/// Obtain via [`channel`]. Call [`Handle::subscribe`] to get an independent
> +/// copy that starts receiving from the current moment.
> ```
>
> **Comment:** The `Handle` doc comment says "Cloneable handle", but
> `Handle` is not `Clone` (and `broadcast::Receiver` isn't cloneable).
> Consider rewording to something like "Resubscribable handle" or explicitly
> describing that cloning semantics are provided via `subscribe()`.

**Why this is good:** Cites the exact doc string, names the specific trait that
makes it false, and proposes a concrete fix. No ambiguity.

---

## Pattern 2: Contract vs implementation mismatch

> **File:** `crates/foo-api/src/lib.rs`
>
> **Diff context:**
> ```
> +                let record = parse_entry(name, l1, l2).expect("stored entry must parse");
> +                let state = compute_or_zero(&record, target);
> +                (i as u32, state.position)
> ```
>
> **Comment:** The `predict()` docs say invalid entries are "silently omitted",
> but the implementation uses `compute_or_zero()` and still includes the zero
> result in `states`, which can produce bogus downstream values. Either switch
> to `compute()` and `filter_map` away errors, or explicitly filter out zero
> results before building output (and update the docs accordingly).

**Why this is good:** Quotes the doc contract ("silently omitted"), shows the
code contradicts it (`compute_or_zero` includes zeros), and names the
downstream consequence. Offers two concrete fix paths.

---

## Pattern 3: Duplicated logic with drift risk

> **File:** `crates/foo-sim/src/systems/bridge.rs`
>
> **Diff context:**
> ```
> +            let latency_ms = distance_km / SPEED_OF_LIGHT_KM_S * 1_000.0;
> ```
>
> **Comment:** `SPEED_OF_LIGHT_KM_S` is imported from `foo_api`, but
> `foo_core` now also defines the same constant. Keeping two independent
> copies across crates risks them drifting (and makes it unclear which one is
> canonical). Prefer sourcing this constant from a single crate and
> re-exporting if needed for API ergonomics.

**Why this is good:** Identifies a structural concern (duplicated constant)
and names the specific risk (drift, unclear canonical source). The fix is
actionable and proportionate.

---

## Pattern 4: Semantic bug from wrong variable

> **File:** `crates/foo-core/src/lib.rs`
>
> **Diff context:**
> ```
> +    let p_rx_dbm = tx.power_dbm + g_tx + g_rx - tx.static_loss_db - l_p - l_fs;
> +    let link_up = p_rx_dbm > tx.threshold_dbm;
> ```
>
> **Comment:** `compute_budget` determines `link_up` using `tx.threshold_dbm`,
> but the threshold is defined as a minimum *received* power, so it should be
> compared against the receiver's threshold. As written, `link_up` will be
> wrong whenever TX/RX thresholds differ.

**Why this is good:** Identifies a subtle domain bug (wrong side's threshold),
explains why it's wrong (threshold is a receiver-side concept), and states the
consequence precisely.

---

## Pattern 5: Test assumption that can silently break

> **File:** `crates/foo-sim/src/systems/propagator.rs`
>
> **Diff context:**
> ```
> +            {
> +                prop_assert_eq!(id_a, id_b);
> +                prop_assert_eq!(pos_a, pos_b, "position mismatch for id {}", id_a);
> +                prop_assert_eq!(vel_a, vel_b, "velocity mismatch for id {}", id_a);
> +            }
> ```
>
> **Comment:** This test assumes query iteration order is identical between
> `states_a` and `states_b` and compares them via `zip()`. Iteration order
> is not an API guarantee and can change with layout changes, making the test
> potentially flaky. Prefer collecting into a map keyed by id (or sorting)
> and comparing by key instead of relying on iteration order.

**Why this is good:** Names the implicit assumption (iteration order), cites
the non-guarantee, and explains the failure mode (flaky, not wrong — the
distinction matters). Fix is concrete and proportionate.

---

## Pattern 6: API boundary that panics instead of returning errors

> **File:** `crates/foo-cli/src/lib.rs`
>
> **Diff context:**
> ```
> +pub fn parse_input_file(path: &PathBuf) -> Vec<(String, String, String)> {
> +    let text = fs::read_to_string(path).expect("failed to read file");
> ```
>
> **Comment:** `parse_input_file` panics on read errors
> (`expect("failed to read file")`), which makes the library API hard to
> use safely (and forces callers to crash on missing/permission errors).
> Consider returning `std::io::Result<Vec<...>>` (or a custom error) and
> taking `&Path` instead of `&PathBuf` so callers can handle failures
> gracefully.

**Why this is good:** Identifies a panic at a library boundary (not internal
code), names who's affected (callers can't recover), and bundles a related
ergonomic fix (`&Path` over `&PathBuf`) that belongs with the same change.

---

## Pattern 7: Sign convention error hiding in docs

> **File:** `crates/foo-sim/src/components.rs`
>
> **Diff context:**
> ```
> +/// To test whether a peer at position `p` is not occluded by the body,
> +/// transform the unit vector toward `p` into the body frame and check
> +/// that its Z component is negative (i.e. the peer is in the lower hemisphere).
> ```
>
> **Comment:** The docs define body `+Z = down`, but then say to check
> the body-frame Z component is *negative* to confirm the peer is in the
> lower hemisphere. If `+Z` is down, then the lower hemisphere corresponds
> to a *positive* Z component. Please clarify/correct the sign convention
> so future checks don't get implemented backwards.

**Why this is good:** Catches a doc error that will cause a *future* bug when
someone implements the check by following the docs. The reviewer traces
the sign convention through two definitions and shows the contradiction.

---

## Pattern 8: Inconsistency across systems

> **File:** `crates/foo-sim/src/systems/bridge.rs`
>
> **Diff context:**
> ```
> +                transitions.push(Event::Up { id: fwd_key });
> +            } else if !budget.link_up && was_up {
> +                transitions.push(Event::Down { id: fwd_key });
> +            }
> +            bridge.prev_up.insert(fwd_key, budget.link_up);
> ```
>
> **Comment:** `budget_system` applies hysteresis using `drop_threshold`
> for previously-up links, but `bridge_system` reports `link_up` straight
> from `compute_budget` (acquisition threshold only). This can make bridge
> events disagree with the system's own state in the hysteresis band.
> Consider mirroring the hysteresis logic in both places.

**Why this is good:** Identifies an inconsistency between two systems that
*should* agree but don't. Names the specific parameter space where they
diverge, and explains the observable consequence. This is the kind of
cross-system bug that unit tests per-system would never catch.

---

## Pattern 9: Stored `f32`/`f64` outside the allowed exceptions

> **File:** `crates/core/src/channel/transform.rs`
>
> **Diff context:**
> ```
> +pub const MAX_SHIFT_MS: f32 = 300.0;
> +
> +pub struct Channel {
> +    pub shift_ms: f32,
> +    pub offset_ms: f32,
> ...
> ```
>
> **Comment:** `MAX_SHIFT_MS`, `shift_ms`, and `offset_ms` are stored
> time-valued configuration — they are neither PI-law state nor audio
> sample data, so the `f32` backing violates the repo's no-stray-float
> rule (CLAUDE.md §Repository conventions). Replace with
> `connections::conn::fixed::Micro` (the decimal ladder rung at µs
> resolution); the ladder already provides the arithmetic via
> `F12F06` composed with `PicoSampleConn`. Annotating the `f32` as
> "argv" or "ABI" would also be wrong — this is downstream of the
> CLI parser, stored in a core type.

**Why this is good:** Cites the no-float rule by section, names
which exception is claimed and why it doesn't apply, points at the
ladder-native replacement.

---

## Pattern 10: Hardcoded `A → C` conversion when `A → B → C` Conns exist

> **File:** `crates/core/src/channel/transform.rs`
>
> **Diff context:**
> ```
> +    let sr_f = stc.sr() as f32;
> +    let shift_samples = (shift_ms * sr_f / 1000.0).round() as u64;
> +    let offset_samples = (channel.offset_ms * sr_f / 1000.0).round() as i64;
> ```
>
> **Comment:** This open-codes a Milli/Micro → Sample conversion
> arithmetically. The ladder already has `Conn<Pico, Micro>`
> (`connections::conn::fixed::F12F06`) and a `PicoSampleConn`
> runtime lookalike; compose them at the call site —
> `psc.ceil(F12F06.inner(micro_value))`. The whole point of the
> Galois-connection library is that `A → C` composes from
> `A → B → C` without new code; adding ad-hoc arithmetic here
> bypasses the rounding contracts that the adjoint laws enforce
> and drifts from whatever rounding the composed Conns would have
> delivered.

**Why this is good:** Names the two existing Conns that compose to
the needed conversion, explains what the composition buys
(adjoint-law-correct rounding instead of whatever ad-hoc rounding
the arithmetic happens to produce), and makes the DRY argument
concrete.

---

## Pattern 11: Bespoke float-conversion helper where a `Conn` would do

> **File:** `crates/core/src/fxp.rs`
>
> **Diff context:**
> ```
> +pub fn f64_ms_to_micro(ms: f64) -> Micro {
> +    let us = (ms * 1000.0).round();
> +    Micro(us as i64)
> +}
> ```
>
> **Comment:** `fxp.rs` should not grow a collection of
> `f64_*_to_*` shims for types that already sit on the decimal
> ladder. CLAUDE.md §Repository conventions: every numerical
> conversion must come from a `Conn` with proptested adjoint laws,
> named per the `fXYfZW` convention. Use
> `F64F06.ceil(ExtendedFloat::Finite(ms × 1e-3))` directly —
> `F64F06` is `Conn<ExtendedFloat<f64>, Extended<Micro>>`, lawful
> over the full IEEE domain. The handful of legitimate bespoke
> helpers (`f64_bpm_to_tempo`, `f64_phase_to_phase`) exist only
> because `Tempo` is u32-backed and `Phase` is a wrapping quotient
> onto a torus — neither admits a lawful `Conn` shape. A straight
> `Micro` conversion isn't in that class.

**Why this is good:** Cites the invariant explicitly (named Conns,
not shims), names the upstream `F64F06` that already does this
lawfully, and explains what a Conn provides that a bespoke fn
doesn't (rounding-direction selection, documented adjoint laws).
Calls out the two legitimate exceptions so reviewers can
distinguish "this is a rule violation" from "this is the
documented escape hatch."
