//! `Display` for `ChannelSpec` — the parser's inverse, used by
//! the `spec_round_trip` proptest.
//!
//! Plan 2026-04-28-06 T4: extracted from `machine/spec.rs`. The
//! Display impl emits parser-stable output (every channel-spec
//! key the parser accepts, in a parser-friendly order) so that
//! `parse(spec.to_string())` recovers the same spec.

use std::fmt::{self, Display};

use crate::channel::role::MidiRole;
use crate::conn::fixed::Micro;
use crate::time::tbase::TBase;

use super::ChannelSpec;

impl Display for ChannelSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // `dev=midi` is the only supported value (audit P4); emitted
        // as a literal so the round-trip parser still sees the
        // required key.
        write!(f, "dev=midi,grid={}", self.grid)?;
        if let Some(id) = &self.id {
            write!(f, ",id={}", quote_if_needed(id))?;
        }
        if let Some(out) = &self.out {
            write!(f, ",out={}", quote_if_needed(out))?;
        }
        // mode + click keys: emit only when non-default
        // (MidiRole::Clock is the implicit default, so it's omitted).
        match &self.mode {
            MidiRole::Clock => {}
            MidiRole::Click(cfg) => {
                write!(
                    f,
                    ",mode=click,note={},vel={},mch={}",
                    cfg.note,
                    cfg.vel,
                    cfg.ch.0 + 1, // user-facing 1-based
                )?;
                if let Some(a) = &cfg.accent {
                    write!(f, ",accent-every={},accent-vel={}", a.every, a.vel)?;
                    // `accent-note` defaults to `note` at parse;
                    // emit it only when it differs so the round-trip
                    // doesn't introduce a redundant key.
                    if a.note != cfg.note {
                        write!(f, ",accent-note={}", a.note)?;
                    }
                }
            }
            // MidiRole::Cc is a spec-surface stub; the parser doesn't
            // produce it today, so the Display side stays silent.
            MidiRole::Cc(_) => {}
        }
        if self.swing.amount != 0 || self.swing.resolution != TBase::T8 {
            if self.swing.resolution == TBase::T8 {
                write!(f, ",swing={}", self.swing.amount)?;
            } else {
                write!(f, ",swing={}:{}", self.swing.resolution, self.swing.amount)?;
            }
        }
        if self.offset_ticks != 0 {
            write!(f, ",offset={}", self.offset_ticks)?;
        }
        if self.delay != Micro::ZERO {
            // Parser rejects negative delay (audit Q3 round-1 fix);
            // the spec layer guarantees `self.delay.0 >= 0`. Assert
            // here so a future path that constructs ChannelSpec
            // directly (test code, arb extension) surfaces the
            // invariant violation rather than silently corrupting
            // the Display output.
            debug_assert!(
                self.delay.0 >= 0,
                "ChannelSpec.delay invariant violated: {:?} < 0",
                self.delay
            );
            // Print as decimal milliseconds (parser-stable).
            // Sub-µs precision was already lost through F064FD06.ceil
            // at parse time; the integer-ms parse path round-trips
            // bit-exactly.
            let us = self.delay.0;
            let ms_int = us / 1_000;
            let frac = (us % 1_000) as u64;
            if frac == 0 {
                write!(f, ",delay={ms_int}")?;
            } else {
                write!(f, ",delay={ms_int}.{frac:03}")?;
            }
        }
        if let Some(q) = self.snap_to_quantum_micro {
            write!(f, ",snap-quantum-us={}", q)?;
        }
        if let Some(b) = self.bars {
            write!(f, ",bars={}", b)?;
        }
        Ok(())
    }
}

fn quote_if_needed(v: &str) -> String {
    if v.chars().any(|c| c == ',' || c == '=' || c.is_whitespace()) {
        format!("\"{}\"", v)
    } else {
        v.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::role::{MidiClickAccent, MidiClickConfig};
    use crate::conn::midi::{U4, U7};
    use crate::time::grid::Grid;
    use crate::time::swing::SwingConfig;
    use core::num::{NonZeroU16, NonZeroU32};
    use proptest::prelude::*;

    // ── Display round-trip ───────────────────────────────────────

    #[test]
    fn display_round_trip_minimal() {
        let spec = ChannelSpec::parse("dev=midi", &[]).unwrap();
        let s = spec.to_string();
        let reparsed = ChannelSpec::parse(&s, &[]).unwrap();
        assert_eq!(spec, reparsed);
    }

    #[test]
    fn display_round_trip_full() {
        let spec =
            ChannelSpec::parse("dev=midi,grid=T16,swing=T16:80,offset=20,delay=5", &[]).unwrap();
        let s = spec.to_string();
        let reparsed = ChannelSpec::parse(&s, &[]).unwrap();
        assert_eq!(spec, reparsed);
    }

    #[test]
    fn display_swing_default_res_omits_resolution() {
        let spec = ChannelSpec::parse("dev=midi,swing=80", &[]).unwrap();
        let s = spec.to_string();
        assert!(s.contains("swing=80"), "got: {s}");
        assert!(
            !s.contains("swing=t8:"),
            "should omit default resolution, got: {s}"
        );
    }

    #[test]
    fn display_swing_explicit_res_includes_resolution() {
        let spec = ChannelSpec::parse("dev=midi,swing=T16:80", &[]).unwrap();
        let s = spec.to_string();
        assert!(s.contains("swing=t16:80"), "got: {s}");
    }

    #[test]
    fn display_quotes_values_with_spaces() {
        let spec = ChannelSpec::parse(r#"dev=midi,out="IAC Bus 1""#, &[]).unwrap();
        let s = spec.to_string();
        assert!(s.contains(r#"out="IAC Bus 1""#), "got: {s}");
        let reparsed = ChannelSpec::parse(&s, &[]).unwrap();
        assert_eq!(spec, reparsed);
    }

    #[test]
    fn display_quotes_values_with_commas() {
        let spec = ChannelSpec::parse(r#"dev=midi,out="port,with,commas""#, &[]).unwrap();
        let s = spec.to_string();
        assert!(s.contains(r#"out="port,with,commas""#), "got: {s}");
        let reparsed = ChannelSpec::parse(&s, &[]).unwrap();
        assert_eq!(spec, reparsed);
    }

    /// Regression: `Micro(116)` (= 0.116 ms) used to round-trip to
    /// `Micro(117)` because Display emitted `delay=0.116` and the
    /// parser fell to its f64 path (`0.116 × 10⁻³` doesn't represent
    /// exactly in f64; `F064FD06.ceil` rounded up by one µs). Pin
    /// the bit-exact round-trip for a handful of fractional-µs
    /// values so a future regression to the f64 path is caught
    /// without waiting for proptest shrinking.
    #[test]
    fn display_round_trip_fractional_us_no_drift() {
        for us in [1, 116, 123, 999, 1_001, 51_123, 300_000_001] {
            let spec = ChannelSpec {
                id: None,
                out: None,
                grid: Grid::ALL[0],
                mode: MidiRole::Clock,
                swing: SwingConfig {
                    resolution: TBase::T8,
                    amount: 0,
                },
                offset_ticks: 0,
                delay: Micro(us),
                snap_to_quantum_micro: None,
                bars: None,
            };
            let s = spec.to_string();
            let reparsed = ChannelSpec::parse(&s, &[]).unwrap();
            assert_eq!(reparsed.delay, Micro(us), "drift on {us} µs (Display: {s})");
        }
    }

    // ── Proptest ─────────────────────────────────────────────────

    fn arb_grid() -> impl Strategy<Value = Grid> {
        prop::sample::select(Grid::ALL.as_slice())
    }

    fn arb_tbase() -> impl Strategy<Value = TBase> {
        prop::sample::select(TBase::ALL.as_slice())
    }

    /// Generate a `MidiRole` reachable from the spec parser:
    /// `Clock` or `Click(MidiClickConfig)` with arbitrary
    /// note/vel/ch and an optional accent.
    ///
    /// `accent.every` spans the full `NonZeroU32` domain — the
    /// round-trip property is u32-shape-preserving (parse-as-u32,
    /// Display via `Display for NonZeroU32`), so the entire domain
    /// is safe to sample. Per CLAUDE.md: don't bound to "keep
    /// things small," only to avoid documented hazards.
    fn arb_mode() -> impl Strategy<Value = MidiRole> {
        let click = (
            0u8..=127,
            1u8..=127,
            0u8..=15,
            prop::option::of((
                any::<u32>().prop_filter("every > 0", |&n| n > 0),
                0u8..=127,
                1u8..=127,
            )),
        )
            .prop_map(|(note, vel, ch, accent_triple)| {
                let accent = accent_triple.map(|(every, an, av)| MidiClickAccent {
                    every: NonZeroU32::new(every).unwrap(),
                    note: U7(an),
                    vel: U7(av),
                });
                MidiRole::Click(MidiClickConfig {
                    note: U7(note),
                    vel: U7(vel),
                    ch: U4(ch),
                    accent,
                })
            });
        prop_oneof![Just(MidiRole::Clock), click]
    }

    /// Full `NonZeroU16` domain for `bars` — same justification as
    /// `arb_mode`'s `every`: parser is u16-shape-preserving, no
    /// arithmetic hazards in the round-trip path.
    fn arb_bars() -> impl Strategy<Value = Option<NonZeroU16>> {
        prop::option::of(
            any::<u16>()
                .prop_filter("bars > 0", |&n| n > 0)
                .prop_map(|n| NonZeroU16::new(n).unwrap()),
        )
    }

    /// Non-negative `Micro` values across the full `i64` µs domain.
    /// `Display` formats any non-negative `Micro` as either `{ms_int}`
    /// or `{ms_int}.{frac:03}` (parser-stable in both cases), and the
    /// parser's `parse_decimal_ms` fast path handles arbitrary
    /// `i64`-representable µs without f64 drift — so the entire
    /// `0..=i64::MAX` µs range round-trips bit-exactly. No documented
    /// hazard, no bound (per CLAUDE.md proptest rule).
    ///
    /// Biased toward boundary cases the previous `0..=300 ms`
    /// generator missed: zero, sub-ms (`Display` emits `0.{frac}`),
    /// the `MAX_DELAY = 300_000 µs` clamp threshold, and the `i64`
    /// upper edge where f64-fallback parsers would have drifted.
    fn arb_delay() -> impl Strategy<Value = Micro> {
        prop_oneof![
            10 => Just(Micro::ZERO),
            10 => (1_i64..=999).prop_map(Micro),                  // sub-ms, fractional Display
            10 => (1_i64..=10_000).prop_map(|ms| Micro(ms * 1_000)), // small whole-ms
            10 => (1_i64..=10_000_000).prop_map(Micro),           // small with fractional µs
            5  => Just(Micro(300_000)),                            // MAX_DELAY clamp boundary
            5  => Just(Micro(i64::MAX)),                           // upper i64 edge
            1  => (0_i64..=i64::MAX).prop_map(Micro),              // full domain sweep
        ]
    }

    fn arb_spec() -> impl Strategy<Value = ChannelSpec> {
        (
            arb_grid(),
            prop::option::of("[a-zA-Z][a-zA-Z0-9]{0,8}"),
            prop::option::of("[a-zA-Z0-9]{1,10}"),
            arb_tbase(),
            any::<i8>(),
            any::<i32>(),
            arb_delay(),
            prop::option::of(any::<i64>()),
            arb_mode(),
            arb_bars(),
        )
            .prop_map(
                |(grid, id, out, swing_res, swing_amt, offset_ticks, delay, snap, mode, bars)| {
                    ChannelSpec {
                        id,
                        out,
                        grid,
                        mode,
                        swing: SwingConfig {
                            resolution: swing_res,
                            amount: swing_amt,
                        },
                        offset_ticks,
                        delay,
                        snap_to_quantum_micro: snap,
                        bars,
                    }
                },
            )
    }

    proptest! {
        /// Plan 14 property `spec_round_trip`: the `Display` impl
        /// emits parser-stable output, so `parse(spec.to_string())`
        /// recovers the same spec for every value the strategy
        /// generates. Plan 2026-04-25-03 extends the strategy with
        /// `mode=click` (note/vel/mch/accent-*) and `bars` so
        /// round-trip pins both the existing and new keys.
        #[test]
        fn spec_round_trip(spec in arb_spec()) {
            let s = spec.to_string();
            let parsed = ChannelSpec::parse(&s, &[])
                .map_err(|e| TestCaseError::fail(format!("parse `{}`: {}", s, e)))?;
            prop_assert_eq!(parsed, spec);
        }
    }
}
