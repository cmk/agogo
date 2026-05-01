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
/// **Channel layout.** `input` and `output` are mono in
/// v0.1 — back-ends enforce `Config::input_channels == 1` (and the
/// CV output side is empty until v0.4). Multi-channel support
/// arrives with v0.4's heterogeneous output work, at which point this
/// struct gains explicit `input_channels` / `output_channels`
/// fields and the buffers carry interleaved frames. Pattern
/// matches against `AudioIo` should use `..` to ride the
/// `#[non_exhaustive]` forward-compat.
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
    /// Output buffer for this buffer. Empty in input-only configs
    /// (v0.1 scope — CV output arrives in v0.4).
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
    /// Current callers pass `None` and ignore the output slice (CV out
    /// lands in v0.4).
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

#[cfg(test)]
mod tests {
    use super::*;

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
            AudioHostError::DeviceNotFound("x".into()),
            AudioHostError::UnsupportedSampleRate(12_345),
            AudioHostError::UnsupportedConfig("no f32 at 48 kHz".into()),
            AudioHostError::Backend("stub".into()),
        ];
        for e in errs {
            match e {
                AudioHostError::NoInputDevice
                | AudioHostError::DeviceNotFound(_)
                | AudioHostError::UnsupportedSampleRate(_)
                | AudioHostError::UnsupportedConfig(_)
                | AudioHostError::Backend(_) => {}
            }
        }
    }
}
