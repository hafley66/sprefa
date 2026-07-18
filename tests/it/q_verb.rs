//! End-to-end coverage of the `dl q <verb>` concept-verb runner (src/verbs.rs +
//! src/cli/query.rs, with the daemon `q` RPC in src/daemon.rs). The no-daemon
//! path forces a cold in-process engine; the daemon path drives the real
//! singleton over a raw `q` RPC (the mcp_daemon idiom). Fixture: a two-file TS
//! project (a model + a repo that calls into it) so the type + call families
//! populate.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::util::DaemonGuard;

const DL: &str = env!("CARGO_BIN_EXE_dl");

/// model.ts declares `lookup`; repo.ts's `Repo.find` calls it.
fn write_fixture(dir: &Path) {
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("src/model.ts"),
        "export interface Entity { id: Id }\n\
         export function lookup(): Entity { return build() }\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/repo.ts"),
        "import { Entity, lookup } from './model'\n\
         export class Repo {\n\
         \x20   find(): Entity {\n\
         \x20       return lookup()\n\
         \x20   }\n\
         }\n",
    )
    .unwrap();
}

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("q_verb_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    write_fixture(&dir);
    dir
}

/// Run a `dl` verb (subcommand) in `dir`, forcing the in-process path.
fn run_verb(dir: &Path, args: &[&str]) -> (i32, String, String) {
    let out = Command::new(DL)
        .args(args)
        .arg("--no-daemon")
        .current_dir(dir)
        .output()
        .expect("run dl q");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn who_calls_finds_the_caller() {
    let d = sandbox("who_calls");
    let (code, out, err) = run_verb(&d, &["q", "who-calls", "lookup"]);
    assert_eq!(code, 0, "who-calls failed:\nstdout={out}\nstderr={err}");
    // Repo.find is the only caller of lookup.
    assert!(out.contains("find"), "caller `find` missing: {out}");
    // The resolution note (resolve_name) reports the candidate families.
    assert!(out.contains("resolved:"), "resolution note missing: {out}");
}

/// `Repo.find` declares at line 3 but only calls `lookup()` at line 7 (padded
/// with blank body lines in between). who-calls must report the CALL's own
/// line (7), not `find`'s declaration line (3).
fn write_padded_fixture(dir: &Path) {
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("src/model.ts"),
        "export interface Entity { id: Id }\n\
         export function lookup(): Entity { return build() }\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/repo.ts"),
        "import { Entity, lookup } from './model'\n\
         export class Repo {\n\
         \x20   find(): Entity {\n\
         \x20       // padding\n\
         \x20       // padding\n\
         \x20       // padding\n\
         \x20       return lookup()\n\
         \x20   }\n\
         }\n",
    )
    .unwrap();
}

fn padded_sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("q_verb_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    write_padded_fixture(&dir);
    dir
}

#[test]
fn who_calls_reports_the_call_site_line_not_the_decl_line() {
    let d = padded_sandbox("who_calls_site_line");
    let (code, out, err) = run_verb(&d, &["q", "who-calls", "lookup"]);
    assert_eq!(code, 0, "who-calls failed:\nstdout={out}\nstderr={err}");
    // The call to lookup() is on line 7 (1-based); find()'s own decl is line 3.
    assert!(out.contains('7'), "expected the call-site line (7) in output: {out}");
    assert!(
        !out.contains("\t3\t") && !out.trim().lines().any(|l| l.split('\t').any(|c| c == "3")),
        "reported the decl line (3) instead of the call-site line: {out}"
    );
}

#[test]
fn where_defined_locates_the_def() {
    let d = sandbox("where_defined");
    let (code, out, err) = run_verb(&d, &["q", "where-defined", "lookup"]);
    assert_eq!(code, 0, "where-defined failed:\nstdout={out}\nstderr={err}");
    assert!(out.contains("model.ts"), "def file model.ts missing: {out}");
    assert!(out.contains("resolved:"), "resolution note missing: {out}");
}

#[test]
fn unknown_verb_exits_two() {
    let d = sandbox("unknown");
    let (code, _out, err) = run_verb(&d, &["q", "no-such-verb", "lookup"]);
    assert_eq!(code, 2, "unknown verb must exit 2: {err}");
    assert!(err.contains("unknown verb"), "message names the problem: {err}");
    assert!(err.contains("who-calls"), "message lists the real verbs: {err}");
}

#[test]
fn bare_q_lists_verbs() {
    let d = sandbox("list");
    let (code, out, _err) = run_verb(&d, &["q"]);
    assert_eq!(code, 0, "bare `dl q` should succeed: {out}");
    assert!(out.contains("who-calls"), "verb listing missing who-calls: {out}");
    assert!(out.contains("where-defined"), "verb listing missing where-defined: {out}");
}

#[test]
fn verb_catalog_rel_lists_the_verbs() {
    // The `verb_catalog` builtin meta rel is the queryable twin of `dl q`.
    let d = sandbox("catalog");
    let out = Command::new(DL)
        .arg("? verb_catalog(verb, args, doc).")
        .args(["--no-daemon"])
        .current_dir(&d)
        .output()
        .expect("run dl inline");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("who-calls"), "verb_catalog missing who-calls: {stdout}");
    assert!(stdout.contains("where-defined"), "verb_catalog missing where-defined: {stdout}");
}

// ---------- daemon path ------------------------------------------------------

struct DaemonBox {
    home: PathBuf,
    root: PathBuf,
}

impl DaemonBox {
    fn new(tag: &str) -> Self {
        let base = std::env::temp_dir().join(format!("q_verb_daemon_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let home = base.join("xdg");
        let root = base.join("repo");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(root.join(".dl")).unwrap();
        write_fixture(&root);
        // A trivial served program; the verb snippet carries its own scan block.
        fs::write(root.join(".dl/serve.dl"), "rel noop(x: int).\nnoop(1).\n? noop(x).\n").unwrap();
        DaemonBox { home, root }
    }
    fn sock(&self) -> PathBuf {
        sprefa_v5::daemon::socket_path_for(&self.home.join("sprefa"))
    }
    fn spawn(&self) -> DaemonGuard {
        DaemonGuard(
            Command::new(DL)
                .args(["daemon", "start", "--foreground"])
                .arg(self.root.join(".dl/serve.dl"))
                .current_dir(&self.root)
                .env("DL_DAEMON_ROOT", &self.root)
                .env("XDG_STATE_HOME", &self.home)
                .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("spawn daemon"),
        )
    }
    fn wait_ready(&self) -> bool {
        for _ in 0..200 {
            if self.sock().exists() && rpc_root(&self.sock(), 1, "ping", &self.root).is_some() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        false
    }
}

fn rpc(sock: &Path, body: &str) -> String {
    crate::util::uds_rpc(sock, body).expect("daemon rpc over the uds socket")
}

fn rpc_root(sock: &Path, id: u64, method: &str, root: &Path) -> Option<serde_json::Value> {
    let body = serde_json::json!({"jsonrpc":"2.0","id":id,"method":method,
        "params":{"root": root.to_string_lossy()}})
    .to_string();
    std::panic::catch_unwind(|| rpc(sock, &body))
        .ok()
        .and_then(|r| serde_json::from_str(&r).ok())
}

fn q_rpc(sock: &Path, root: &Path, verb: &str, target: &str) -> serde_json::Value {
    let body = serde_json::json!({"jsonrpc":"2.0","id":9,"method":"q",
        "params":{"root": root.to_string_lossy(), "verb": verb, "target": target}})
    .to_string();
    serde_json::from_str(&rpc(sock, &body)).expect("q response is JSON")
}

/// Both verbs answered through the daemon's warm-scratch `q` RPC.
#[test]
fn daemon_q_runs_both_verbs() {
    let b = DaemonBox::new("both");
    let _guard = b.spawn();
    assert!(b.wait_ready(), "daemon did not come up");

    let who = q_rpc(&b.sock(), &b.root, "who-calls", "lookup");
    let who_rows = who["result"]["rows"].as_array().expect("who-calls rows");
    let who_flat = serde_json::to_string(who_rows).unwrap();
    assert!(who_flat.contains("find"), "who-calls via daemon missing `find`: {who}");

    let wd = q_rpc(&b.sock(), &b.root, "where-defined", "lookup");
    let wd_rows = wd["result"]["rows"].as_array().expect("where-defined rows");
    let wd_flat = serde_json::to_string(wd_rows).unwrap();
    assert!(wd_flat.contains("model.ts"), "where-defined via daemon missing model.ts: {wd}");
    // Count-first contract: the envelope carries the pre-paging total.
    assert!(wd["result"]["total"].as_u64().is_some(), "total missing: {wd}");
}
