//! `CpalHost` — cpal-backed [`AudioHost`] implementation.

pub mod control;

use ::cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ::cpal::{BufferSize, InputCallbackInfo, SampleFormat, SampleRate, StreamConfig, StreamError};
use agogo_core::host::{AudioHost, AudioHostError, AudioIo, Config, Handle};
use std::sync::mpsc::Sender;
use std::thread;

/// cpal-backed audio host. Opens the system's default cpal host
/// (CoreAudio on macOS, WASAPI on Windows, ALSA on Linux by default)
/// and routes an input stream through `AudioHost::run`'s callback.
pub struct CpalHost {
    device: ::cpal::Device,
}

impl CpalHost {
    /// Use the system default input device.
    pub fn default_input() -> Result<Self, AudioHostError> {
        let host = ::cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or(AudioHostError::NoInputDevice)?;
        Ok(Self { device })
    }

    /// Select an input device by name. Names are those returned by
    /// [`list_input_devices`]. Matching is case-sensitive.
    pub fn with_input_name(name: &str) -> Result<Self, AudioHostError> {
        let host = ::cpal::default_host();
        let device = host
            .input_devices()
            .map_err(|e| AudioHostError::Backend(Box::new(e)))?
            .find(|d| d.name().ok().as_deref() == Some(name))
            .ok_or_else(|| AudioHostError::DeviceNotFound(name.to_owned()))?;
        Ok(Self { device })
    }

    /// Enumerate the names of input devices visible to cpal's
    /// default host. A device whose name can't be fetched (e.g. a
    /// host-side I/O error) is silently dropped rather than
    /// propagated — callers that need the error path should use
    /// `::cpal::default_host().input_devices()` directly.
    pub fn list_input_devices() -> Vec<String> {
        ::cpal::default_host()
            .input_devices()
            .into_iter()
            .flatten()
            .filter_map(|d| d.name().ok())
            .collect()
    }
}

impl AudioHost for CpalHost {
    fn run(
        self,
        cfg: Config,
        mut cb: Box<dyn FnMut(&mut AudioIo) + Send>,
    ) -> Result<Handle, AudioHostError> {
        // Verify the device supports f32 input at the requested rate.
        // cpal's default_input_config often returns f32 already on
        // macOS/Windows, but ALSA can default to i16 on some cards;
        // scanning the supported configs surfaces the mismatch as
        // `UnsupportedSampleRate` rather than a cryptic cpal error
        // later.
        let supports_rate = self
            .device
            .supported_input_configs()
            .map_err(|e| AudioHostError::Backend(Box::new(e)))?
            .any(|c| {
                c.sample_format() == SampleFormat::F32
                    && c.min_sample_rate().0 <= cfg.sample_rate
                    && c.max_sample_rate().0 >= cfg.sample_rate
                    && cfg.input_channels >= c.channels()
            });
        if !supports_rate {
            return Err(AudioHostError::UnsupportedSampleRate(cfg.sample_rate));
        }

        let stream_config = StreamConfig {
            channels: cfg.input_channels,
            sample_rate: SampleRate(cfg.sample_rate),
            buffer_size: BufferSize::Fixed(cfg.buffer_frames),
        };

        let sample_rate = cfg.sample_rate;
        let channels = cfg.input_channels as usize;

        // `cpal::Stream` is `!Send` on macOS (CoreAudio) and Windows
        // (WASAPI) — the native handles are thread-bound. Run the
        // stream on a dedicated thread that owns it; the thread
        // parks until a stop signal arrives, then drops the stream
        // (which tears the audio down on its owning thread). The
        // Stream is never exposed through a `Send` context.
        let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
        let (ready_tx, ready_rx) =
            std::sync::mpsc::sync_channel::<Result<(), AudioHostError>>(1);

        let thread = thread::Builder::new()
            .name("agogo-cpal-stream".into())
            .spawn(move || {
                // Per-stream `buffer_start_sample` accumulator. cpal
                // guarantees single-threaded invocation of the data
                // callback per stream, so a captured `u64` (no atomic)
                // is sufficient.
                let mut next_start: u64 = 0;
                // Zero-length output stub — CV output is v0.4's
                // `out/audio`. Kept as a stable empty slice so the
                // callback closure doesn't allocate per buffer.
                let mut output_stub: [f32; 0] = [];

                let data_cb = move |samples: &[f32], _info: &InputCallbackInfo| {
                    let frames = samples.len() / channels.max(1);
                    let mut io = AudioIo::new(
                        samples,
                        &mut output_stub,
                        next_start,
                        sample_rate,
                        frames,
                    );
                    cb(&mut io);
                    next_start = next_start.saturating_add(frames as u64);
                };

                let err_cb = |e: StreamError| {
                    tracing::error!(?e, "cpal stream error");
                };

                let stream = match self.device.build_input_stream(
                    &stream_config,
                    data_cb,
                    err_cb,
                    None,
                ) {
                    Ok(s) => s,
                    Err(e) => {
                        let _ =
                            ready_tx.send(Err(AudioHostError::Backend(Box::new(e))));
                        return;
                    }
                };

                if let Err(e) = stream.play() {
                    let _ =
                        ready_tx.send(Err(AudioHostError::Backend(Box::new(e))));
                    return;
                }

                // Stream is live; tell the constructor it can
                // return. From here the thread parks on `stop_rx`
                // and drops `stream` when the Handle is dropped.
                let _ = ready_tx.send(Ok(()));
                let _ = stop_rx.recv();
                drop(stream);
            })
            .map_err(|e| AudioHostError::Backend(Box::new(e)))?;

        // Block until the stream is either live or has surfaced an
        // error during build/play. `ready_rx` is `SyncSender(1)`
        // so the thread never blocks on send.
        match ready_rx.recv() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                // Thread already returned on error; join to tidy.
                let _ = thread.join();
                return Err(e);
            }
            Err(_) => {
                // Thread panicked before sending. Propagate.
                let _ = thread.join();
                return Err(AudioHostError::Backend(
                    "agogo-cpal-stream thread died before stream was ready".into(),
                ));
            }
        }

        Ok(Handle::from_payload(StreamOwner {
            stop_tx,
            thread: Some(thread),
        }))
    }
}

/// Send + 'static wrapper around a `cpal::Stream`. Dropping this
/// (via `Handle`'s Drop glue) signals the owner thread to drop the
/// Stream on its own thread — the only safe point where a non-Send
/// `cpal::Stream` can be torn down.
struct StreamOwner {
    stop_tx: Sender<()>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Drop for StreamOwner {
    fn drop(&mut self) {
        // Signal the stream-owner thread to exit.
        let _ = self.stop_tx.send(());
        if let Some(t) = self.thread.take() {
            // Best-effort join; a panicked thread isn't actionable
            // here — the process is about to lose the audio stream
            // regardless.
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `CpalHost::list_input_devices` never panics; it may return
    /// an empty list on a host with no input devices (typical for
    /// CI runners without audio). That's acceptable — hardware
    /// smoke tests fixture-gate on `cpal_default_input` instead.
    #[test]
    fn list_input_devices_is_infallible() {
        let _devices = CpalHost::list_input_devices();
    }

    /// `with_input_name` surfaces `DeviceNotFound` for a name no
    /// device will have. `.is_err()` rather than `.expect_err()`
    /// because `cpal::Device` inside `CpalHost` isn't `Debug`.
    #[test]
    fn with_input_name_rejects_bogus_name() {
        let result = CpalHost::with_input_name(
            "definitely-not-a-real-device-name-\u{00A0}\u{2603}",
        );
        match result {
            Err(AudioHostError::DeviceNotFound(_))
            | Err(AudioHostError::Backend(_)) => {}
            Err(other) => {
                panic!("expected DeviceNotFound / Backend, got {other:?}")
            }
            Ok(_) => panic!("bogus name must not match a real device"),
        }
    }
}
