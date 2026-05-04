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
const AUDIO_CLICK_NORMAL_FREQ_HZ: u32 = 1_200;
const AUDIO_CLICK_ACCENT_FREQ_HZ: u32 = 1_800;
const AUDIO_CLICK_NORMAL_AMP_Q15: i32 = 14_000;
const AUDIO_CLICK_ACCENT_AMP_Q15: i32 = 24_000;
const AUDIO_CLICK_MIN_FRAMES: usize = 24;
const AUDIO_CLICK_MAX_FRAMES: usize = 960;
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
/// **Channel layout.** `input` and `output` are mono in the current
/// host shape. Audio click and MVP CV pulse rendering both write to
/// this mono output slice. Multi-channel support arrives with
/// v0.4's heterogeneous output work, at which point this struct gains
/// explicit `input_channels` / `output_channels` fields and the
/// buffers carry interleaved frames. Pattern matches against
/// `AudioIo` should use `..` to ride the `#[non_exhaustive]`
/// forward-compat.
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
}

impl<'a> AudioIo<'a> {
    /// Construct an `AudioIo` for a back-end's per-buffer callback.
    /// Back-ends (like `host-cpal`) use this rather than the struct
    /// literal because `AudioIo` is `#[non_exhaustive]` for
    /// forward-compat with future fields (see the struct doc for
    /// the v0.3 Link timestamp rationale). When a new field lands,
    /// this constructor's signature breaks intentionally so every
    /// back-end is forced to acknowledge it.
    pub fn new(
        input: &'a [f32],
        output: &'a mut [f32],
        buffer_start_sample: u64,
        sample_rate: u32,
        frames: usize,
    ) -> Self {
        Self {
            input,
            output,
            buffer_start_sample,
            sample_rate,
            frames,
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
/// code rather than exposed through `--ch` parameters. The output
/// buffer is mono in this feature slice, matching [`AudioIo`]'s
/// current v0.1/v0.4 transition shape.
pub fn render_audio_click_block(
    events: &[ScheduledEvent],
    role: &AudioRole,
    click_counter: &mut u32,
    io: &mut AudioIo<'_>,
) {
    if io.output.is_empty() {
        return;
    }

    match role {
        AudioRole::Click => {
            let writable = io.output.len().min(io.frames);
            for ev in events {
                let Some(offset) = ev.sample_index.checked_sub(io.buffer_start_sample) else {
                    continue;
                };
                let offset = offset as usize;
                if offset >= writable {
                    continue;
                }
                let accent = (*click_counter).is_multiple_of(AUDIO_CLICK_ACCENT_EVERY);
                *click_counter = click_counter.wrapping_add(1);
                render_one_click(offset, accent, io.sample_rate, &mut io.output[..writable]);
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
    io: &mut AudioIo<'_>,
) {
    if io.output.is_empty() {
        return;
    }

    match role {
        CvRole::Pulse => {
            let writable = io.output.len().min(io.frames);
            if writable == 0 {
                return;
            }

            if state.pending_bipolar_reset {
                mix_pcm(&mut io.output[0], CV_PULSE_NEGATIVE);
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
                mix_pcm(&mut io.output[offset], CV_PULSE_POSITIVE);
                if state.shape == CvPulseShape::Bipolar {
                    let reset = offset + 1;
                    if reset < writable {
                        mix_pcm(&mut io.output[reset], CV_PULSE_NEGATIVE);
                    } else {
                        state.pending_bipolar_reset = true;
                    }
                }
            }
        }
        CvRole::Lfo => {}
    }
}

fn render_one_click(start: usize, accent: bool, sample_rate: u32, output: &mut [f32]) {
    let len = click_len(sample_rate);
    let freq = if accent {
        AUDIO_CLICK_ACCENT_FREQ_HZ
    } else {
        AUDIO_CLICK_NORMAL_FREQ_HZ
    };
    let amp = if accent {
        AUDIO_CLICK_ACCENT_AMP_Q15
    } else {
        AUDIO_CLICK_NORMAL_AMP_Q15
    };
    let half_period = (sample_rate / freq.saturating_mul(2)).max(1) as usize;

    for i in 0..len {
        let Some(dst) = output.get_mut(start + i) else {
            break;
        };
        let envelope = (len - i) as i32;
        let signed = if (i / half_period).is_multiple_of(2) {
            amp
        } else {
            -amp
        };
        let sample_q15 = signed * envelope / len as i32;
        mix_q15(dst, sample_q15);
    }
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

    #[test]
    fn audio_click_events_write_nonzero_samples() {
        let input: [f32; 0] = []; // PCM ABI
        let mut output = vec![0.0_f32; 128]; // PCM ABI
        let mut io = AudioIo::new(&input, &mut output, 1_000, 48_000, 128);
        let mut counter = 0;

        render_audio_click_block(&[event(1_010)], &AudioRole::Click, &mut counter, &mut io);

        assert!(io.output.iter().any(|&s| s != 0.0));
        assert_eq!(counter, 1);
    }

    #[test]
    fn audio_click_ignores_out_of_buffer_events() {
        let input: [f32; 0] = []; // PCM ABI
        let mut output = vec![0.0_f32; 64]; // PCM ABI
        let mut io = AudioIo::new(&input, &mut output, 1_000, 48_000, 64);
        let mut counter = 0;

        render_audio_click_block(
            &[event(999), event(1_064)],
            &AudioRole::Click,
            &mut counter,
            &mut io,
        );

        assert!(io.output.iter().all(|&s| s == 0.0));
        assert_eq!(counter, 0);
    }

    #[test]
    fn audio_accent_lands_every_n_emitted_clicks_from_zero() {
        let input: [f32; 0] = []; // PCM ABI
        let mut accent_output = vec![0.0_f32; 64]; // PCM ABI
        let mut normal_output = vec![0.0_f32; 64]; // PCM ABI
        let mut counter = 0;

        {
            let mut io = AudioIo::new(&input, &mut accent_output, 0, 48_000, 64);
            render_audio_click_block(&[event(0)], &AudioRole::Click, &mut counter, &mut io);
        }
        {
            let mut io = AudioIo::new(&input, &mut normal_output, 0, 48_000, 64);
            render_audio_click_block(&[event(0)], &AudioRole::Click, &mut counter, &mut io);
        }

        assert_eq!(counter, 2);
        assert!(accent_output[0].abs() > normal_output[0].abs());
    }

    #[test]
    fn audio_click_counter_advances_across_buffer_boundaries() {
        let input: [f32; 0] = []; // PCM ABI
        let events = [event(10), event(250)];
        let mut combined = vec![0.0_f32; 500]; // PCM ABI
        let mut split = vec![0.0_f32; 500]; // PCM ABI

        let mut combined_counter = 0;
        {
            let mut io = AudioIo::new(&input, &mut combined, 0, 48_000, 500);
            render_audio_click_block(&events, &AudioRole::Click, &mut combined_counter, &mut io);
        }

        let mut split_counter = 0;
        {
            let (left, right) = split.split_at_mut(240);
            let mut io = AudioIo::new(&input, left, 0, 48_000, 240);
            render_audio_click_block(&events, &AudioRole::Click, &mut split_counter, &mut io);
            let mut io = AudioIo::new(&input, right, 240, 48_000, 260);
            render_audio_click_block(&events, &AudioRole::Click, &mut split_counter, &mut io);
        }

        assert_eq!(combined_counter, split_counter);
        assert_eq!(combined, split);
    }

    #[test]
    fn cv_monopolar_pulse_writes_one_positive_sample() {
        let input: [f32; 0] = []; // PCM ABI
        let mut output = vec![0.0_f32; 8]; // PCM ABI
        let mut io = AudioIo::new(&input, &mut output, 10, 48_000, 8);
        let mut state = CvPulseState::new(CvPulseShape::Monopolar);

        render_cv_pulse_block(&[event(13)], &CvRole::Pulse, &mut state, &mut io);

        assert_eq!(io.output[3], 1.0);
        assert_eq!(io.output.iter().filter(|&&s| s != 0.0).count(), 1);
    }

    #[test]
    fn cv_bipolar_pulse_writes_reset_sample() {
        let input: [f32; 0] = []; // PCM ABI
        let mut output = vec![0.0_f32; 8]; // PCM ABI
        let mut io = AudioIo::new(&input, &mut output, 10, 48_000, 8);
        let mut state = CvPulseState::bipolar();

        render_cv_pulse_block(&[event(13)], &CvRole::Pulse, &mut state, &mut io);

        assert_eq!(io.output[3], 1.0);
        assert_eq!(io.output[4], -1.0);
        assert_eq!(io.output.iter().filter(|&&s| s != 0.0).count(), 2);
    }

    #[test]
    fn cv_bipolar_reset_spills_to_next_buffer() {
        let input: [f32; 0] = []; // PCM ABI
        let mut state = CvPulseState::bipolar();

        let mut first = vec![0.0_f32; 4]; // PCM ABI
        {
            let mut io = AudioIo::new(&input, &mut first, 0, 48_000, 4);
            render_cv_pulse_block(&[event(3)], &CvRole::Pulse, &mut state, &mut io);
        }
        assert_eq!(first, vec![0.0, 0.0, 0.0, 1.0]);

        let mut second = vec![0.0_f32; 4]; // PCM ABI
        {
            let mut io = AudioIo::new(&input, &mut second, 4, 48_000, 4);
            render_cv_pulse_block(&[], &CvRole::Pulse, &mut state, &mut io);
        }
        assert_eq!(second, vec![-1.0, 0.0, 0.0, 0.0]);
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
            sample_rate in prop::sample::select(&[44_100_u32, 48_000, 88_200, 96_000, 176_400, 192_000]),
        ) {
            let offset = offset % frames;
            let input: [f32; 0] = []; // PCM ABI
            let mut output = vec![0.0_f32; frames]; // PCM ABI
            let start = 1_000_u64;
            let mut io = AudioIo::new(&input, &mut output, start, sample_rate, frames);
            let mut state = CvPulseState::bipolar();

            render_cv_pulse_block(
                &[event(start + offset as u64)],
                &CvRole::Pulse,
                &mut state,
                &mut io,
            );

            prop_assert_eq!(io.output[offset], 1.0);
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

            render_cv_pulse_block(
                &[event(start + offset as u64)],
                &CvRole::Pulse,
                &mut state,
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
                render_cv_pulse_block(
                    &[event(start + frames as u64 - 1)],
                    &CvRole::Pulse,
                    &mut state,
                    &mut io,
                );
            }
            prop_assert_eq!(first[frames - 1], 1.0);

            let mut second = vec![0.0_f32; frames]; // PCM ABI
            {
                let mut io = AudioIo::new(&input, &mut second, start + frames as u64, 48_000, frames);
                render_cv_pulse_block(&[], &CvRole::Pulse, &mut state, &mut io);
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
