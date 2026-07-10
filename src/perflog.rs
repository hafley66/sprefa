//! Append-only JSONL performance log, tail-able while the daemon runs. One
//! `phase` record per phase transition + one `tick` record per tick, so
//!     tail -f <root>/.dl/perf.jsonl | jq .
//! shows where each tick spends its time and what moved — the answer to "why
//! are the fans on at scale" and "what re-ran on that edit."
//!
//! Default ON: appended to `<root>/.dl/perf.jsonl` (alongside `cache.db`).
//! Disable with `DL_PERF_LOG=0`; redirect with `DL_PERF_LOG=<path>`. Records
//! are small and few (one per phase boundary, ticks are human/poll-speed), so
//! the default-on cost is negligible and the log CATCHES the spike when it
//! happens rather than after a flag is set and the bug is reproduced.
//!
//! Phase records are driven by the `activity` slot's transitions (so the perf
//! log comes for free from the same instrumentation `ping`/`status` read). The
//! per-tick counts record is emitted from the tick orchestrator, which already
//! computes every field (reconcile delta, derived strategy, rebuilt rels,
//! effects). `stmt_ms`/`rel_count` relations remain the after-the-fact query
//! surface; this log is the live, append-only timeline.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// The per-tick counts record. Built from locals the tick orchestrator already
/// computes; emitted once per tick (full or incremental).
pub struct TickRec<'a> {
    pub tick: u64,
    pub kind: &'a str, // "full" | "incremental"
    pub files_parsed: usize,
    pub files_total: usize,
    pub extracted: usize,
    pub retracted: usize,
    pub derived_strategy: &'a str, // "full" | "scoped" | "unchanged"
    pub derived_rebuilt: &'a [String],
    pub changed_rels: &'a [String],
    /// WHY `derived_strategy` was "full", e.g. `"blank-slate"` /
    /// `"program-edit"` / `"carry-changed"` / `"derived-missing:<rel,...>"` /
    /// `"closure-missing:<edge>"`. `None` when the strategy wasn't "full"
    /// (P3: the --check perf defect's tick-record reason field).
    pub full_reason: Option<&'a str>,
    pub effects_inflight: usize,
    pub total_ms: u64,
}

static ROOT: OnceLock<PathBuf> = OnceLock::new();
static FILE: OnceLock<Mutex<Option<std::fs::File>>> = OnceLock::new();

/// Seed the resolver with the daemon/engine root so `<root>/.dl/perf.jsonl`
/// can be resolved lazily on first emit. Idempotent. Called from `Engine::new`
/// (covers the daemon + in-process one-shots).
pub fn set_root(root: &Path) {
    let _ = ROOT.set(root.to_path_buf());
}

/// Logging is on unless `DL_PERF_LOG` is exactly a falsy token. A truthy or
/// unset value keeps the default `<root>/.dl/perf.jsonl`; any other string is
/// taken as an explicit path.
fn enabled() -> bool {
    !matches!(
        std::env::var("DL_PERF_LOG").as_deref(),
        Ok("0") | Ok("false") | Ok("off") | Ok("no")
    )
}

/// The resolved log path, for the daemon to announce at startup. `None` when
/// disabled, or when neither an explicit path nor a root is known (an engine
/// with no root and no env override).
pub fn path() -> Option<PathBuf> {
    if !enabled() {
        return None;
    }
    match std::env::var("DL_PERF_LOG") {
        Ok(p) if is_truthy(&p) => ROOT.get().map(|r| r.join(".dl").join("perf.jsonl")),
        Ok(p) => Some(PathBuf::from(p)),
        Err(_) => ROOT.get().map(|r| r.join(".dl").join("perf.jsonl")),
    }
}

fn is_truthy(s: &str) -> bool {
    matches!(s, "" | "1" | "true" | "on" | "yes")
}

fn file() -> &'static Mutex<Option<std::fs::File>> {
    FILE.get_or_init(|| {
        if !enabled() {
            return Mutex::new(None);
        }
        let Some(path) = path() else { return Mutex::new(None) };
        // Never be the thing that creates `<root>/.dl`. The default perf.jsonl
        // path lives under `.dl/`; if that dir does not already exist (a one-shot
        // `dl p.dl` in a fresh dir, a `--check` over a non-setup repo), SKIP
        // logging entirely rather than leave a `.dl/` behind — that side effect
        // broke discovery ("explicit program must not create .dl/") and wastes a
        // caller's tokens tracking a spurious dir. The daemon and any setup'd
        // repo already have `.dl/`, so they log normally. An explicit
        // `DL_PERF_LOG=<path>` opt-in MAY create its parent (the user asked).
        let is_default = matches!(
            std::env::var("DL_PERF_LOG").as_deref(),
            Err(_) | Ok("" | "1" | "true" | "on" | "yes")
        );
        let parent = path.parent().unwrap_or(Path::new("."));
        if is_default && !parent.exists() {
            return Mutex::new(None);
        }
        if !is_default {
            let _ = std::fs::create_dir_all(parent);
        }
        // Append-mode: each `write` is a syscall, immediately visible to
        // `tail -f` (no userspace buffer to flush). Truncation is never done;
        // the log grows monotonically so a spike stays in view.
        let f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .ok();
        Mutex::new(f)
    })
}

fn write_json(line: String) {
    if !enabled() {
        return;
    }
    if let Ok(mut g) = file().lock() {
        if let Some(f) = g.as_mut() {
            use std::io::Write;
            let _ = writeln!(f, "{line}");
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Build the phase record's JSON shape (split out from `emit_phase` so a
/// unit test can assert on it directly, without touching the process-global
/// `ROOT`/file `OnceLock`s the write path depends on).
fn phase_record(tick: u64, phase: &str, ms: u64, detail: &str) -> serde_json::Value {
    serde_json::json!({
        "ts_ms": now_ms(),
        "type": "phase",
        "pid": std::process::id(),
        "tick": tick,
        "phase": phase,
        "ms": ms,
        "detail": detail,
    })
}

/// One record per phase transition: how long the phase that just ended ran,
/// with its detail string. Called from the `activity` slot.
pub fn emit_phase(tick: u64, phase: &str, ms: u64, detail: &str) {
    write_json(phase_record(tick, phase, ms, detail).to_string());
}

/// Build the tick record's JSON shape (see `phase_record`).
fn tick_record_json(rec: &TickRec<'_>) -> serde_json::Value {
    serde_json::json!({
        "ts_ms": now_ms(),
        "type": "tick",
        "pid": std::process::id(),
        "tick": rec.tick,
        "kind": rec.kind,
        "files": {
            "parsed": rec.files_parsed,
            "total": rec.files_total,
            "extracted": rec.extracted,
            "retracted": rec.retracted,
        },
        "derived": {
            "strategy": rec.derived_strategy,
            "rebuilt": rec.derived_rebuilt,
            "full_reason": rec.full_reason,
        },
        "changed_rels": rec.changed_rels,
        "effects_inflight": rec.effects_inflight,
        "total_ms": rec.total_ms,
    })
}

/// One record per tick: what moved, what re-derived, what the whole tick cost.
/// `jq 'select(.type=="tick") | {tick,total_ms,derived:.derived.strategy}'` for
/// the spike timeline; `select(.type=="phase") | select(.ms>100)'` for the slow
/// step inside a tick.
pub fn emit_tick(rec: &TickRec<'_>) {
    write_json(tick_record_json(rec).to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P2 (--check perf defect ledger): every phase/tick record carries a
    /// `pid` field matching the current process, so a shared perf.jsonl (a
    /// daemon plus a one-shot `--check` writing the same file) can be split
    /// by process. Exercises the pure JSON-builders directly — they don't
    /// touch the process-global `ROOT`/file `OnceLock`s, so this stays
    /// independent of `path_resolver`'s env mutation below.
    #[test]
    fn records_carry_pid() {
        let want = std::process::id() as u64;

        let phase = phase_record(3, "derived", 12, "2 rel(s): live, gate");
        assert_eq!(phase["pid"].as_u64(), Some(want));
        assert_eq!(phase["type"], "phase");
        assert_eq!(phase["phase"], "derived");

        let changed_rels = vec!["src_a".to_string()];
        let rebuilt = vec!["live".to_string()];
        let tick = tick_record_json(&TickRec {
            tick: 3,
            kind: "full",
            files_parsed: 1,
            files_total: 1,
            extracted: 1,
            retracted: 0,
            derived_strategy: "full",
            derived_rebuilt: &rebuilt,
            changed_rels: &changed_rels,
            full_reason: Some("blank-slate"),
            effects_inflight: 0,
            total_ms: 5,
        });
        assert_eq!(tick["pid"].as_u64(), Some(want));
        assert_eq!(tick["type"], "tick");
        assert_eq!(tick["derived"]["full_reason"], "blank-slate");

        // `full_reason: None` serializes as JSON null, not an absent key —
        // the shape the perf_log_end_to_end e2e test (tests/it/tick_digest.rs)
        // asserts `.is_null()` against on a warm, non-full tick.
        let unchanged = tick_record_json(&TickRec {
            tick: 4, kind: "full", files_parsed: 1, files_total: 1,
            extracted: 0, retracted: 0, derived_strategy: "unchanged",
            derived_rebuilt: &[], changed_rels: &[], full_reason: None,
            effects_inflight: 0, total_ms: 1,
        });
        assert!(unchanged["derived"]["full_reason"].is_null());
    }

    /// Pin the resolver: default → `<root>/.dl/perf.jsonl`; `DL_PERF_LOG=0` →
    /// disabled (None); an explicit path wins. Env mutation is process-wide,
    /// so this is the only perflog path test touching env/ROOT (keep it
    /// self-contained).
    #[test]
    fn path_resolver() {
        std::env::remove_var("DL_PERF_LOG");
        let _ = ROOT.set(PathBuf::from("/tmp/dl_perf_test_root"));
        assert_eq!(
            path(),
            Some(PathBuf::from("/tmp/dl_perf_test_root/.dl/perf.jsonl")),
            "default path is <root>/.dl/perf.jsonl"
        );
        std::env::set_var("DL_PERF_LOG", "/tmp/alt-perf.jsonl");
        assert_eq!(path(), Some(PathBuf::from("/tmp/alt-perf.jsonl")),
            "explicit DL_PERF_LOG path wins");
        std::env::set_var("DL_PERF_LOG", "0");
        assert_eq!(path(), None, "DL_PERF_LOG=0 disables");
        std::env::remove_var("DL_PERF_LOG");
    }
}
