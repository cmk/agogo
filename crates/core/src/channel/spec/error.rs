//! Error type for `--ch <spec>` parsing + validation.
//!
//! Plan 2026-04-28-06 T2: extracted from `machine/spec.rs`.

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ChannelSpecError {
    #[error("channel spec: empty key")]
    EmptyKey,
    #[error("channel spec: unknown key `{0}`")]
    UnknownKey(String),
    #[error("channel spec: missing `{0}`")]
    MissingKey(&'static str),
    #[error("channel spec: bad value for `{0}`: {1}")]
    BadValue(&'static str, String),
    #[error("channel spec: dev=audio requires v0.4 (out/audio)")]
    AudioDeferred,
    #[error("channel spec: malformed (expected `key=val,...`): {0}")]
    Malformed(String),
}
