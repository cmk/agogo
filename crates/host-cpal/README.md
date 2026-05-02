# agogo-host-cpal

cpal back-end for `agogo::core::sink::audio::AudioHost`. This is the
first platform-audio integration; v0.1 wires the audio-in path, and
v0.4 owns CV/gate output through the heterogeneous output layer.

Not a workspace member by design — `cargo test --workspace` skips it
so the default CI path doesn't pull cpal + its platform system
libraries (`libasound` on Linux, CoreAudio on macOS, WASAPI on
Windows). CI runs `cargo test` from this crate's directory in a
dedicated `host-cpal` job that installs `libasound2-dev` up front.

## Dev workflow

### Build + test

`agogo-host-cpal` is intentionally not a `[workspace].members`
entry, so `-p agogo-host-cpal` from the repo root will not resolve
it. Use `--manifest-path` (or `cd` into the crate):

```
cargo build --manifest-path crates/host-cpal/Cargo.toml
cargo test  --manifest-path crates/host-cpal/Cargo.toml

# or, equivalently:
cd crates/host-cpal
cargo build
cargo test
```

On Linux, install ALSA dev headers first:

```
sudo apt-get install libasound2-dev
```

macOS and Windows builds link the system audio frameworks; no extra
setup needed.

### Hardware smoke test

The `cpal_default_input_smoke` fixture-gated hardware test belongs
with `agogo run`'s acceptance scenario, where a real audio device is
in scope.

## Where the logic lives

- `src/cpal.rs` — `CpalHost` (impl `AudioHost`) + device enumeration.
- `src/cpal/callback.rs` — `CallbackState` + `on_buffer`.
- `src/cpal/control.rs` — rtrb SPSC + RT→drain plumbing.
