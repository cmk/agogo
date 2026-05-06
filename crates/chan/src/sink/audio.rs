//! Audio-host trait + RT-callback shape.
//!
//! Mirrors the `MidiSink` / `MidirSink` split: core holds the trait
//! contract, back-end crates hold the implementations. `host-cpal` is
//! the first implementor; future JACK / CoreAudio /
//! ASIO back-ends plug in via the same shape.
//!
//! See `doc/agogo.md` §5 for the pinned trait shape and
//! `doc/designs/control-plane.md` for the RT-safety contract.
//!
//! **RT-safety contract.** The callback handed to [`AudioHost::run`]
//! runs on the host's audio thread. Implementations must not block,
//! allocate, or take locks inside the callback. Parameter updates
//! from the control thread come in via an atomic snapshot at the top
//! of each buffer (`control-plane.md`, v0.2 scope).

use thiserror::Error;

use crate::channel::{AudioRole, CvRole, ScheduledEvent};

const AUDIO_CLICK_ACCENT_EVERY: u32 = 4;
const AUDIO_CLICK_NORMAL_AMP_Q15: i32 = 4_000;
const AUDIO_CLICK_ACCENT_AMP_Q15: i32 = 7_000;
const AUDIO_CLICK_MIN_FRAMES: usize = 24;
const AUDIO_CLICK_MAX_FRAMES: usize = 960;
const AUDIO_CLICK_MIN_CUTOFF_HZ: u32 = 200;
const AUDIO_CLICK_MAX_CUTOFF_HZ: u32 = 8_000;
const AUDIO_CLICK_CUTOFF_SPAN_HZ: u32 = AUDIO_CLICK_MAX_CUTOFF_HZ - AUDIO_CLICK_MIN_CUTOFF_HZ + 1;
const CV_PULSE_POSITIVE: f32 = 1.0_f32; // PCM ABI
const CV_PULSE_NEGATIVE: f32 = -1.0_f32; // PCM ABI

/// Cross-platform audio-host trait. Concrete back-ends live in
/// sibling crates (`host-cpal`, future `host-jack`, ...) and
/// implement this trait against their native audio stream API.
pub trait AudioHost {
    /// Start the audio stream with `cb` installed as the per-buffer
    /// callback. Ownership of the underlying platform stream lives
    /// inside the returned [`Handle`]; dropping the handle tears the
    /// stream down.
    fn run(
        self,
        cfg: Config,
        cb: Box<dyn FnMut(&mut AudioIo) + Send>,
    ) -> Result<Handle, AudioHostError>;
}

/// Per-buffer callback payload.
///
/// **Channel layout.** `input` is mono in the current host shape.
/// `output` is interleaved when `output_channels > 1`; renderers
/// interpret `frames` as the number of frames, not the raw output
/// slice length. Pattern matches against `AudioIo` should use `..`
/// to ride the `#[non_exhaustive]` forward-compat.
///
/// Marked `#[non_exhaustive]` so v0.3's Link work can add a cpal
/// `timestamp().playback` field without breaking downstream pattern
/// matches — `doc/designs/link.md:23-29` requires the
/// "first-sample-hits-DAC" instant for sync-accurate Link queries.
#[non_exhaustive]
pub struct AudioIo<'a> {
    /// Captured input samples for this buffer. Empty when the host
    /// was opened without an input device.
    pub input: &'a [f32],
    /// Output buffer for this buffer. Empty in input-only configs.
    /// When non-empty, back-ends give the callback undefined-content
    /// memory and the callback must write every sample
    /// (`doc/designs/cv-pulse.md:47-52`).
    pub output: &'a mut [f32],
    /// Stream-global sample index of `input[0]` / `output[0]`.
    /// Monotonic across calls within a single stream session.
    pub buffer_start_sample: u64,
    /// Sample rate reported by the host for this stream.
    pub sample_rate: u32,
    /// Number of frames (samples per channel) in this buffer.
    pub frames: usize,
    /// Number of interleaved output channels in `output`. Zero when
    /// the stream has no output buffer.
    pub output_channels: u16,
}

impl<'a> AudioIo<'a> {
    /// Construct an `AudioIo` for a back-end's per-buffer callback.
    /// This preserves the original mono-output shorthand; stereo
    /// and other interleaved output paths must call
    /// [`Self::with_output_channels`] so the frame layout is
    /// explicit.
    pub fn new(
        input: &'a [f32],
        output: &'a mut [f32],
        buffer_start_sample: u64,
        sample_rate: u32,
        frames: usize,
    ) -> Self {
        let output_channels = if output.is_empty() { 0 } else { 1 };
        Self::with_output_channels(
            input,
            output,
            buffer_start_sample,
            sample_rate,
            frames,
            output_channels,
        )
    }

    pub fn with_output_channels(
        input: &'a [f32],
        output: &'a mut [f32],
        buffer_start_sample: u64,
        sample_rate: u32,
        frames: usize,
        output_channels: u16,
    ) -> Self {
        Self {
            input,
            output,
            buffer_start_sample,
            sample_rate,
            frames,
            output_channels,
        }
    }
}

/// Stream configuration passed to [`AudioHost::run`].
#[derive(Debug, Clone)]
pub struct Config {
    /// Input device name; `None` selects the host's default input.
    /// Back-ends that don't know how to resolve a name return
    /// [`AudioHostError::DeviceNotFound`].
    pub input_device: Option<String>,
    /// Output device name; `None` selects the host's default output.
    /// Current callers pass `None` for the default output device.
    pub output_device: Option<String>,
    /// Target sample rate. Back-ends surface unsupported rates as
    /// [`AudioHostError::UnsupportedSampleRate`] rather than
    /// silently resampling.
    pub sample_rate: u32,
    /// Target buffer size in frames. Back-ends may round to the
    /// nearest value the OS allows.
    pub buffer_frames: u32,
    pub input_channels: u16,
    pub output_channels: u16,
}

/// Opaque stream handle. Back-ends wrap whatever they need to own
/// for the stream's lifetime (a `cpal::Stream`, a JACK client, ...)
/// inside the payload; dropping the `Handle` drops the payload,
/// which is how back-ends tear down their streams without exposing
/// a platform-specific type in `agogo-core`.
pub struct Handle {
    _payload: Box<dyn std::any::Any + Send>,
}

/// Fixed MVP CV pulse shape.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum CvPulseShape {
    /// One positive sample per scheduled event.
    Monopolar,
    /// One positive sample followed by one negative reset sample.
    Bipolar,
}

/// Per-channel CV pulse render state.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct CvPulseState {
    shape: CvPulseShape,
    pending_bipolar_reset: bool,
}

/// Per-channel generated metronome-click state.
///
/// `pending_samples` and `pending_accent` track the tail of an
/// in-progress click that was truncated by a buffer boundary; the
/// next [`render_audio_click_block`] call resumes that click at
/// output offset 0 before processing new events. This makes the
/// rendered audio bit-identical across buffer-frame sizes for the
/// musical configurations the proptest battery exercises (event
/// spacing >= `click_len`).
///
/// **Known limitation — overlapping-click tails.** Only the
/// most-recently-truncated click's tail is preserved across a
/// boundary. If two or more clicks from the same channel both
/// straddle the same boundary (possible for fine grids like
/// `T256` / `T512P` at high BPM where event spacing drops below
/// `click_len`), only the latest tail resumes and the earlier
/// tails are silently lost. Properly fixing this requires a queue
/// of pending tails *and* decoupling state advancement from
/// output writing so the unsplit-rendering order is preserved
/// (the shared filter / RNG state currently interleaves between
/// overlapping clicks). Tracked as a follow-up; the proptest
/// `prop_buffer_boundary_invariance` does not exercise grids in
/// this regime.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct AudioClickState {
    click_counter: u32,
    rng_state: u32,
    filter_q15: i32,
    cutoff_hz: u32,
    /// Samples of the last truncated click that still need to be
    /// rendered at the start of the next buffer. 0 means no
    /// continuation pending.
    pending_samples: u16,
    /// Accent flag for the pending click (controls amplitude).
    pending_accent: bool,
}

impl AudioClickState {
    pub const fn new(channel_index: usize) -> Self {
        let seed = seed_for_channel(channel_index);
        Self {
            click_counter: 0,
            rng_state: seed,
            filter_q15: 0,
            cutoff_hz: cutoff_for_seed(seed),
            pending_samples: 0,
            pending_accent: false,
        }
    }

    pub const fn click_counter(&self) -> u32 {
        self.click_counter
    }

    pub const fn cutoff_hz(&self) -> u32 {
        self.cutoff_hz
    }

    pub fn reset(&mut self) {
        self.click_counter = 0;
        self.filter_q15 = 0;
        self.pending_samples = 0;
        self.pending_accent = false;
    }
}

impl CvPulseState {
    pub const fn new(shape: CvPulseShape) -> Self {
        Self {
            shape,
            pending_bipolar_reset: false,
        }
    }

    pub const fn bipolar() -> Self {
        Self::new(CvPulseShape::Bipolar)
    }

    pub fn reset(&mut self) {
        self.pending_bipolar_reset = false;
    }
}

impl Handle {
    /// Wrap a back-end-specific payload. `Any + Send` gives us
    /// type-erased ownership without requiring downcasting: the
    /// `Handle` doesn't *do* anything with the payload except drop
    /// it when the handle itself is dropped.
    pub fn from_payload<T: std::any::Any + Send>(payload: T) -> Self {
        Self {
            _payload: Box::new(payload),
        }
    }
}

impl std::fmt::Debug for Handle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Handle").finish_non_exhaustive()
    }
}

/// Errors a back-end can surface from [`AudioHost::run`].
///
/// Marked `#[non_exhaustive]` so additional variants (per-back-end
/// specifics, v0.4's CV-output side, etc.) can land without
/// breaking downstream `match` statements — pattern-match callers
/// should always use a `_` fallback arm.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AudioHostError {
    #[error("no default input device")]
    NoInputDevice,
    #[error("no default output device")]
    NoOutputDevice,
    #[error("device not found: {0}")]
    DeviceNotFound(String),
    /// The requested `Config::sample_rate` isn't in any of the
    /// device's supported-config ranges. Distinct from
    /// [`Self::UnsupportedConfig`] which covers format / channel
    /// mismatches when the rate itself IS supported.
    #[error("unsupported sample rate: {0}")]
    UnsupportedSampleRate(u32),
    /// The device's supported configs cover the requested sample
    /// rate but not the full `Config` combination — typically a
    /// sample-format or channel-count mismatch. The string details
    /// what was actually available vs. requested.
    #[error("unsupported config: {0}")]
    UnsupportedConfig(String),
    /// Back-end-specific failure (cpal build error, JACK client
    /// error, etc.). Wrapped so `agogo-core` can surface the message
    /// without linking the back-end's error type.
    #[error("back-end: {0}")]
    Backend(Box<dyn std::error::Error + Send + Sync>),
}

/// Render generated audio clicks into the current output buffer.
///
/// Test-feature renderer: click shape is intentionally fixed in
/// code rather than exposed through `--ch` parameters. For interleaved
/// output, `output_channel` selects the lane that this logical click
/// channel writes.
pub fn render_audio_click_block(
    events: &[ScheduledEvent],
    role: &AudioRole,
    state: &mut AudioClickState,
    output_channel: usize,
    io: &mut AudioIo<'_>,
) {
    if io.output.is_empty() {
        return;
    }

    match role {
        AudioRole::Click => {
            let channels = usize::from(io.output_channels);
            if channels == 0 || output_channel >= channels {
                return;
            }
            let writable = io.frames.min(io.output.len() / channels);
            // Resume any click that was truncated by the previous
            // buffer boundary. Resume happens at output offset 0
            // (the first frame of this buffer) and consumes the
            // first `pending_samples` writable frames.
            if state.pending_samples > 0 {
                let len = click_len(io.sample_rate);
                let pending = state.pending_samples as usize;
                let env_start = len.saturating_sub(pending);
                let written = render_click_samples(
                    0,
                    output_channel,
                    channels,
                    state.pending_accent,
                    io.sample_rate,
                    env_start,
                    pending,
                    state,
                    io.output,
                );
                state.pending_samples = (pending - written) as u16;
                if state.pending_samples > 0 {
                    // Buffer was too short to finish the pending
                    // click; new events for this buffer fall after
                    // the pending region's end and would have
                    // overlapped it anyway — we still process
                    // them, but with reduced writable space.
                }
            }

            for ev in events {
                let Some(offset) = ev.sample_index.checked_sub(io.buffer_start_sample) else {
                    continue;
                };
                let offset = offset as usize;
                if offset >= writable {
                    continue;
                }
                let accent = state.click_counter.is_multiple_of(AUDIO_CLICK_ACCENT_EVERY);
                state.click_counter = state.click_counter.wrapping_add(1);
                let len = click_len(io.sample_rate);
                let written = render_click_samples(
                    offset,
                    output_channel,
                    channels,
                    accent,
                    io.sample_rate,
                    /* env_start = */ 0,
                    /* requested = */ len,
                    state,
                    io.output,
                );
                if written < len {
                    // The click was truncated by the buffer end.
                    // Save the tail so the next call resumes it.
                    // Only the most-recently-truncated click is
                    // preserved; earlier overlapping truncations
                    // lose their tails (see `AudioClickState`
                    // doc).
                    state.pending_samples = (len - written) as u16;
                    state.pending_accent = accent;
                }
            }
        }
    }
}

/// Render fixed-shape CV pulse events into the current output buffer.
///
/// The public `dev=cv,mode=pulse` surface uses bipolar pulses by
/// default via [`CvPulseState::bipolar`]. The explicit state parameter
/// keeps the buffer-boundary reset flag per channel and lets tests
/// exercise the monopolar shape without adding CLI knobs.
pub fn render_cv_pulse_block(
    events: &[ScheduledEvent],
    role: &CvRole,
    state: &mut CvPulseState,
    cv_positive_samples: &mut [bool],
    io: &mut AudioIo<'_>,
) {
    if io.output.is_empty() {
        return;
    }

    match role {
        CvRole::Pulse => {
            let channels = usize::from(io.output_channels);
            if channels == 0 {
                return;
            }
            let writable = io.frames.min(io.output.len() / channels);
            if writable == 0 {
                return;
            }

            if state.pending_bipolar_reset {
                if !cv_positive_at(cv_positive_samples, 0)
                    && !events_contain_sample(events, io.buffer_start_sample)
                {
                    write_cv_frame(&mut io.output[..], channels, 0, CV_PULSE_NEGATIVE);
                }
                state.pending_bipolar_reset = false;
            }

            for ev in events {
                let Some(offset) = ev.sample_index.checked_sub(io.buffer_start_sample) else {
                    continue;
                };
                let offset = offset as usize;
                if offset >= writable {
                    continue;
                }
                write_cv_frame(&mut io.output[..], channels, offset, CV_PULSE_POSITIVE);
                mark_cv_positive(cv_positive_samples, offset);
                if state.shape == CvPulseShape::Bipolar {
                    let reset = offset + 1;
                    let reset_sample = ev.sample_index.saturating_add(1);
                    if cv_positive_at(cv_positive_samples, reset)
                        || events_contain_sample(events, reset_sample)
                    {
                        continue;
                    }
                    if reset < writable {
                        write_cv_frame(&mut io.output[..], channels, reset, CV_PULSE_NEGATIVE);
                    } else {
                        state.pending_bipolar_reset = true;
                    }
                }
            }
        }
        CvRole::Lfo => {}
    }
}

fn events_contain_sample(events: &[ScheduledEvent], sample_index: u64) -> bool {
    debug_assert!(
        events
            .windows(2)
            .all(|pair| pair[0].sample_index <= pair[1].sample_index)
    );
    events
        .binary_search_by_key(&sample_index, |ev| ev.sample_index)
        .is_ok()
}

fn write_cv_frame(output: &mut [f32], channels: usize, frame: usize, sample: f32) {
    let start = frame.saturating_mul(channels);
    let Some(dst) = output.get_mut(start..start.saturating_add(channels)) else {
        return;
    };
    dst.fill(sample); // PCM ABI
}

fn mark_cv_positive(mask: &mut [bool], offset: usize) {
    if let Some(slot) = mask.get_mut(offset) {
        *slot = true;
    }
}

fn cv_positive_at(mask: &[bool], offset: usize) -> bool {
    mask.get(offset).copied().unwrap_or(false)
}

/// Render up to `requested` consecutive samples of a click
/// envelope, starting at envelope index `env_start`. Returns the
/// number of samples actually written (may be less than
/// `requested` when the buffer ends first).
///
/// The envelope is `(len - env_index)` for `env_index in
/// 0..len`, where `len = click_len(sample_rate)`. Resuming a
/// truncated click means calling this with `env_start = len -
/// pending_samples` and `requested = pending_samples`.
#[allow(clippy::too_many_arguments)]
fn render_click_samples(
    start: usize,
    output_channel: usize,
    channels: usize,
    accent: bool,
    sample_rate: u32,
    env_start: usize,
    requested: usize,
    state: &mut AudioClickState,
    output: &mut [f32],
) -> usize {
    let len = click_len(sample_rate);
    let amp = if accent {
        AUDIO_CLICK_ACCENT_AMP_Q15
    } else {
        AUDIO_CLICK_NORMAL_AMP_Q15
    };
    let alpha_q15 = lowpass_alpha_q15(state.cutoff_hz, sample_rate);

    let mut written = 0;
    for i in 0..requested {
        let env_index = env_start + i;
        if env_index >= len {
            break;
        }
        let frame = start + i;
        let sample_index = frame
            .saturating_mul(channels)
            .saturating_add(output_channel);
        let Some(dst) = output.get_mut(sample_index) else {
            break;
        };
        let envelope = (len - env_index) as i32;
        let noise_q15 = next_noise_q15(state);
        let delta = noise_q15 - state.filter_q15;
        state.filter_q15 += (delta * alpha_q15) >> 15;
        let sample_q15 = state.filter_q15 * amp / 32_768 * envelope / len as i32;
        mix_q15(dst, sample_q15);
        written = i + 1;
    }
    written
}

fn click_len(sample_rate: u32) -> usize {
    (sample_rate as usize / 250).clamp(AUDIO_CLICK_MIN_FRAMES, AUDIO_CLICK_MAX_FRAMES)
}

fn mix_q15(dst: &mut f32, sample_q15: i32) {
    // PCM ABI: convert the generated fixed-point sample at the
    // output boundary and clamp the mixed output to the f32 PCM
    // range cpal expects.
    let sample = sample_q15 as f32 / 32_768.0_f32; // PCM ABI
    mix_pcm(dst, sample);
}

fn mix_pcm(dst: &mut f32, sample: f32) {
    *dst = (*dst + sample).clamp(-1.0_f32, 1.0_f32); // PCM ABI
}

const fn seed_for_channel(channel_index: usize) -> u32 {
    let n = channel_index as u32;
    let seed = 0x9E37_79B9_u32 ^ n.wrapping_mul(0x85EB_CA6B);
    if seed == 0 { 0xA5A5_5A5A } else { seed }
}

const fn cutoff_for_seed(seed: u32) -> u32 {
    AUDIO_CLICK_MIN_CUTOFF_HZ + seed % AUDIO_CLICK_CUTOFF_SPAN_HZ
}

fn lowpass_alpha_q15(cutoff_hz: u32, sample_rate: u32) -> i32 {
    let denom = cutoff_hz.saturating_add(sample_rate).max(1);
    ((u64::from(cutoff_hz) * 32_768) / u64::from(denom)) as i32
}

fn next_noise_q15(state: &mut AudioClickState) -> i32 {
    let mut x = state.rng_state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    state.rng_state = if x == 0 { 0xA5A5_5A5A } else { x };
    ((state.rng_state >> 16) as i32) - 32_768
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::tick::Tick;
    use proptest::prelude::*;

    /// `Handle` drops its payload, which is how back-ends signal
    /// stream teardown. Uses an `Arc<AtomicBool>` witness: the
    /// payload sets the flag in its `Drop` impl; dropping the
    /// `Handle` must flip the flag.
    #[test]
    fn handle_drops_payload() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        struct Witness {
            flag: Arc<AtomicBool>,
        }
        impl Drop for Witness {
            fn drop(&mut self) {
                self.flag.store(true, Ordering::SeqCst);
            }
        }

        let flag = Arc::new(AtomicBool::new(false));
        let handle = Handle::from_payload(Witness {
            flag: Arc::clone(&flag),
        });
        assert!(!flag.load(Ordering::SeqCst));
        drop(handle);
        assert!(flag.load(Ordering::SeqCst));
    }

    /// The `Send` bound on the `AudioHost::run` callback plus
    /// `Handle`'s `Any + Send` payload lets a back-end move a
    /// stream across threads. Trait-object construction check —
    /// compiles iff the bounds line up.
    #[test]
    fn audio_host_trait_object_is_constructible() {
        fn _accepts_dyn_audio_host(_h: Box<dyn AudioHost>) {}
    }

    fn event(sample_index: u64) -> ScheduledEvent {
        ScheduledEvent {
            sample_index,
            tick: Tick(0),
        }
    }

    fn cv_mask(frames: usize) -> Vec<bool> {
        vec![false; frames]
    }

    #[test]
    fn audio_click_events_write_nonzero_samples() {
        let input: [f32; 0] = []; // PCM ABI
        let mut output = vec![0.0_f32; 128]; // PCM ABI
        let mut io = AudioIo::new(&input, &mut output, 1_000, 48_000, 128);
        let mut state = AudioClickState::new(0);

        render_audio_click_block(&[event(1_010)], &AudioRole::Click, &mut state, 0, &mut io);

        assert!(io.output.iter().any(|&s| s != 0.0));
        assert_eq!(state.click_counter(), 1);
    }

    #[test]
    fn audio_click_ignores_out_of_buffer_events() {
        let input: [f32; 0] = []; // PCM ABI
        let mut output = vec![0.0_f32; 64]; // PCM ABI
        let mut io = AudioIo::new(&input, &mut output, 1_000, 48_000, 64);
        let mut state = AudioClickState::new(0);

        render_audio_click_block(
            &[event(999), event(1_064)],
            &AudioRole::Click,
            &mut state,
            0,
            &mut io,
        );

        assert!(io.output.iter().all(|&s| s == 0.0));
        assert_eq!(state.click_counter(), 0);
    }

    #[test]
    fn audio_accent_lands_every_n_emitted_clicks_from_zero() {
        let input: [f32; 0] = []; // PCM ABI
        let mut state = AudioClickState::new(0);

        // Render two clicks back-to-back in one buffer, spaced
        // far enough apart that the accent click fully completes
        // before the normal click begins (192-sample click at 48
        // kHz, 256-sample spacing). The accent click is click 0
        // (counter starts at 0, 0 % 4 == 0); the normal click is
        // click 1.
        let mut output = vec![0.0_f32; 512]; // PCM ABI
        let mut io = AudioIo::new(&input, &mut output, 0, 48_000, 512);
        render_audio_click_block(
            &[event(0), event(256)],
            &AudioRole::Click,
            &mut state,
            0,
            &mut io,
        );

        assert_eq!(state.click_counter(), 2);
        // Compare per-click peak magnitudes: filter state at the
        // very first sample of each click is wherever the prior
        // click left it, which makes per-sample comparison
        // brittle. Peak-over-the-click-window is the stable
        // signal the accent amplification is supposed to produce.
        let click_len_48k = click_len(48_000);
        let accent_peak = output[0..click_len_48k]
            .iter()
            .fold(0.0_f32, |a, &b| a.max(b.abs()));
        let normal_peak = output[256..256 + click_len_48k]
            .iter()
            .fold(0.0_f32, |a, &b| a.max(b.abs()));
        assert!(
            accent_peak > normal_peak,
            "accent peak {accent_peak} should exceed normal peak {normal_peak}"
        );
    }

    #[test]
    fn audio_click_counter_advances_across_buffer_boundaries() {
        let input: [f32; 0] = []; // PCM ABI
        let events = [event(10), event(250)];
        let mut combined = vec![0.0_f32; 500]; // PCM ABI
        let mut split = vec![0.0_f32; 500]; // PCM ABI

        let mut combined_state = AudioClickState::new(0);
        {
            let mut io = AudioIo::new(&input, &mut combined, 0, 48_000, 500);
            render_audio_click_block(&events, &AudioRole::Click, &mut combined_state, 0, &mut io);
        }

        let mut split_state = AudioClickState::new(0);
        {
            let (left, right) = split.split_at_mut(240);
            let mut io = AudioIo::new(&input, left, 0, 48_000, 240);
            render_audio_click_block(&events, &AudioRole::Click, &mut split_state, 0, &mut io);
            let mut io = AudioIo::new(&input, right, 240, 48_000, 260);
            render_audio_click_block(&events, &AudioRole::Click, &mut split_state, 0, &mut io);
        }

        assert_eq!(combined_state.click_counter(), split_state.click_counter());
        assert_eq!(combined, split);
    }

    #[test]
    fn audio_click_writes_only_selected_stereo_lane() {
        let input: [f32; 0] = []; // PCM ABI
        let mut output = vec![0.0_f32; 128]; // PCM ABI
        let mut io = AudioIo::with_output_channels(&input, &mut output, 0, 48_000, 64, 2);
        let mut state = AudioClickState::new(1);

        render_audio_click_block(&[event(0)], &AudioRole::Click, &mut state, 1, &mut io);

        assert!(io.output.chunks_exact(2).any(|frame| frame[1] != 0.0));
        assert!(io.output.chunks_exact(2).all(|frame| frame[0] == 0.0));
    }

    #[test]
    fn audio_click_cutoff_is_stable_and_in_range() {
        let left = AudioClickState::new(0);
        let left_again = AudioClickState::new(0);
        let right = AudioClickState::new(1);

        assert_eq!(left.cutoff_hz(), left_again.cutoff_hz());
        assert_ne!(left.cutoff_hz(), right.cutoff_hz());
        assert!(
            (AUDIO_CLICK_MIN_CUTOFF_HZ..=AUDIO_CLICK_MAX_CUTOFF_HZ).contains(&left.cutoff_hz())
        );
        assert!(
            (AUDIO_CLICK_MIN_CUTOFF_HZ..=AUDIO_CLICK_MAX_CUTOFF_HZ).contains(&right.cutoff_hz())
        );
    }

    #[test]
    fn cv_monopolar_pulse_writes_one_positive_sample() {
        let input: [f32; 0] = []; // PCM ABI
        let mut output = vec![0.0_f32; 8]; // PCM ABI
        let mut io = AudioIo::new(&input, &mut output, 10, 48_000, 8);
        let mut state = CvPulseState::new(CvPulseShape::Monopolar);
        let mut mask = cv_mask(8);

        render_cv_pulse_block(&[event(13)], &CvRole::Pulse, &mut state, &mut mask, &mut io);

        assert_eq!(io.output[3], 1.0);
        assert_eq!(io.output.iter().filter(|&&s| s != 0.0).count(), 1);
    }

    #[test]
    fn cv_bipolar_pulse_writes_reset_sample() {
        let input: [f32; 0] = []; // PCM ABI
        let mut output = vec![0.0_f32; 8]; // PCM ABI
        let mut io = AudioIo::new(&input, &mut output, 10, 48_000, 8);
        let mut state = CvPulseState::bipolar();
        let mut mask = cv_mask(8);

        render_cv_pulse_block(&[event(13)], &CvRole::Pulse, &mut state, &mut mask, &mut io);

        assert_eq!(io.output[3], 1.0);
        assert_eq!(io.output[4], -1.0);
        assert_eq!(io.output.iter().filter(|&&s| s != 0.0).count(), 2);
    }

    #[test]
    fn cv_pulse_writes_dual_mono_in_stereo_output() {
        let input: [f32; 0] = []; // PCM ABI
        let mut output = vec![0.0_f32; 16]; // PCM ABI
        let mut io = AudioIo::with_output_channels(&input, &mut output, 10, 48_000, 8, 2);
        let mut state = CvPulseState::new(CvPulseShape::Monopolar);
        let mut mask = cv_mask(8);

        render_cv_pulse_block(&[event(13)], &CvRole::Pulse, &mut state, &mut mask, &mut io);

        assert_eq!(io.output[6], 1.0);
        assert_eq!(io.output[7], 1.0);
        assert_eq!(io.output.iter().filter(|&&s| s != 0.0).count(), 2);
    }

    #[test]
    fn cv_bipolar_reset_spills_to_next_buffer() {
        let input: [f32; 0] = []; // PCM ABI
        let mut state = CvPulseState::bipolar();

        let mut first = vec![0.0_f32; 4]; // PCM ABI
        {
            let mut io = AudioIo::new(&input, &mut first, 0, 48_000, 4);
            let mut mask = cv_mask(4);
            render_cv_pulse_block(&[event(3)], &CvRole::Pulse, &mut state, &mut mask, &mut io);
        }
        assert_eq!(first, vec![0.0, 0.0, 0.0, 1.0]);

        let mut second = vec![0.0_f32; 4]; // PCM ABI
        {
            let mut io = AudioIo::new(&input, &mut second, 4, 48_000, 4);
            let mut mask = cv_mask(4);
            render_cv_pulse_block(&[], &CvRole::Pulse, &mut state, &mut mask, &mut io);
        }
        assert_eq!(second, vec![-1.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn cv_bipolar_reset_does_not_cancel_adjacent_pulse() {
        let input: [f32; 0] = []; // PCM ABI
        let mut output = vec![0.0_f32; 8]; // PCM ABI
        let mut io = AudioIo::new(&input, &mut output, 10, 48_000, 8);
        let mut state = CvPulseState::bipolar();
        let mut mask = cv_mask(8);

        render_cv_pulse_block(
            &[event(13), event(14)],
            &CvRole::Pulse,
            &mut state,
            &mut mask,
            &mut io,
        );

        assert_eq!(io.output[3], 1.0);
        assert_eq!(io.output[4], 1.0);
        assert_eq!(io.output[5], -1.0);
    }

    #[test]
    fn cv_bipolar_reset_does_not_cancel_adjacent_channel_pulse() {
        let input: [f32; 0] = []; // PCM ABI
        let mut output = vec![0.0_f32; 8]; // PCM ABI
        let mut io = AudioIo::new(&input, &mut output, 10, 48_000, 8);
        let mut left = CvPulseState::bipolar();
        let mut right = CvPulseState::bipolar();
        let mut mask = cv_mask(8);

        render_cv_pulse_block(&[event(13)], &CvRole::Pulse, &mut left, &mut mask, &mut io);
        render_cv_pulse_block(&[event(14)], &CvRole::Pulse, &mut right, &mut mask, &mut io);

        assert_eq!(io.output[3], 1.0);
        assert_eq!(io.output[4], 1.0);
        assert_eq!(io.output[5], -1.0);
    }

    #[test]
    fn cv_later_reset_does_not_cancel_earlier_channel_pulse() {
        let input: [f32; 0] = []; // PCM ABI
        let mut output = vec![0.0_f32; 8]; // PCM ABI
        let mut io = AudioIo::new(&input, &mut output, 10, 48_000, 8);
        let mut left = CvPulseState::bipolar();
        let mut right = CvPulseState::bipolar();
        let mut mask = cv_mask(8);

        render_cv_pulse_block(&[event(14)], &CvRole::Pulse, &mut right, &mut mask, &mut io);
        render_cv_pulse_block(&[event(13)], &CvRole::Pulse, &mut left, &mut mask, &mut io);

        assert_eq!(io.output[3], 1.0);
        assert_eq!(io.output[4], 1.0);
    }

    #[test]
    fn cv_bipolar_reset_overwrites_existing_click_sample() {
        let input: [f32; 0] = []; // PCM ABI
        let mut output = vec![0.0_f32; 8]; // PCM ABI
        output[4] = 0.5_f32; // PCM ABI
        let mut io = AudioIo::new(&input, &mut output, 10, 48_000, 8);
        let mut state = CvPulseState::bipolar();
        let mut mask = cv_mask(8);

        render_cv_pulse_block(&[event(13)], &CvRole::Pulse, &mut state, &mut mask, &mut io);

        assert_eq!(io.output[3], 1.0);
        assert_eq!(io.output[4], -1.0);
    }

    #[test]
    fn cv_bipolar_reset_preserves_existing_cv_positive_sample() {
        let input: [f32; 0] = []; // PCM ABI
        let mut output = vec![0.0_f32; 8]; // PCM ABI
        output[4] = 1.0_f32; // PCM ABI
        let mut io = AudioIo::new(&input, &mut output, 10, 48_000, 8);
        let mut state = CvPulseState::bipolar();
        let mut mask = cv_mask(8);
        mask[4] = true;

        render_cv_pulse_block(&[event(13)], &CvRole::Pulse, &mut state, &mut mask, &mut io);

        assert_eq!(io.output[3], 1.0);
        assert_eq!(io.output[4], 1.0);
    }

    proptest! {
        // Renderer-level property over bounded buffers: the input type
        // is a per-buffer PCM slice, so the meaningful domain is the
        // finite slice length handed to this function. Larger scheduling
        // domains are covered by transport/offline render tests.
        #[test]
        fn cv_impulse_sample_exact(
            frames in 1usize..=512,
            offset in 0usize..512,
            adjacent in any::<bool>(),
            sample_rate in prop::sample::select(&[44_100_u32, 48_000, 88_200, 96_000, 176_400, 192_000]),
        ) {
            let offset = offset % frames;
            let input: [f32; 0] = []; // PCM ABI
            let mut output = vec![0.0_f32; frames]; // PCM ABI
            let start = 1_000_u64;
            let mut io = AudioIo::new(&input, &mut output, start, sample_rate, frames);
            let mut state = CvPulseState::bipolar();
            let mut mask = cv_mask(frames);

            let mut events = vec![event(start + offset as u64)];
            if adjacent && offset + 1 < frames {
                events.push(event(start + offset as u64 + 1));
            }

            render_cv_pulse_block(&events, &CvRole::Pulse, &mut state, &mut mask, &mut io);

            prop_assert_eq!(io.output[offset], 1.0);
            if adjacent && offset + 1 < frames {
                prop_assert_eq!(io.output[offset + 1], 1.0);
            }
        }

        // Same bounded per-buffer domain as `cv_impulse_sample_exact`.
        // This pins the monopolar shape used by future cancellation
        // and calibration tests without exposing it on the CLI yet.
        #[test]
        fn cv_impulse_one_sample_energy(
            frames in 1usize..=512,
            offset in 0usize..512,
        ) {
            let offset = offset % frames;
            let input: [f32; 0] = []; // PCM ABI
            let mut output = vec![0.0_f32; frames]; // PCM ABI
            let start = 1_000_u64;
            let mut io = AudioIo::new(&input, &mut output, start, 48_000, frames);
            let mut state = CvPulseState::new(CvPulseShape::Monopolar);
            let mut mask = cv_mask(frames);

            render_cv_pulse_block(
                &[event(start + offset as u64)],
                &CvRole::Pulse,
                &mut state,
                &mut mask,
                &mut io,
            );

            prop_assert_eq!(io.output.iter().filter(|&&s| s != 0.0).count(), 1);
            prop_assert_eq!(io.output[offset], 1.0);
        }

        #[test]
        fn cv_bipolar_reset_crosses_buffer(frames in 1usize..=512) {
            let input: [f32; 0] = []; // PCM ABI
            let mut state = CvPulseState::bipolar();
            let start = 8_000_u64;

            let mut first = vec![0.0_f32; frames]; // PCM ABI
            {
                let mut io = AudioIo::new(&input, &mut first, start, 48_000, frames);
                let mut mask = cv_mask(frames);
                render_cv_pulse_block(
                    &[event(start + frames as u64 - 1)],
                    &CvRole::Pulse,
                    &mut state,
                    &mut mask,
                    &mut io,
                );
            }
            prop_assert_eq!(first[frames - 1], 1.0);

            let mut second = vec![0.0_f32; frames]; // PCM ABI
            {
                let mut io = AudioIo::new(&input, &mut second, start + frames as u64, 48_000, frames);
                let mut mask = cv_mask(frames);
                render_cv_pulse_block(&[], &CvRole::Pulse, &mut state, &mut mask, &mut io);
            }
            prop_assert_eq!(second[0], -1.0);
            prop_assert_eq!(second.iter().filter(|&&s| s != 0.0).count(), 1);
        }
    }

    /// Sanity match over every `AudioHostError` variant.
    ///
    /// Inside `agogo-core` (the defining crate), `#[non_exhaustive]`
    /// has no effect on exhaustiveness checks — adding a new
    /// variant here will fail to compile until this match is
    /// updated, which is what we want. Downstream crates DO see
    /// `#[non_exhaustive]`, and their own tests should include a
    /// wildcard arm; clippy's `unreachable_patterns` warns on
    /// that wildcard in this local match because it is unreachable
    /// here.
    #[test]
    fn audio_host_error_variants_constructible() {
        let errs = [
            AudioHostError::NoInputDevice,
            AudioHostError::NoOutputDevice,
            AudioHostError::DeviceNotFound("x".into()),
            AudioHostError::UnsupportedSampleRate(12_345),
            AudioHostError::UnsupportedConfig("no f32 at 48 kHz".into()),
            AudioHostError::Backend("stub".into()),
        ];
        for e in errs {
            match e {
                AudioHostError::NoInputDevice
                | AudioHostError::NoOutputDevice
                | AudioHostError::DeviceNotFound(_)
                | AudioHostError::UnsupportedSampleRate(_)
                | AudioHostError::UnsupportedConfig(_)
                | AudioHostError::Backend(_) => {}
            }
        }
    }
}
