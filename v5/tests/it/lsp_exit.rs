//! LSP termination contract: after `exit` (or a bare stdin EOF) the server
//! process must die on its own. Exit code 0 when `shutdown` came first, 1
//! otherwise.
//!
//! FAIL-PRE-FIX RECEIPT: against the pre-fix tree the two `--diag-db` tests hang
//! and are SIGKILLed at the 15s deadline (`test result: FAILED. 1 passed; 2
//! failed`), with the shutdown response already on stdout. The poll thread holds
//! a clone of `connection.sender`, so lsp-server's `IoThreads::join` never sees
//! the writer channel disconnect. `engine_mode_exits_after_shutdown` passed
//! pre-fix and is a non-regression guard: its explicit `--db` suppresses the
//! daemon subscriber, the other thread that holds that same sender clone.
//!
//! Every session runs under its own `DL_STATE_DIR` with `DL_NO_DAEMON=1` so no
//! real daemon or shared state is reachable.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const DL: &str = env!("CARGO_BIN_EXE_dl");

/// How long a well-behaved server gets to die before the test calls it hung.
/// Generous: the pre-fix binary never exits at all, so this only trades test
/// wall-clock against flakiness on a loaded machine.
const EXIT_DEADLINE: Duration = Duration::from_secs(15);

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lsp_exit_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn spawn(root: &Path, args: &[&str]) -> Child {
    let mut child = Command::new(DL)
        .args(args)
        .current_dir(root)
        .env("DL_NO_DAEMON", "1")
        .env("DL_STATE_DIR", root.join("state"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn dl --lsp");
    // stdout must be drained or a full pipe blocks the server's writer thread
    // and the test would measure the wrong stall.
    let stdout = child.stdout.take().unwrap();
    std::thread::spawn(move || {
        let mut r = BufReader::new(stdout);
        let mut sink = String::new();
        while r.read_line(&mut sink).unwrap_or(0) > 0 {
            sink.clear();
        }
    });
    child
}

fn send(child: &mut Child, msg: serde_json::Value) {
    let body = serde_json::to_vec(&msg).unwrap();
    let stdin = child.stdin.as_mut().unwrap();
    write!(stdin, "Content-Length: {}\r\n\r\n", body.len()).unwrap();
    stdin.write_all(&body).unwrap();
    stdin.flush().unwrap();
}

fn initialize(child: &mut Child, root: &Path) {
    send(
        child,
        serde_json::json!({
            "jsonrpc":"2.0","id":1,"method":"initialize",
            "params":{"processId":serde_json::Value::Null,
                      "rootUri": format!("file://{}", root.to_str().unwrap()),
                      "capabilities":{}}
        }),
    );
    send(
        child,
        serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
}

/// Wait out `EXIT_DEADLINE` for the child to die on its own. Returns its exit
/// code; kills and panics with `what` when the deadline passes (the hang).
fn await_exit(child: &mut Child, what: &str) -> i32 {
    let start = Instant::now();
    while start.elapsed() < EXIT_DEADLINE {
        if let Ok(Some(status)) = child.try_wait() {
            return status.code().unwrap_or(-1);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    let _ = child.wait();
    panic!("{what}: still alive after {EXIT_DEADLINE:?}, SIGKILLed");
}

/// `--diag-db` mode: the poll thread runs forever against a db that never
/// appears, which is exactly the shape that used to pin the writer channel open.
#[test]
fn diag_db_mode_exits_after_shutdown() {
    let root = sandbox("diagdb_shutdown");
    let mut child = spawn(
        &root,
        &[
            "--lsp",
            "--diag-db",
            root.join("never-created.sqlite").to_str().unwrap(),
        ],
    );
    initialize(&mut child, &root);
    send(
        &mut child,
        serde_json::json!({"jsonrpc":"2.0","id":99,"method":"shutdown","params":{}}),
    );
    send(
        &mut child,
        serde_json::json!({"jsonrpc":"2.0","method":"exit","params":{}}),
    );
    drop(child.stdin.take());

    let code = await_exit(&mut child, "diag-db shutdown->exit");
    assert_eq!(code, 0, "shutdown before exit is the code-0 case");
}

/// EOF with no `exit` notification: the spec's "no shutdown seen" branch.
#[test]
fn diag_db_mode_exits_on_stdin_eof() {
    let root = sandbox("diagdb_eof");
    let mut child = spawn(
        &root,
        &[
            "--lsp",
            "--diag-db",
            root.join("never-created.sqlite").to_str().unwrap(),
        ],
    );
    initialize(&mut child, &root);
    drop(child.stdin.take());

    let code = await_exit(&mut child, "diag-db stdin EOF");
    assert_eq!(code, 1, "EOF without a prior shutdown is the code-1 case");
}

/// The engine-booting `--lsp` path keeps the same contract. An explicit `--db`
/// means no daemon subscriber thread, so this one already passed pre-fix; it is
/// here so the fix cannot regress the normal path's exit code.
#[test]
fn engine_mode_exits_after_shutdown() {
    let root = sandbox("engine_shutdown");
    fs::write(root.join("a.rs"), "fn main() {}\n").unwrap();
    let prog = root.join("p.dl");
    fs::write(
        &prog,
        "rel seen(path: file).\nseen(path) <- scan(\"**/*.rs\", path).\n",
    )
    .unwrap();

    let mut child = spawn(
        &root,
        &[
            prog.to_str().unwrap(),
            "--lsp",
            "--db",
            root.join("db.sqlite").to_str().unwrap(),
        ],
    );
    initialize(&mut child, &root);
    send(
        &mut child,
        serde_json::json!({"jsonrpc":"2.0","id":99,"method":"shutdown","params":{}}),
    );
    send(
        &mut child,
        serde_json::json!({"jsonrpc":"2.0","method":"exit","params":{}}),
    );
    drop(child.stdin.take());

    let code = await_exit(&mut child, "engine-mode shutdown->exit");
    assert_eq!(code, 0, "shutdown before exit is the code-0 case");
}
