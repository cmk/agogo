# agogo-host-midi

midir back-end for `agogo::core::sink::midi::MidiSink`. Cross-platform
MIDI output for v0.1 (~1 ms USB-bus dispatch jitter per
`doc/agogo.md` §4); platform-native sinks (CoreMIDI, JACK,
ALSA-MIDI, WinMM) that tighten the precision side land as v0.2+
timestamped-output sibling crates.

Not a workspace member by design — `cargo test --workspace` skips
it so the default CI path doesn't pull midir + its platform
system libraries (`libasound` on Linux, CoreMIDI on macOS, WinMM on
Windows). CI runs `cargo test` from this crate's directory in a
dedicated `host-midi` job (`working-directory: crates/host-midi`
in `.github/workflows/ci.yml`) that installs `libasound2-dev` up
front.

## Dev workflow

### Build + test

`agogo-host-midi` is intentionally not a `[workspace].members`
entry, so `-p agogo-host-midi` from the repo root will not resolve
it. Use `--manifest-path` (or `cd` into the crate):

```
cargo build --manifest-path crates/host-midi/Cargo.toml
cargo test  --manifest-path crates/host-midi/Cargo.toml

# or, equivalently:
cd crates/host-midi
cargo build
cargo test
```

On Linux, install ALSA dev headers first:

```
sudo apt-get install libasound2-dev
```

### Hardware loopback test

The `midir_loopback_roundtrip` fixture-gated hardware test belongs
with `agogo run`'s hardware-backed acceptance scenario. The local
set-up will be:

- **macOS**: enable IAC bus in Audio MIDI Setup.
- **Linux**: load `snd-virmidi` and connect the two virtual ports.
- **Windows**: install `loopMIDI` and create a port pair.

## Where the logic lives

- `src/midir.rs` — `MidirSink` (impl `MidiSink`) + port enumeration.
