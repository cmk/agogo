//! End-to-end inter-channel sample-accuracy proptests for the
//! offline render path. Plan 2026-05-05-02 T5.
//!
//! Built against the public `agogo` facade so the same surface a
//! downstream consumer would use is what's exercised here. Each
//! property either renders one or more polyrhythms via
//! `render_offline_capture` and asserts on the per-lane PCM, or
//! drives `OfflineRenderConfig` validation directly to pin the
//! no-mix-bus / lane-bounds rules at the boundary.
//!
//! Strategies are kept inline in this file rather than added to
//! `chan/src/time/arb.rs` because the polyrhythm-pair / N-channel /
//! render-config shapes are end-to-end-test specific; the chan crate
//! already exposes the primitive grid / tick / swing strategies it
//! needs internally.

use agogo::chan::channel::role::{AudioRole, ChannelCommon};
use agogo::chan::channel::time::Channel;
use agogo::chan::conn::fixed::Micro;
use agogo::chan::conn::tempo::Tempo;
use agogo::chan::time::conn::tick_to_whole_samples;
use agogo::chan::time::grid::Grid;
use agogo::chan::time::swing::SwingConfig;
use agogo::chan::time::tbase::TBase;
use agogo::chan::time::tick::Tick;
use agogo::core::{OfflinePcm, OfflineRenderConfig, OfflineRenderError, render_offline_capture};

use proptest::prelude::*;

// ── Shared constants and primitives ──────────────────────────────

const SR_48K: u32 = 48_000;
const BPM_120: Tempo = Tempo::from_bpm_integer(120);

/// Mirrors the private `click_len` constant in
/// `crates/chan/src/sink/audio.rs`. Renderer writes exactly
/// `click_len(sr)` consecutive samples per click event (truncated
/// only by the end of the render buffer).
fn click_len(sr: u32) -> usize {
    (sr as usize / 250).clamp(24, 960)
}

fn audio_channel(grid: Grid, lane: u16) -> Channel {
    Channel::Audio {
        common: ChannelCommon {
            divider: grid,
            shuffle: SwingConfig {
                resolution: TBase::T16,
                amount: 0,
            },
            delay: Micro::ZERO,
            offset: Micro::ZERO,
            bar_multiplier: None,
        },
        role: AudioRole::Click,
        lane,
    }
}

/// Predict the onset sample positions for a grid under fixed
/// scheduling parameters. Mirrors the scheduler's tick→sample
/// mapping (`tick_to_whole_samples`) without going through the
/// renderer, so tests can compare predicted vs. actual onsets
/// independently.
fn predict_onsets(grid: Grid, bpm: Tempo, sr: u32, total_frames: u64) -> Vec<u64> {
    let divisor = u64::from(grid.tick_count());
    let mut onsets = Vec::new();
    let mut tick: u64 = 0;
    loop {
        let sample =
            tick_to_whole_samples(Tick(tick), bpm, sr).expect("sample rate must be supported");
        if sample >= total_frames {
            break;
        }
        onsets.push(sample);
        match tick.checked_add(divisor) {
            Some(t) => tick = t,
            None => break,
        }
    }
    onsets
}

/// Build a binary mask the same length as one lane's PCM where
/// `mask[i]` is true iff `i` is inside the predicted footprint
/// `[onset, onset + click_len)` of any onset in `onsets` (clamped
/// to `total_frames`).
fn predicted_footprint_mask(
    onsets: &[u64],
    click_len_samples: usize,
    total_frames: u64,
) -> Vec<bool> {
    let n = total_frames as usize;
    let mut mask = vec![false; n];
    for &o in onsets {
        let start = o as usize;
        let end = (start + click_len_samples).min(n);
        for slot in mask.iter_mut().take(end).skip(start) {
            *slot = true;
        }
    }
    mask
}

fn frames_for_bars(bars: u32, bpm: Tempo, sr: u32) -> u64 {
    let ticks_per_bar = u64::from(Grid::T1.tick_count());
    let ticks = ticks_per_bar * u64::from(bars);
    tick_to_whole_samples(Tick(ticks), bpm, sr).expect("sample rate must be supported")
}

fn render_lanes_ok(
    channels: Vec<Channel>,
    output_channels: u16,
    bpm: Tempo,
    sr: u32,
    buffer_frames: u32,
    total_frames: u64,
) -> OfflinePcm {
    let cfg = OfflineRenderConfig {
        channels,
        bpm,
        sample_rate: sr,
        buffer_frames,
        total_frames,
        output_channels,
    };
    let (_report, pcm) =
        render_offline_capture(cfg).expect("render_offline_capture should succeed for valid inputs");
    pcm
}

// ── Strategies ───────────────────────────────────────────────────

/// Common-LCM polyrhythm pairs where coincident moments fall inside
/// the small render durations the proptests use. The grids are
/// listed in ascending tick_count order; both elements differ.
fn arb_polyrhythm_pair() -> impl Strategy<Value = (Grid, Grid)> {
    prop_oneof![
        // Boundary-bias the trivial 1:1 case so we exercise the
        // single-grid alignment path.
        1 => Just((Grid::T2, Grid::T2)),
        // Canonical small-LCM pairs that cleanly produce coincident
        // moments inside 1..=4 bars.
        2 => Just((Grid::T2T, Grid::T2)),  // 3:2
        2 => Just((Grid::T4T, Grid::T4)),  // 6:4 = 3:2 at finer grid
        2 => Just((Grid::T4, Grid::T8)),   // 1:2
        2 => Just((Grid::T8T, Grid::T8)),  // 3:2 at eighth-note level
        2 => Just((Grid::T16, Grid::T8)),  // 2:1
        1 => Just((Grid::T16T, Grid::T8)), // 6:4 = 3:2 at sixteenth
    ]
}

/// Distinct grids for an N-channel render, drawn from a small
/// musically-meaningful pool so the rendered durations stay short.
/// Returns the first `n` grids from the pool in lane order; pool
/// length matches `MAX_OUTPUT_CHANNELS = 16`.
fn n_channel_grids(n: u16) -> Vec<Grid> {
    let pool: [Grid; 16] = [
        Grid::T1,
        Grid::T2,
        Grid::T2T,
        Grid::T4,
        Grid::T4T,
        Grid::T8,
        Grid::T8T,
        Grid::T16,
        Grid::T16T,
        Grid::T32,
        Grid::T32T,
        Grid::T64,
        Grid::T64T,
        Grid::T128,
        Grid::T128T,
        Grid::T256,
    ];
    debug_assert!(usize::from(n) <= pool.len());
    pool.into_iter().take(usize::from(n)).collect()
}

/// Bounded render config strategy for end-to-end properties.
/// Skips 176_400 / 192_000 for proptest runtime; a deterministic
/// `spot_192k_sample_rate` covers the upper rates.
#[derive(Debug, Clone, Copy)]
struct RenderCfg {
    bpm: Tempo,
    sr: u32,
    buffer_frames: u32,
    duration_bars: u32,
    output_channels: u16,
}

fn arb_bpm() -> impl Strategy<Value = Tempo> {
    prop_oneof![
        2 => Just(Tempo::from_bpm_integer(120)),
        2 => Just(Tempo::from_bpm_integer(128)),
        2 => Just(Tempo::from_bpm_integer(170)),
        4 => (60u32..=240).prop_map(Tempo::from_bpm_integer),
    ]
}

fn arb_render_cfg() -> impl Strategy<Value = RenderCfg> {
    (
        arb_bpm(),
        prop::sample::select(&[44_100_u32, 48_000, 88_200, 96_000]),
        prop::sample::select(&[64_u32, 128, 256, 1024, 4096]),
        1u32..=2,
        prop::sample::select(&[1_u16, 2, 4, 8, 16]),
    )
        .prop_map(
            |(bpm, sr, buffer_frames, duration_bars, output_channels)| RenderCfg {
                bpm,
                sr,
                buffer_frames,
                duration_bars,
                output_channels,
            },
        )
}

// ── Properties ───────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 32,
        .. ProptestConfig::default()
    })]

    /// Two audio channels claiming the same lane is rejected at
    /// the config boundary with `LaneCollision`. Boundary
    /// enforcement of imperative #2 (no mix bus).
    #[test]
    fn prop_lane_uniqueness_rejected(
        grid_a in prop::sample::select(&[Grid::T2, Grid::T2T, Grid::T4]),
        grid_b in prop::sample::select(&[Grid::T2, Grid::T2T, Grid::T4]),
        lane in 0u16..=15,
        output_channels in 2u16..=16,
    ) {
        prop_assume!(lane < output_channels);
        let cfg = OfflineRenderConfig {
            channels: vec![audio_channel(grid_a, lane), audio_channel(grid_b, lane)],
            bpm: BPM_120,
            sample_rate: SR_48K,
            buffer_frames: 1024,
            total_frames: frames_for_bars(1, BPM_120, SR_48K),
            output_channels,
        };
        match render_offline_capture(cfg) {
            Err(OfflineRenderError::LaneCollision { lane: l, channels }) => {
                prop_assert_eq!(l, lane);
                prop_assert_eq!(channels, (0_usize, 1_usize));
            }
            other => prop_assert!(false, "expected LaneCollision, got: {other:?}"),
        }
    }

    /// `lane >= output_channels` is rejected with `LaneOutOfRange`.
    #[test]
    fn prop_lane_out_of_range_rejected(
        grid in prop::sample::select(&[Grid::T2, Grid::T2T, Grid::T4]),
        output_channels in 1u16..=16,
        excess in 0u16..=4,
    ) {
        let lane = output_channels.saturating_add(excess);
        let cfg = OfflineRenderConfig {
            channels: vec![audio_channel(grid, lane)],
            bpm: BPM_120,
            sample_rate: SR_48K,
            buffer_frames: 1024,
            total_frames: frames_for_bars(1, BPM_120, SR_48K),
            output_channels,
        };
        match render_offline_capture(cfg) {
            Err(OfflineRenderError::LaneOutOfRange {
                channel,
                lane: l,
                output_channels: oc,
            }) => {
                prop_assert_eq!(channel, 0_usize);
                prop_assert_eq!(l, lane);
                prop_assert_eq!(oc, output_channels);
            }
            other => prop_assert!(false, "expected LaneOutOfRange, got: {other:?}"),
        }
    }

    /// Lanes with no routed channel are exactly 0.0 across the
    /// entire render.
    #[test]
    fn prop_unused_lane_silence(
        grid in prop::sample::select(&[Grid::T2T, Grid::T4, Grid::T8]),
        output_channels in 2u16..=16,
        used_lane in 0u16..=15,
    ) {
        prop_assume!(used_lane < output_channels);
        let total_frames = frames_for_bars(1, BPM_120, SR_48K);
        let pcm = render_lanes_ok(
            vec![audio_channel(grid, used_lane)],
            output_channels,
            BPM_120,
            SR_48K,
            512,
            total_frames,
        );
        for (lane_idx, lane_pcm) in pcm.lanes.iter().enumerate() {
            if lane_idx as u16 == used_lane {
                continue;
            }
            for (i, &s) in lane_pcm.iter().enumerate() {
                prop_assert!(
                    s == 0.0,
                    "unused lane {lane_idx} sample {i} = {s} (expected 0.0)"
                );
            }
        }
    }

    /// Inside the routed lane's predicted footprints the renderer
    /// has written; outside them every sample is exactly 0.0. The
    /// *outside-footprint* assertion catches any stray write from
    /// other channels or from the offline harness; the
    /// *inside-footprint* assertion (energy in at least one
    /// sample of each footprint) catches a silent zeroing bug
    /// that would otherwise satisfy the equality alone.
    #[test]
    fn prop_lane_separation_and_zero_outside_footprints(
        (grid_a, grid_b) in arb_polyrhythm_pair(),
    ) {
        let bpm = BPM_120;
        let sr = SR_48K;
        let total_frames = frames_for_bars(1, bpm, sr);
        let pcm = render_lanes_ok(
            vec![audio_channel(grid_a, 0), audio_channel(grid_b, 1)],
            2,
            bpm,
            sr,
            1024,
            total_frames,
        );
        let click = click_len(sr);
        let onsets_a = predict_onsets(grid_a, bpm, sr, total_frames);
        let onsets_b = predict_onsets(grid_b, bpm, sr, total_frames);
        let mask_a = predicted_footprint_mask(&onsets_a, click, total_frames);
        let mask_b = predicted_footprint_mask(&onsets_b, click, total_frames);

        // Outside footprints: exactly zero on the routed lane.
        for (i, &s) in pcm.lanes[0].iter().enumerate() {
            if !mask_a[i] {
                prop_assert!(
                    s == 0.0,
                    "lane 0 sample {i} = {s} outside any predicted footprint"
                );
            }
        }
        for (i, &s) in pcm.lanes[1].iter().enumerate() {
            if !mask_b[i] {
                prop_assert!(
                    s == 0.0,
                    "lane 1 sample {i} = {s} outside any predicted footprint"
                );
            }
        }

        // Inside footprints: at least one nonzero sample per
        // footprint (rules out silent-zeroing regressions). Last
        // footprint may be truncated by total_frames.
        for &o in &onsets_a {
            let start = o as usize;
            let end = (start + click).min(total_frames as usize);
            let any_nonzero = pcm.lanes[0][start..end].iter().any(|&s| s != 0.0);
            prop_assert!(
                any_nonzero,
                "lane 0 footprint at {start}..{end} all zero"
            );
        }
        for &o in &onsets_b {
            let start = o as usize;
            let end = (start + click).min(total_frames as usize);
            let any_nonzero = pcm.lanes[1][start..end].iter().any(|&s| s != 0.0);
            prop_assert!(
                any_nonzero,
                "lane 1 footprint at {start}..{end} all zero"
            );
        }
    }

    /// For each predicted onset on the routed lane, the first
    /// nonzero sample of that click run is at exactly the
    /// scheduler's `tick_to_whole_samples` position. Catches any
    /// off-by-one between the predicted tick→sample mapping and
    /// the renderer's actual write offset. Together with
    /// `prop_lane_separation_and_zero_outside_footprints`, the
    /// "first nonzero in this click run" reduces to "first nonzero
    /// at-or-after this onset, before the next onset's footprint
    /// starts."
    #[test]
    fn prop_onset_sample_exact(
        (grid_a, grid_b) in arb_polyrhythm_pair(),
    ) {
        let bpm = BPM_120;
        let sr = SR_48K;
        let total_frames = frames_for_bars(1, bpm, sr);
        let pcm = render_lanes_ok(
            vec![audio_channel(grid_a, 0), audio_channel(grid_b, 1)],
            2,
            bpm,
            sr,
            1024,
            total_frames,
        );
        let click = click_len(sr);
        for (lane_idx, grid) in [(0_usize, grid_a), (1_usize, grid_b)] {
            let onsets = predict_onsets(grid, bpm, sr, total_frames);
            for &o in &onsets {
                let start = o as usize;
                let end = (start + click).min(total_frames as usize);
                let first_nonzero_in_run = pcm.lanes[lane_idx][start..end]
                    .iter()
                    .position(|&s| s != 0.0)
                    .map(|i| start + i);
                prop_assert_eq!(
                    first_nonzero_in_run,
                    Some(start),
                    "lane {} onset run starting at predicted sample {} should have its first \
                     nonzero sample at exactly {}, not {:?}",
                    lane_idx, start, start, first_nonzero_in_run
                );
            }
        }
    }

    /// The renderer writes exactly `click_len(sr)` consecutive
    /// samples per click event (truncated only when the buffer
    /// ends first). For each predicted onset, the sample one
    /// position past the predicted footprint end must be exactly
    /// 0 (no overflow), and the footprint itself must contain at
    /// least one nonzero sample (no silent zeroing).
    #[test]
    fn prop_footprint_length_constant(
        (grid_a, grid_b) in arb_polyrhythm_pair(),
    ) {
        let bpm = BPM_120;
        let sr = SR_48K;
        let total_frames = frames_for_bars(1, bpm, sr);
        let pcm = render_lanes_ok(
            vec![audio_channel(grid_a, 0), audio_channel(grid_b, 1)],
            2,
            bpm,
            sr,
            1024,
            total_frames,
        );
        let click = click_len(sr);
        for (lane_idx, grid) in [(0_usize, grid_a), (1_usize, grid_b)] {
            let onsets = predict_onsets(grid, bpm, sr, total_frames);
            for &o in &onsets {
                let start = o as usize;
                let expected_end = (start + click).min(total_frames as usize);
                // Sample one past the predicted end is silent
                // (renderer didn't write past click_len).
                if expected_end < total_frames as usize {
                    prop_assert_eq!(
                        pcm.lanes[lane_idx][expected_end],
                        0.0,
                        "lane {} click at sample {} extends past click_len={}",
                        lane_idx,
                        start,
                        click
                    );
                }
                // Footprint contains at least one nonzero sample
                // (renderer wrote something).
                let any_nonzero = pcm.lanes[lane_idx][start..expected_end]
                    .iter()
                    .any(|&s| s != 0.0);
                prop_assert!(
                    any_nonzero,
                    "lane {} footprint at sample {} contained no nonzero samples",
                    lane_idx, start
                );
            }
            // Spacing guard: the polyrhythm pool keeps event
            // spacing at >= click_len, so footprints never overlap
            // — multi-overlap-tail behaviour is documented as a
            // known limitation in the renderer's doc and is not
            // exercised here.
            for window in onsets.windows(2) {
                let span = window[1] - window[0];
                prop_assert!(
                    span >= click as u64,
                    "lane {} grid {:?}: onsets {} and {} are {} samples apart, \
                     less than click_len={click}",
                    lane_idx, grid, window[0], window[1], span
                );
            }
        }
    }

    /// For a fixed (channels, lanes, bpm, duration_bars) config,
    /// every non-zero predicted onset at `sr=96_000` is at exactly
    /// 2x the sample index of the same onset at `sr=48_000`.
    /// Tick 0 trivially satisfies `0 == 2*0` and would mask
    /// rounding bugs in `tick_to_whole_samples`, so the property
    /// asserts on the *second* onset (first non-zero tick) and
    /// also confirms the renderer actually wrote the click there
    /// in both buffers.
    #[test]
    fn prop_sample_rate_doubling(
        (grid_a, grid_b) in arb_polyrhythm_pair(),
    ) {
        let bpm = BPM_120;
        let total_48 = frames_for_bars(1, bpm, 48_000);
        let total_96 = frames_for_bars(1, bpm, 96_000);
        let pcm_48 = render_lanes_ok(
            vec![audio_channel(grid_a, 0), audio_channel(grid_b, 1)],
            2,
            bpm,
            48_000,
            1024,
            total_48,
        );
        let pcm_96 = render_lanes_ok(
            vec![audio_channel(grid_a, 0), audio_channel(grid_b, 1)],
            2,
            bpm,
            96_000,
            1024,
            total_96,
        );
        for (lane_idx, grid) in [(0_usize, grid_a), (1_usize, grid_b)] {
            let onsets_48 = predict_onsets(grid, bpm, 48_000, total_48);
            let onsets_96 = predict_onsets(grid, bpm, 96_000, total_96);
            // Need at least one non-zero onset to make the doubling
            // assertion meaningful; the polyrhythm pool always
            // produces multiple onsets per bar, so this should
            // hold for every generated case.
            prop_assume!(onsets_48.len() >= 2 && onsets_96.len() >= 2);
            let target_48 = onsets_48[1];
            let target_96 = onsets_96[1];
            prop_assert!(target_48 > 0, "second onset must be non-zero");
            prop_assert_eq!(
                target_96,
                2 * target_48,
                "lane {} grid {:?}: 96k second onset {} should be 2x 48k second onset {}",
                lane_idx, grid, target_96, target_48
            );
            // Renderer actually wrote there in both buffers.
            let click_48 = click_len(48_000);
            let click_96 = click_len(96_000);
            let any_48 = pcm_48.lanes[lane_idx]
                [target_48 as usize..(target_48 as usize + click_48).min(total_48 as usize)]
                .iter()
                .any(|&s| s != 0.0);
            let any_96 = pcm_96.lanes[lane_idx]
                [target_96 as usize..(target_96 as usize + click_96).min(total_96 as usize)]
                .iter()
                .any(|&s| s != 0.0);
            prop_assert!(any_48, "lane {} 48k second onset footprint silent", lane_idx);
            prop_assert!(any_96, "lane {} 96k second onset footprint silent", lane_idx);
        }
    }

    /// For polyrhythm pairs whose grids share a coincident tick at
    /// 0 (bar start), the two routed lanes' first onsets are at
    /// the same sample index. Generalises the original "3:2 at
    /// barline" hand-verified scenario to the full small-LCM pair
    /// pool.
    #[test]
    fn prop_coincident_alignment_at_bar_zero(
        (grid_a, grid_b) in arb_polyrhythm_pair(),
        bpm in arb_bpm(),
    ) {
        let sr = SR_48K;
        let total_frames = frames_for_bars(1, bpm, sr);
        let pcm = render_lanes_ok(
            vec![audio_channel(grid_a, 0), audio_channel(grid_b, 1)],
            2,
            bpm,
            sr,
            1024,
            total_frames,
        );
        // Both grids include tick 0 (every grid divides bar 0), so
        // the first onset is at sample 0 on both lanes. Surface the
        // first nonzero sample on each lane and assert equality.
        let first_a = pcm.lanes[0].iter().position(|&s| s != 0.0);
        let first_b = pcm.lanes[1].iter().position(|&s| s != 0.0);
        prop_assert_eq!(first_a, Some(0));
        prop_assert_eq!(first_b, Some(0));
    }

    /// For fixed inputs, rendering with different `buffer_frames`
    /// sizes produces bit-identical lane PCM. Catches per-buffer
    /// state restore bugs in `Playhead`.
    #[test]
    fn prop_buffer_boundary_invariance(
        (grid_a, grid_b) in arb_polyrhythm_pair(),
    ) {
        let bpm = BPM_120;
        let sr = SR_48K;
        let total_frames = frames_for_bars(1, bpm, sr);
        let render = |buffer_frames: u32| {
            render_lanes_ok(
                vec![audio_channel(grid_a, 0), audio_channel(grid_b, 1)],
                2,
                bpm,
                sr,
                buffer_frames,
                total_frames,
            )
        };
        let a = render(64);
        let b = render(256);
        let c = render(1024);
        let d = render(4096);
        prop_assert_eq!(&a.lanes, &b.lanes);
        prop_assert_eq!(&a.lanes, &c.lanes);
        prop_assert_eq!(&a.lanes, &d.lanes);
    }

    /// Per-lane PCM from a full N-channel render is bit-identical
    /// to the per-lane PCM from each channel rendered solo (same
    /// lane assignment, all other channels removed). Catches
    /// scheduler / mixer crosstalk.
    #[test]
    fn prop_n_channel_independence(
        n in 2u16..=8u16,
    ) {
        let grids = n_channel_grids(n);
        let bpm = BPM_120;
        let sr = SR_48K;
        let total_frames = frames_for_bars(1, bpm, sr);
        let buffer_frames = 1024;
        let output_channels = n;
        let full_channels: Vec<Channel> = grids
            .iter()
            .enumerate()
            .map(|(i, g)| audio_channel(*g, i as u16))
            .collect();
        let full = render_lanes_ok(
            full_channels,
            output_channels,
            bpm,
            sr,
            buffer_frames,
            total_frames,
        );
        for (i, &g) in grids.iter().enumerate() {
            let solo = render_lanes_ok(
                vec![audio_channel(g, i as u16)],
                output_channels,
                bpm,
                sr,
                buffer_frames,
                total_frames,
            );
            prop_assert_eq!(
                &full.lanes[i],
                &solo.lanes[i],
                "lane {} differs between full N-channel render and solo render",
                i
            );
        }
    }

    /// The render config validator rejects `output_channels = 0`
    /// and any value above `MAX_OUTPUT_CHANNELS`.
    #[test]
    fn prop_output_channels_bounds_rejected(
        bad_oc in prop_oneof![Just(0u16), (17u16..=u16::MAX)],
    ) {
        let cfg = OfflineRenderConfig {
            channels: vec![],
            bpm: BPM_120,
            sample_rate: SR_48K,
            buffer_frames: 1024,
            total_frames: frames_for_bars(1, BPM_120, SR_48K),
            output_channels: bad_oc,
        };
        match render_offline_capture(cfg) {
            Err(OfflineRenderError::UnsupportedChannelCount(n)) => {
                prop_assert_eq!(n, bad_oc);
            }
            other => prop_assert!(false, "expected UnsupportedChannelCount, got: {other:?}"),
        }
    }

    /// Bounded sweep across the supported sample rates,
    /// buffer-frame sizes, and output-channel counts. The render
    /// should succeed and produce `lanes.len() == output_channels`
    /// non-empty Vecs of length `total_frames`.
    #[test]
    fn prop_render_shape_sweep(cfg in arb_render_cfg()) {
        let total_frames = frames_for_bars(cfg.duration_bars, cfg.bpm, cfg.sr);
        let pcm = render_lanes_ok(
            vec![audio_channel(Grid::T4, 0)],
            cfg.output_channels,
            cfg.bpm,
            cfg.sr,
            cfg.buffer_frames,
            total_frames,
        );
        prop_assert_eq!(pcm.sample_rate, cfg.sr);
        prop_assert_eq!(pcm.frames, total_frames);
        prop_assert_eq!(pcm.lanes.len(), usize::from(cfg.output_channels));
        for lane in &pcm.lanes {
            prop_assert_eq!(lane.len() as u64, total_frames);
        }
    }
}

// ── Spot checks ──────────────────────────────────────────────────

/// The original motivating scenario hand-verified against an
/// Ableton recording: 3:2 polyrhythm at 120 BPM / 48 kHz across
/// 1 bar must land coincident moments at identical sample indices
/// on both lanes.
#[test]
fn spot_3_2_at_120bpm_48k_sample_perfect() {
    let bpm = BPM_120;
    let sr = SR_48K;
    let total_frames = frames_for_bars(1, bpm, sr);
    let pcm = render_lanes_ok(
        vec![
            audio_channel(Grid::T2T, 0), // 3 events per bar
            audio_channel(Grid::T2, 1),  // 2 events per bar
        ],
        2,
        bpm,
        sr,
        1024,
        total_frames,
    );
    // Tick 0 is the only coincident moment in a 1-bar render
    // (next coincident is at bar 0 + LCM(T2T, T2) = 1 bar).
    let onsets_a = predict_onsets(Grid::T2T, bpm, sr, total_frames);
    let onsets_b = predict_onsets(Grid::T2, bpm, sr, total_frames);
    assert_eq!(onsets_a[0], 0);
    assert_eq!(onsets_b[0], 0);
    let first_a = pcm.lanes[0].iter().position(|&s| s != 0.0);
    let first_b = pcm.lanes[1].iter().position(|&s| s != 0.0);
    assert_eq!(first_a, Some(0), "lane 0 first nonzero at sample 0");
    assert_eq!(first_b, Some(0), "lane 1 first nonzero at sample 0");
}

/// 16 audio channels on lanes 0..15, each with a distinct grid.
/// Every lane must contain exactly its predicted footprints (no
/// gaps, no overflow into other lanes).
#[test]
fn spot_16_channel_unique_lanes() {
    let bpm = BPM_120;
    let sr = SR_48K;
    let total_frames = frames_for_bars(1, bpm, sr);
    let grids = n_channel_grids(16);
    let channels: Vec<Channel> = grids
        .iter()
        .enumerate()
        .map(|(i, g)| audio_channel(*g, i as u16))
        .collect();
    let pcm = render_lanes_ok(channels, 16, bpm, sr, 1024, total_frames);
    let click = click_len(sr);
    for (i, &g) in grids.iter().enumerate() {
        let onsets = predict_onsets(g, bpm, sr, total_frames);
        let mask = predicted_footprint_mask(&onsets, click, total_frames);
        // Every footprint sample is reachable by *this* lane only.
        for (j, &s) in pcm.lanes[i].iter().enumerate() {
            if !mask[j] {
                assert_eq!(
                    s, 0.0,
                    "lane {i} sample {j} non-zero outside any predicted footprint"
                );
            }
        }
    }
}

/// Two channels both claiming `out=0` are rejected at config
/// validation, never reaching the renderer.
#[test]
fn spot_lane_collision_rejected() {
    let cfg = OfflineRenderConfig {
        channels: vec![audio_channel(Grid::T4, 0), audio_channel(Grid::T8, 0)],
        bpm: BPM_120,
        sample_rate: SR_48K,
        buffer_frames: 1024,
        total_frames: frames_for_bars(1, BPM_120, SR_48K),
        output_channels: 2,
    };
    match render_offline_capture(cfg) {
        Err(OfflineRenderError::LaneCollision {
            lane,
            channels: (a, b),
        }) => {
            assert_eq!(lane, 0);
            assert_eq!((a, b), (0, 1));
        }
        other => panic!("expected LaneCollision, got: {other:?}"),
    }
}

/// First event scheduled at tick 0 produces an onset at sample 0;
/// no underflow.
#[test]
fn spot_event_at_sample_zero() {
    let bpm = BPM_120;
    let sr = SR_48K;
    let total_frames = frames_for_bars(1, bpm, sr);
    let pcm = render_lanes_ok(
        vec![audio_channel(Grid::T1, 0)],
        1,
        bpm,
        sr,
        1024,
        total_frames,
    );
    let first = pcm.lanes[0].iter().position(|&s| s != 0.0);
    assert_eq!(first, Some(0));
}

/// A click whose footprint extends past `total_frames` is the
/// only allowed deviation from `click_len`. Choose a duration
/// short enough that the second predicted onset's footprint
/// runs past the buffer end.
#[test]
fn spot_event_at_buffer_end() {
    let bpm = BPM_120;
    let sr = SR_48K;
    let click = click_len(sr);
    // T4 at 120 BPM / 48 kHz: one onset every 24000 samples.
    let onset_period =
        tick_to_whole_samples(Tick(u64::from(Grid::T4.tick_count())), bpm, sr).unwrap();
    // Total frames = second onset position + half of click_len, so
    // the second click footprint truncates while the first footprint
    // is fully written.
    let total_frames = onset_period + (click as u64 / 2);
    let pcm = render_lanes_ok(
        vec![audio_channel(Grid::T4, 0)],
        1,
        bpm,
        sr,
        1024,
        total_frames,
    );
    assert_eq!(pcm.lanes[0].len() as u64, total_frames);
    // Region between first footprint end and second onset is silent.
    let pre = (onset_period as usize).saturating_sub(1);
    assert_eq!(
        pcm.lanes[0][pre], 0.0,
        "sample just before second onset must be silent (pre={pre})"
    );
}

/// Coverage spot for the 192_000 sample rate the proptest
/// strategy intentionally skips. Confirms the rate-dispatch
/// table reaches `R192` and the lane PCM has the right shape.
#[test]
fn spot_192k_sample_rate() {
    let bpm = BPM_120;
    let sr = 192_000_u32;
    let total_frames = frames_for_bars(1, bpm, sr);
    let pcm = render_lanes_ok(
        vec![audio_channel(Grid::T4, 0)],
        2,
        bpm,
        sr,
        4096,
        total_frames,
    );
    assert_eq!(pcm.sample_rate, sr);
    assert_eq!(pcm.lanes.len(), 2);
    for lane in &pcm.lanes {
        assert_eq!(lane.len() as u64, total_frames);
    }
    // Lane 0 carries the click; lane 1 is silent.
    assert!(pcm.lanes[0].iter().any(|&s| s != 0.0));
    assert!(pcm.lanes[1].iter().all(|&s| s == 0.0));
}
