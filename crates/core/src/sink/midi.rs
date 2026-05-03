//! MIDI clock byte emission + the [`MidiSink`] trait.
//!
//! Pure-logic MIDI output: defines the back-end-agnostic sink contract
//! and emits `0xF8` clock bytes + `0xFA`/`0xFB`/`0xFC` transport
//! bytes. Real back-ends (midir, CoreMIDI, JACK, ...) live in sibling
//! crates and implement [`MidiSink`]. `crates/host-midi` is the
//! current best-effort midir back-end.

// ── Status bytes (MIDI 1.0 §System Real-Time Messages) ──────────────

/// Timing clock. Emitted 24 × per quarter note by the master.
pub const MIDI_CLOCK: u8 = 0xF8;
/// Start playback from position 0.
pub const MIDI_START: u8 = 0xFA;
/// Resume playback from the current position.
pub const MIDI_CONTINUE: u8 = 0xFB;
/// Stop playback.
pub const MIDI_STOP: u8 = 0xFC;

// ── Channel-voice status nibbles (MIDI 1.0 §Channel Voice) ──────────

/// Note On status nibble. OR with channel `0..=15` for the full byte.
pub const MIDI_NOTE_ON: u8 = 0x90;
/// Note Off status nibble. OR with channel `0..=15`.
pub const MIDI_NOTE_OFF: u8 = 0x80;

// ── Timing capability reports ──────────────────────────────────────

/// How a MIDI backend dispatches a message once it reaches the sink.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MidiSchedulingClass {
    /// Sends when the drain path calls [`MidiSink::send_at`].
    Immediate,
    /// Schedules against the audio sample timeline.
    NativeSampleTime,
    /// Schedules against a platform host-time clock derived from
    /// audio sample time.
    NativeHostTime,
}

/// Whether a backend can compensate output latency itself.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MidiLatencyCompensation {
    /// No backend latency compensation is available.
    None,
    /// A fixed latency value is applied by the backend.
    Static,
    /// Runtime measurement or calibration is applied by the backend.
    Measured,
}

/// Whether [`MidiSink::send_at`]'s `at_sample` argument controls
/// physical dispatch or is only retained as diagnostic metadata.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum AtSampleSupport {
    MetadataOnly,
    Honored,
}

/// Timing capability report for a MIDI sink implementation.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct MidiTimingCapability {
    pub backend_name: &'static str,
    pub scheduling: MidiSchedulingClass,
    pub latency_compensation: MidiLatencyCompensation,
    pub at_sample: AtSampleSupport,
}

impl MidiTimingCapability {
    /// Report for sinks that preserve `at_sample` only as metadata
    /// and send at drain time.
    pub const fn best_effort(backend_name: &'static str) -> Self {
        Self {
            backend_name,
            scheduling: MidiSchedulingClass::Immediate,
            latency_compensation: MidiLatencyCompensation::None,
            at_sample: AtSampleSupport::MetadataOnly,
        }
    }

    pub const fn honors_at_sample(self) -> bool {
        matches!(self.at_sample, AtSampleSupport::Honored)
    }

    pub const fn is_timestamped(self) -> bool {
        self.honors_at_sample()
            || matches!(
                self.scheduling,
                MidiSchedulingClass::NativeSampleTime | MidiSchedulingClass::NativeHostTime
            )
    }
}

/// Implemented by MIDI sinks that can report their output timing
/// capabilities. There is intentionally no blanket default: each
/// current and future backend must make an explicit claim.
pub trait MidiTimingCapabilities {
    fn timing_capability(&self) -> MidiTimingCapability;
}

// ── Core trait ──────────────────────────────────────────────────────

/// Back-end-agnostic MIDI output sink.
///
/// Each back-end converts `at_sample` to its native timebase inside
/// `send_at` — mach time for CoreMIDI, frame index for JACK,
/// `QueryPerformanceCounter` for WinMM (`doc/agogo.md` §5). The core
/// only ever sees monotonic sample counts.
///
/// **RT safety.** Implementations may allocate or take locks
/// (`midir` does both). `host-cpal` wires an `rtrb`
/// drain thread so the audio callback enqueues `(bytes, at_sample)`
/// pairs without calling `send_at` directly.
pub trait MidiSink: Send + MidiTimingCapabilities {
    fn send_at(&self, msg: &[u8], at_sample: u64);
}

// ── Synthetic in-memory sink for tests ──────────────────────────────

/// One capture produced by [`TestSink::send_at`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TestRecord {
    pub at_sample: u64,
    pub bytes: Vec<u8>,
}

/// In-memory [`MidiSink`] for tests. Stores every `send_at` call in
/// FIFO order. Not RT-safe (takes a `Mutex`); back-end sinks are the
/// production path.
#[derive(Default, Debug)]
pub struct TestSink {
    inner: std::sync::Mutex<Vec<TestRecord>>,
}

impl TestSink {
    pub fn new() -> Self {
        Self::default()
    }

    /// Cloned snapshot of every record, in FIFO order.
    pub fn records(&self) -> Vec<TestRecord> {
        self.inner.lock().unwrap().clone()
    }

    pub fn clear(&self) {
        self.inner.lock().unwrap().clear();
    }

    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl MidiSink for TestSink {
    fn send_at(&self, msg: &[u8], at_sample: u64) {
        self.inner.lock().unwrap().push(TestRecord {
            at_sample,
            bytes: msg.to_vec(),
        });
    }
}

impl MidiTimingCapabilities for TestSink {
    fn timing_capability(&self) -> MidiTimingCapability {
        MidiTimingCapability::best_effort("test")
    }
}

// ── Diagnostic sink for timing reports ─────────────────────────────

/// One capture produced by [`DiagnosticSink`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticRecord {
    pub at_sample: u64,
    pub bytes: Vec<u8>,
    pub drain_order: u64,
    pub observed_sample: Option<u64>,
    pub delay_samples: Option<i128>,
}

/// Integer summary of diagnostic drain observations.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DiagnosticSummary {
    pub record_count: usize,
    pub observed_count: usize,
    pub late_count: usize,
    pub early_count: usize,
    pub on_time_count: usize,
    pub max_late_samples: u64,
    pub max_early_samples: u64,
    pub total_abs_delay_samples: u128,
}

/// In-memory diagnostic [`MidiSink`]. It records intended sample
/// times and byte payloads in FIFO order; tests or soft-side tooling
/// may also supply an observed drain sample to compute delay outside
/// the realtime callback path.
///
/// Not RT-safe: each record takes a `Mutex`. Use `RtProducer` from
/// `agogo-host-cpal` on the audio callback path.
#[derive(Debug)]
pub struct DiagnosticSink {
    capability: MidiTimingCapability,
    inner: std::sync::Mutex<Vec<DiagnosticRecord>>,
}

impl DiagnosticSink {
    pub fn new(backend_name: &'static str) -> Self {
        Self {
            capability: MidiTimingCapability::best_effort(backend_name),
            inner: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub fn with_capability(capability: MidiTimingCapability) -> Self {
        Self {
            capability,
            inner: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub fn records(&self) -> Vec<DiagnosticRecord> {
        self.inner.lock().unwrap().clone()
    }

    pub fn summary(&self) -> DiagnosticSummary {
        let records = self.inner.lock().unwrap();
        let mut summary = DiagnosticSummary {
            record_count: records.len(),
            ..DiagnosticSummary::default()
        };

        for record in records.iter() {
            let Some(delay) = record.delay_samples else {
                continue;
            };
            summary.observed_count += 1;
            match delay.cmp(&0) {
                core::cmp::Ordering::Greater => {
                    summary.late_count += 1;
                    summary.max_late_samples = summary.max_late_samples.max(delay as u64);
                    summary.total_abs_delay_samples += delay as u128;
                }
                core::cmp::Ordering::Less => {
                    let early = delay.unsigned_abs();
                    summary.early_count += 1;
                    summary.max_early_samples = summary.max_early_samples.max(early as u64);
                    summary.total_abs_delay_samples += early;
                }
                core::cmp::Ordering::Equal => {
                    summary.on_time_count += 1;
                }
            }
        }

        summary
    }

    pub fn clear(&self) {
        self.inner.lock().unwrap().clear();
    }

    /// Record a drain observation with an externally supplied sample
    /// cursor. This is for tests and soft-side diagnostics; production
    /// render code still calls [`MidiSink::send_at`].
    pub fn record_observed_at_sample(&self, msg: &[u8], at_sample: u64, observed_sample: u64) {
        self.push_record(msg, at_sample, Some(observed_sample));
    }

    fn push_record(&self, msg: &[u8], at_sample: u64, observed_sample: Option<u64>) {
        let delay_samples = observed_sample.map(|observed| observed as i128 - at_sample as i128);
        let mut records = self.inner.lock().unwrap();
        let drain_order = records.len() as u64;
        records.push(DiagnosticRecord {
            at_sample,
            bytes: msg.to_vec(),
            drain_order,
            observed_sample,
            delay_samples,
        });
    }
}

impl Default for DiagnosticSink {
    fn default() -> Self {
        Self::new("diagnostic")
    }
}

impl MidiSink for DiagnosticSink {
    fn send_at(&self, msg: &[u8], at_sample: u64) {
        self.push_record(msg, at_sample, None);
    }
}

impl MidiTimingCapabilities for DiagnosticSink {
    fn timing_capability(&self) -> MidiTimingCapability {
        self.capability
    }
}

// ── Clock rendering ─────────────────────────────────────────────────

use crate::channel::ScheduledEvent;

/// Render a block of MidiClock [`ScheduledEvent`]s. Emits one
/// `0xF8` byte per event at its `sample_index`. No allocation — the
/// one-byte slice is stack-local per iteration.
pub fn render_clock_block(events: &[ScheduledEvent], sink: &dyn MidiSink) {
    for ev in events {
        sink.send_at(&[MIDI_CLOCK], ev.sample_index);
    }
}

// ── Transport bytes ─────────────────────────────────────────────────

/// Single-byte MIDI System Real-Time transport messages.
///
/// The byte-level enum stays deliberately small. The transport FSM
/// (`doc/designs/transport.md`) owns higher-level transport events
/// (`Play`, `Stop`, `Locate`, `PhaseSource*`) and maps its transitions
/// down to these bytes per buffer.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MidiRtByte {
    Start,
    Continue,
    Stop,
}

impl MidiRtByte {
    pub fn status_byte(self) -> u8 {
        match self {
            Self::Start => MIDI_START,
            Self::Continue => MIDI_CONTINUE,
            Self::Stop => MIDI_STOP,
        }
    }
}

/// Per-buffer render: emits the optional transport byte at
/// `buffer_start_sample` ahead of the clock stream, then the clock
/// events. Callers that want to place the transport byte mid-buffer
/// can call [`MidiSink::send_at`] directly with a custom sample
/// index.
pub fn render_buffer(
    events: &[ScheduledEvent],
    transport: Option<MidiRtByte>,
    buffer_start_sample: u64,
    sink: &dyn MidiSink,
) {
    if let Some(t) = transport {
        sink.send_at(&[t.status_byte()], buffer_start_sample);
    }
    render_clock_block(events, sink);
}

// ── MIDI Note (click) rendering ─────────────────────────────────────

use crate::channel::role::MidiClickConfig;

/// Render a block of MIDI click events. Per scheduled tick: emits a
/// 3-byte Note On followed by a same-sample 3-byte Note Off (vel=0).
/// The same-sample Off keeps stateful synths from holding the note;
/// percussive drum patches ignore the Off and run their own envelope
/// to silence.
///
/// `counter` is read+advanced once per event. When `cfg.accent` is
/// `Some(a)`, the renderer substitutes `a.note` / `a.vel` whenever
/// `*counter % a.every.get() == 0`. Counter wraps on overflow
/// (`saturating_add` would freeze accents at `u32::MAX`).
pub fn render_midi_click_block(
    events: &[ScheduledEvent],
    cfg: &MidiClickConfig,
    counter: &mut u32,
    sink: &dyn MidiSink,
) {
    let ch_byte: u8 = cfg.ch.into();
    for ev in events {
        let (n, v) = match cfg.accent {
            Some(a) if *counter % a.every.get() == 0 => (a.note, a.vel),
            _ => (cfg.note, cfg.vel),
        };
        sink.send_at(
            &[MIDI_NOTE_ON | ch_byte, n.into(), v.into()],
            ev.sample_index,
        );
        sink.send_at(&[MIDI_NOTE_OFF | ch_byte, n.into(), 0], ev.sample_index);
        *counter = counter.wrapping_add(1);
    }
}

// ── Per-channel dispatch ────────────────────────────────────────────

use crate::channel::role::{ChannelCommon, MidiRole};

/// Render one MIDI channel's block to the sink. Match-exhaustive on
/// [`MidiRole`]; the `Playhead`-level dispatch hands us only
/// `Channel::Midi` variants — a `Cv` / `Din` channel literally
/// cannot reach this function (compile-time, not runtime, guarantee).
///
/// Plan 21 (audit P3) replaced the old `render_channel_block`
/// (which dispatched on a flat `ChannelMode` and silently no-op'd
/// non-MIDI variants) with this typed version. `click_counter`
/// must be `Some(_)` for `MidiRole::Click(_)`; non-`Playhead`
/// callers (the `Clock`-only render-path tests) pass `None`.
///
/// `common` is unused today — later per-channel mute / mix /
/// transport-state logic threads through it. Borrowed (not
/// copied) so the unused parameter doesn't silently grow.
pub fn render_midi_channel(
    _common: &ChannelCommon,
    role: &MidiRole,
    events: &[ScheduledEvent],
    transport: Option<MidiRtByte>,
    buffer_start_sample: u64,
    click_counter: Option<&mut u32>,
    sink: &dyn MidiSink,
) {
    match role {
        MidiRole::Clock => {
            render_buffer(events, transport, buffer_start_sample, sink);
        }
        MidiRole::Click(cfg) => {
            let counter =
                click_counter.expect("MidiRole::Click(_) requires a counter slot from Playhead");
            render_midi_click_block(events, cfg, counter, sink);
        }
        // Spec-surface stub; rendering lands in v0.2+.
        MidiRole::Cc(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::tick::Tick;
    use proptest::prelude::*;

    fn ev(at_sample: u64) -> ScheduledEvent {
        ScheduledEvent {
            sample_index: at_sample,
            tick: Tick(0),
        }
    }

    #[test]
    fn status_byte_constants_match_spec() {
        assert_eq!(MIDI_CLOCK, 0xF8);
        assert_eq!(MIDI_START, 0xFA);
        assert_eq!(MIDI_CONTINUE, 0xFB);
        assert_eq!(MIDI_STOP, 0xFC);
    }

    #[test]
    fn test_sink_records_round_trip() {
        let sink = TestSink::new();
        assert!(sink.is_empty());
        sink.send_at(&[MIDI_CLOCK], 0);
        sink.send_at(&[MIDI_START], 1024);
        assert_eq!(sink.len(), 2);
        let recs = sink.records();
        assert_eq!(
            recs,
            vec![
                TestRecord {
                    at_sample: 0,
                    bytes: vec![MIDI_CLOCK],
                },
                TestRecord {
                    at_sample: 1024,
                    bytes: vec![MIDI_START],
                },
            ],
        );
        sink.clear();
        assert!(sink.is_empty());
    }

    #[test]
    fn timing_capability_truthful() {
        let test_sink = TestSink::new();
        let diagnostic = DiagnosticSink::default();
        let reports = [
            test_sink.timing_capability(),
            diagnostic.timing_capability(),
        ];

        for report in reports {
            assert!(!report.backend_name.is_empty());
            assert_eq!(report.at_sample, AtSampleSupport::MetadataOnly);
            assert_eq!(report.scheduling, MidiSchedulingClass::Immediate);
            assert_eq!(report.latency_compensation, MidiLatencyCompensation::None);
            assert!(!report.is_timestamped());
        }
    }

    #[test]
    fn diagnostic_sink_records_intended_order() {
        let sink = DiagnosticSink::default();
        sink.send_at(&[MIDI_START], 1024);
        sink.send_at(&[MIDI_CLOCK], 2048);

        assert_eq!(
            sink.records(),
            vec![
                DiagnosticRecord {
                    at_sample: 1024,
                    bytes: vec![MIDI_START],
                    drain_order: 0,
                    observed_sample: None,
                    delay_samples: None,
                },
                DiagnosticRecord {
                    at_sample: 2048,
                    bytes: vec![MIDI_CLOCK],
                    drain_order: 1,
                    observed_sample: None,
                    delay_samples: None,
                },
            ]
        );
    }

    #[test]
    fn diagnostic_sink_reports_drain_delay() {
        let sink = DiagnosticSink::default();
        sink.record_observed_at_sample(&[MIDI_CLOCK], 10_000, 10_144);
        sink.record_observed_at_sample(&[MIDI_CLOCK], 20_000, 19_952);
        sink.record_observed_at_sample(&[MIDI_STOP], 30_000, 30_000);

        let records = sink.records();
        assert_eq!(records[0].delay_samples, Some(144));
        assert_eq!(records[1].delay_samples, Some(-48));
        assert_eq!(records[2].delay_samples, Some(0));

        assert_eq!(
            sink.summary(),
            DiagnosticSummary {
                record_count: 3,
                observed_count: 3,
                late_count: 1,
                early_count: 1,
                on_time_count: 1,
                max_late_samples: 144,
                max_early_samples: 48,
                total_abs_delay_samples: 192,
            }
        );
    }

    /// `TestSink` is `Send + Sync` — two threads pushing 1 000 records
    /// each produce 2 000 total records with no loss, confirming the
    /// `Mutex`-backed design handles concurrent callers. The trait
    /// bound is `Send`; `TestSink` is `Sync` by virtue of its field
    /// types, which is what callers sharing an `Arc<TestSink>` across
    /// threads rely on.
    #[test]
    fn test_sink_send_across_threads() {
        use std::sync::Arc;

        let sink = Arc::new(TestSink::new());
        let s1 = Arc::clone(&sink);
        let s2 = Arc::clone(&sink);

        std::thread::scope(|scope| {
            scope.spawn(move || {
                for i in 0..1000 {
                    s1.send_at(&[MIDI_CLOCK], i);
                }
            });
            scope.spawn(move || {
                for i in 0..1000 {
                    s2.send_at(&[MIDI_CLOCK], 1_000_000 + i);
                }
            });
        });

        assert_eq!(sink.len(), 2_000);
    }

    // ── render_clock_block ────────────────────────────────────────

    #[test]
    fn render_clock_block_spot_check_four_events() {
        let evs = [ev(0), ev(24_000), ev(48_000), ev(72_000)];
        let sink = TestSink::new();
        render_clock_block(&evs, &sink);
        let recs = sink.records();
        assert_eq!(recs.len(), 4);
        for (r, e) in recs.iter().zip(evs.iter()) {
            assert_eq!(r.at_sample, e.sample_index);
            assert_eq!(r.bytes, vec![MIDI_CLOCK]);
        }
    }

    #[test]
    fn render_clock_block_empty_is_noop() {
        let sink = TestSink::new();
        render_clock_block(&[], &sink);
        assert!(sink.is_empty());
    }

    proptest! {
        /// Plan 12 property `clock_every_event_produces_one_record`:
        /// rendering N events produces exactly N records, each a
        /// single `0xF8` byte.
        #[test]
        fn clock_every_event_produces_one_record(
            samples in prop::collection::vec(any::<u64>(), 0..64),
        ) {
            let evs: Vec<ScheduledEvent> = samples.iter().copied().map(ev).collect();
            let sink = TestSink::new();
            render_clock_block(&evs, &sink);
            let recs = sink.records();
            prop_assert_eq!(recs.len(), evs.len());
            for r in &recs {
                prop_assert_eq!(r.bytes.as_slice(), &[MIDI_CLOCK]);
            }
        }

        /// Plan 12 property `clock_sample_order_preserved`: record
        /// `at_sample` values appear in the same FIFO order as input
        /// `ScheduledEvent.sample_index` values.
        #[test]
        fn clock_sample_order_preserved(
            samples in prop::collection::vec(any::<u64>(), 0..64),
        ) {
            let evs: Vec<ScheduledEvent> = samples.iter().copied().map(ev).collect();
            let sink = TestSink::new();
            render_clock_block(&evs, &sink);
            let emitted: Vec<u64> = sink.records().iter().map(|r| r.at_sample).collect();
            prop_assert_eq!(emitted, samples);
        }
    }

    // ── MidiRtByte + render_buffer ────────────────────────────────

    #[test]
    fn midi_rt_byte_status_values() {
        assert_eq!(MidiRtByte::Start.status_byte(), MIDI_START);
        assert_eq!(MidiRtByte::Continue.status_byte(), MIDI_CONTINUE);
        assert_eq!(MidiRtByte::Stop.status_byte(), MIDI_STOP);
    }

    #[test]
    fn render_buffer_transport_only_no_events() {
        let sink = TestSink::new();
        render_buffer(&[], Some(MidiRtByte::Start), 1024, &sink);
        let recs = sink.records();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].at_sample, 1024);
        assert_eq!(recs[0].bytes, vec![MIDI_START]);
    }

    #[test]
    fn render_buffer_no_transport_just_clock_events() {
        let sink = TestSink::new();
        render_buffer(&[ev(0), ev(24_000)], None, 0, &sink);
        let recs = sink.records();
        assert_eq!(recs.len(), 2);
        for r in &recs {
            assert_eq!(r.bytes, vec![MIDI_CLOCK]);
        }
    }

    proptest! {
        /// Plan 12 property `midi_rt_byte_in_expected_range`: every
        /// variant maps to the MIDI 1.0 real-time range
        /// `{0xFA, 0xFB, 0xFC}`.
        #[test]
        fn midi_rt_byte_in_expected_range(
            variant in prop::sample::select(&[
                MidiRtByte::Start,
                MidiRtByte::Continue,
                MidiRtByte::Stop,
            ]),
        ) {
            let b = variant.status_byte();
            prop_assert!(b == MIDI_START || b == MIDI_CONTINUE || b == MIDI_STOP);
        }

        /// Plan 12 property `render_buffer_emits_rt_byte_first`: when
        /// `transport = Some(t)`, the first `TestSink` record is the
        /// one-byte `[t.status_byte()]` at `buffer_start_sample`, even
        /// if a clock event also shares that sample.
        #[test]
        fn render_buffer_emits_rt_byte_first(
            variant in prop::sample::select(&[
                MidiRtByte::Start,
                MidiRtByte::Continue,
                MidiRtByte::Stop,
            ]),
            buffer_start in any::<u64>(),
            samples in prop::collection::vec(any::<u64>(), 0..16),
        ) {
            let evs: Vec<ScheduledEvent> = samples.iter().copied().map(ev).collect();
            let sink = TestSink::new();
            render_buffer(&evs, Some(variant), buffer_start, &sink);
            let recs = sink.records();
            prop_assert_eq!(recs.len(), evs.len() + 1);
            prop_assert_eq!(recs[0].at_sample, buffer_start);
            prop_assert_eq!(recs[0].bytes.as_slice(), &[variant.status_byte()]);
        }
    }

    // ── render_midi_channel ───────────────────────────────────────

    use crate::channel::Channel;
    use crate::conn::fixed::Micro;
    use crate::conn::tempo::Tempo;
    use crate::control::event::tick_stream;
    use crate::time::grid::Grid;
    use crate::time::swing::SwingConfig;
    use crate::time::tbase::TBase;

    fn midi_common(divider: Grid) -> ChannelCommon {
        ChannelCommon {
            divider,
            shuffle: SwingConfig {
                resolution: TBase::T16,
                amount: 0,
            },
            delay: Micro::ZERO,
            offset: Micro::ZERO,
            bar_multiplier: None,
        }
    }

    #[test]
    fn midi_clock_role_routes_through_render_buffer() {
        let common = midi_common(Grid::T4);
        let role = MidiRole::Clock;
        let evs = [ev(0), ev(24_000)];
        let sink = TestSink::new();
        render_midi_channel(
            &common,
            &role,
            &evs,
            Some(MidiRtByte::Start),
            0,
            None,
            &sink,
        );
        let recs = sink.records();
        assert_eq!(recs.len(), 3);
        assert_eq!(recs[0].bytes, vec![MIDI_START]);
        assert_eq!(recs[1].bytes, vec![MIDI_CLOCK]);
        assert_eq!(recs[2].bytes, vec![MIDI_CLOCK]);
    }

    /// Plan 21 (audit P3): `Channel::Midi { role: MidiRole::Cc(_) }`
    /// is a v0.2+ stub — the renderer's `MidiRole::Cc(_)` arm is
    /// `=> {}`. The pre-P3 `non_clock_modes_are_noop` proptest
    /// covered Din / AnalogPulse / AnalogLfo / MidiCc all dispatching
    /// to the no-op arm; under P3 the first three are *structurally
    /// unreachable* by `render_midi_channel` (they're `Channel::Din`
    /// / `Channel::Cv`, not `Channel::Midi`) so the no-op contract
    /// only needs to cover `MidiRole::Cc(_)`.
    #[test]
    fn cc_role_is_noop_until_v02() {
        let common = midi_common(Grid::T4);
        let role = MidiRole::Cc(crate::channel::MidiCcConfig {
            cc: crate::conn::midi::U7(74),
            range: (crate::conn::midi::U7(0), crate::conn::midi::U7(127)),
        });
        let evs = [ev(0), ev(24_000)];
        let sink = TestSink::new();
        render_midi_channel(
            &common,
            &role,
            &evs,
            Some(MidiRtByte::Start),
            0,
            None,
            &sink,
        );
        assert!(sink.is_empty());
    }

    proptest! {
        /// Plan 12 property `block_render_matches_scheduler`: for an
        /// arbitrary MidiClock channel and buffer window, the
        /// `TestSink.at_sample` list emitted by
        /// `render_midi_channel(..., transport: None, ...)` equals
        /// `tick_stream(...)`'s `ScheduledEvent.sample_index` list
        /// bit-for-bit. Pins the composition contract the RT callback
        /// relies on.
        // Bounds stay within `tick_stream`'s own tested domain
        // (`scheduler_block_equivalence` covers the same window).
        // The render path has no arithmetic on these values — this
        // test verifies the scheduler → render composition, not
        // `tick_stream`'s internal invariants, so the bounds are
        // about shrinkage speed rather than coverage-faking.
        #[test]
        fn block_render_matches_scheduler(
            buffer_start in 0u64..=1_000_000,
            frames in 1usize..=8_192,
        ) {
            let common = midi_common(Grid::T16);
            let role = MidiRole::Clock;
            let evs = tick_stream(
                &common,
                48_000,
                Tempo::from_bpm_integer(120),
                buffer_start,
                frames,
            );
            let Ok(evs) = evs else {
                prop_assert!(false, "valid schedule fixture failed: {evs:?}");
                return Ok(());
            };
            let sink = TestSink::new();
            render_midi_channel(&common, &role, &evs, None, buffer_start, None, &sink);
            let emitted: Vec<u64> = sink.records().iter().map(|r| r.at_sample).collect();
            let expected: Vec<u64> = evs.iter().map(|e| e.sample_index).collect();
            prop_assert_eq!(emitted, expected);
        }
    }

    // Suppress unused-import warning when the only consumer of
    // `Channel` in this module is the click-dispatch tests below.
    #[allow(dead_code)]
    fn _channel_type_is_used(_: Channel) {}

    // ── render_midi_click_block ───────────────────────────────────

    use crate::channel::role::{MidiClickAccent, MidiClickConfig};
    use crate::conn::midi::{U4, U7};
    use core::num::NonZeroU32;

    fn click_cfg(note: u8, vel: u8, ch: u8, accent: Option<MidiClickAccent>) -> MidiClickConfig {
        MidiClickConfig {
            note: U7::new(note).expect("test value: note in 0..=127"),
            vel: U7::new(vel).expect("test value: vel in 0..=127"),
            ch: U4::new(ch).expect("test value: ch in 0..=15"),
            accent,
        }
    }

    #[test]
    fn click_emits_note_on_then_note_off_per_event() {
        let cfg = click_cfg(76, 100, 9, None);
        let evs = [ev(0), ev(24_000)];
        let sink = TestSink::new();
        let mut counter = 0u32;
        render_midi_click_block(&evs, &cfg, &mut counter, &sink);
        let recs = sink.records();
        assert_eq!(recs.len(), 4);
        // (on, off, on, off)
        assert_eq!(recs[0].at_sample, 0);
        assert_eq!(recs[0].bytes, vec![MIDI_NOTE_ON | 9, 76, 100]);
        assert_eq!(recs[1].at_sample, 0);
        assert_eq!(recs[1].bytes, vec![MIDI_NOTE_OFF | 9, 76, 0]);
        assert_eq!(recs[2].at_sample, 24_000);
        assert_eq!(recs[2].bytes, vec![MIDI_NOTE_ON | 9, 76, 100]);
        assert_eq!(recs[3].at_sample, 24_000);
        assert_eq!(recs[3].bytes, vec![MIDI_NOTE_OFF | 9, 76, 0]);
        assert_eq!(counter, 2);
    }

    #[test]
    fn click_accent_lands_on_counter_zero_then_every_n() {
        // accent=4: counter 0,4,8,... use accent values; others use base.
        let accent = MidiClickAccent {
            every: NonZeroU32::new(4).unwrap(),
            note: U7(38),
            vel: U7(120),
        };
        let cfg = click_cfg(37, 70, 9, Some(accent));
        // 5 events covers counter 0..4 — one full accent period plus one.
        let evs: Vec<ScheduledEvent> = (0..5).map(|i| ev(i * 1000)).collect();
        let sink = TestSink::new();
        let mut counter = 0u32;
        render_midi_click_block(&evs, &cfg, &mut counter, &sink);
        let recs = sink.records();
        // 5 events × 2 records (on/off) = 10
        assert_eq!(recs.len(), 10);
        // Expected note/vel by counter index:
        //   i=0: accent (38, 120)
        //   i=1: base   (37, 70)
        //   i=2: base   (37, 70)
        //   i=3: base   (37, 70)
        //   i=4: accent (38, 120)
        let expected_notes = [38, 37, 37, 37, 38];
        let expected_vels = [120, 70, 70, 70, 120];
        for (i, (n, v)) in expected_notes.iter().zip(expected_vels.iter()).enumerate() {
            assert_eq!(recs[i * 2].bytes, vec![MIDI_NOTE_ON | 9, *n, *v]);
            assert_eq!(recs[i * 2 + 1].bytes, vec![MIDI_NOTE_OFF | 9, *n, 0]);
        }
        assert_eq!(counter, 5);
    }

    proptest! {
        /// Plan 2026-04-25-03 property
        /// `click_counter_advances_across_buffer_boundaries`: two
        /// render calls with cumulative event counts m+n behave
        /// identically to a single call with m+n events. The
        /// counter is the only piece of state that crosses the
        /// boundary, so equality of (sink records, counter) across
        /// the two paths pins the persistence contract.
        ///
        /// Domain: full `NonZeroU32` for the accent period (the
        /// renderer's `counter % every` is well-defined for any
        /// non-zero u32; bounding would hide wrap behaviour at
        /// large `every` per CLAUDE.md's coverage-faking rule).
        /// Event counts capped at 64 each — proptest needs to
        /// exercise the boundary repeatedly during shrink, not the
        /// full 2^64 input space; a separate spot check at much
        /// larger counts isn't useful since the counter wraps at
        /// `u32::MAX`, far beyond any real audio buffer pipeline.
        #[test]
        fn click_counter_advances_across_buffer_boundaries(
            sample_indices in prop::collection::vec(any::<u64>(), 0..=64),
            split in 0usize..=64,
            every in any::<u32>().prop_filter("every > 0", |&n| n > 0),
            note in 0u8..=127,
            vel in 1u8..=127,
            ch in 0u8..=15,
            accent_note in 0u8..=127,
            accent_vel in 1u8..=127,
        ) {
            let accent = MidiClickAccent {
                every: NonZeroU32::new(every).unwrap(),
                note: U7(accent_note),
                vel: U7(accent_vel),
            };
            let cfg = click_cfg(note, vel, ch, Some(accent));
            let all_evs: Vec<ScheduledEvent> =
                sample_indices.into_iter().map(ev).collect();
            let split = split.min(all_evs.len());

            // Single-call baseline.
            let sink_a = TestSink::new();
            let mut ctr_a = 0u32;
            render_midi_click_block(&all_evs, &cfg, &mut ctr_a, &sink_a);

            // Two-call: split at the generated index.
            let sink_b = TestSink::new();
            let mut ctr_b = 0u32;
            render_midi_click_block(&all_evs[..split], &cfg, &mut ctr_b, &sink_b);
            render_midi_click_block(&all_evs[split..], &cfg, &mut ctr_b, &sink_b);

            prop_assert_eq!(sink_a.records(), sink_b.records());
            prop_assert_eq!(ctr_a, ctr_b);
            prop_assert_eq!(ctr_a as usize, all_evs.len());
        }
    }

    proptest! {
        /// Plan 2026-04-25-03 property
        /// `click_every_event_produces_two_records`.
        #[test]
        fn click_every_event_produces_two_records(
            samples in prop::collection::vec(any::<u64>(), 0..32),
            note in 0u8..=127,
            vel in 1u8..=127,
            ch in 0u8..=15,
        ) {
            let cfg = click_cfg(note, vel, ch, None);
            let evs: Vec<ScheduledEvent> = samples.iter().copied().map(ev).collect();
            let sink = TestSink::new();
            let mut counter = 0u32;
            render_midi_click_block(&evs, &cfg, &mut counter, &sink);
            let recs = sink.records();
            prop_assert_eq!(recs.len(), 2 * evs.len());
            prop_assert_eq!(counter as usize, evs.len());
        }

        /// Plan 2026-04-25-03 property
        /// `click_records_are_paired_on_off`: every odd-indexed record
        /// is a Note Off matching the preceding Note On's note + ch.
        #[test]
        fn click_records_are_paired_on_off(
            samples in prop::collection::vec(any::<u64>(), 0..32),
            note in 0u8..=127,
            vel in 1u8..=127,
            ch in 0u8..=15,
        ) {
            let cfg = click_cfg(note, vel, ch, None);
            let evs: Vec<ScheduledEvent> = samples.iter().copied().map(ev).collect();
            let sink = TestSink::new();
            let mut counter = 0u32;
            render_midi_click_block(&evs, &cfg, &mut counter, &sink);
            let recs = sink.records();
            for pair in recs.chunks_exact(2) {
                let on  = &pair[0];
                let off = &pair[1];
                prop_assert_eq!(on.at_sample,  off.at_sample);
                prop_assert_eq!(on.bytes[0],  MIDI_NOTE_ON  | ch);
                prop_assert_eq!(off.bytes[0], MIDI_NOTE_OFF | ch);
                prop_assert_eq!(on.bytes[1], off.bytes[1]);
                prop_assert_eq!(off.bytes[2], 0);
            }
        }

        /// Plan 2026-04-25-03 property
        /// `click_status_byte_carries_channel`: status nibble is
        /// `0x90|ch` / `0x80|ch` for any `ch ∈ 0..=15`.
        #[test]
        fn click_status_byte_carries_channel(
            samples in prop::collection::vec(any::<u64>(), 1..16),
            ch in 0u8..=15,
        ) {
            let cfg = click_cfg(60, 80, ch, None);
            let evs: Vec<ScheduledEvent> = samples.iter().copied().map(ev).collect();
            let sink = TestSink::new();
            let mut counter = 0u32;
            render_midi_click_block(&evs, &cfg, &mut counter, &sink);
            for r in sink.records() {
                prop_assert!(
                    r.bytes[0] == (MIDI_NOTE_ON | ch) || r.bytes[0] == (MIDI_NOTE_OFF | ch),
                    "status byte {:#x} not in expected set for ch={}",
                    r.bytes[0], ch,
                );
            }
        }

        /// Plan 2026-04-25-03 property
        /// `accent_lands_every_n_emitted_clicks_from_zero`: with
        /// `accent.every = N` the i-th emitted Note On uses the
        /// accent note iff `i % N == 0`. Counter starts at 0.
        #[test]
        fn accent_lands_every_n_emitted_clicks_from_zero(
            n in 1u32..=12,
            event_count in 0usize..=40,
        ) {
            let accent = MidiClickAccent {
                every: NonZeroU32::new(n).unwrap(),
                note: U7(38),
                vel: U7(120),
            };
            let cfg = click_cfg(37, 70, 9, Some(accent));
            let evs: Vec<ScheduledEvent> = (0..event_count as u64).map(ev).collect();
            let sink = TestSink::new();
            let mut counter = 0u32;
            render_midi_click_block(&evs, &cfg, &mut counter, &sink);
            let recs = sink.records();
            for i in 0..event_count {
                let on = &recs[i * 2];
                let is_accent = (i as u32) % n == 0;
                let expected_note = if is_accent { 38 } else { 37 };
                let expected_vel = if is_accent { 120 } else { 70 };
                prop_assert_eq!(
                    on.bytes[1], expected_note,
                    "tick {}: expected note {}", i, expected_note,
                );
                prop_assert_eq!(
                    on.bytes[2], expected_vel,
                    "tick {}: expected vel {}", i, expected_vel,
                );
            }
        }
    }

    // ── render_midi_channel dispatch on Click ─────────────────────

    #[test]
    fn click_role_routes_through_render_midi_click_block() {
        let cfg = click_cfg(76, 100, 9, None);
        let common = midi_common(Grid::T4);
        let role = MidiRole::Click(cfg);
        let evs = [ev(0), ev(24_000)];
        let sink = TestSink::new();
        let mut counter = 0u32;
        render_midi_channel(
            &common,
            &role,
            &evs,
            Some(MidiRtByte::Start),
            0,
            Some(&mut counter),
            &sink,
        );
        let recs = sink.records();
        // Click mode ignores transport bytes — only Note On/Off pairs.
        assert_eq!(recs.len(), 4);
        assert_eq!(recs[0].bytes, vec![MIDI_NOTE_ON | 9, 76, 100]);
        assert_eq!(recs[1].bytes, vec![MIDI_NOTE_OFF | 9, 76, 0]);
        assert_eq!(recs[2].bytes, vec![MIDI_NOTE_ON | 9, 76, 100]);
        assert_eq!(recs[3].bytes, vec![MIDI_NOTE_OFF | 9, 76, 0]);
        assert_eq!(counter, 2);
    }

    #[test]
    #[should_panic(expected = "MidiRole::Click(_) requires a counter slot")]
    fn click_role_panics_without_counter() {
        let cfg = click_cfg(76, 100, 9, None);
        let common = midi_common(Grid::T4);
        let role = MidiRole::Click(cfg);
        let evs = [ev(0)];
        let sink = TestSink::new();
        // Plan 2026-04-25-03 contract: Playhead always supplies the
        // counter; passing None is a programmer error and panics fast.
        render_midi_channel(&common, &role, &evs, None, 0, None, &sink);
    }
}
