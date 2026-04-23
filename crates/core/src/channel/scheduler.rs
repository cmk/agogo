//! Per-buffer scheduler: given `(channel, sr, bpm, buffer_window)`
//! emits the `ScheduledEvent`s that fall inside the window. Filled in
//! by T4 of Plan 03.
