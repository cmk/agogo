# Review 00076

## Summary

### What Changed

- Added the plan for a public-demo CV pulse MVP.
- Taught channel specs to parse and lower `dev=cv,mode=pulse` into
  `Channel::Cv`.
- Added fixed-shape CV pulse rendering through the existing mono audio output
  path, including bipolar reset state across buffer boundaries.
- Extended offline render JSON with positive/negative audio peak diagnostics
  and added a hardware-free CV pulse render test.
- Updated README and roadmap docs to distinguish the CV pulse MVP from the
  later heterogeneous output/calibration work.

### Verification

- `cargo test -p agogo-chan cv --quiet`
- `cargo test -p agogo-core transport --quiet`
- `cargo test -p agogo-cli --test render --quiet`
- `cargo run -p agogo-cli --bin agogo -- render --source internal --bpm 120 --sr 48000 --duration-bars 1 --ch 'id=cv,dev=cv,mode=pulse,grid=t4,out=diag'`
- `scripts/check-pii.sh`
- `cargo test --workspace --quiet`
- `cargo clippy --all-targets --quiet -- -D warnings`
