//! Stale-binary rail (failure-modes class 13 promotion,
//! docs/failure-modes.md:300-327): when `dl` runs inside a repo that BUILDS
//! `dl` itself (this crate, self-hosted/dogfooded) and `cargo build` has
//! produced a fresher `target/{debug,release}/dl` than the executable
//! actually running, warn once — a hook, daemon, or PATH lookup that keeps
//! answering from a stale install is exactly the freeze #1 shape (a worker
//! ran `dl --check`, auto-started the daemon with the STALE installed
//! binary, and the boot cascade + swap froze the machine until it was killed
//! and the binary reinstalled).
//!
//! The identity half (crate version + exe mtime) is the same shape as
//! `crate::daemon::build_id` — that function answers "is the RUNNING daemon
//! this process's binary"; this module answers "does this checkout have a
//! NEWER build sitting in `target/` than what's currently running", which
//! `build_id` alone can't say (it only ever compares two running processes'
//! identities, never a running process against an on-disk `cargo build`
//! artifact).
//!
//! Never fatal: a hook or daemon that keeps running against a stale binary
//! is a bug to surface, not a reason to refuse to run (that would trade a
//! slow bug for an outage). `DL_NO_STALE_WARN=1` suppresses entirely.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

const BIN_NAME: &str = "dl";

/// Flips true on the first emitted warning so a process that calls this from
/// more than one surface (e.g. daemon start THEN a status probe) prints the
/// line at most once — "once per invocation" per the rail's contract.
static ALREADY_WARNED: AtomicBool = AtomicBool::new(false);

/// Warn on stderr (+ `perf.jsonl`, via `crate::verdict`) when a repo checkout
/// under `start` (or an ancestor) builds this crate AND `target/{debug,
/// release}/dl` there is newer than the binary currently executing. `start`
/// is the working root the caller already resolved (the served root for a
/// daemon, cwd for a one-shot `--check`/`daemon status`) — never re-derived
/// here, since root resolution has its own `.dl`-ancestor rules this module
/// has no business reimplementing.
pub fn warn_if_stale(start: &Path) {
    let Some(running_exe) = std::env::current_exe().ok() else {
        return;
    };
    let Some(running_mtime) = mtime(&running_exe) else {
        return;
    };
    warn_if_stale_with_reference(start, &running_exe.to_string_lossy(), running_mtime);
}

/// Same rail, compared against an explicit `(reference_label, reference_mtime)`
/// instead of this process's own `current_exe()` — the launchd/systemd
/// re-point (plans/2026-07-18-infra-library-adoption.md section 4c): once
/// supervision owns `dl daemon start`, `dl daemon status` is asking whether
/// the SUPERVISED daemon binary is stale, not whether the `dl` binary running
/// the status check itself is — those can differ (multiple installs on PATH,
/// a `cargo build` run since the daemon was last (re)started). The daemon's
/// own self-check at `run_daemon` startup still calls `warn_if_stale` plain
/// (its `current_exe()` IS the supervised binary from inside that process);
/// this variant is for a caller checking on a DIFFERENT process's binary via
/// out-of-band identity (e.g. the daemon's reported `build_id` mtime, decoded
/// by `crate::cli::daemon_cmd`'s supervised `status` path).
pub fn warn_if_stale_with_reference(
    start: &Path,
    reference_label: &str,
    reference_mtime: SystemTime,
) {
    if std::env::var("DL_NO_STALE_WARN").ok().as_deref() == Some("1") {
        return;
    }
    if ALREADY_WARNED.load(Ordering::Relaxed) {
        return;
    }
    let Some(build_root) = crate_build_root(start) else {
        return;
    };
    for profile in ["debug", "release"] {
        let candidate = build_root.join("target").join(profile).join(BIN_NAME);
        let Some(built_mtime) = mtime(&candidate) else {
            continue;
        };
        if !built_is_strictly_newer(built_mtime, reference_mtime) {
            continue;
        }
        // Compare-and-set so a race between two call sites in the same
        // process (unlikely — one-shot CLI runs are single-threaded at this
        // point) still prints exactly once.
        if ALREADY_WARNED.swap(true, Ordering::SeqCst) {
            return;
        }
        let msg = format!(
            "[dl] warning: installed dl is older than this repo's build — \
             installed {reference_label} ({running_age} ago) vs {candidate} ({built_age} ago) — \
             cargo install --path . to deploy",
            running_age = humanize_age(reference_mtime),
            candidate = candidate.display(),
            built_age = humanize_age(built_mtime),
        );
        crate::verdict::verdict(
            "stale-binary",
            &msg,
            &[
                ("running_exe", reference_label),
                (
                    "running_mtime_secs",
                    &epoch_secs(reference_mtime).to_string(),
                ),
                ("build_candidate", &candidate.to_string_lossy()),
                ("build_mtime_secs", &epoch_secs(built_mtime).to_string()),
            ],
        );
        return;
    }
}

/// Walk `start` and its ancestors for a `Cargo.toml` whose `[package].name`
/// matches this crate's published name (`env!("CARGO_PKG_NAME")` = the
/// compile-time constant `sprefa-dl`, not the runtime binary name `dl`) — a
/// checkout that BUILDS `dl`, not merely a repo `dl` happens to be pointed
/// at. Returns that Cargo.toml's directory, the root a plain `cargo build`
/// run from inside it drops `target/` under (works for both a plain crate
/// and this crate's own `[workspace]` manifest, since here the workspace
/// root IS the package root).
fn crate_build_root(start: &Path) -> Option<PathBuf> {
    start.ancestors().find_map(|dir| {
        let text = std::fs::read_to_string(dir.join("Cargo.toml")).ok()?;
        let value = toml::from_str::<toml::Value>(&text).ok()?;
        let name = value
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str());
        (name == Some(env!("CARGO_PKG_NAME"))).then(|| dir.to_path_buf())
    })
}

/// Strictly-newer at whole-second granularity. The supervised-status path
/// feeds a reference decoded from `build_id` ("version:exe_mtime_secs"),
/// which truncates to whole seconds; the fs mtime of the `target/` candidate
/// keeps nanoseconds. Comparing raw `SystemTime`s made the SAME binary read
/// as "newer" by its sub-second remainder (the equal-age "7m45s vs 7m45s"
/// false warn). Truncate both sides to epoch seconds so equal means silent
/// and only a genuinely later build (>= 1s apart) triggers.
fn built_is_strictly_newer(built_mtime: SystemTime, reference_mtime: SystemTime) -> bool {
    epoch_secs(built_mtime) > epoch_secs(reference_mtime)
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

fn epoch_secs(t: SystemTime) -> u64 {
    t.duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `t`'s age relative to now as a terse human string (`"3h12m"`, `"45s"`) —
/// no date-formatting crate pulled in for one warning line's two timestamps
/// (build-vs-buy: `chrono`/`time` are not dependencies today and a ~15-line
/// duration bucketer is the whole cost). A future `t` (clock skew, or a
/// `target/` artifact touched by a concurrent build mid-check) reads as
/// `"0s"` rather than underflowing.
fn humanize_age(t: SystemTime) -> String {
    let secs = SystemTime::now()
        .duration_since(t)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m{}s", secs / 60, secs % 60)
    } else if secs < 86_400 {
        format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d{}h", secs / 86_400, (secs % 86_400) / 3600)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn humanize_age_buckets() {
        assert_eq!(humanize_age(SystemTime::now()), "0s");
        assert_eq!(
            humanize_age(SystemTime::now() - std::time::Duration::from_secs(90)),
            "1m30s"
        );
        assert_eq!(
            humanize_age(SystemTime::now() - std::time::Duration::from_secs(3 * 3600 + 12 * 60)),
            "3h12m"
        );
        assert_eq!(
            humanize_age(SystemTime::now() - std::time::Duration::from_secs(2 * 86_400 + 5 * 3600)),
            "2d5h"
        );
        // Future mtime (clock skew) never underflows/panics.
        assert_eq!(
            humanize_age(SystemTime::now() + std::time::Duration::from_secs(60)),
            "0s"
        );
    }

    #[test]
    fn equal_mtime_is_not_stale() {
        let reference = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_800_000_000);
        // Exactly equal: silent.
        assert!(!built_is_strictly_newer(reference, reference));
        // Same whole second, built carries a sub-second remainder the
        // seconds-truncated build_id reference lost: still silent (this was
        // the "7m45s vs 7m45s" false warn).
        let built_same_second = reference + std::time::Duration::from_millis(400);
        assert!(!built_is_strictly_newer(built_same_second, reference));
        // Built older: silent.
        let built_older = reference - std::time::Duration::from_secs(5);
        assert!(!built_is_strictly_newer(built_older, reference));
        // Built a full second later: warns.
        let built_newer = reference + std::time::Duration::from_secs(1);
        assert!(built_is_strictly_newer(built_newer, reference));
    }

    #[test]
    fn crate_build_root_finds_this_crates_own_manifest() {
        // This test binary IS built from this repo's Cargo.toml, so walking
        // up from the test's own source-relative cwd (the workspace root
        // cargo sets as `CARGO_MANIFEST_DIR` at compile time) must resolve.
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let found = crate_build_root(&manifest_dir);
        assert_eq!(found.as_deref(), Some(manifest_dir.as_path()));
    }

    #[test]
    fn crate_build_root_none_outside_any_crate() {
        // A fresh, empty temp dir (never a Cargo.toml naming this package,
        // unlike `/tmp` itself which could theoretically sit under some
        // ancestor crate root on an unusual dev machine).
        let dir = std::env::temp_dir().join(format!(
            "dl_stale_binary_test_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(crate_build_root(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
