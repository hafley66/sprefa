//! Observability "verdict line" logging: one terse stderr line + one
//! structured `perf.jsonl` row per notable decision (extraction rebuild vs
//! skip, a slow lock wait, a busy-sqlite retry, a tick summary, a corpus
//! scan, the process run header). Complements the existing `perflog`
//! phase/tick timeline (`src/perflog.rs`) rather than replacing it: this
//! module answers "what DECISION did the engine make and why", `perflog`
//! answers "how long did each phase take."
//!
//! Level is read once from `DL_LOG` via a `OnceLock` (unset/unrecognized =
//! `Default`). `DL_LOG=quiet` suppresses everything (stderr AND jsonl);
//! `DL_LOG=debug` additionally emits `debug_verdict` lines. `perf.jsonl`
//! rows always ride the existing `crate::perflog` writer (its own
//! `DL_PERF_LOG` gate still applies) — this module never opens a second
//! file.

use std::sync::OnceLock;

/// A lock wait past this many ms gets a `[wait]` verdict line.
pub const LOCK_WAIT_WARN_MS: u64 = 250;
/// A sqlite busy-retry stretch past this many ms gets a `[sqlite]` verdict line.
pub const SQLITE_BUSY_WARN_MS: u64 = 100;
/// A single directory contributing more than this percent of the scanned
/// corpus's file count gets a loud `[corpus]` line.
pub const DIR_SHARE_WARN_PCT: u32 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Quiet,
    Default,
    Debug,
}

static LEVEL: OnceLock<Level> = OnceLock::new();

fn level() -> Level {
    *LEVEL.get_or_init(|| match std::env::var("DL_LOG").as_deref() {
        Ok("quiet") => Level::Quiet,
        Ok("debug") => Level::Debug,
        _ => Level::Default,
    })
}

/// Build the shared verdict JSON row and hand it to the existing
/// `perflog` writer (`src/perflog.rs`), which already owns the
/// `<root>/.dl/perf.jsonl` path resolution, `DL_PERF_LOG` gate, and
/// append-only file handle. `kind` is one of the seven verdict kinds this
/// module emits; `fields` are the kind-specific extra columns.
fn write_jsonl(kind: &str, fields: &[(&str, &str)]) {
    // `perflog`'s file handle is a `OnceLock`: the FIRST call into it decides
    // forever whether perf.jsonl is open. `db::open` can run before
    // `Engine::new` calls `perflog::set_root` (the db is constructed first,
    // then handed to `Engine::new`), so a verdict fired from inside
    // `db::open` (the `[db] opened ...` line) must not be the call that
    // forces that decision while root is still unknown — it would wedge
    // perf.jsonl off for the rest of the process. `perflog::path()` only
    // reads the `ROOT` OnceLock (no side effect), so checking it first is
    // free; skip the jsonl side (stderr still printed by the caller) when
    // root isn't set yet rather than risk silently disabling every later
    // verdict's jsonl row.
    if crate::perflog::path().is_none() {
        return;
    }
    let mut obj = serde_json::Map::new();
    obj.insert("type".into(), serde_json::Value::String("verdict".into()));
    obj.insert("kind".into(), serde_json::Value::String(kind.into()));
    for (key, value) in fields {
        obj.insert((*key).to_string(), serde_json::Value::String((*value).to_string()));
    }
    crate::perflog::emit_verdict(serde_json::Value::Object(obj));
}

/// Emit at `Default` level and above: stderr line + `perf.jsonl` row.
/// `kind` names the perf.jsonl row kind (see the module doc); `msg` is the
/// human-readable stderr text, printed WITHOUT a kind-specific prefix — call
/// sites already write the `[extract]`/`[wait]`/etc. bracket themselves so
/// the terseness matches the repo's existing `[tick]`/`[config]` style.
pub fn verdict(kind: &str, msg: &str, fields: &[(&str, &str)]) {
    if level() == Level::Quiet {
        return;
    }
    eprintln!("{msg}");
    write_jsonl(kind, fields);
}

/// Emit ONLY at `Debug` level: same stderr + jsonl shape as `verdict`, gated
/// one level stricter (a warm-tick skip verdict is chatty enough to want an
/// opt-in).
pub fn debug_verdict(kind: &str, msg: &str, fields: &[(&str, &str)]) {
    if level() != Level::Debug {
        return;
    }
    eprintln!("{msg}");
    write_jsonl(kind, fields);
}

/// Run-header verdict: mode, root, db path (+ why chosen), daemon routing,
/// binary version + build id. Emitted once per process start. Every field
/// rides as a jsonl column so a log scraper can pivot without re-parsing the
/// stderr line; the stderr line stays a single terse `[dl]`-prefixed sentence.
#[allow(clippy::too_many_arguments)]
pub fn emit_run_header(
    mode: &str,
    root: &str,
    db_path: &str,
    db_reason: &str,
    daemon_routing: &str,
    version: &str,
    build_id: &str,
) {
    let msg = format!(
        "[dl] mode={mode} root={root} db={db_path} ({db_reason}) daemon={daemon_routing} version={version} build={build_id}"
    );
    verdict(
        "run-header",
        &msg,
        &[
            ("mode", mode),
            ("root", root),
            ("db_path", db_path),
            ("db_reason", db_reason),
            ("daemon_routing", daemon_routing),
            ("version", version),
            ("build_id", build_id),
        ],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The DL_LOG parser: unset/garbage falls to Default, exact tokens hit
    /// Quiet/Debug. Reads the env directly rather than through `level()`'s
    /// process-global OnceLock so this test does not race the cache other
    /// tests may have already warmed.
    #[test]
    fn parses_dl_log_tokens() {
        let parse = |raw: Option<&str>| match raw {
            Some("quiet") => Level::Quiet,
            Some("debug") => Level::Debug,
            _ => Level::Default,
        };
        assert_eq!(parse(None), Level::Default);
        assert_eq!(parse(Some("nonsense")), Level::Default);
        assert_eq!(parse(Some("quiet")), Level::Quiet);
        assert_eq!(parse(Some("debug")), Level::Debug);
    }
}
