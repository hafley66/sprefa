//! `db bytes / corpus bytes` verdict (failure-modes class 17,
//! docs/failure-modes.md:407-441): "db bytes are a budgeted resource like CPU
//! and I/O — a root db an order of magnitude larger than its corpus is a
//! defect to explain, not a cost to absorb." This module answers that
//! question with one verdict line + a threshold warn, following the
//! `crate::verdict` "run-header" pattern already used for the `[db] opened
//! ...` line (`src/db.rs::open`).
//!
//! Fires twice: once per root at daemon boot (`ServedRoot::open`,
//! `src/daemon.rs`) and once per completed tick of a cold `--no-daemon
//! --check` run (`run_check_inproc`, `src/lib.rs`) — the two "a corpus just
//! got read into a db" moments the class 16+17 incident named. Deliberately
//! NOT on every warm daemon tick: the ratio doesn't move tick-to-tick, and
//! re-walking the whole corpus on a hot path would just be new I/O in the
//! name of measuring old I/O.

use std::path::Path;

/// Ratio (db_bytes / corpus_bytes) above which the verdict escalates from an
/// info line to a `tracing::warn!` + a loud verdict line. `DL_DB_RATIO_WARN`
/// overrides. The incident root measured ~140x; scip on the same repo runs
/// ~3x, CodeQL 5-20x (docs/failure-modes.md:434) — 100x sits solidly past
/// both those references without firing on a healthy db.
const DEFAULT_RATIO_WARN: f64 = 100.0;

fn ratio_warn_threshold() -> f64 {
    parse_ratio_threshold(std::env::var("DL_DB_RATIO_WARN").ok())
}

/// Pure parse, split out from `ratio_warn_threshold` so the parsing rules
/// (garbage/zero/negative all fall back to the default) are unit-testable
/// without mutating process-global env state under a parallel test run.
fn parse_ratio_threshold(raw: Option<String>) -> f64 {
    raw.as_deref()
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|v| *v > 0.0)
        .unwrap_or(DEFAULT_RATIO_WARN)
}

/// Sum of file bytes under `root`, honoring `.gitignore` and pruning nested
/// `.git`-owning directories (submodules) — the same walk shape
/// `engine::scan::enumerate_with_hash`'s WORK arm uses, minus its per-rule
/// glob filter: this is the whole corpus denominator, not one rule's scanned
/// subset. Best-effort: an unreadable entry contributes 0 rather than
/// aborting the whole verdict; a symlink loop is bounded by `ignore`'s own
/// walk (it does not follow symlinks by default).
fn corpus_bytes(root: &Path) -> u64 {
    let mut walk = ignore::WalkBuilder::new(root);
    walk.hidden(false).filter_entry(|entry| {
        if entry.file_name() == ".git" {
            return false;
        }
        if entry.depth() >= 1
            && entry.file_type().is_some_and(|ft| ft.is_dir())
            && entry.path().join(".git").exists()
        {
            return false;
        }
        true
    });
    walk.build()
        .flatten()
        .filter(|entry| entry.file_type().is_some_and(|ft| ft.is_file()))
        .filter_map(|entry| entry.metadata().ok())
        .map(|meta| meta.len())
        .sum()
}

/// Emit the db-ratio verdict for `root`'s db at `db_path`. No-op if the db
/// file doesn't exist yet, or the corpus is empty (a served program with no
/// scan rules — the ratio is undefined, not infinite, so there's nothing
/// honest to warn about).
pub fn emit_verdict(root: &Path, db_path: &Path) {
    let Ok(db_meta) = std::fs::metadata(db_path) else { return };
    let db_bytes = db_meta.len();
    let corpus_bytes = corpus_bytes(root);
    if corpus_bytes == 0 {
        return;
    }
    let ratio = db_bytes as f64 / corpus_bytes as f64;
    let msg = format!(
        "[db-ratio] {} db={db_bytes}B corpus={corpus_bytes}B ratio={ratio:.1}x",
        root.display()
    );
    crate::verdict::verdict(
        "db-ratio",
        &msg,
        &[
            ("root", &root.to_string_lossy()),
            ("db_bytes", &db_bytes.to_string()),
            ("corpus_bytes", &corpus_bytes.to_string()),
            ("ratio", &format!("{ratio:.3}")),
        ],
    );
    let threshold = ratio_warn_threshold();
    if ratio > threshold {
        let warn_msg = format!(
            "[db-ratio] {} WARNING ratio {ratio:.1}x exceeds {threshold:.1}x \
             (db={db_bytes}B, corpus={corpus_bytes}B) — see docs/failure-modes.md class 17 \
             (storage-diet arc)",
            root.display()
        );
        tracing::warn!(
            root = %root.display(), db_bytes, corpus_bytes, ratio, threshold,
            "db-ratio ceiling exceeded"
        );
        crate::verdict::verdict(
            "db-ratio",
            &warn_msg,
            &[
                ("root", &root.to_string_lossy()),
                ("db_bytes", &db_bytes.to_string()),
                ("corpus_bytes", &corpus_bytes.to_string()),
                ("ratio", &format!("{ratio:.3}")),
                ("threshold", &format!("{threshold:.3}")),
                ("outcome", "ratio-warn"),
            ],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn sandbox(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("dl_db_ratio_unit_{tag}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn corpus_bytes_sums_files_and_skips_dotgit() {
        let dir = sandbox("sum");
        fs::write(dir.join("a.rs"), "0123456789").unwrap(); // 10 bytes
        fs::write(dir.join("b.rs"), "01234").unwrap(); // 5 bytes
        let git_dir = dir.join(".git");
        fs::create_dir_all(&git_dir).unwrap();
        fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        assert_eq!(corpus_bytes(&dir), 15);
    }

    #[test]
    fn ratio_threshold_parse_falls_back_on_garbage_zero_negative_and_unset() {
        assert_eq!(parse_ratio_threshold(None), DEFAULT_RATIO_WARN);
        assert_eq!(parse_ratio_threshold(Some("42".into())), 42.0);
        assert_eq!(parse_ratio_threshold(Some("not-a-number".into())), DEFAULT_RATIO_WARN);
        assert_eq!(parse_ratio_threshold(Some("0".into())), DEFAULT_RATIO_WARN);
        assert_eq!(parse_ratio_threshold(Some("-5".into())), DEFAULT_RATIO_WARN);
    }
}
