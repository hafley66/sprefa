//! Brace-pattern walker subsystem. Drives `json({ ... })` and any future
//! data-DSL ops (yaml, toml). Ported from v2/src/walk/. v3 drops the
//! per-capture jq-path field — captures carry only `name` + byte range,
//! matching the v3 `Capture` model.

pub mod brace_parse;
pub mod compile;
pub mod compiled;
pub mod pattern;
pub mod walker;
