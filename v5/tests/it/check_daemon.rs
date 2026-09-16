//! Discovery-mode `dl --check` daemon-first regression tests.
//!
//! Before this fix, `--check` in discovery mode (no positional program) always
//! defaulted the db to `<root>/.dl/.state/cache.db` and passed `Some(path)` into
//! `run_check`, whose daemon gate only checked `db_path.is_none()` — so it
//! could never attach to a warm singleton daemon serving the same root, and
//! paid a full cold extraction on every hook/check invocation instead.
//!
//! HERMETICITY: every test sets `XDG_STATE_HOME` to its own sandbox (mirrors
//! `tests/it/daemon.rs`), so a leaked/misbehaving daemon can never touch a
//! developer's real socket or state.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::util::DaemonGuard;

const DL: &str = env!("CARGO_BIN_EXE_dl");

struct Sandbox {
    home: PathBuf,
    root: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let base =
            std::env::temp_dir().join(format!("dl_check_daemon_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let home = base.join("xdg");
        let root = base.join("repo");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(root.join(".dl")).unwrap();
        Sandbox { home, root }
    }

    fn sock(&self) -> PathBuf {
        sprefa_v5::daemon::socket_path_for(&self.home.join("sprefa"))
    }

    fn write_program(&self) {
        // Clean program: zero diag rows, but the `diag` rel must exist and be
        // queryable so the daemon-vs-in-process paths are distinguishable by
        // side effects (cache.db presence), not by output content.
        fs::write(
            self.root.join(".dl").join("p.dl"),
            "rel edge(a: text, b: text).\n\
             edge(\"a\", \"b\").\n",
        )
        .unwrap();
    }

    fn spawn(&self) -> DaemonGuard {
        let mut cmd = Command::new(DL);
        cmd.args(["daemon", "start", "--foreground"])
            .current_dir(&self.root)
            .env("DL_DAEMON_ROOT", &self.root)
            .env("XDG_STATE_HOME", &self.home)
            .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        DaemonGuard(cmd.spawn().expect("spawn singleton daemon"))
    }

    fn wait_ready(&self) -> bool {
        let sock = self.sock();
        for _ in 0..200 {
            if sock.exists()
                && std::os::unix::net::UnixStream::connect(&sock).is_ok()
                && rpc_root(&sock, 1, "ping", &self.root, serde_json::json!({})).is_some()
            {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        false
    }

    fn shutdown(&self) {
        let _ = rpc(
            &self.sock(),
            r#"{"jsonrpc":"2.0","id":9000,"method":"shutdown","params":{}}"#,
        );
    }

    /// Run `dl --check` in this sandbox's root, same env as a real hook/CI
    /// invocation would use (no explicit --db, no positional program).
    fn run_check(&self) -> std::process::Output {
        Command::new(DL)
            .arg("--check")
            .current_dir(&self.root)
            .env("DL_DAEMON_ROOT", &self.root)
            .env("XDG_STATE_HOME", &self.home)
            .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
            .output()
            .expect("run dl --check")
    }

    fn cache_db_path(&self) -> PathBuf {
        self.root.join(".dl").join(".state").join("cache.db")
    }

    /// Every `roots/<key>/db.sqlite` in this sandbox's daemon home — the ONE
    /// per-corpus db world (storage-endgame L2); daemon and fallback share it.
    fn root_dbs(&self) -> Vec<PathBuf> {
        let roots = self.home.join("sprefa").join("roots");
        fs::read_dir(&roots)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path().join("db.sqlite"))
            .filter(|db| db.is_file())
            .collect()
    }
}

fn rpc(sock: &std::path::Path, body: &str) -> Option<String> {
    crate::util::uds_rpc(sock, body)
}

fn rpc_root(
    sock: &std::path::Path,
    id: u64,
    method: &str,
    root: &std::path::Path,
    mut params: serde_json::Value,
) -> Option<serde_json::Value> {
    params["root"] = serde_json::json!(root.to_string_lossy());
    let body =
        serde_json::json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}).to_string();
    let resp = rpc(sock, &body)?;
    serde_json::from_str(&resp).ok()
}

/// (a) A sandboxed daemon serving the fixture root answers `--check` in
/// discovery mode, and no fresh in-process extraction happens: `cache.db`
/// is never created (the daemon's engine lives at
/// $XDG_STATE_HOME/sprefa/roots/<hash>/db.sqlite, not `<root>/.dl/.state/cache.db`).
#[test]
fn discovery_check_prefers_warm_daemon_no_cache_db() {
    let sb = Sandbox::new("warm");
    sb.write_program();
    let _daemon = sb.spawn();
    assert!(sb.wait_ready(), "singleton daemon not ready");

    let out = sb.run_check();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "clean program should exit 0: {stderr}"
    );
    assert!(
        !stderr.contains("no daemon serving this root"),
        "a warm daemon must be used, not the loud cold-fallback path: {stderr}"
    );
    assert!(
        !stderr.contains("check attach failed"),
        "daemon attach should succeed cleanly: {stderr}"
    );
    assert!(
        !sb.cache_db_path().exists(),
        "discovery-mode --check against a warm daemon must NOT extract into .dl/.state/cache.db"
    );

    sb.shutdown();
}

/// (b) Without any daemon reachable, `--check` still works in-process AND
/// prints the loud cold-fallback line (never a silent cold extraction) — and
/// it lands in the SAME `roots/<key>/db.sqlite` a daemon would serve
/// (storage-endgame L2: one db per corpus), never a `.dl/.state/cache.db`
/// second world.
#[test]
fn discovery_check_cold_fallback_is_loud() {
    let sb = Sandbox::new("cold");
    sb.write_program();
    // No daemon spawned. DL_NO_DAEMON makes this hermetic against any stray
    // singleton on the machine (belt + suspenders with the isolated XDG home).
    let out = Command::new(DL)
        .arg("--check")
        .current_dir(&sb.root)
        .env("DL_NO_DAEMON", "1")
        .env("XDG_STATE_HOME", &sb.home)
        .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
        .output()
        .expect("run dl --check");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "clean program should exit 0: {stderr}"
    );
    assert!(
        stderr.contains("no daemon serving this root"),
        "a cold one-shot check must name itself loudly: {stderr}"
    );
    assert_eq!(
        sb.root_dbs().len(),
        1,
        "the daemonless one-shot must build the shared roots/<key>/db.sqlite: {stderr}"
    );
    assert!(
        !sb.cache_db_path().exists(),
        "no .dl/.state/cache.db world may exist after L2: {stderr}"
    );
}

/// (b2) Lock contention (the L2 plan gate): a daemon serves the root while a
/// FORCED in-process run (`DL_NO_DAEMON=1`, discovery mode) opens the very
/// same `roots/<key>/db.sqlite`. WAL + busy_timeout is the shared-access
/// discipline — the one-shot must complete, not fail "database is locked".
#[test]
fn forced_inprocess_check_shares_the_daemon_db() {
    let sb = Sandbox::new("contend");
    sb.write_program();
    let _daemon = sb.spawn();
    assert!(sb.wait_ready(), "singleton daemon not ready");
    assert_eq!(
        sb.root_dbs().len(),
        1,
        "daemon must have opened its root db"
    );

    let out = Command::new(DL)
        .arg("--check")
        .current_dir(&sb.root)
        .env("DL_NO_DAEMON", "1")
        .env("XDG_STATE_HOME", &sb.home)
        .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
        .output()
        .expect("run dl --check under a live daemon");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "in-process check against the daemon's live db must succeed: {stderr}"
    );
    assert!(
        !stderr.to_lowercase().contains("database is locked"),
        "WAL + busy_timeout must absorb the write pairing: {stderr}"
    );
    assert_eq!(
        sb.root_dbs().len(),
        1,
        "both worlds must be on ONE db; a second roots entry means a key mismatch"
    );
    assert!(
        !sb.cache_db_path().exists(),
        "no cache.db world under contention either"
    );

    sb.shutdown();
}

/// (b3) The hook fast path (`--check --max-wall`, L2 candidate (b)): with no
/// daemon reachable it renders the LAST COMMITTED diag rows read-only — the
/// db file's bytes must not change — and with no db at all it must not
/// cold-build one.
#[test]
fn hook_check_fallback_is_read_only() {
    let sb = Sandbox::new("readonly");
    // A program whose rail always fires: one error diag row from a fact.
    fs::write(sb.root.join(".dl").join("p.dl"),
        "rel tripwire(name: text).\n\
         tripwire(\"always\").\n\
         diag(path: name, line: 1, severity: \"error\", code: \"rail-ro\", msg: \"tripped\") <- tripwire(name).\n").unwrap();

    let run_check_with = |extra: &[&str]| {
        let mut cmd = Command::new(DL);
        cmd.arg("--check")
            .args(extra)
            .current_dir(&sb.root)
            .env("DL_NO_DAEMON", "1")
            .env("XDG_STATE_HOME", &sb.home)
            .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml");
        cmd.output().expect("run dl --check")
    };

    // No db yet: the hook surface must NOT cold-build one.
    let out = run_check_with(&["--max-wall", "30"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "no-db hook check is a warned no-op, exit 0: {stderr}"
    );
    assert_eq!(
        sb.root_dbs().len(),
        0,
        "a hook check must never cold-build the root db: {stderr}"
    );

    // Warm the db with a full one-shot (exit 2: the rail fires).
    let out = run_check_with(&[]);
    assert_eq!(out.status.code(), Some(2), "warming run: rail-ro must trip");
    let db = sb.root_dbs().pop().expect("warming run built the root db");
    let bytes_before = fs::read(&db).expect("read warm db");

    // Hook surface again: same verdict, read-only — the db bytes are untouched.
    let out = run_check_with(&["--max-wall", "30"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(2),
        "read-only fallback must render the committed rail rows: {stderr}"
    );
    assert!(
        stderr.contains("rail-ro"),
        "the committed diag row renders: {stderr}"
    );
    assert_eq!(
        fs::read(&db).expect("re-read db"),
        bytes_before,
        "the hook fast path must not write the root db"
    );
}

/// (b4) Steady state (the L2 no-re-extraction gate): a SECOND fallback open
/// of the shared root db with nothing changed re-extracts nothing — the
/// `extract:*` digests the first one-shot persisted match, so no
/// `[extract] ... REBUILD` verdict fires on the re-open.
#[test]
fn fallback_reopen_is_steady_state_no_reextraction() {
    let sb = Sandbox::new("steady");
    // Demand a builtin extraction family (`type_entity` -> the type extractor)
    // so family extraction really runs — the `[extract] ... REBUILD` /
    // digest-skip verdicts only exist for extract families, not scan+match
    // source rules (same fixture shape as tests/it/verdict.rs).
    fs::create_dir_all(sb.root.join("src")).unwrap();
    fs::write(
        sb.root.join("src").join("x.rs"),
        "pub struct Alpha { pub n: u32 }\n",
    )
    .unwrap();
    fs::write(sb.root.join(".dl").join("p.dl"),
        "rel seen(path: file).\n\
         seen(path) <- scan(\"WORK\", \"src/**/*.rs\", path, rev).\n\
         rel entity_seen(entity_name: text).\n\
         entity_seen(entity_name) <- type_entity(repo, sym, entity_name, kind, parent, file, line).\n").unwrap();

    let run_check = || {
        Command::new(DL)
            .arg("--check")
            .current_dir(&sb.root)
            .env("DL_NO_DAEMON", "1")
            .env("XDG_STATE_HOME", &sb.home)
            .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
            .output()
            .expect("run dl --check")
    };

    // Cold fallback: builds the shared root db, extraction rebuilds (first-run).
    let out = run_check();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "clean program should exit 0: {stderr}"
    );
    assert!(
        stderr
            .lines()
            .any(|line| line.contains("[extract]") && line.contains("REBUILD")),
        "the cold fallback run must extract (REBUILD verdict expected): {stderr}"
    );
    assert_eq!(
        sb.root_dbs().len(),
        1,
        "cold run lands in roots/<key>/db.sqlite"
    );

    // Steady-state re-open: same root db, nothing changed — zero re-extraction.
    let out = run_check();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "warm re-open should exit 0: {stderr}");
    assert!(
        !stderr
            .lines()
            .any(|line| line.contains("[extract]") && line.contains("REBUILD")),
        "re-opening the shared root db with nothing changed must not re-extract: {stderr}"
    );
    assert_eq!(
        sb.root_dbs().len(),
        1,
        "the re-open must reuse the same root db, never mint a second"
    );
}

/// (c) An explicit program + `--db` keeps the old in-process path exactly:
/// no daemon attach attempted even when a daemon is running for the root.
#[test]
fn explicit_program_and_db_stays_inprocess() {
    let sb = Sandbox::new("explicit");
    sb.write_program();
    let _daemon = sb.spawn();
    assert!(sb.wait_ready(), "singleton daemon not ready");

    let scratch_db = sb.home.join("explicit.sqlite");
    let out = Command::new(DL)
        .arg("--check")
        .arg(sb.root.join(".dl").join("p.dl"))
        .arg("--db")
        .arg(&scratch_db)
        .current_dir(&sb.root)
        .env("DL_DAEMON_ROOT", &sb.root)
        .env("XDG_STATE_HOME", &sb.home)
        .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
        .output()
        .expect("run dl --check --db");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "clean program should exit 0: {stderr}"
    );
    // Positive evidence of the in-process path: the [db] verdict names the
    // explicit db as the one actually opened. (A bare `contains("daemon")`
    // assert false-positived on this test's own sandbox dir name appearing
    // in that verdict's path.)
    assert!(
        stderr.contains("explicit.sqlite"),
        "the [db] open verdict must name the explicit --db path: {stderr}"
    );
    assert!(
        !stderr.contains("no daemon serving"),
        "an explicit --db run must not even consult daemon routing: {stderr}"
    );
    assert!(
        scratch_db.exists(),
        "the explicit --db path is what gets used"
    );
    assert!(
        !sb.cache_db_path().exists(),
        "no cache.db side effect from an explicit --db run"
    );

    sb.shutdown();
}
