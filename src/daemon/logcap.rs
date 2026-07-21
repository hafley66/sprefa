//! External log-size cap sweep (failure-modes class 31,
//! docs/failure-modes.md): the two files this process never opens for
//! writing itself, but still costs unbounded disk if left ungoverned.
//!
//! `launchd-stdout.log` / `launchd-stderr.log` (`home::launchd_stdout_log_path`
//! / `home::launchd_stderr_log_path`) are opened by launchd itself
//! (`StandardOutPath`/`StandardErrorPath` in the plist, `crate::supervise`)
//! with `O_CREAT|O_APPEND`, then `dup2`'d onto this process's stdout/stderr
//! BEFORE it execs. No `tracing_subscriber::Layer`, no `tracing-appender`, no
//! other in-process rotator crate can rotate them: rotation means closing the
//! current writer and opening a fresh file, and the writer here (fd 1/2) is
//! not a handle this process owns or can repoint — that is launchd's
//! machinery, running before any Rust code in this binary runs. This is the
//! reason the incident names two separate problems with two separate
//! answers: `crate::trace`'s `RollingWriter` (dl.log/error.log) and
//! `crate::why` (why.jsonl) already bound the files THIS process opens for
//! itself; this module bounds the ones it does not.
//!
//! `daemon.log` (`home::daemon_log_path`) sits in between: the un-supervised
//! `daemon::client::spawn_detached` fallback DOES open it itself (via
//! `std::process::Stdio::from`), so an in-process rotator could apply there —
//! but its existing cap only runs once, at spawn time, before the child
//! execs. A daemon that then runs for weeks has no periodic recheck. Folding
//! it into this same sweep costs nothing extra (one more `stat`) and gives it
//! the same continuous cap the launchd pair needs anyway.
//!
//! The mechanism for all three is one primitive: truncate the file to 0 bytes
//! IN PLACE (same path, same inode) once it crosses a cap. This is safe
//! specifically because every writer here opens the file `O_APPEND`
//! (launchd's redirect; `spawn_detached`'s `OpenOptions::append(true)`):
//! POSIX recomputes the write offset to the current end-of-file on EVERY
//! `O_APPEND` write, so a truncate-to-0 issued from a completely separate fd
//! (this sweep, running inside the daemon process) is visible to the
//! writer's very NEXT write with no signal, no reopen, no torn line. A RENAME
//! — the `RollingWriter`/`why.rs` house pattern for files this process itself
//! opens fresh on every write — would be WRONG here: it would orphan the
//! launchd-held (or spawn_detached child's) fd onto a now-unlinked inode that
//! keeps growing forever, invisible to `ls`, until the process next restarts.
//! Truncate-in-place is the only external-redirect-safe move; see
//! `truncate_in_place_is_safe_under_a_live_o_append_writer` below for the
//! proof.
//!
//! Swept from the daemon's existing 30s idle-task cadence
//! (`daemon_shell::timers::idle_task`) plus once at boot (`run_daemon`) — not
//! a new timer. Three `stat` calls (four bytes of syscall, no lock, no
//! engine) is noise-floor cheap; nothing here can stall a tick or block the
//! hot path.

use std::path::{Path, PathBuf};

/// Matches the cap `daemon::client::spawn_detached` already used for
/// `daemon.log` before this module existed — one number, one place, for
/// every externally-redirected log this sweep governs. Small enough that a
/// runaway process never meaningfully dents a disk, big enough to hold a real
/// incident's worth of lines (the same sizing reasoning `trace::ROTATE_BYTES`
/// documents for dl.log/error.log, just realized as a hard cap instead of a
/// rename-with-one-generation since these files cannot be renamed safely).
pub(crate) const EXTERNAL_LOG_CAP_BYTES: u64 = 8 * 1024 * 1024;

/// The three externally-redirected log paths under `home` this sweep governs,
/// in the order it checks them.
fn governed_paths(home: &Path) -> [PathBuf; 3] {
    [
        super::launchd_stdout_log_path(home),
        super::launchd_stderr_log_path(home),
        super::daemon_log_path(home),
    ]
}

/// Truncate `path` to 0 bytes IN PLACE (no rename, same inode) if it exceeds
/// `cap_bytes`. Returns whether it truncated, so callers can log/test the
/// outcome. A missing file is a no-op, not an error: a fresh daemon home has
/// none of these yet, and a purely-supervised install never creates
/// `daemon.log` at all (only `spawn_detached` does).
fn cap_in_place(path: &Path, cap_bytes: u64) -> bool {
    let Ok(meta) = std::fs::metadata(path) else { return false };
    if meta.len() <= cap_bytes {
        return false;
    }
    match std::fs::OpenOptions::new().write(true).open(path) {
        Ok(file) => match file.set_len(0) {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!("[logcap] truncate {}: {e}", path.display());
                false
            }
        },
        Err(e) => {
            tracing::warn!("[logcap] open {} for truncate: {e}", path.display());
            false
        }
    }
}

/// Sweep every externally-redirected log under `home`, truncating in place
/// whichever crossed `EXTERNAL_LOG_CAP_BYTES`. Called once at daemon boot
/// (`run_daemon`, so a long-down daemon does not wait out a stale oversized
/// file) and every idle-task tick thereafter (`daemon_shell::timers`).
pub(crate) fn sweep(home: &Path) {
    for path in governed_paths(home) {
        if cap_in_place(&path, EXTERNAL_LOG_CAP_BYTES) {
            tracing::info!(
                "[logcap] capped {} at {}MB (external redirect, truncated in place)",
                path.display(),
                EXTERNAL_LOG_CAP_BYTES / (1024 * 1024)
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Seek, SeekFrom, Write as _};

    fn sandbox(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dl_logcap_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Baseline contract: over-cap truncates, under-cap is left alone, and a
    /// missing file is a quiet no-op (not a panic/error).
    #[test]
    fn cap_in_place_truncates_over_cap_leaves_under_cap_alone() {
        let dir = sandbox("basic");
        let big = dir.join("big.log");
        let small = dir.join("small.log");
        let missing = dir.join("missing.log");
        std::fs::write(&big, vec![b'x'; 100]).unwrap();
        std::fs::write(&small, vec![b'x'; 5]).unwrap();

        assert!(cap_in_place(&big, 10), "over-cap file must report truncated");
        assert_eq!(std::fs::metadata(&big).unwrap().len(), 0, "over-cap file truncated to 0");

        assert!(!cap_in_place(&small, 10), "under-cap file must not be touched");
        assert_eq!(std::fs::metadata(&small).unwrap().len(), 5, "under-cap file untouched");

        assert!(!cap_in_place(&missing, 10), "missing file is a no-op, not a panic");
    }

    /// THE crux safety proof this whole design rests on: a writer that opened
    /// the file `O_APPEND` (exactly what launchd does for
    /// `StandardOutPath`/`StandardErrorPath`, and what `spawn_detached` does
    /// for `daemon.log`) can keep writing through an EXTERNAL truncate-in-place
    /// with no torn output, no reopen, no signal — because POSIX recomputes
    /// the append offset to the current end-of-file on every `write(2)` call.
    /// Simulates exactly the production shape: one long-lived handle (the
    /// "launchd"/"spawn_detached child" side) writes before AND after a
    /// second, independent handle (this sweep) truncates the same path.
    #[test]
    fn truncate_in_place_is_safe_under_a_live_o_append_writer() {
        let dir = sandbox("o_append");
        let path = dir.join("launchd-stderr.log");

        // The "launchd redirect" side: one long-lived O_APPEND handle, opened
        // once, exactly like a real redirected fd that never reopens.
        let mut writer = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .unwrap();
        writer.write_all(b"before-cap line one\n").unwrap();
        writer.write_all(b"before-cap line two\n").unwrap();
        writer.flush().unwrap();
        assert!(std::fs::metadata(&path).unwrap().len() > 0, "pre-truncate content landed");

        // The sweep's side: a SEPARATE fd truncates the SAME path in place —
        // no rename, so the writer's fd still points at the same inode/name.
        assert!(cap_in_place(&path, 5), "over-cap (cap=5, content > 5 bytes) truncates");
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 0, "truncated to 0 in place");

        // The ORIGINAL O_APPEND handle, never reopened, keeps writing — and
        // must land at offset 0, not at the pre-truncate offset (which would
        // either error past-EOF or leave a zero-filled hole).
        writer.write_all(b"after-cap line\n").unwrap();
        writer.flush().unwrap();
        drop(writer);

        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            content, "after-cap line\n",
            "O_APPEND writer must resume cleanly at offset 0 after an external truncate-in-place, \
             with no hole and no leftover pre-truncate bytes: {content:?}"
        );
    }

    /// A rename-based rotation (the `RollingWriter`/`why.rs` house pattern for
    /// files THIS process opens fresh) would be the wrong move for an
    /// externally-redirected writer: the writer's fd stays bound to the
    /// renamed (now unlinked-from-the-original-name) inode and keeps writing
    /// there forever, invisible under the original path. This test pins that
    /// failure mode so nobody "fixes" this module by copy-pasting
    /// `RollingWriter`'s rename step.
    #[test]
    fn rename_would_orphan_an_external_o_append_writer_truncate_does_not() {
        let dir = sandbox("rename_orphan");
        let path = dir.join("launchd-stdout.log");
        let mut writer = std::fs::OpenOptions::new().create(true).append(true).open(&path).unwrap();
        writer.write_all(b"line one\n").unwrap();

        // Rename the file out from under the writer (what a naive port of
        // `RollingWriter`'s rotation would do).
        let rotated = dir.join("launchd-stdout.log.1");
        std::fs::rename(&path, &rotated).unwrap();
        writer.write_all(b"line two, still going to the OLD inode\n").unwrap();
        writer.flush().unwrap();

        // The original path is either missing or freshly-created-empty by
        // something else — never the writer's continued output. The writer's
        // bytes landed in the ROTATED file instead, proving the orphan.
        let original_has_writer_output = std::fs::read_to_string(&path)
            .map(|s| s.contains("line two"))
            .unwrap_or(false);
        assert!(!original_has_writer_output,
            "a rename-rotated writer's output must NOT reappear under the original path");
        let rotated_content = std::fs::read_to_string(&rotated).unwrap();
        assert!(rotated_content.contains("line two"),
            "the still-open writer's output lands in the renamed (orphaned) file: {rotated_content:?}");
    }

    /// `sweep` covers all three governed paths and leaves an under-cap file
    /// alone, driving the real path helpers (`home::launchd_stdout_log_path`
    /// etc.) rather than hand-rolled joins, so a path drift between this
    /// module and `supervise.rs`/`client.rs` would fail this test.
    #[test]
    fn sweep_truncates_all_three_governed_paths_over_cap() {
        let home = sandbox("sweep_all");
        let paths = governed_paths(&home);
        for path in &paths {
            std::fs::write(path, vec![b'x'; (EXTERNAL_LOG_CAP_BYTES + 1) as usize]).unwrap();
        }
        let under_cap = home.join("unrelated-small.log");
        std::fs::write(&under_cap, b"tiny").unwrap();

        sweep(&home);

        for path in &paths {
            assert_eq!(
                std::fs::metadata(path).unwrap().len(),
                0,
                "{} must be truncated by sweep",
                path.display()
            );
        }
        assert_eq!(std::fs::metadata(&under_cap).unwrap().len(), 4,
            "sweep must not touch files it does not govern");
    }
}
