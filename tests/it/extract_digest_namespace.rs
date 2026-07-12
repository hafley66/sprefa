//! Regression: the extract-skip digest is namespaced by the running binary's
//! identity (a KEY segment), not folded into the digest VALUE at a shared key.
//!
//! The all-day CPU/battery storm: a `dl daemon` served the OLD image while a
//! freshly `cargo install`ed CLI ran the SAME repo against the SAME `cache.db`.
//! With the exe stamp folded into the digest value at a shared key, each process
//! saw the other's stamp, mismatched, and re-extracted the whole corpus, writing
//! its own stamp back — so the next tick of the other process rebuilt again.
//! Perpetual full re-extraction on every tick (629 files x 5 repos), pinning the
//! machine. Namespacing the KEY by exe stamp gives each binary its own skip
//! state: a new binary does exactly one cold rebuild, then both skip forever.
//!
//! `DL_EXE_STAMP` overrides the stamp so two "binaries" are one test process.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn fixture(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("extract_ns_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/a.rs"), "fn alpha() { beta(); }\nfn beta() {}\n").unwrap();
    fs::write(dir.join("src/b.rs"), "struct Foo { x: i32 }\nfn make() -> Foo { Foo { x: 1 } }\n").unwrap();
    fs::write(
        dir.join("p.dl"),
        "rel seen(path: path).\n\
         seen(path) <- scan(\"WORK\", \"src/**/*.rs\", path, rev).\n\
         rel call_name_q(sym: text, name: text).\n\
         call_name_q(sym, name) <- call_name(sym, name).\n\
         ? call_name_q(sym, name).\n",
    )
    .unwrap();
    dir
}

/// Returns the `[extract] call: ...` verdict line for one run under a given
/// exe-stamp identity, against a shared db in `dir`.
fn run(dir: &Path, exe_stamp: &str) -> String {
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap()])
        .current_dir(dir)
        .env("SPREFA_CONFIG", "/nonexistent/x.toml")
        .env("DL_NO_DAEMON", "1")
        .env("DL_LOG", "debug")
        .env("DL_EXE_STAMP", exe_stamp)
        .output()
        .expect("run dl");
    String::from_utf8_lossy(&out.stderr)
        .lines()
        .find(|l| l.contains("[extract] call:"))
        .unwrap_or("<no call verdict>")
        .to_string()
}

#[test]
fn one_binary_warm_tick_skips_the_second_run() {
    let dir = fixture("warm");
    assert!(run(&dir, "A").contains("REBUILD"), "first run is a cold build");
    assert!(
        run(&dir, "A").contains("skipped"),
        "an unchanged corpus under the same binary must skip the second run"
    );
}

/// The storm's exact shape: A warm, then B runs (its own cold build), then A
/// again. A MUST still skip — B's run does not clobber A's skip state. On the
/// pre-fix code (exe stamp in the value at a shared key) this fourth run
/// rebuilt with reason `exe-identity-changed`, the ping-pong that never ends.
#[test]
fn a_different_binary_does_not_invalidate_the_first_binarys_warm_state() {
    let dir = fixture("twobin");
    assert!(run(&dir, "A").contains("REBUILD"), "A cold");
    assert!(run(&dir, "A").contains("skipped"), "A warm");
    assert!(run(&dir, "B").contains("REBUILD"), "B is a new binary: one cold build in its own namespace");
    let a_again = run(&dir, "A");
    assert!(
        a_again.contains("skipped"),
        "A must stay warm after B ran against the shared db (no ping-pong); got: {a_again}"
    );
    let b_again = run(&dir, "B");
    assert!(
        b_again.contains("skipped"),
        "B must also stay warm (both namespaces coexist); got: {b_again}"
    );
}
