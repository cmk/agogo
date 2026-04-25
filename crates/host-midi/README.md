# agogo-host-midi

midir back-end for `agogo_core::out::midi::MidiSink`. Cross-platform
MIDI output for v0.1 (~1 ms USB-bus dispatch jitter per
`doc/agogo.md` §4); platform-native sinks (CoreMIDI, JACK,
ALSA-MIDI, WinMM) that tighten the precision side land post-v0.5
as sibling crates.

Not a workspace member by design — `cargo test --workspace` skips
it so the default CI path doesn't pull midir + its platform
system libraries (`libasound` on Linux, CoreMIDI on macOS, WinMM on
Windows). CI runs `cargo test -p agogo-host-midi` in a dedicated
job that installs `libasound2-dev` up front.

## Dev workflow

### Build + test

```
cargo build -p agogo-host-midi
cargo test -p agogo-host-midi
```

On Linux, install ALSA dev headers first:

```
sudo apt-get install libasound2-dev
```

### Hardware loopback test

The `midir_loopback_roundtrip` test (Plan 13 T6) opens a paired
loopback port and asserts MIDI bytes round-trip. Fixture-gated as
`midir_loopback`; skips cleanly when no loopback environment is
present.

To exercise it locally:

- **macOS**: enable IAC bus in Audio MIDI Setup.
- **Linux**: load `snd-virmidi` and connect the two virtual ports.
- **Windows**: install `loopMIDI` and create a port pair.

Then:

```
cargo test -p agogo-host-midi -- midir_loopback_roundtrip
```

## Where the logic lives

- `src/midir.rs` — `MidirSink` (impl `MidiSink`) + port enumeration.
