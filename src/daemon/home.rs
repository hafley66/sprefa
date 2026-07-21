//! Daemon home/socket/pid/roots.json path helpers (relocated from the old
//! monolithic `daemon.rs`; decomposition plan step 6).
use super::*;

// ---------- path helpers ----------

/// The daemon's home: `$XDG_STATE_HOME/sprefa` (or `~/.local/state/sprefa`). ONE
/// singleton serving daemon lives here, decoupled from any project root — the
/// "folders in view" model. Tests set `XDG_STATE_HOME` to a sandbox, which makes
/// a stray test daemon structurally unable to bind a developer's socket. Created
/// on demand.
pub fn daemon_home() -> PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".local/state")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("sprefa")
}

/// macOS caps `sockaddr_un.sun_path` at 104 bytes; Linux at 108. A deep
/// `$XDG_STATE_HOME` (a sandbox under a long temp path) can overrun it, so
/// `socket_path` relocates the socket to a short hashed path when the natural
/// one is too long. Both bind and every connect derive it from the same home, so
/// they always agree. Leave slack for the trailing NUL.
const SUN_MAX: usize = 100;

/// `<home>/daemon.sock`, unless that path is too long for `sun_path` — then a
/// short, deterministic path keyed by the home's hash. THE singleton socket.
pub fn socket_path() -> PathBuf {
    socket_path_for(&daemon_home())
}

/// The socket for a given home dir (env-independent; the deep-home test drives
/// this directly). `<home>/daemon.sock`, relocated to a short hashed path when
/// the natural one overruns `sun_path`.
pub fn socket_path_for(home: &Path) -> PathBuf {
    let natural = home.join("daemon.sock");
    if natural.as_os_str().len() < SUN_MAX {
        natural
    } else {
        let hash = blake3::hash(home.as_os_str().as_encoded_bytes());
        short_sock_dir().join(format!("{}.sock", &hash.to_hex()[..16]))
    }
}

/// Short base directory for a relocated socket. `TMPDIR`-derived so it stays on a
/// short path; created 0700 on first use.
fn short_sock_dir() -> PathBuf {
    let base = std::env::temp_dir().join("dl-sock");
    if let Err(e) = std::fs::create_dir_all(&base) {
        tracing::debug!("short_sock_dir create {}: {e}", base.display());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o700));
    }
    base
}

/// `<home>/daemon.pid`.
pub fn pid_path() -> PathBuf {
    daemon_home().join("daemon.pid")
}

/// `<home>/daemon.log` — the un-supervised fallback's own redirect target
/// (`daemon::client::spawn_detached` opens this file itself via
/// `std::process::Stdio::from`, so it is a real in-process writer, unlike the
/// two paths below). One named function so `spawn_detached` and the log-cap
/// sweep (`daemon::logcap`) always agree on the path, the same reason
/// `root_db_path` exists instead of every caller inlining `.join(...)`.
pub fn daemon_log_path(home: &Path) -> PathBuf {
    home.join("daemon.log")
}

/// `<home>/launchd-stdout.log` — launchd's `StandardOutPath` redirect target
/// for the supervised daemon (`crate::supervise::plist_contents`). launchd
/// itself opens this file (`O_CREAT|O_APPEND`) and `dup2`s it onto the
/// process's stdout BEFORE this binary execs; no `dl` code ever holds this as
/// a writer it can rotate by closing and reopening a new file. The log-cap
/// sweep (`daemon::logcap`) still needs the exact path so it can bound the
/// file from the OUTSIDE (truncate in place) — see that module's doc for why
/// truncate, not rename.
pub fn launchd_stdout_log_path(home: &Path) -> PathBuf {
    home.join("launchd-stdout.log")
}

/// `<home>/launchd-stderr.log` — launchd's `StandardErrorPath` twin of
/// `launchd_stdout_log_path`. This is the file the class-28 incident
/// (docs/failure-modes.md) found at 440MB: every `tracing` event that reaches
/// stderr under launchd supervision lands here, unrotated, because nothing in
/// the process owns this fd's lifecycle.
pub fn launchd_stderr_log_path(home: &Path) -> PathBuf {
    home.join("launchd-stderr.log")
}

/// `<home>/roots.json` — the registered-root persistence file.
pub(crate) fn roots_json_path() -> PathBuf {
    daemon_home().join("roots.json")
}

/// `<home>/roots/<key>` — one dir per registered root; holds `db.sqlite`.
pub(crate) fn root_db_dir(key: &str) -> PathBuf {
    daemon_home().join("roots").join(key)
}

/// ONE db per corpus (storage-endgame L2): the db for `root`, daemon-served or
/// not — `<home>/roots/<key>/db.sqlite`, the exact file `add_root` opens when a
/// daemon serves this root. The daemonless one-shot / hook / LSP fallback opens
/// this same file instead of a second `.dl/.state/cache.db` world; WAL +
/// busy_timeout (db.rs) is the shared-access discipline for the
/// daemon-up-but-attach-failed pairing. Canonicalization mirrors `add_root` so
/// both sides compute the same key. Pure path math; callers `create_dir_all`
/// the parent before opening.
pub fn root_db_path(root: &Path) -> PathBuf {
    let canon = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    root_db_dir(&key_of(&canon)).join("db.sqlite")
}

/// The registry key for a root: blake3-16hex of its canonical path. Symlinked
/// aliases collapse to one entry.
pub(crate) fn key_of(canon: &Path) -> String {
    blake3::hash(canon.as_os_str().as_encoded_bytes()).to_hex()[..16].to_string()
}

/// Open (create if missing) `<home>/daemon.pid` for the fd-lock single-
/// instance witness (plan section 3.3: "belt-and-suspenders against `dl serve
/// --foreground` beside the [launchd] agent" — replaces the old bespoke
/// pidfile, which was informational only and enforced nothing; a real
/// advisory `flock` now makes a second `run_daemon` in this home fail fast
/// instead of silently double-serving). Split out of the lock-acquisition
/// call site so the open/create-dir step stays testable without the
/// borrow-checker shape `fd_lock::RwLock`'s guard imposes on its caller.
// Deliberately no `.truncate(true)`: truncating here, before the fd-lock is
// even attempted, would wipe a RUNNING daemon's pid content out from under it
// on the losing side of a lock race. The winner truncates explicitly (via
// `set_len(0)` on the locked guard) only after `try_write` confirms it holds
// the lock; the loser's `open` must leave the holder's file untouched.
#[allow(clippy::suspicious_open_options)]
pub(crate) fn open_pid_lock_file() -> Result<std::fs::File> {
    let dir = daemon_home();
    std::fs::create_dir_all(&dir)?;
    std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        // Never truncate on open: a second instance opens this file too (to
        // attempt the lock) and must not blank the holder's pid/start-time
        // record. The lock HOLDER truncates after `try_write` succeeds.
        .truncate(false)
        .open(pid_path())
        .with_context(|| format!("open singleton lock {}", pid_path().display()))
}
