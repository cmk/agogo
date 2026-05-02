//! `ChannelSpec::into_channel` — the spec→runtime-`Channel` lowering.
//!
//! Cross-key validation (e.g. tempo-dependent offset rejection) plus
//! the `MAX_DELAY` clamp live here. The parser (`spec/parser.rs`)
//! and the type definition (`spec/types.rs`) are the producers;
//! this is the consumer that turns a parsed `ChannelSpec` into a
//! ready-to-run `Channel`.
//!
//! Plan 2026-04-28-06 T5: extracted from `machine/spec.rs`.

use crate::channel::Channel;
use crate::channel::role::ChannelCommon;
use crate::channel::time::MAX_DELAY;
use crate::conn::fixed::Micro;

use super::ChannelSpec;
use super::error::ChannelSpecError;
use super::types::ChannelSpecRole;

impl ChannelSpec {
    /// Convert the parsed spec into the runtime [`Channel`] type.
    pub fn into_channel(self) -> Result<Channel, ChannelSpecError> {
        // Target/mode validation already happened in `parse`, so
        // `self.role` is ready to lower into the runtime `Channel`
        // shape.
        // delay is already typed as Micro at the spec layer (Q3
        // closure for audit K). The parser body called micro_from_user_ms
        // — into_channel just clamps to MAX_DELAY.
        let delay = Micro(self.delay.0.clamp(0, MAX_DELAY.0));

        // Offset in ticks requires tempo to convert to Micro. Until
        // the tempo-dependent Tick→Micro path is wired, reject non-zero
        // values rather than silently storing ticks as microseconds.
        if self.offset_ticks != 0 {
            return Err(ChannelSpecError::BadValue(
                "offset",
                format!(
                    "non-zero offset ({} ticks) requires tempo-dependent conversion \
                     (not yet implemented)",
                    self.offset_ticks
                ),
            ));
        }

        let common = ChannelCommon {
            divider: self.grid,
            shuffle: self.swing,
            delay,
            offset: Micro::ZERO,
            bar_multiplier: self.bars,
        };

        Ok(match self.role {
            ChannelSpecRole::Midi(role) => Channel::Midi { common, role },
            ChannelSpecRole::Audio(role) => Channel::Audio { common, role },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::role::{AudioRole, MidiRole};
    use crate::conn::midi::U4;
    use core::num::NonZeroU16;
    use proptest::prelude::*;

    // ── Delay ────────────────────────────────────────────────────

    #[test]
    fn into_channel_clamps_delay() {
        let spec = ChannelSpec::parse("dev=midi,delay=500", &[]).unwrap();
        let ch = spec.into_channel().unwrap();
        assert_eq!(ch.common().delay, MAX_DELAY);
    }

    #[test]
    fn into_channel_bars_round_trips_via_nonzero() {
        let spec = ChannelSpec::parse("dev=midi,grid=t1,bars=4", &[]).unwrap();
        let ch = spec.into_channel().unwrap();
        assert_eq!(ch.common().bar_multiplier, NonZeroU16::new(4));
    }

    #[test]
    fn into_channel_click_maps_mch_to_zero_based() {
        let spec =
            ChannelSpec::parse("dev=midi,mode=click,grid=t4,note=37,vel=80,mch=10", &[]).unwrap();
        let ch = spec.into_channel().unwrap();
        match ch {
            Channel::Midi {
                role: MidiRole::Click(cfg),
                ..
            } => assert_eq!(cfg.ch, U4(9)),
            _ => panic!("expected Channel::Midi {{ role: Click(_) }}"),
        }
    }

    #[test]
    fn into_channel_click_no_accent_when_accent_every_absent() {
        let spec = ChannelSpec::parse("dev=midi,mode=click,grid=t4,note=37,vel=80", &[]).unwrap();
        let ch = spec.into_channel().unwrap();
        match ch {
            Channel::Midi {
                role: MidiRole::Click(cfg),
                ..
            } => assert!(cfg.accent.is_none()),
            _ => panic!(),
        }
    }

    #[test]
    fn into_channel_audio_click_returns_audio_variant() {
        let spec = ChannelSpec::parse("dev=audio,mode=click,grid=t4,bars=4", &[]).unwrap();
        let ch = spec.into_channel().unwrap();
        match ch {
            Channel::Audio {
                role: AudioRole::Click,
                common,
            } => assert_eq!(common.bar_multiplier, NonZeroU16::new(4)),
            _ => panic!("expected Channel::Audio {{ role: Click }}"),
        }
    }

    // ── Plan 20: snap_intent accessor (audit P2). ──

    #[test]
    fn snap_intent_none_when_key_absent() {
        let spec = ChannelSpec::parse("dev=midi,grid=t4", &[]).unwrap();
        assert!(spec.snap_intent().is_none());
    }

    proptest! {
        /// `snap-quantum-us=N` parsed back through `snap_intent()`
        /// recovers `Some(Micro(N))` for every signed `i64`.
        /// Generator spans the full domain — the parse path stores
        /// the raw `i64` and `snap_intent` just rewraps; bounding
        /// would hide nothing.
        #[test]
        fn snap_intent_round_trips_through_spec(n in any::<i64>()) {
            let s = format!("dev=midi,grid=t4,snap-quantum-us={n}");
            let spec = ChannelSpec::parse(&s, &[])
                .map_err(|e| TestCaseError::fail(format!("parse `{s}`: {e}")))?;
            prop_assert_eq!(spec.snap_intent(), Some(Micro(n)));
        }
    }
}
