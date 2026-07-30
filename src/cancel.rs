//! Mid-run cancellation probe: the thread-local seam that lets a RUNNING
//! engine tick observe the flag `jobq::JobQueue::cancel_req` sets (class-18
//! residual — before this, the flag was consulted only at the job boundary in
//! `jobq::workers::run_job`, so a disconnect during a long tick cancelled
//! nothing until the tick finished on its own).
//!
//! Shape mirrors `crate::reqid`: the worker enters a [`scope`] INSIDE its
//! `spawn_blocking` closure (the tick runs synchronously on that thread), and
//! the engine calls [`checkpoint`] at safe stage boundaries — places where a
//! bail leaves the db in the same recoverable shape a SIGKILL would (the
//! crash-window discipline: per-component completion markers, deferred digest
//! saves). A checkpoint with no scope installed (one-shot CLI, LSP, inline
//! dispatch ticks, unit tests) is a no-op.
//!
//! This is a COOPERATIVE check, never a thread kill: the probe returns `true`,
//! the checkpoint bails, and the error unwinds through the tick's ordinary
//! `?` discipline (SQLite transactions roll back, RAII pragma guards restore,
//! completion markers stay truthful).

use std::cell::RefCell;
use std::sync::Arc;

use anyhow::{bail, Result};

/// The probe a scope installs: called with the checkpoint's stage label,
/// returns whether the surrounding job has been cancelled. The stage argument
/// exists for targeted test probes (fire exactly at `"derived-component"`);
/// the production probe (a `JobQueue::is_cancelled` peek) ignores it.
pub type CancelProbe = Arc<dyn Fn(&str) -> bool>;

thread_local! {
    static PROBE: RefCell<Option<CancelProbe>> = const { RefCell::new(None) };
}

/// Install `probe` on this thread for the guard's lifetime; restores whatever
/// was active before on drop (nesting-safe, like `reqid::scope`).
pub fn scope(probe: CancelProbe) -> ScopeGuard {
    let previous = PROBE.with(|p| p.replace(Some(probe)));
    ScopeGuard { previous }
}

/// Whether the probe (if any) reports this thread's job cancelled at `stage`.
/// `false` when no scope is installed.
pub fn cancelled(stage: &str) -> bool {
    PROBE.with(|p| {
        p.borrow()
            .as_ref()
            .map(|probe| probe(stage))
            .unwrap_or(false)
    })
}

/// Stage-boundary check: bail when the surrounding job's request is gone.
/// Call ONLY at points where an error return leaves the db recoverable — the
/// same points a `fail_rebuild_at_rel`-style injected crash is already proven
/// safe (completion markers unmoved or truthfully cleared, digest baselines
/// unflushed, so the next tick re-detects and repairs).
pub fn checkpoint(stage: &str) -> Result<()> {
    if cancelled(stage) {
        // Close THIS thread's activity spans before unwinding: an erroring
        // tick never reaches its caller's `end_tick`, and the spans would
        // stay entered until the next tick reuses the thread. Thread-local
        // only — the shared activity slot is left to the next tick.
        crate::activity::exit_thread_spans();
        bail!("tick cancelled at {stage} boundary: client request gone");
    }
    Ok(())
}

pub struct ScopeGuard {
    previous: Option<CancelProbe>,
}

impl Drop for ScopeGuard {
    fn drop(&mut self) {
        let previous = self.previous.take();
        PROBE.with(|p| *p.borrow_mut() = previous);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_is_a_noop_without_a_scope_and_bails_inside_one() {
        assert!(
            checkpoint("anywhere").is_ok(),
            "no scope installed => no-op"
        );
        {
            let _guard = scope(Arc::new(|stage: &str| stage == "derived"));
            assert!(checkpoint("reconcile").is_ok(), "probe is stage-selective");
            let err = checkpoint("derived").unwrap_err();
            assert!(
                err.to_string().contains("cancelled at derived"),
                "got: {err}"
            );
        }
        assert!(
            checkpoint("derived").is_ok(),
            "guard drop restores the empty state"
        );
    }

    #[test]
    fn nested_scopes_restore_the_outer_probe() {
        let _outer = scope(Arc::new(|_stage: &str| true));
        assert!(checkpoint("x").is_err());
        {
            let _inner = scope(Arc::new(|_stage: &str| false));
            assert!(checkpoint("x").is_ok(), "inner scope shadows the outer");
        }
        assert!(
            checkpoint("x").is_err(),
            "outer probe restored on inner drop"
        );
    }
}

/// Engine-level pin: a cancellation observed MID-TICK aborts at the
/// `derived-component` boundary with the db fully consistent — old derived
/// rows intact and still marked complete (never wiped-empty), source digests
/// unflushed so a clean re-tick re-detects the change and converges. Mirrors
/// `engine::derive::crash_window_tests`, with the abort landing BEFORE the
/// unmark/wipe bracket instead of inside it.
#[cfg(test)]
mod tick_abort_tests {
    use super::*;
    use crate::{db, engine::Engine, lex, parse};
    use std::fs;
    use std::path::{Path, PathBuf};

    const PROG: &str = r#"
rel src_a(path: file, word: text).
src_a(path, word) <- scan("WORK", "src/*.rs", path, rev), match(path, rev, /alpha_(?<word>\w+)/, line).
rel reach_a(word: text).
reach_a(word) <- src_a(_, word).
rel reach_b(word: text).
reach_b(word) <- src_a(_, word).
"#;

    fn sandbox(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cancel_tick_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        dir
    }

    fn fresh_engine(dir: &Path) -> (Engine, crate::ast::Program) {
        let prog = parse::parse(lex::lex(PROG).unwrap()).unwrap();
        let conn = db::open(Some(dir.join("db").to_str().unwrap())).unwrap();
        (Engine::new(conn, dir.to_path_buf()), prog)
    }

    fn rel_vals(eng: &Engine, rel: &str) -> Vec<String> {
        eng.query_sql(&format!("SELECT word FROM rel_{rel}_txt"), &[])
            .unwrap()
            .into_iter()
            .map(|row| row[0].as_str().unwrap().to_string())
            .collect()
    }

    fn complete_set(eng: &Engine) -> std::collections::HashSet<String> {
        eng.query_sql("SELECT rel FROM _derived_complete", &[])
            .unwrap()
            .into_iter()
            .map(|row| row[0].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn cancelled_tick_aborts_at_component_boundary_db_consistent_and_rerun_converges() {
        // The tick mutates the process-global activity slot (`set`, and the
        // checkpoint's abort-path `end_tick`), so hold the crate-wide
        // test-globals lock like every other slot-mutating test.
        let _slot = crate::perflog::TEST_GLOBALS_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = sandbox("component");
        fs::write(dir.join("src/a.rs"), "// alpha_one\n").unwrap();
        let (mut eng, prog) = fresh_engine(&dir);

        // Clean tick: both derived rels populated + marked complete.
        eng.tick(&prog, true).unwrap();
        assert_eq!(rel_vals(&eng, "reach_a"), vec!["one"]);
        assert_eq!(rel_vals(&eng, "reach_b"), vec!["one"]);

        // Edit the source, then run a tick under a probe that fires at the
        // first derived-component boundary — the moment `cancel_req`'s flag
        // becomes observable mid-tick.
        fs::write(dir.join("src/a.rs"), "// alpha_seventeen\n").unwrap();
        {
            let _guard = scope(std::sync::Arc::new(|stage: &str| {
                stage == "derived-component"
            }));
            let err = eng.tick(&prog, true).unwrap_err();
            assert!(
                err.to_string().contains("cancelled at derived-component"),
                "the tick must abort at the component boundary, got: {err}"
            );
        }

        // No half-written state: the abort landed BEFORE any unmark/wipe, so
        // both rels keep their OLD rows and their completion markers.
        assert_eq!(
            rel_vals(&eng, "reach_a"),
            vec!["one"],
            "old rows survive the abort"
        );
        assert_eq!(
            rel_vals(&eng, "reach_b"),
            vec!["one"],
            "old rows survive the abort"
        );
        let done = complete_set(&eng);
        assert!(
            done.contains("reach_a") && done.contains("reach_b"),
            "completion markers stay truthful across the abort, got {done:?}"
        );

        // Digest deferral: the aborted tick never flushed src_a's new digest,
        // so a clean re-tick re-detects the change and converges.
        let report = eng.tick_report(&prog, true).unwrap();
        assert!(
            report.changed_rels.contains(&"src_a".to_string()),
            "the rerun must re-detect the change the aborted tick left unpersisted, got {:?}",
            report.changed_rels
        );
        assert_eq!(rel_vals(&eng, "reach_a"), vec!["seventeen"]);
        assert_eq!(rel_vals(&eng, "reach_b"), vec!["seventeen"]);

        // Close the activity spans the direct `tick_report` calls opened
        // (the daemon layer normally owns the begin/end pairing): an entered
        // span left in TLS at thread death aborts the test binary when a
        // global subscriber is installed by a sibling test.
        crate::activity::end_tick();
    }
}
