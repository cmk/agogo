# PR #11 — Plan 11: boundary sweep + grep gate + rules

## Summary

Completes Plan 11 (post-fxp enforcement continuation of Plan 10 /
PR #9). PR #10 landed the plan doc + upstream rev bump as the
sprint opener; this PR ships the actual migrations + the CI gate
+ the codified rules, on top of that base.

### What ships

- **T2 — `Channel` state flips to `Micro`.** `shift_ms: f32` /
  `offset_ms: f32` / `MAX_SHIFT_MS: f32` become `shift: Micro` /
  `offset: Micro` / `MAX_SHIFT: Micro` (300 000 µs). Both
  `transform` and `scheduler` route through a shared
  `micro_to_samples(Micro, sr) → i64` helper that composes
  upstream `F12F06` (Micro → Pico, exact embed) with
  `PicoSampleConn::ceil` (Pico → Q48.16 samples, lawful Galois)
  and rounds to whole samples. The `ms × sr_f / 1000.0` formulae
  are gone.
- **T3/T4 — CLI argv f64; `ProbeRow` drops stored floats.** All
  bpaf float args are f64 now (`sync trace --bpm`, `--jitter-us`,
  `channel trace --shift-ms`, `--offset-ms`). Each handler
  converts on the first line via a named upstream Conn or helper
  (`f64_bpm_to_tempo`, `F64F06.ceil(ExtendedFloat::Finite(...))`,
  `F64F12.ceil(...)`). The three now-unused `parse_*_f32` helpers
  are deleted. `link_probe::ProbeRow.tempo_bpm: f64` becomes
  `tempo: Tempo`; `phase: f64` becomes `phase: Phase`. The f64
  conversion moves to `println!` time (display-only, f64 dies in
  the format string).
- **T5 — `LinkClock` surface exposes `Tempo`.** `pub fn new(...,
  initial_bpm: f64, ...)` → `initial_bpm: Tempo`. `pub fn
  tempo(&mut self) -> f64` → `-> Tempo`. The AblLink C++ ABI stays
  `f64` but is contained to two `// Link FFI`-commented lines
  inside the impl.
- **T7 — `scripts/check-floats.sh` + CI wire.** New bash script
  walks every `crates/*/src/**/*.rs` and flags any unannotated
  `f32` / `f64` outside seven allowlisted modules (PI controller,
  PCM audio, parabolic-fit, fxp helpers, test fixtures, Link FFI,
  CLI argv). Wired to CI as a `floats` job parallel to `test` /
  `clippy` / `deny`, and into the pre-commit hook so local
  commits that violate the rule fail before reaching CI.
- **T8 — CLAUDE.md + review-calibration.md rules.** New bullets
  under Repository conventions: no-stored-float rule with
  glossary (PI, PI-exempt, ABI-local, argv, Link FFI, PCM ABI)
  and enumerated exception list; every-conversion-is-a-Conn rule;
  compose-don't-hardcode rule with concrete good/bad examples.
  Patterns 9 / 10 / 11 appended to `review-calibration.md` in the
  existing diff/comment/why format.

### Why the stack is shaped this way

T2 is the structural change; T3/T4 consume the new `Channel`
shape; T5 is independent (Link surface); T7 needs the earlier
migrations to actually pass the gate; T8 references the types
T2–T5 introduce. Each commit leaves the repo buildable, green,
and gate-clean on its own.

## Test plan

- [x] `cargo build --workspace` and `cargo build --workspace
  --features link` — both clean.
- [x] `cargo test --workspace --features link` — 235 passed (223
  core + 12 cli/host-link combined); zero failures.
- [x] `cargo test --workspace` (no link feature) — 234 passed.
- [x] `cargo clippy --all-targets --features link -- -D warnings`
  — clean.
- [x] `scripts/check-floats.sh` — OK.
- [x] Negative test on the gate: adding a canary `fn
  _canary(_x: f32)` to a non-allowlisted file produces the
  expected `FAIL` output; restoring the file re-greens it.
- [x] CSV output of `agogo link probe` is character-for-character
  identical to pre-T4 (format preserved; only the intermediate
  type is stricter).

## Local review (2026-04-24)

**Branch:** `plan/2026-04-23-08`
**Commits:** 6 (origin/main..HEAD)
**Reviewer:** Claude (sonnet, independent)

### Findings

**Critical — Finding 2 (must fix):** `crates/host-link/src/link.rs` tests
at lines 295 and 333–336 passed raw `f64` to `LinkClock::new`, which
after T5 takes `Tempo`. My earlier `cargo test --workspace --features
link` report was misleading — `--features link` on the workspace
doesn't activate `rusty-link` on the host-link crate itself;
`cargo test --manifest-path crates/host-link/Cargo.toml --features
rusty-link` produced two E0308 type errors. **Fixed** by converting
both call sites: the spot-check test uses
`Tempo::from_bpm_integer(bpm_u32)` with the integer BPMs [30, 120,
200, 999]; the proptest constructs `Tempo(bpm_mbpm)` directly since
the generator is already µBPM. 10 host-link tests now pass under
`--features rusty-link`.

**Important — Finding 1 (must fix):** `crates/cli/src/main.rs` had
`ScheduleArgs.bpm: f32` (stored, unused — "informational only") and
`ScheduleArgs.swing: f32`. Main.rs is allowlisted for the grep gate
but the stored-state rule applies regardless. **Fixed**: dropped the
unused `bpm` field; promoted `swing` to `f64` (consistent with T3's
argv-f64 convention) and updated `swing_to_config` signature. The
eprintln header drops the bpm mention since there's no data.

**Follow-up — Finding 4 (fixed):** `ms_to_micro` was a bespoke
helper that Pattern 11 specifically flags. Inlined both call sites
with the `F64F06.ceil(ExtendedFloat::Finite(ms × 1e-3))` + saturation
match. Added an inline comment pointing at the rule.

**Follow-up — Finding 3 (doc):** `check-floats.sh` grep can
false-positive on string literals / format-string tokens. Added a
"Known limitation" paragraph to the script's header comment so
future contributors know to check before blaming the gate.

### Build verification (post-fixes)

- `cargo test --workspace` — 234 passed (223 core + 11 cli).
- `cargo test --manifest-path crates/host-link/Cargo.toml --features
  rusty-link` — 10 passed.
- `cargo clippy --all-targets -- -D warnings` — clean.
- `scripts/check-floats.sh` — OK.

<!-- gh-id: 3136471937 -->
### Copilot on [`scripts/check-floats.sh:70`](https://github.com/cmk/agogo/pull/11#discussion_r3136471937) (2026-04-24 08:37 UTC)

`stripped="${line_body##*([[:space:]])}"` does not actually strip leading whitespace (and also relies on extglob), so indented comment lines like `    // ... f32 ...` will be treated as code and can incorrectly fail the gate. Consider replacing this with a whitespace-trim that works in plain bash parameter expansion (or enable extglob + use a correct pattern), so comment-only hits are reliably skipped.
```suggestion
    stripped="${line_body#"${line_body%%[![:space:]]*}"}"
```

<!-- gh-id: 4169102700 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-24 08:37 UTC](https://github.com/cmk/agogo/pull/11#pullrequestreview-4169102700))

## Pull request overview

Implements Plan 11’s “boundary sweep” by migrating remaining time/tempo/phase state from `f32`/`f64` into fixed-point newtypes, adding a CI gate to prevent new stray floats, and documenting the new repository rules that enforce these constraints.

**Changes:**
- Migrate core channel shift/offset config to `Micro` and route sample conversions through `PicoSampleConn` composition.
- Update CLI parsing and Link integration to use `f64` only at argv/FFI boundaries, storing `Tempo`/`Phase` internally.
- Add `scripts/check-floats.sh` plus CI wiring and documentation/rules updates.

### Reviewed changes

Copilot reviewed 11 out of 11 changed files in this pull request and generated 4 comments.

<details>
<summary>Show a summary per file</summary>

| File | Description |
| ---- | ----------- |
| scripts/check-floats.sh | New float-usage gate script intended to prevent unallowlisted `f32`/`f64` in Rust sources. |
| .github/workflows/ci.yml | Adds a `floats` job that runs the float gate in CI. |
| .claude/settings.json | Adds float gate to the local commit hook command chain. |
| crates/core/src/channel/time.rs | Switches channel config to `Micro`, introduces `micro_to_samples`, updates transform logic and tests. |
| crates/core/src/control/event.rs | Uses `Micro` shift/offset and shares the new `micro_to_samples` conversion path. |
| crates/core/src/channel.rs | Re-exports `MAX_SHIFT` instead of `MAX_SHIFT_MS`. |
| crates/host-link/src/link.rs | Changes `LinkClock` surface to take/return `Tempo` and confines f64 to Link FFI conversion lines; updates tests. |
| crates/cli/src/main.rs | Moves CLI float args to `f64`, converts immediately to fxp types, and removes stored float fields from `ProbeRow`. |
| CLAUDE.md | Documents new no-stored-float / Conn composition rules and exception glossary. |
| doc/reviews/review-calibration.md | Adds Patterns 9–11 to calibrate future reviews against the new float/Conn rules. |
| doc/reviews/review-00011.md | Adds the sprint review record for PR #11. |
</details>






<!-- gh-id: 3136471997 -->
### Copilot on [`CLAUDE.md:128`](https://github.com/cmk/agogo/pull/11#discussion_r3136471997) (2026-04-24 08:37 UTC)

CLAUDE.md says `scripts/check-floats.sh` fails when an `f32`/`f64` “lacks its annotation comment within 2 lines”, but the script currently (1) skips allowlisted files entirely and (2) does not check for annotation comments near a hit. Either update the script to enforce the documented annotation rule, or adjust this description to match the gate’s actual behavior (file allowlist only).
```suggestion
  lives outside these documented exceptions / allowlisted files.
  Use the annotation comments above to mark the permitted cases.
```

<!-- gh-id: 3136472023 -->
### Copilot on [`CLAUDE.md:146`](https://github.com/cmk/agogo/pull/11#discussion_r3136472023) (2026-04-24 08:37 UTC)

The Conn legend example appears inconsistent with current usage: the code composes `psc.ceil(F12F06.inner(micro_value))`, so `F12F06.inner` is being used as `Micro -> Pico`. Here it’s described as “`F12F06` = Pico → Micro”, which is likely backwards / ambiguous and could mislead future call sites. Please clarify the directionality (especially what `inner` maps) in this legend snippet.
```suggestion
  6-char convention. Read the legend in terms of the `Conn` pair,
  but be explicit about call direction: for example, `F12F06`
  connects Pico and Micro, and `F12F06.inner` maps `Micro -> Pico`;
  `F12S48.inner` maps `S48 -> Pico`; `F64F06.inner` maps
  `Micro -> f64-seconds`. The crate-level doc in
  `connections/src/lib.rs` spells out the full legend.

- **Cross-conversions compose existing `Conn`s — they are not
  hardcoded.** If `A → C` is needed and `Conn<A, B>` + `Conn<B, C>`
  already exist, compose the two at the call site:

  ```rust
  // Good: convert Micro -> Pico with F12F06.inner, then Pico -> Sample with
  // PicoSampleConn.
```

<!-- gh-id: 3136472056 -->
### Copilot on [`crates/core/src/channel/time.rs:65`](https://github.com/cmk/agogo/pull/11#discussion_r3136472056) (2026-04-24 08:37 UTC)

`micro_to_samples` is a new helper that encapsulates the Pico↔Sample composition, but CLAUDE.md’s “compose at the call site” rule explicitly lists a `micro_to_samples_at_sr` helper as an example of what *not* to add. Either inline the composition in both call sites, or update the documented rule to allow this specific helper (and explain why it doesn’t conflict with the composition guideline).

<!-- gh-id: 3136509471 -->
#### ↳ cmk ([2026-04-24 08:44 UTC](https://github.com/cmk/agogo/pull/11#discussion_r3136509471))

Real bug, confirmed with a quick bash test: `${line_body##*([[:space:]])}` needs `shopt -s extglob` to actually work as a pattern and silently no-ops otherwise. Switched to the nested-expansion idiom you suggested (`${line_body#"${line_body%%[![:space:]]*}"}`) which works in plain bash. Verified by appending an indented `// f32` comment to a non-allowlisted file — now correctly skipped.

<!-- gh-id: 3136511043 -->
#### ↳ cmk ([2026-04-24 08:44 UTC](https://github.com/cmk/agogo/pull/11#discussion_r3136511043))

Fixed — the rule text said "lacks its annotation comment within 2 lines" but the script is file-allowlist only. Rewrote that paragraph to match the gate's actual behaviour: file-level allowlist is the hard gate; the annotation comments inside allowlisted files are reviewer-oriented markers, not machine-enforced. Pattern 9 in review-calibration.md is the complementary human check that catches stored-state violations the gate misses (like the `ScheduleArgs.bpm: f32` I just fixed in the prior `fix:` commit).

<!-- gh-id: 3136512724 -->
#### ↳ cmk ([2026-04-24 08:44 UTC](https://github.com/cmk/agogo/pull/11#discussion_r3136512724))

Fixed the ambiguity. The legend now says `F12F06: Conn<Pico, Micro>` and explicitly calls out which method goes which way: `ceil(Pico) → Micro` rounds a Pico value to the nearest Micro, `inner(Micro) → Pico` is the exact embed in the reverse direction. Same pattern spelled out for F64F06 and F12S48. The `psc.ceil(F12F06.inner(micro_value))` composition pattern is then unambiguous: `inner` goes Micro → Pico (up-refinement), then `ceil` goes Pico → Sample (down-coarsening).

<!-- gh-id: 3136515345 -->
#### ↳ cmk ([2026-04-24 08:45 UTC](https://github.com/cmk/agogo/pull/11#discussion_r3136515345))

Went with updating the rule rather than inlining. The "bad example" in CLAUDE.md was too broad — it conflated "helper that HIDES a composition" with "any helper that composes." The spirit was about helpers that dress up a single Conn call in a new name, or open-code the arithmetic. `micro_to_samples` doesn't do either: its body shows the full `F12F06.inner + PicoSampleConn::ceil + Q48.16.round().to_num::<i64>()` chain, the name reflects the operation rather than hiding it, and it's called from both `transform` and `scheduler` where inlining would risk drift between the two paths.

Rewrote the rule to draw the line more precisely:
- **Good:** helper that wraps a *visible* composition (name reflects operation, body shows the chain) when used at ≥2 call sites that would otherwise drift. `micro_to_samples` is now explicitly cited as the example.
- **Bad:** helper that hides a single Conn call behind a new name (just renaming). Now shown as `fn micro_to_pico(m: Micro) -> Pico { F12F06.inner(m) }` — nope.
- **Bad:** open-coded arithmetic / bespoke `f64_*_to_*` where a Conn exists. Unchanged.

The `ms_to_micro` helper I inlined earlier in this PR stays inlined — its composition was lighter (single `F64F06.ceil` call + unit-adjustment prefix), and the inline form reads fine.

<!-- gh-id: 3136573799 -->
### Copilot on [`CLAUDE.md:151`](https://github.com/cmk/agogo/pull/11#discussion_r3136573799) (2026-04-24 08:56 UTC)

`F64F06` legend still references `FloatExt(seconds)`, but the codebase uses `ExtendedFloat` (renamed upstream). This is a factual mismatch that could mislead readers; update the example to use `ExtendedFloat::Finite(seconds)` (or the current wrapper type used with `F64F06.ceil`).
```suggestion
    pattern — `.ceil(ExtendedFloat::Finite(seconds)) → Extended<Micro>`.
```

<!-- gh-id: 4169215342 -->
### copilot-pull-request-reviewer[bot] — COMMENTED ([2026-04-24 08:56 UTC](https://github.com/cmk/agogo/pull/11#pullrequestreview-4169215342))

## Pull request overview

Copilot reviewed 11 out of 11 changed files in this pull request and generated 4 comments.





<!-- gh-id: 3136573847 -->
### Copilot on [`scripts/check-floats.sh:75`](https://github.com/cmk/agogo/pull/11#discussion_r3136573847) (2026-04-24 08:56 UTC)

The `case` pattern treats any line whose first non-whitespace character is `*` as a comment. This can create false negatives for real Rust code lines that legitimately start with `*` (e.g., deref or multiplication), letting `f32`/`f64` slip past the gate. Consider narrowing this to only match block-comment continuation lines (e.g., `\* `) rather than any leading `*`.

<!-- gh-id: 3136573870 -->
### Copilot on [`scripts/check-floats.sh:81`](https://github.com/cmk/agogo/pull/11#discussion_r3136573870) (2026-04-24 08:56 UTC)

The grep pattern uses `\b` word boundaries with `grep -E`. `\b` is not portable across grep implementations (notably BSD grep), which could weaken the local pre-commit hook on non-GNU systems. Consider switching to a POSIX-compatible boundary pattern (e.g., `(^|[^[:alnum:]_])(f32|f64)([^[:alnum:]_]|$)`) or explicitly using a grep mode/implementation that guarantees `\b` support.
```suggestion
  done < <(grep -nE '(^|[^[:alnum:]_])(f32|f64)([^[:alnum:]_]|$)' "$file" || true)
```

<!-- gh-id: 3136573894 -->
### Copilot on [`crates/cli/src/main.rs:562`](https://github.com/cmk/agogo/pull/11#discussion_r3136573894) (2026-04-24 08:56 UTC)

In `channel_trace::trace`, out-of-range `--shift-ms` / `--offset-ms` values (finite but outside `Micro`’s representable range) are mapped to `Micro::ZERO` via the `Extended::NegInf | Extended::PosInf` arm. This means extremely large shifts/offsets silently become *no shift/offset* rather than saturating/clamping (e.g., large positive shift should probably become `MAX_SHIFT` after clamping) or producing an error. Consider handling `PosInf`/`NegInf` in a direction-preserving way (or returning a range error) so the CLI doesn’t unexpectedly drop user-provided values to zero.
```suggestion
        // lawful conn. Reject out-of-range values instead of silently
        // mapping them to `Micro::ZERO`, which would drop user intent.
        let parse_micro_ms = |flag: &str, ms: f64| -> Result<Micro, String> {
            match F64F06.ceil(ExtendedFloat::Finite(ms * 1.0e-3)) {
                Extended::Finite(m) => Ok(m),
                Extended::NegInf | Extended::PosInf => {
                    Err(format!("{flag} {ms}ms out of range"))
                }
            }
        };
        let shift = parse_micro_ms("--shift-ms", args.shift_ms)?;
        let offset = parse_micro_ms("--offset-ms", args.offset_ms)?;
```

<!-- gh-id: 3136654139 -->
#### ↳ cmk ([2026-04-24 09:11 UTC](https://github.com/cmk/agogo/pull/11#discussion_r3136654139))

Fixed — `FloatExt` → `ExtendedFloat` (post-upstream rename). Pure typo carried over from an earlier rev of the rule text.

<!-- gh-id: 3136654355 -->
#### ↳ cmk ([2026-04-24 09:11 UTC](https://github.com/cmk/agogo/pull/11#discussion_r3136654355))

Fixed — narrowed the pattern to skip only `* ` (with trailing space, rustfmt-produced doc-block continuation) and `*/`. Bare leading `*` like `*mut_ptr = 0.0_f32;` is now correctly flagged, which a quick positive test confirms.

<!-- gh-id: 3136654621 -->
#### ↳ cmk ([2026-04-24 09:11 UTC](https://github.com/cmk/agogo/pull/11#discussion_r3136654621))

Fixed — switched to `(^|[^[:alnum:]_])(f32|f64)([^[:alnum:]_]|$)` which is POSIX-compatible across BSD and GNU grep. Added an inline comment explaining why. (Also: the POSIX form correctly excludes `_f32` / `_f64` numeric literal suffixes from flagging, because `_` is in `[:alnum:]_`, same as `\b`'s behaviour — so no semantic drift.)

<!-- gh-id: 3136654880 -->
#### ↳ cmk ([2026-04-24 09:11 UTC](https://github.com/cmk/agogo/pull/11#discussion_r3136654880))

Fixed — wrapped the Micro conversion in an `ms_to_micro(flag, ms)` closure that returns `Result<Micro, String>`. Out-of-range values now produce a clear argv error (`"--shift-ms <value> out of range"`) rather than silently collapsing to `Micro::ZERO`. `parse_non_negative_f64` / `parse_finite_f64` at the bpaf layer already reject NaN / ±∞, so the `Extended::PosInf` / `NegInf` path now exclusively represents "finite but outside i64 µs range," which is genuine user error.
