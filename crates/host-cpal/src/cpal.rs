//! `CpalHost` — cpal-backed [`AudioHost`] implementation.

pub mod callback;
pub mod control;

use ::cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ::cpal::{
    BufferSize, InputCallbackInfo, OutputCallbackInfo, SampleFormat, SampleRate, StreamConfig,
    StreamError,
};
use agogo::core::sink::audio::{AudioHost, AudioHostError, AudioIo, Config, Handle};
use std::sync::mpsc::Sender;
use std::thread;

/// cpal-backed audio host. Opens the system's default cpal host
/// (CoreAudio on macOS, WASAPI on Windows, ALSA on Linux by default)
/// and routes either an input-only or output-only stream through
/// `AudioHost::run`'s callback.
pub struct CpalHost {
    device: ::cpal::Device,
    kind: CpalDeviceKind,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum CpalDeviceKind {
    Input,
    Output,
}

impl CpalHost {
    /// Use the system default input device.
    pub fn default_input() -> Result<Self, AudioHostError> {
        let host = ::cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or(AudioHostError::NoInputDevice)?;
        Ok(Self {
            device,
            kind: CpalDeviceKind::Input,
        })
    }

    /// Use the system default output device.
    pub fn default_output() -> Result<Self, AudioHostError> {
        let host = ::cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(AudioHostError::NoOutputDevice)?;
        Ok(Self {
            device,
            kind: CpalDeviceKind::Output,
        })
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
        Ok(Self {
            device,
            kind: CpalDeviceKind::Input,
        })
    }

    /// Select an output device by name. Names are those returned by
    /// [`list_output_devices`]. Matching is case-sensitive.
    pub fn with_output_name(name: &str) -> Result<Self, AudioHostError> {
        let host = ::cpal::default_host();
        let device = host
            .output_devices()
            .map_err(|e| AudioHostError::Backend(Box::new(e)))?
            .find(|d| d.name().ok().as_deref() == Some(name))
            .ok_or_else(|| AudioHostError::DeviceNotFound(name.to_owned()))?;
        Ok(Self {
            device,
            kind: CpalDeviceKind::Output,
        })
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

    /// Enumerate the names of output devices visible to cpal's
    /// default host. A device whose name can't be fetched is
    /// silently dropped, matching [`list_input_devices`].
    pub fn list_output_devices() -> Vec<String> {
        ::cpal::default_host()
            .output_devices()
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
        cb: Box<dyn FnMut(&mut AudioIo) + Send>,
    ) -> Result<Handle, AudioHostError> {
        match self.kind {
            CpalDeviceKind::Input => run_input_stream(self.device, cfg, cb),
            CpalDeviceKind::Output => run_output_stream(self.device, cfg, cb),
        }
    }
}

fn run_input_stream(
    device: ::cpal::Device,
    cfg: Config,
    mut cb: Box<dyn FnMut(&mut AudioIo) + Send>,
) -> Result<Handle, AudioHostError> {
    if cfg.input_channels != 1 || cfg.output_channels != 0 {
        return Err(AudioHostError::UnsupportedConfig(format!(
            "cpal input stream supports input_channels=1, output_channels=0; got \
             input_channels={}, output_channels={}",
            cfg.input_channels, cfg.output_channels
        )));
    }

    let supported: Vec<_> = device
        .supported_input_configs()
        .map_err(|e| AudioHostError::Backend(Box::new(e)))?
        .collect();
    validate_supported_config(&supported, cfg.sample_rate, cfg.input_channels, "input")?;

    let stream_config = StreamConfig {
        channels: cfg.input_channels,
        sample_rate: SampleRate(cfg.sample_rate),
        buffer_size: BufferSize::Fixed(cfg.buffer_frames),
    };
    let sample_rate = cfg.sample_rate;
    spawn_stream(move |stop_rx, ready_tx| {
        let mut next_start: u64 = 0;
        let mut output_stub: [f32; 0] = []; // PCM ABI
        let data_cb = move |samples: &[f32], _info: &InputCallbackInfo| {
            let frames = samples.len();
            let mut io = AudioIo::new(samples, &mut output_stub, next_start, sample_rate, frames);
            cb(&mut io);
            next_start = next_start.saturating_add(frames as u64);
        };
        let err_cb = |e: StreamError| {
            tracing::error!(?e, "cpal stream error");
        };
        let stream = match device.build_input_stream(&stream_config, data_cb, err_cb, None) {
            Ok(s) => s,
            Err(e) => {
                let _ = ready_tx.send(Err(AudioHostError::Backend(Box::new(e))));
                return;
            }
        };
        play_and_park(stream, stop_rx, ready_tx);
    })
}

fn run_output_stream(
    device: ::cpal::Device,
    cfg: Config,
    mut cb: Box<dyn FnMut(&mut AudioIo) + Send>,
) -> Result<Handle, AudioHostError> {
    if cfg.input_channels != 0 || cfg.output_channels != 1 {
        return Err(AudioHostError::UnsupportedConfig(format!(
            "cpal output stream supports input_channels=0, output_channels=1; got \
             input_channels={}, output_channels={}",
            cfg.input_channels, cfg.output_channels
        )));
    }

    let supported: Vec<_> = device
        .supported_output_configs()
        .map_err(|e| AudioHostError::Backend(Box::new(e)))?
        .collect();
    validate_supported_config(&supported, cfg.sample_rate, cfg.output_channels, "output")?;

    let stream_config = StreamConfig {
        channels: cfg.output_channels,
        sample_rate: SampleRate(cfg.sample_rate),
        buffer_size: BufferSize::Fixed(cfg.buffer_frames),
    };
    let sample_rate = cfg.sample_rate;
    spawn_stream(move |stop_rx, ready_tx| {
        let mut next_start: u64 = 0;
        let input_stub: [f32; 0] = []; // PCM ABI
        let data_cb = move |samples: &mut [f32], _info: &OutputCallbackInfo| {
            let frames = samples.len();
            let mut io = AudioIo::new(&input_stub, samples, next_start, sample_rate, frames);
            cb(&mut io);
            next_start = next_start.saturating_add(frames as u64);
        };
        let err_cb = |e: StreamError| {
            tracing::error!(?e, "cpal stream error");
        };
        let stream = match device.build_output_stream(&stream_config, data_cb, err_cb, None) {
            Ok(s) => s,
            Err(e) => {
                let _ = ready_tx.send(Err(AudioHostError::Backend(Box::new(e))));
                return;
            }
        };
        play_and_park(stream, stop_rx, ready_tx);
    })
}

fn validate_supported_config(
    supported: &[::cpal::SupportedStreamConfigRange],
    sample_rate: u32,
    channels: u16,
    direction: &str,
) -> Result<(), AudioHostError> {
    let rate_ok = |c: &::cpal::SupportedStreamConfigRange| {
        c.min_sample_rate().0 <= sample_rate && c.max_sample_rate().0 >= sample_rate
    };
    let any_rate = supported.iter().any(rate_ok);
    let exact_match = supported
        .iter()
        .any(|c| rate_ok(c) && c.sample_format() == SampleFormat::F32 && c.channels() == channels);
    if exact_match {
        return Ok(());
    }
    if !any_rate {
        return Err(AudioHostError::UnsupportedSampleRate(sample_rate));
    }
    let any_f32_at_rate = supported
        .iter()
        .any(|c| rate_ok(c) && c.sample_format() == SampleFormat::F32);
    let any_channels_at_rate = supported
        .iter()
        .any(|c| rate_ok(c) && c.channels() == channels);
    Err(AudioHostError::UnsupportedConfig(format!(
        "{direction} device supports {sample_rate} Hz but not f32 mono \
         (f32 available at rate: {any_f32_at_rate}, channels={channels} available at rate: \
         {any_channels_at_rate})",
    )))
}

fn spawn_stream(
    build: impl FnOnce(
        std::sync::mpsc::Receiver<()>,
        std::sync::mpsc::SyncSender<Result<(), AudioHostError>>,
    ) + Send
    + 'static,
) -> Result<Handle, AudioHostError> {
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel::<Result<(), AudioHostError>>(1);
    let thread = thread::Builder::new()
        .name("agogo-cpal-stream".into())
        .spawn(move || build(stop_rx, ready_tx))
        .map_err(|e| AudioHostError::Backend(Box::new(e)))?;

    match ready_rx.recv() {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            let _ = thread.join();
            return Err(e);
        }
        Err(_) => {
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

fn play_and_park(
    stream: ::cpal::Stream,
    stop_rx: std::sync::mpsc::Receiver<()>,
    ready_tx: std::sync::mpsc::SyncSender<Result<(), AudioHostError>>,
) {
    if let Err(e) = stream.play() {
        let _ = ready_tx.send(Err(AudioHostError::Backend(Box::new(e))));
        return;
    }
    let _ = ready_tx.send(Ok(()));
    let _ = stop_rx.recv();
    drop(stream);
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
    /// CI runners without audio). That's acceptable. The
    /// hardware-backed `agogo run` acceptance path is where a real
    /// fixture belongs.
    #[test]
    fn list_input_devices_is_infallible() {
        let _devices = CpalHost::list_input_devices();
    }

    #[test]
    fn list_output_devices_is_infallible() {
        let _devices = CpalHost::list_output_devices();
    }

    /// `with_input_name` surfaces `DeviceNotFound` for a name no
    /// device will have. `.is_err()` rather than `.expect_err()`
    /// because `cpal::Device` inside `CpalHost` isn't `Debug`.
    #[test]
    fn with_input_name_rejects_bogus_name() {
        let result =
            CpalHost::with_input_name("definitely-not-a-real-device-name-\u{00A0}\u{2603}");
        match result {
            Err(AudioHostError::DeviceNotFound(_)) | Err(AudioHostError::Backend(_)) => {}
            Err(other) => {
                panic!("expected DeviceNotFound / Backend, got {other:?}")
            }
            Ok(_) => panic!("bogus name must not match a real device"),
        }
    }

    #[test]
    fn with_output_name_rejects_bogus_name() {
        let result =
            CpalHost::with_output_name("definitely-not-a-real-output-device-name-\u{00A0}\u{2603}");
        match result {
            Err(AudioHostError::DeviceNotFound(_)) | Err(AudioHostError::Backend(_)) => {}
            Err(other) => {
                panic!("expected DeviceNotFound / Backend, got {other:?}")
            }
            Ok(_) => panic!("bogus name must not match a real device"),
        }
    }
}
