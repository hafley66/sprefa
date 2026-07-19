//! Discrete IO event trail: `<daemon_home>/events.jsonl`, one line the moment
//! something happens, carrying its ARGUMENTS — the changed paths, the rel
//! names, the effect request, the file actually written.
//!
//! This is the twin of `why.jsonl`, and the difference is the whole point:
//! `why` samples the activity slot every 2s and renders it to a human string
//! ("15 changed path(s)"), so the paths that caused a tick are never written
//! down. Reconstructing causality from samples is impossible — the 2026-07-19
//! rebuild burned an afternoon on exactly that ("which 15 files?" was
//! unanswerable from the trail). Events record the args; samples record the
//! cost. You need both.
//!
//! Standing law compliance (CLAUDE.md, "Infra is bought, never built" +
//! "logging = the `tracing` crate spine"): emission goes through `tracing`
//! macros on a dedicated target, and this module's writer is a
//! `tracing_subscriber::Layer` — NOT a parallel bespoke pipeline. Anything
//! that can already `tracing::info!` can emit an event; nothing has to reach
//! into this module's internals.
//!
//! Line shape:
//!   {"ts":..,"pid":..,"kind":"file_changed","root":"/path","data":{..}}
//!
//! `kind` is the event name, `data` its arguments as a JSON object. The reader
//! (`dl daemon events`) filters on `kind` and `root`.

use serde_json::{json, Map, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

/// The tracing target every event rides. A call site emits with
/// `tracing::info!(target: EVENT_TARGET, ...)`; the layer below claims exactly
/// this target and nothing else, so ordinary diagnostics never land in the
/// event trail (and events never spam the human log).
pub const EVENT_TARGET: &str = "dl::event";

/// Rotate at ~8MB (twice `why.jsonl`: events are denser than samples), one
/// generation kept as `events.jsonl.1`.
const ROTATE_BYTES: u64 = 8 * 1024 * 1024;

/// Where the layer writes. Set once at subscriber init; `None` (unset) makes
/// every emit a no-op, which is what one-shot CLI runs and the test suite want.
static TRAIL: OnceLock<PathBuf> = OnceLock::new();

/// Point the event layer at `<home>/events.jsonl`. Called from the daemon's
/// tracing init, before the subscriber is installed.
pub fn set_home(home: &Path) {
    let _ = TRAIL.set(home.join("events.jsonl"));
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Append one line. Same discipline as `why::append_line`: a single O_APPEND
/// `write_all` per line, because two threads emitting concurrently would
/// interleave a streaming `writeln!` into corrupt JSON.
fn append(line: &Value) {
    let Some(path) = TRAIL.get() else { return };
    if let Ok(meta) = std::fs::metadata(path) {
        if meta.len() > ROTATE_BYTES {
            let rotated = path.with_extension("jsonl.1");
            let _ = std::fs::rename(path, rotated);
        }
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let mut buf = line.to_string();
        buf.push('\n');
        let _ = f.write_all(buf.as_bytes());
    }
}

/// Emit one event. `kind` names it, `root` scopes it to a served root (None for
/// process-wide events), `data` carries the arguments.
///
/// Prefer this over a raw `tracing::info!` at a call site: it keeps the field
/// shape uniform so `dl daemon events` can filter without knowing every
/// emitter. The `data` object is where the ARGS go — be generous, that is the
/// entire reason this file exists. A path list belongs here in full, not as a
/// count.
pub fn emit(kind: &str, root: Option<&Path>, data: Value) {
    let root_s = root.map(|r| r.display().to_string()).unwrap_or_default();
    // Rides the tracing spine (standing law): the layer below turns this back
    // into a JSONL row. `data` is pre-serialized because tracing fields are
    // flat primitives, not nested structures.
    tracing::info!(
        target: EVENT_TARGET,
        kind,
        root = %root_s,
        data = %data,
    );
}

/// Convenience for the overwhelmingly common shape: a list of paths.
pub fn emit_paths(kind: &str, root: Option<&Path>, paths: &[PathBuf], extra: Value) {
    let list: Vec<String> = paths.iter().map(|p| p.display().to_string()).collect();
    let mut data = json!({ "count": list.len(), "paths": list });
    if let (Some(obj), Some(extra_obj)) = (data.as_object_mut(), extra.as_object()) {
        for (k, v) in extra_obj {
            obj.insert(k.clone(), v.clone());
        }
    }
    emit(kind, root, data);
}

// ---------- the subscriber layer ----------

/// Collects a tracing event's fields into a flat JSON map.
#[derive(Default)]
struct FieldGrab {
    fields: Map<String, Value>,
}

impl tracing::field::Visit for FieldGrab {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.fields.insert(field.name().to_string(), Value::String(value.to_string()));
    }
    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.fields.insert(field.name().to_string(), json!(value));
    }
    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.fields.insert(field.name().to_string(), json!(value));
    }
    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.fields.insert(field.name().to_string(), json!(value));
    }
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.fields.insert(field.name().to_string(), Value::String(format!("{value:?}")));
    }
}

/// The event-trail layer: claims `EVENT_TARGET` events and appends them to
/// `events.jsonl`. Every other target passes through untouched to the rest of
/// the subscriber stack.
pub struct EventLayer;

impl<S: tracing::Subscriber> Layer<S> for EventLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        if event.metadata().target() != EVENT_TARGET {
            return;
        }
        let mut grab = FieldGrab::default();
        event.record(&mut grab);
        // `data` arrives as a JSON string (tracing has no nested-value field);
        // re-parse so the trail holds real structure rather than an escaped
        // blob a reader would have to unescape by hand.
        let data = grab
            .fields
            .get("data")
            .and_then(|v| v.as_str())
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
            .unwrap_or(Value::Null);
        let kind = grab.fields.get("kind").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let root = grab.fields.get("root").and_then(|v| v.as_str()).unwrap_or("").to_string();
        append(&json!({
            "ts": now_ms(),
            "pid": std::process::id(),
            "kind": kind,
            "root": root,
            "data": data,
        }));
    }
}

// ---------- reader (`dl daemon events`) ----------

/// Read the event trail back, newest last, optionally filtered by `kind`
/// substring and `root` substring. `limit` caps the returned rows (from the
/// end). Reads the file only — no socket, no lock — so it answers while the
/// daemon is wedged, matching `why`'s contract.
pub fn read(home: &Path, kind_filter: Option<&str>, root_filter: Option<&str>, limit: usize) -> Vec<Value> {
    let mut rows: Vec<Value> = Vec::new();
    for generation in ["events.jsonl.1", "events.jsonl"] {
        let path = home.join(generation);
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        for line in text.lines() {
            let Ok(value) = serde_json::from_str::<Value>(line) else { continue };
            if let Some(want) = kind_filter {
                if !value["kind"].as_str().unwrap_or("").contains(want) { continue; }
            }
            if let Some(want) = root_filter {
                if !value["root"].as_str().unwrap_or("").contains(want) { continue; }
            }
            rows.push(value);
        }
    }
    if rows.len() > limit {
        rows.drain(..rows.len() - limit);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emit_paths_carries_every_path_not_a_count() {
        let paths = vec![PathBuf::from("a/one.rs"), PathBuf::from("b/two.rs")];
        let list: Vec<String> = paths.iter().map(|p| p.display().to_string()).collect();
        let data = json!({ "count": list.len(), "paths": list });
        // The whole point of the module: the ARGS survive, not a rendered
        // "2 changed path(s)" string.
        assert_eq!(data["count"], 2);
        assert_eq!(data["paths"][0], "a/one.rs");
        assert_eq!(data["paths"][1], "b/two.rs");
    }

    #[test]
    fn read_filters_by_kind_and_root_and_tails_to_limit() {
        let home = std::env::temp_dir().join(format!("dl_eventlog_read_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let mut text = String::new();
        for i in 0..5 {
            text.push_str(&json!({
                "ts": i, "pid": 1, "kind": if i % 2 == 0 { "tick" } else { "file_changed" },
                "root": "/repo/alpha", "data": {"n": i},
            }).to_string());
            text.push('\n');
        }
        text.push_str(&json!({
            "ts": 9, "pid": 1, "kind": "tick", "root": "/repo/beta", "data": {},
        }).to_string());
        text.push('\n');
        std::fs::write(home.join("events.jsonl"), text).unwrap();

        let ticks = read(&home, Some("tick"), None, 100);
        assert_eq!(ticks.len(), 4, "3 alpha ticks + 1 beta tick");
        let alpha = read(&home, Some("tick"), Some("alpha"), 100);
        assert_eq!(alpha.len(), 3, "root filter drops the beta tick");
        let tail = read(&home, None, None, 2);
        assert_eq!(tail.len(), 2, "limit tails from the end");
        assert_eq!(tail[1]["root"], "/repo/beta", "newest row is last");
    }
}
