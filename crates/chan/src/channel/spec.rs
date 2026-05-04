//! Channel-spec mini-language for `agogo run --ch <spec>`.
//!
//! Grammar: `key=val[,key=val]*`. Whitespace tolerated; `=` and `,`
//! separate; values may be quoted `"..."` to embed spaces or commas.
//!
//! Required keys: `dev`. Optional keys: `grid` (DSL expression,
//! default `T4`), `id`, `out`, `mode` (`clock`|`click`; `dev=midi`
//! defaults to `clock`, `dev=audio` requires `click`), `swing` (`[TBase:]i8`,
//! default `T8:0`), `offset` (signed `i32` ticks — **note:**
//! non-zero values are currently rejected until the
//! tempo-dependent Tick→Micro conversion is wired), `delay` (ms),
//! `snap-quantum-us`, `bars` (divider-agnostic period multiplier:
//! keep every `N`-th scheduled event; `grid=t1` is the idiomatic
//! "bars" case — see Plan 2026-04-25-03). MIDI `mode=click` adds:
//! `note`, `vel` (required), `mch` (default 10), `accent-every`
//! (optional; if set, requires `accent-vel` and optionally
//! `accent-note`). Audio `mode=click` uses fixed internal click
//! constants and accepts no sound-shaping keys. Unknown keys are
//! hard errors so typos are
//! caught early.
//!
//! The `grid` value is parsed via `dsl::parse` with a channel
//! environment, so expressions like `kick&T16` that reference
//! earlier channels are valid. Use [`parse_channels`] to resolve
//! a sequence of specs in order, building the environment as it goes.
//!
//! [`ChannelSpec`] holds the parsed form; [`ChannelSpec::into_channel`]
//! converts to a [`crate::channel::Channel`] at the CLI argv boundary, where the only
//! `f64` field (`delay_ms`) crosses via the `F064FD06` Conn per CLAUDE.md
//! float exception 4.

pub mod display;
pub mod error;
pub mod parser;
pub mod types;
pub mod validate;

pub use error::ChannelSpecError;
pub use parser::parse_channels;
pub use types::{ChannelSpec, ChannelSpecRole};
