# PR #3 — CLI parser: clap → bpaf

## Summary

Swap `agogo-cli`'s argument parser from `clap` (derive) to `bpaf`
(derive). Pure refactor — the `agogo sync trace` and
`agogo time schedule` flag surfaces and behaviours are preserved,
every pre-existing test still passes, and clippy stays clean at
`-D warnings`.

### Why now (and why a separate PR)

- **Unblocks Plan 03.** The channel/ sprint (already opened on
  `plan/2026-04-23-01`) plans to extend the CLI with
  `agogo channel trace`; its Task T5 assumes bpaf. Landing the swap
  on main first lets Plan 03 consume bpaf from day one.
- **Single parser style.** Without this PR, the next CLI-extending
  sprint would either have to continue in clap or introduce a mix.
  One canonical parser keeps the surface consistent.

### What changes

- `Cargo.toml`: workspace dep `clap 4 (derive)` → `bpaf 0.9 (derive)`.
- `crates/cli/Cargo.toml`: matching per-crate switch.
- `crates/cli/src/main.rs`:
  - `#[derive(Parser/Subcommand/Args)]` → `#[derive(Bpaf)]` with
    `#[bpaf(options)]` at the top and
    `#[bpaf(command("sync"))] / #[bpaf(command("trace"))] /
    #[bpaf(command("time"))] / #[bpaf(command("schedule"))]` across
    the two subcommand trees.
  - Validators (`parse_positive_f32`, `parse_non_negative_f32`, and
    the new `parse_positive_u32` — replacing clap's built-in
    `value_parser!(u32).range(1..)`) take the parsed primitive and
    return `Result<T, String>`, wired via bpaf's `parse(fn)`
    combinator.
  - `--tbase` uses `argument::<String>("TBASE")` + `parse(parse_tbase)`
    with a three-line `parse_tbase(s: String) -> Result<TBase, String>`
    wrapper around `TBase::from_str` (TBase's `FromStr::Err` is already
    `String`).
  - `Cli::parse()` → `cli().run()`; `Option<Command>` carries
    `#[bpaf(external(command), optional)]` so the bare `agogo`
    invocation keeps its old tag-line behaviour.
- `Cargo.lock`: net reduction of ~8 crates (clap's transitive
  dependency tree vs bpaf's).

### Verification

- 161 `agogo-core` tests pass (1 pre-existing `#[ignore]` carried
  from PR #2, unchanged).
- 9 `agogo-cli` tests pass, including:
  - `sync_trace_converges` — 256 pulses at 120 BPM / 48 kHz / 24 PPQ
    / 50 µs jitter converge to within ±0.05 BPM of 120.
  - `schedule_ticks_*` — five tests covering T16 two-bar count,
    swing offset, T128t 192-per-bar, T1 one-per-bar, swing-to-config
    boundaries.
- `cargo clippy --all-targets -- -D warnings` clean.
- Manual smokes:
  - `cargo run -p agogo-cli -- sync trace --bpm 120 --sr 48000 --ppq 24 --jitter-us 50 --pulses 256 --seed 1`
    → CSV header + 256 rows; final `bpm_estimate` ≈ 120.002.
  - `cargo run -p agogo-cli -- time schedule --bpm 120 --tbase t16 --swing 0.54 --bars 2`
    → 32 tick positions with off-beats shifted by -4.
- Invalid-input smoke: `--bpm 0`, `--sr 0`, bare `agogo` no-args all
  exit as expected (1 for validation errors, 0 for the tag-line).

### Known behaviour deviation

`--bpm -1` used to be accepted by clap as the f32 `-1` and then
rejected by the validator ("must be a positive finite number, got
-1"). Under bpaf, a token starting with `-` is read as a flag, so
the message becomes:

```
Error: `--bpm` requires an argument `BPM`, got a flag `-1`, try
  `--bpm=-1` to use it as an argument
```

Both paths exit status 1. A literal negative value must now be
written `--bpm=-1` — the standard bpaf idiom. Negative values are
invalid for every current numeric flag anyway, so this is a pure
message-wording change on the error path.

### Out of scope

- `--version` top-level flag (bpaf makes this trivial; deferred to
  a follow-up to keep this PR a pure swap).
- Any flag-surface changes for Plan 03 — they live on
  `plan/2026-04-23-01`.

### Test plan

- `cargo test --workspace` — all passing (161 core + 9 CLI + doc).
- `cargo clippy --all-targets -- -D warnings` — clean.
- `cargo run -p agogo-cli -- sync trace --bpm 120 --sr 48000 --ppq 24 --jitter-us 50 --pulses 256 --seed 1`
  prints 256 convergent rows.
- `cargo run -p agogo-cli -- time schedule --bpm 120 --tbase t16 --swing 0.54 --bars 2`
  prints 32 tick positions with swing-shifted off-beats.

## Local review (2026-04-23)

**Branch:** plan/2026-04-23-02
**Commits:** 3 (origin/main..plan/2026-04-23-02)
**Reviewer:** Claude (sonnet, independent)

---

### Commit Hygiene

Three commits, correct prefixes, correct order: `plan:` opens the branch, `feat(cli):` carries the implementation, `doc:` closes. Each commit is atomic for its tier — T0/T1 are bundled in the single `feat` commit, which is appropriate for a pure mechanical swap with no logic changes. The `feat` commit builds on its own because T0 (dep change) and T1 (code rewrite) are inseparable at compile time. No merge commits; history is linear. All good.

---

### Code Quality

**Flag surface fidelity — sync trace**

`--bpm`, `--sr`, `--ppq`, `--jitter-us`, `--pulses`, `--seed` are all present. Validation parity:

- `--bpm`: `argument("BPM") + parse(parse_positive_f32)` where `parse_positive_f32(v: f32)` receives an already-parsed `f32` (bpaf's `parse()` receives `T` from the inner `Parser<T>`). Correct.
- `--sr` / `--ppq` / `--pulses`: `argument("SR") + parse(parse_positive_u32)`. `argument` on a `u32` field infers `argument::<u32>`, so `parse_positive_u32(v: u32)` receives a `u32`. Correct; catches zero.
- `--jitter-us`: `argument("JITTER_US") + parse(parse_non_negative_f32) + fallback(0.0)`. Correct.
- `--seed`: `argument("SEED") + fallback(1)` — no validator, any `u64` is accepted. Old clap behaviour was identical (no range constraint). Correct.

**Flag surface fidelity — time schedule**

- `--bpm`: `argument("BPM")`, no validator. Old clap: `#[arg(long)]` with no validator. Preserved.
- `--tbase`: `argument::<String>("TBASE") + parse(parse_tbase)`. `parse_tbase(s: String) -> Result<TBase, String>` calls `s.parse()`. `TBase`'s `FromStr::Err` is `String`, so this is a zero-copy one-liner. Correct.
- `--swing`: `argument("SWING") + fallback(0.5)`. Old clap: `default_value_t = 0.5`. Preserved.
- `--bars`: `argument("BARS")`. Old clap: `#[arg(long)]` on `u16`. Preserved.

**`ScheduleArgs` external parser indirection**

The plan says "`use time_sched::schedule_args;` at the top lets the enum variant reference the derive-generated external parser by identifier." The derive macro on `pub struct ScheduleArgs` generates a `pub fn schedule_args() -> impl Parser<ScheduleArgs>` function inside `time_sched`. The `use time_sched::schedule_args;` import at `main.rs` line 4 brings it into scope, and `#[bpaf(external(schedule_args))]` at line 59 uses it by name. This is the documented bpaf pattern for composing external parsers into enum variants. Correct.

**`parse_positive_u32` — edge case**

`crates/cli/src/main.rs` lines 78–84:
```rust
fn parse_positive_u32(v: u32) -> Result<u32, String> {
    if v == 0 {
        Err("must be ≥ 1, got 0".to_string())
    } else {
        Ok(v)
    }
}
```
This only checks `v == 0`. Because `v` is already a `u32`, the type guarantees it is non-negative, so the only invalid value is 0. Logic is complete.

**No `unsafe` code.** `#![forbid(unsafe_code)]` is the first line of `main.rs`. The added code stays within safe Rust. Convention met.

**Module layout.** `sync_trace` and `time_sched` remain as inline modules inside `main.rs`, unchanged from before this PR. Not a modern layout concern introduced here.

**Dead code.** No dead code introduced. The `parse_positive_u32` function is used in three field annotations. All helpers are used.

---

### Test Coverage

All nine pre-existing CLI tests (`sync_trace_converges` + five `schedule_ticks_*` + three `swing_to_config_*`) exercise the logic paths; they remain unchanged and pass. As a pure parser-annotation swap with no logic changes, that coverage is sufficient.

One gap worth noting for follow-up: `--tbase` invalid input (e.g. `--tbase garbage`) reaches `parse_tbase` → `TBase::from_str` → `Err(String)`. There is no unit test covering the rejection path of `parse_tbase`. This is not a blocker for a pure swap (the old clap code had the same gap: `value_parser = str::parse::<TBase>` was not tested for rejection either), but a spot-check test of `parse_tbase("garbage")` returning `Err` would be a cheap addition.

---

### Plan Conformance

- **T0** (workspace dep swap): Done — `Cargo.toml`, `crates/cli/Cargo.toml`, `Cargo.lock` all updated.
- **T1** (rewrite `main.rs`): Done — full derive swap covering both subcommand trees.
- **T2** (verification): The plan's build-gate language is future-tense ("must pass"), which is appropriate for a plan doc; the review file and PR summary confirm all gates were exercised.

Four deviations are documented in the Review section:
1. `--bpm -1` error message change — accurate; bpaf's flag-vs-argument disambiguation causes this.
2. `parse_positive_u32` addition — accurate; Deviations #1 and #2 are correctly characterised.
3. Program name in `--help` is `agogo-cli` not `agogo` — accurate.
4. Scope widened by rebase — accurate; the plan body was updated to cover both subcommand trees.

No undocumented deviations found.

---

### Risks

**TODOs / stubs:** None.

**Scripts that parse help output:** `README.md` contains only a CI badge; no help-output scraping. The `scripts/` directory contains workflow tooling (review path, PR number extraction, autosquash) — none parse CLI help text. No breakage risk.

**Security:** No new network surface, no file I/O, no new parsing of untrusted input beyond what existed under clap. The bpaf dependency tree is smaller than clap's, reducing supply-chain exposure. No new risk.

**bpaf 0.9 / bpaf_derive 0.5 version choice:** The workspace pins `version = "0.9"` (SemVer minor range). `bpaf` 0.9 has been the stable release line since 2023; 0.9.25 is the locked version. `bpaf_derive` 0.5.23 is the matching proc-macro crate and is pinned in `Cargo.lock`. No concerns.

**Error message regression on `--sr 0` is undocumented:**

`crates/cli/src/main.rs`, lines 78–84.

Under clap, `value_parser!(u32).range(1..)` produced a message like:
```
error: invalid value '0' for '--sr <SR>': 0 is not in 1..
```
Under bpaf, `argument("SR") + parse(parse_positive_u32)` produces the custom message:
```
must be ≥ 1, got 0
```
The plan documents the `--bpm -1` error path change as Deviation #1, but it does not document the `--sr 0` / `--ppq 0` / `--pulses 0` error wording change. The plan's Verification section explicitly lists `--sr 0` as a negative-path smoke, saying it should "exit with a non-zero status and a clear message (matching the pre-swap clap behaviour, modulo wording)." The parenthetical "modulo wording" gives room — the exit code is still 1 and the message is clear — but the plan's Review section only calls out the `--bpm -1` path, leaving the range-validator wording change undocumented. This should either be absorbed into Deviation #2 or noted as an additional deviation. Not a blocker, but the documentation is incomplete.

---

### Recommendations

**Must fix before push:**

None. The swap is mechanically correct, flag surfaces are preserved, validators are wired correctly, all pre-existing tests pass, and the plan's four deviations are accurately described.

**Follow-up (future work):**

1. Absorb the `--sr 0` / `--ppq 0` / `--pulses 0` error wording change into the plan's Review section (either as a note under Deviation #2, or as a new Deviation #5). The plan already acknowledges "modulo wording" for this smoke path; making it explicit keeps the audit trail complete. Bundle with the next plan branch per project convention.
2. Add a one-line unit test for `parse_tbase("garbage")` returning `Err`. The old clap path had the same gap; closing it is cheap and has no urgency.
3. Consider `#[bpaf(version)]` on `Cli` (already captured as a plan recommendation; no action needed here).
