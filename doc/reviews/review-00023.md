# PR #23 — `U7` / `U4` newtypes for MIDI domain types (audit P1)

## Summary

Lifts MIDI 7-bit / 4-bit field validation from runtime parser checks
into compile-time impossibility, by introducing two newtypes in
`crates/core/src/conn/midi.rs`:

- `U7(pub u8)` — `0..=127`. Covers MIDI note numbers, velocities,
  CC values, and channel-pressure bytes.
- `U4(pub u8)` — `0..=15`. Covers zero-based MIDI channel numbers
  (user-facing `mch=1..=16` is mapped at the spec parser).

Both newtypes ship with checked constructors (`U7::new(u8) ->
Option<Self>`, same for `U4`) and saturating Galois connections
`U7U8: Conn<U7, u8>` / `U4U8: Conn<U4, u8>` adapted from the Haskell
`Cast 'L` `conn` pattern in `Data.Connection.Word`:

- `ceil: Narrow → u8` is the exact embedding (no rounding needed —
  the narrow domain is a subset of `u8`).
- `inner: u8 → Narrow` saturates to `Narrow::MAX`.
- `Conn::new_left` sets `floor = ceil`, matching the one-sided
  `'L` shape from Haskell.

Migrated `MidiClickConfig`, `MidiClickAccent`, and
`ChannelMode::MidiCc` to use the new types instead of raw `u8`.
The parser collapses six inline range-check blocks (`if n > 127`,
`if !(1..=16).contains(&n)`) into one-line `U7::new(n).ok_or(...)` /
`U4::new(n - 1).expect(...)` calls. Renderer constructs MIDI bytes
through the `From<U7> for u8` / `From<U4> for u8` impls — no `as`
casts.

This is **P1 of the structural-type audit** documented at
`/Users/cmk/.claude/plans/harmonic-snacking-conway.md` (audit
finding D). It's the smallest actionable unit and unblocks several
later phases (P3's per-target role enums can re-use the migrated
`MidiClickConfig` / `MidiCc` shapes verbatim).

### What changed

- **New module** `crates/core/src/conn/midi.rs` (273 lines): `U7`, `U4`,
  `Ple`, `From`, `Display` impls, `U7U8` / `U4U8` connections,
  10 spot tests + 7 property tests covering range constructors,
  round-trip on the narrow side, saturation on the wide side, and
  the Galois adjoint law for both connections.
- **`crates/core/src/channel/mode.rs`**: `MidiClickConfig.{note,
  vel, ch}` and `MidiClickAccent.{note, vel}` switch to `U7` /
  `U4`. `ChannelMode::MidiCc { cc, range }` switches to
  `(U7, U7)` for `range`.
- **`crates/core/src/channel/spec.rs`**: parser uses
  `U7::new` / `U4::new`. The `mch_one_based` local renames to
  `mch_zero_based: Option<U4>` since U4 already encodes the
  zero-based domain. `vel=0` rejection stays a separate check
  so the "Note Off" error message remains clear.
- **`crates/core/src/sink/midi.rs`**: `render_midi_click_block`
  builds bytes via `cfg.ch.into()` once, then `n.into()` /
  `v.into()` per event. Test helper `click_cfg(u8, u8, u8, ...)`
  keeps its u8 signature for call-site brevity but constructs
  U7/U4 internally with `expect`.
- **`crates/core/src/control.rs`**: `collect_tick_samples` helper
  takes `U4` directly. All `MidiClickConfig` literals in the
  test suite migrate to `U7(...)` / `U4(...)` constructors.

### What did not change

- Public CLI surface: `--ch dev=midi,mode=click,note=37,vel=80,
  mch=10,...` parses and renders identically to before. All
  existing integration tests pass without modification.
- Display output: `Display for U7 / U4` forwards to the inner u8,
  so `format!("{}", spec)` produces the same text.
- The `vel=0 is Note Off` reject path: still a separate check
  ahead of `U7::new`, so the error message stays helpful.
- `MidiSink::send_at(&[u8], u64)`: still untyped at the trait
  boundary (audit finding E remains deferred).

### Why this is the right shape

- **Locally defined, not upstream.** U7 and U4 are MIDI-specific
  by intent. The upstream `connections` crate is general-purpose;
  shoving MIDI domain types into it would be cross-domain leakage
  (memory: `feedback_no_output_specifics_in_core_types`).
- **Saturating connections, not panicking ones.** `U7U8.inner(b)`
  is total over `u8`. The parser uses the *checked* constructor
  `U7::new` for "reject out-of-range user input"; the connection
  is the lawful conversion for any future call site that wants
  saturation semantics (e.g. clamping a control value during
  realtime processing).
- **Galois law is property-tested.** `u7u8_galois_law` and
  `u4u8_galois_law` confirm `ceil(a) ≤ b ⟺ a ≤ inner(b)` over
  the full `(narrow domain × full u8)` strategy space. Generators
  span `any::<u8>()` on the saturation side per CLAUDE.md's
  coverage-faking rule.

### Phasing context

This is **PR #23**, the first of seven phased PRs from the
structural-type audit (`/Users/cmk/.claude/plans/harmonic-snacking-conway.md`):

| Phase | Status |
|-------|--------|
| P0a — Conn-discipline sweep | DEFERRED (upstream `Conn::then`) |
| P0b — Float surface area | DEFERRED (depends on P0a) |
| **P1 — U7 / U4 newtypes** | **this PR** |
| P2 — drop `Channel.snap_to_quantum` | next |
| P3 — sum-typed `Channel` + role enums | after P2 |
| P4 — sum-typed `ChannelSpec` | after P3 |
| P5 — host-link decoupling | after P4 |
| P6 — host-cpal output typing | opportunistic |

P3 will re-use the migrated `MidiClickConfig` / `MidiCc` shapes
verbatim — no further field changes during the reshape.

### Test plan

- [x] `cargo test --workspace` green (514 passing, 4 ignored —
  unchanged from main).
- [x] `cargo clippy --all-targets -- -D warnings` clean.
- [x] `scripts/check-floats.sh` exit 0 (no f64 changes).
- [x] Pre-commit hook green on every commit.
- [x] Property tests cover Galois law + round-trip + saturation
  on the full u8 domain.
- [x] Existing parser tests (`parse_rejects_note_above_127`,
  `parse_rejects_vel_zero`, `parse_rejects_mch_zero_or_above_16`)
  pass unchanged — behaviour preserved.

## Local review (2026-04-25)

**Branch:** plan/2026-04-25-05
**Commits:** 4 (origin/main..plan/2026-04-25-05)
**Reviewer:** Claude (sonnet, independent)

---

### Reviewing diff on branch `plan/2026-04-25-05` (4 commits, 1355 lines)

This sprint introduces `U7`/`U4` newtypes in `crates/core/src/conn/midi.rs` and migrates all MIDI field types in `MidiClickConfig`, `MidiClickAccent`, and `ChannelMode::MidiCc` away from raw `u8`.

### Commit Hygiene

All four commits are correctly prefixed (`plan:`, `feat(midi):` x2, `doc:`). The history is linear. Each commit is self-contained: the `plan:` commit is the sprint opener, the two `feat:` commits carry the implementation and tests together, and the `doc:` commit finalizes plan + review file. No unrelated changes mixed in.

### Code Quality

The implementation is clean throughout. The conventions are followed:

- No `unsafe`, no stored floats, no open-coded unit arithmetic.
- `U7`/`U4` are `#[repr(transparent)]` with `pub` inner fields — deliberate by the plan's design (`.0` access is acceptable in the renderer).
- `From<U7> for u8` / `From<U4> for u8` are provided; the renderer uses `.into()` which is the documented deviation from the plan's `.0` suggestion. This is fine — it is noted in the plan's Review section.
- `U7U8` / `U4U8` are `const`-initialized, which is correct for function-pointer-based `Conn::new_left`.
- The parser's `vel=0` rejection remains a standalone check so the "Note Off" error message is distinct from the `U7::new` saturation path — good separation of concerns.
- `mch_zero_based` rename is accurate (noted in plan's Review section as intended deviation).
- No dead code, no obvious clippy hazards.

One minor observation, not a rule violation: `click_cfg` in `crates/core/src/sink/midi.rs` at line 537 keeps `(u8, u8, u8, ...)` parameter types and constructs internally via `U7::new(...).expect(...)`. The plan explicitly describes this as a deliberate call-site brevity tradeoff. The uses all pass values known at compile time (76, 100, 9, etc.), so the `expect` is safe in practice. This is acceptable.

### Test Coverage

**Property tests.** All seven required properties from the Verification table are present and correctly named in `crates/core/src/conn/midi.rs`:

| Plan property | Present | Generator domain |
|---|---|---|
| `u7_new_iff_in_range` | Yes | `any::<u8>()` — full domain |
| `u4_new_iff_in_range` | Yes | `any::<u8>()` — full domain |
| `u7u8_inner_round_trip_on_u7` | Yes | `0u8..=U7::MAX` — full U7 domain |
| `u7u8_ceil_inner_saturates` | Yes | `any::<u8>()` — includes the saturation boundary |
| `u7u8_galois_law` | Yes | `(0u8..=U7::MAX, any::<u8>())` |
| `u4u8_round_trip_on_u4` | Yes (as `u4u8_inner_round_trip_on_u4`) | `0u8..=U4::MAX` — full U4 domain |
| `u4u8_galois_law` | Yes | `(0u8..=U4::MAX, any::<u8>())` |

The plan also required `spec_round_trip` (the existing proptest) to keep passing with the migrated strategy. The `arb_mode` function at spec.rs line 913–938 now generates `U7(note)`, `U7(vel)`, `U4(ch)` directly within the same bounded ranges (`0u8..=127`, `1u8..=127`, `0u8..=15`) as before, and `spec_round_trip` at line 1005 exercises it. Confirmed present.

**One generator domain note (confidence 82):** The `u7u8_galois_law` property at `midi.rs` generates the left-hand argument `a` from `0u8..=U7::MAX` rather than `any::<u8>()`. Since `a` is immediately constructed as `U7(a)`, this is correct — any `u8` above 127 cannot be represented as `U7`, so `0..=127` is the full domain of the left-hand type. This is not a coverage-faking violation; it is the type-appropriate domain. The right-hand argument `b` is `any::<u8>()`, which exercises the saturation region on the `inner` side. No issue.

**Spot checks.** All six required spot checks from the plan are present in the diff (`u7_new_at_boundary`, `u4_new_at_boundary`, `u7u8_inner_at_127`, `u7u8_inner_at_max_u8_saturates` covers "inner_at_max", `u7u8_ceil_zero`, plus the `u7u8_floor_equals_ceil` exhaustive loop). Parser rejection tests (`parse_rejects_note_above_127`, `parse_rejects_vel_zero`, `parse_rejects_mch_zero_or_above_16`) are inherited from prior plan — they are not new in this diff, but the plan calls for verifying they still pass, which the review summary's build gate checklist confirms.

### Plan Conformance

**T1** — `crates/core/src/conn/midi.rs` created with `U7`, `U4`, `Ple` impls, `From` impls, `Display` impls, `U7U8`/`U4U8` constants. All correct. Module added to `lib.rs`.

**T2** — `channel/mode.rs` fields migrated. `all_variants_constructible` test updated. The plan says "construct via `U7::new(76).unwrap()`"; the implementation uses direct tuple-struct syntax `U7(76)`. Both are valid and equivalent — this is not a deviation worth noting since the plan's wording is illustrative, not prescriptive.

**T3** — Parser migration complete at `spec.rs`. All five parse sites handled. `mch_zero_based` rename documented.

**T4** — Renderer at `out/midi.rs` migrated. The `ch_byte` local computed once at function entry, `n.into()`/`v.into()` used per-event. Matches plan intent with the noted `.into()` vs. `.0` deviation.

**T5** — `Display for ChannelSpec` at `spec.rs:523` uses `cfg.ch.0 + 1` as planned. CLI smoke verification described in plan; covered by `cargo test --workspace`.

**One item not in the plan but present in the diff:** The `collect_tick_samples` helper in `control/transport.rs` was updated to accept `U4` directly. The plan's Review section documents this as an intentional deviation. Confirmed accurate.

### Risks

**No TODOs or stubs** were introduced in this diff.

**Existing call sites:** The migration is exhaustive over the files in scope. The `U7`/`U4` fields are `pub`, so crates outside `agogo-core` that construct `MidiClickConfig` directly (not via the spec parser) would need to be updated. The diff shows only one external consumer updating literals: `crates/core/src/control.rs` (the test module). There is no evidence of `crates/cli/` constructing `MidiClickConfig` directly — the review summary confirms CLI goes through the spec parser. This is not a risk.

**The `pub` inner field exposure:** Both `U7(pub u8)` and `U4(pub u8)` expose their inner field, meaning code can still construct `U7(200)` or `U4(16)` without going through `U7::new`. This is a deliberate design choice mirroring the Haskell `newtype` pattern (the connection handles saturation; the constructor handles rejection). The plan's design rationale explains this. Not a bug, but worth noting that the type does not make invalid states unrepresentable at the constructor level — it makes them unproducible via the *checked* path. The `Display` and `Ple` impls will behave unexpectedly for an out-of-range `U7(200)` constructed via tuple syntax, but no code in the diff does this for production paths.

**Security:** No shell/FS/network access. No new dependencies. `Cargo.lock` change is a cosmetic SHA normalization in the `connections` rev pin — not a dependency upgrade.

### Recommendations

**Must fix before push:** None.

**Follow-up (future work):**

- The `pub` inner field on `U7`/`U4` allows `U7(200)` to be constructed without going through `U7::new`. If downstream code (especially P3's role enum construction) ever builds values from untrusted sources, consider whether a private field + `pub fn new` + `impl From<U7> for u8` is the correct shape. For now all production construction paths use `U7::new` or a verified `U7(literal)`, so this is a P3-or-later question.
- The `u7u8_floor_equals_ceil` test at `midi.rs` uses a manual loop over `0..=U7::MAX` rather than a proptest. This is fine for 128 values, but it is consistent with the codebase's proptest discipline to note that a proptest version over `0u8..=U7::MAX` would be equivalent and more uniform. Not a blocking issue.
