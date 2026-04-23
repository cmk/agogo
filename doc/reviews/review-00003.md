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
