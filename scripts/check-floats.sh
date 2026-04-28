#!/usr/bin/env bash
# check-floats.sh — CI gate for CLAUDE.md's no-stored-float rule.
#
# `f32` / `f64` may appear only in the documented exception
# modules listed below. Each file is allowed because its contents
# fall into one of the five enumerated exception categories
# (CLAUDE.md §Repository conventions):
#
#   crates/core/src/sync/pll.rs                   PI controller state + control law
#   crates/core/src/sync/detect.rs                parabolic-fit ABI-local locals
#   crates/core/src/sync/source.rs                PCM audio intake (`&[f32]`) + tests
#   crates/core/src/boundary.rs                   argv-boundary + PI-exempt helpers (split from
#                                                 the deleted fxp.rs in Plan 2026-04-28-03 T5)
#   crates/core/src/arb.rs                        test-fixture PCM generators
#   crates/core/src/host.rs                       PCM ABI shape (AudioIo `&[f32]` slices)
#   crates/core/src/machine.rs                    PCM ABI (empty `[f32; 0]` for AudioIo construction in tests)
#   crates/core/src/machine/spec.rs               argv-boundary (--ch shift-ms / offset-ms via F64F06)
#   crates/core/src/time/float.rs                 vendored from connections — F064FDxx Conns
#                                                 with f64-correction loops are intrinsic
#                                                 (split from time/decimal.rs in Plan 2026-04-28-03 T1)
#   crates/core/src/time/sample.rs                vendored from connections — FD12↔Sxxx Conn
#                                                 walk needs f64 internally
#   crates/host-link/src/link.rs                  Link FFI (AblLink C++ ABI)
#   crates/host-link/src/source.rs                PCM ABI (PhaseSourceImpl::feed_samples slice param)
#   crates/host-link/src/quantum.rs               Link FFI parity helper (f64_beats_to_quantum
#                                                 round-half-away-from-zero matches std::llround;
#                                                 moved from fxp.rs in Plan 2026-04-28-03 T4)
#   crates/host-cpal/src/cpal.rs                  PCM ABI (cpal stream callback)
#   crates/host-cpal/src/cpal/callback.rs         PCM ABI (AudioIo input/output slices)
#   crates/cli/src/main.rs                        argv parsers (`parse_bpm_to_tempo` / `parse_quantum_from_beats` / `parse_jitter_us_to_pico` / `parse_ms_to_micro`)
#   crates/cli/src/run.rs                         argv-parser proptests + `--ch shift-ms` parsing helper
#
# Any `f32` / `f64` in a non-allowlisted file is a build failure.
# To add a new allowlisted file, amend both this script and
# CLAUDE.md so the rule and the gate stay in sync.
#
# Known limitation: the regex `\bf32\b|\bf64\b` matches occurrences
# inside string literals, format strings, and inline comments after
# code. False positives are possible in error messages like
# `return Err("expected f32 sample data")`. The gate is a
# "type-position-or-adjacent-comment" approximation, not a
# full-fidelity Rust parser. Check manually before blaming the gate;
# rewrite the offending string to not literally contain the token if
# avoidance is simpler than refactor.

set -euo pipefail

ALLOWED=(
  "crates/core/src/sync/pll.rs"
  "crates/core/src/sync/detect.rs"
  "crates/core/src/sync/source.rs"
  # Replaces the deleted `crates/core/src/fxp.rs` entry from before
  # Plan 2026-04-28-03 T5: argv-boundary helpers (`f64_bpm_to_tempo`,
  # `f64_phase_to_phase`) and PI-exempt control-law helpers
  # (`tempo_to_hz`, `bits_q48_16_to_seconds`, `tempo_to_f64_bpm`,
  # `pico_to_f64_seconds`) plus the `MAX_BPM_F64` argv-bound constant.
  "crates/core/src/boundary.rs"
  "crates/core/src/arb.rs"
  "crates/core/src/host.rs"
  "crates/core/src/machine.rs"
  "crates/core/src/machine/spec.rs"
  # Vendored from connections — both modules ship with f64 inside
  # their float→fixed Conn machinery (`F064FDxx` correction loops
  # for time/float.rs; the FD12↔Sxxx adjoint walk for time/sample.rs).
  # The f64 surface is intrinsic to the abstraction and was upstream-
  # allowlisted for the same reason; the file move downstream brings
  # the allowlist entry with it. (Plan 2026-04-28-03 T1 split float
  # out of decimal — decimal.rs is no longer allowlisted because it
  # contains no live f64 after the split.)
  "crates/core/src/time/float.rs"
  "crates/core/src/time/sample.rs"
  "crates/host-link/src/link.rs"
  "crates/host-link/src/source.rs"
  # Plan 2026-04-28-03 T4: `Quantum` + `f64_beats_to_quantum` +
  # `parse_quantum_from_beats` moved here from `agogo_core::fxp`.
  # The f64 surface is the FFI-parity exception — Link's C++ side
  # does `std::llround(q × 1e6)`; agogo's `Quantum` round-trip must
  # agree bit-for-bit at the seam.
  "crates/host-link/src/quantum.rs"
  "crates/host-cpal/src/cpal.rs"
  "crates/host-cpal/src/cpal/callback.rs"
  "crates/cli/src/main.rs"
  "crates/cli/src/run.rs"
)

REPO_ROOT=$(git rev-parse --show-toplevel)
cd "$REPO_ROOT"

FAIL=0
while IFS= read -r -d '' file; do
  rel_file="${file#./}"

  # Skip allowlisted files.
  allowed=0
  for a in "${ALLOWED[@]}"; do
    if [[ "$rel_file" == "$a" ]]; then
      allowed=1
      break
    fi
  done
  if (( allowed )); then
    continue
  fi

  # Scan every line for `f32` / `f64` identifiers. Skip any line that
  # looks like a comment (`///`, `//!`, `//`, `* `) — docs and inline
  # commentary about floats are fine, actual `f32` / `f64` types /
  # values are the real target.
  while IFS= read -r hit; do
    line_num="${hit%%:*}"
    line_body="${hit#*:}"

    # Strip leading whitespace. `${var##*([[:space:]])}` requires
    # extglob (off by default); the nested-expansion idiom below
    # works in plain bash.
    stripped="${line_body#"${line_body%%[![:space:]]*}"}"
    # Matched cases: line-comments (`//`), block-comment starts
    # (`/*`), block-comment ends (`*/`), and doc-block-comment
    # continuation lines (`* ` with trailing space, as rustfmt
    # produces). We deliberately do NOT match bare leading `*`
    # since that's a valid Rust token (deref, multiplication) and
    # a false-negative would let `*mut_ptr = 0.0_f32;` slip past.
    case "$stripped" in
      "//"*|"/*"*|"*/"*|"* "*) continue ;;
    esac

    printf '%s:%s: unannotated f32/f64 (move to an allowlisted module or refactor to fxp)\n' \
      "$rel_file" "$line_num" >&2
    printf '    %s\n' "$line_body" >&2
    FAIL=1
    # POSIX-compatible word-boundary: `\b` in grep isn't portable
    # across BSD / GNU implementations; the explicit non-alnum
    # character class works everywhere grep -E runs.
  done < <(grep -nE '(^|[^[:alnum:]_])(f32|f64)([^[:alnum:]_]|$)' "$file" || true)
done < <(find crates -type f -name '*.rs' -path '*/src/*' -not -path '*/target/*' -print0)

if (( FAIL )); then
  cat >&2 <<'HINT'

check-floats.sh: FAIL. Fix by one of:

  - Store the value in a fixed-point type (Micro, Pico, Tempo, Phase)
    and compose an existing Conn to do the conversion at the boundary.
  - If the float genuinely belongs in one of the documented exception
    classes (PI, PCM ABI, ABI-local, argv, Link FFI), add the file to
    this script's ALLOWED list AND amend CLAUDE.md's exception list
    so the gate and the rule stay in sync.

HINT
  exit 1
fi

printf 'check-floats.sh: OK — all f32/f64 uses contained to allowlisted modules.\n'
