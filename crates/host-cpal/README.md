# agogo-host-cpal

cpal back-end for `agogo_core::host::AudioHost`. Plan 13's first
platform-audio integration; v0.1 wires the audio-in path (CV output
lands in v0.4 via `out/audio`).

Not a workspace member by design — `cargo test --workspace` skips it
so the default CI path doesn't pull cpal + its platform system
libraries (`libasound` on Linux, CoreAudio on macOS, WASAPI on
Windows). CI runs `cargo test -p agogo-host-cpal` in a dedicated
job that installs `libasound2-dev` up front.

## Dev workflow

### Build + test

```
cargo build -p agogo-host-cpal
cargo test -p agogo-host-cpal
```

On Linux, install ALSA dev headers first:

```
sudo apt-get install libasound2-dev
```

macOS and Windows builds link the system audio frameworks; no extra
setup needed.

### Hardware smoke test

The `cpal_default_input_smoke` test opens the default input device,
captures 100 ms, and asserts at least one non-zero sample. Fixture-
gated as `cpal_default_input` — skips cleanly on a CI runner that
has no audio device.

To exercise it locally, connect a microphone (or any input source
that produces signal) and run:

```
cargo test -p agogo-host-cpal -- cpal_default_input_smoke
```

## Where the logic lives

- `src/cpal.rs` — `CpalHost` (impl `AudioHost`) + device enumeration.
- `src/cpal/callback.rs` — (Plan 13 T4) `CallbackState` + `on_buffer`.
- `src/cpal/control.rs` — (Plan 13 T3) rtrb SPSC + RT→drain plumbing.
