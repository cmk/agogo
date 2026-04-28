//! `agogo time sched` handler — emits the absolute tick positions
//! for a swung-grid schedule.
//!
//! Pure function — useful for testing without capturing stdout.
//! `swing_to_config` converts a `0.5..=0.75` ratio into a
//! `SwingConfig` on a T16 resolution; `schedule_ticks` walks the
//! grid for `bars` 4/4 bars and returns the post-swing tick
//! positions.
//!
//! Plan 2026-04-28-05 T3: extracted from `cli/main.rs`.

use agogo_core::time::grid::Grid;
use agogo_core::time::swing::{self, SwingConfig};
use agogo_core::time::tbase::TBase;
use agogo_core::time::tick::Tick;
use bpaf::Bpaf;

#[derive(Bpaf, Debug, Clone)]
pub struct ScheduleArgs {
    /// Grid resolution (e.g. `t16`, `t8t`, `t8q`, `t512p`).
    /// Renamed from `--tbase` after the v0.2 lattice extension.
    #[bpaf(long, argument::<String>("GRID"), parse(parse_grid))]
    pub grid: Grid,

    /// Swing ratio in `[0.5, 0.75]`: 0.5 = straight, 0.667 =
    /// triplet feel (off-beat at 2/3 of the next on-beat), 0.75
    /// = max useful swing (off-beat at 3/4). f64 per the CLI
    /// argv-boundary rule (CLAUDE.md §Repository conventions).
    #[bpaf(long, argument("SWING"), fallback(0.5))]
    pub swing: f64,

    /// Number of 4/4 bars to schedule. Bounded to `u16` (≤ 65535)
    /// so memory and stdout stay reasonable.
    #[bpaf(long, argument("BARS"))]
    pub bars: u16,
}

fn parse_grid(s: String) -> Result<Grid, String> {
    s.parse()
}

/// Convert a `0.5..=0.75` swing ratio into a `SwingConfig` on a
/// T16 resolution grid.
///
/// Two consecutive on-beats at the T16 resolution are
/// `2 × T16.tick_count() = 480` ticks apart at 960 PPQN. A swing
/// ratio `r` places the off-beat `r × 480` ticks past the on-beat;
/// the displacement from straight (`r = 0.5`, off-beat at 240) is
/// therefore `(r − 0.5) × 480` ticks. Spot values:
///
/// - `r = 0.5  → amount = 0`   (straight)
/// - `r = 0.667 → amount = 80` (triplet feel: off-beat at 320 / 480)
/// - `r = 0.75 → amount = 120` (max useful: off-beat at 360 / 480)
///
/// Values outside `[0.5, 0.75]` are clamped. The amount is
/// further clamped to `i8` (max 127) but the clamp is unreachable
/// inside the `[0.5, 0.75]` range since `120 < 127`.
///
/// Note: the multiplier is derived from `T16.tick_count()` so the
/// musical meaning of `r` stays accurate at any PPQN — Plan 15's
/// 192 → 960 PPQN bump scaled the multiplier from 96 to 480
/// automatically rather than baking the old 192-PPQN constant.
pub fn swing_to_config(swing: f64) -> SwingConfig {
    let clamped = swing.clamp(0.5, 0.75);
    let half_step = (TBase::T16.tick_count() as f64) * 2.0;
    let amount_i32 = ((clamped - 0.5) * half_step).round() as i32;
    let amount = amount_i32.clamp(i8::MIN as i32, i8::MAX as i32) as i8;
    SwingConfig {
        resolution: TBase::T16,
        amount,
    }
}

/// Produce the absolute tick positions for a schedule (one per
/// grid step; swing, if any, is already folded in). Pure function
/// — useful for testing without capturing stdout.
pub fn schedule_ticks(args: &ScheduleArgs) -> Vec<Tick> {
    let cfg = swing_to_config(args.swing);
    // 4/4 assumption: one bar = 4 * PPQN ticks.
    let ticks_per_bar = 4 * agogo_core::time::tick::PPQN;
    let step_tc = args.grid.tick_count();
    let steps_per_bar = ticks_per_bar / step_tc;
    // `bars` is `u16`; `u32::from(bars) * steps_per_bar` is bounded
    // by 65535 * BAR/T512P = 65535 * 3840 ≈ 251M, inside u32.
    let total_steps = u32::from(args.bars) * steps_per_bar;

    (0..total_steps)
        .map(|step| {
            let nominal = Tick(step * step_tc);
            swing::effective_tick(&cfg, nominal)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swing_to_config_straight() {
        assert_eq!(
            swing_to_config(0.5),
            SwingConfig {
                resolution: TBase::T16,
                amount: 0,
            }
        );
    }

    #[test]
    fn swing_to_config_054() {
        // (0.54 - 0.5) * 480 = 19.2 → round to 19.
        assert_eq!(
            swing_to_config(0.54),
            SwingConfig {
                resolution: TBase::T16,
                amount: 19,
            }
        );
    }

    #[test]
    fn swing_to_config_0667_is_triplet_feel() {
        // (0.6666… - 0.5) * 480 = 80 (off-beat at 320/480 = 2/3).
        assert_eq!(
            swing_to_config(2.0 / 3.0),
            SwingConfig {
                resolution: TBase::T16,
                amount: 80,
            }
        );
    }

    #[test]
    fn swing_to_config_075_is_max_swing() {
        // (0.75 - 0.5) * 480 = 120 (off-beat at 360/480 = 3/4).
        assert_eq!(
            swing_to_config(0.75),
            SwingConfig {
                resolution: TBase::T16,
                amount: 120,
            }
        );
    }

    #[test]
    fn swing_to_config_clamps() {
        assert_eq!(swing_to_config(0.0).amount, 0);
        assert_eq!(swing_to_config(1.0).amount, 120);
    }

    #[test]
    fn schedule_ticks_two_bars_t16_yields_32_positions() {
        let ticks = schedule_ticks(&ScheduleArgs {
            grid: Grid::T16,
            swing: 0.5,
            bars: 2,
        });
        assert_eq!(ticks.len(), 32);
        // Straight T16 schedule at 960 PPQN: 0, 240, 480, ..., 7440.
        for (i, t) in ticks.iter().enumerate() {
            assert_eq!(t.0, (i as u32) * 240);
        }
    }

    #[test]
    fn schedule_ticks_swing_054_shifts_off_beats() {
        let ticks = schedule_ticks(&ScheduleArgs {
            grid: Grid::T16,
            swing: 0.54,
            bars: 1,
        });
        // 16 steps. Off-beats (indices 1, 3, 5, …, 15) shifted by +19
        // ticks: (0.54 - 0.5) × 480 = 19.2 → 19. Drum-machine sign
        // convention: positive amount delays the off-beat.
        let expected: Vec<u32> = (0..16u32)
            .map(|i| if i % 2 == 1 { i * 240 + 19 } else { i * 240 })
            .collect();
        let got: Vec<u32> = ticks.iter().map(|t| t.0).collect();
        assert_eq!(got, expected);
    }

    #[test]
    fn schedule_ticks_t512p_has_3840_steps_per_bar() {
        // T512P = 1 tick at 960 PPQN — bar = 3840 ticks → 3840 steps.
        let ticks = schedule_ticks(&ScheduleArgs {
            grid: Grid::T512P,
            swing: 0.5,
            bars: 1,
        });
        assert_eq!(ticks.len(), 3840);
    }

    #[test]
    fn schedule_ticks_t1_has_one_step_per_bar() {
        let ticks = schedule_ticks(&ScheduleArgs {
            grid: Grid::T1,
            swing: 0.5,
            bars: 4,
        });
        assert_eq!(ticks.len(), 4);
        // BAR = 3840 ticks at 960 PPQN.
        assert_eq!(
            ticks.iter().map(|t| t.0).collect::<Vec<_>>(),
            vec![0, 3840, 7680, 11520]
        );
    }
}
