//! Auto-refactor sink (Route A): `dl --move OLD=NEW` rewrites `use`-path
//! references after a module move, splicing the new path at the byte coordinate
//! the ref-spine located. Bare uses rewrite; brace leaves are reported skipped
//! (their located span is the leaf name, not the full path — F1b).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mv_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

fn run_move(dir: &Path, spec: &str, fix: bool) -> (i32, String, String) {
    let mut cmd = Command::new(DL);
    cmd.args(["--move", spec, "--root", dir.to_str().unwrap(),
        "--db", dir.join("db").to_str().unwrap()]);
    if fix { cmd.arg("--fix"); }
    let out = cmd.output().expect("run dl --move");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn move_rewrites_bare_use_and_reports_brace_skips() {
    let d = sandbox("bare");
    fs::write(d.join("src/lib.rs"), "mod utils;\nmod app;\nfn main() {}\n").unwrap();
    fs::write(d.join("src/app.rs"),
        "use crate::utils::Foo;\nuse crate::utils::{Bar, Baz};\nfn go() {}\n").unwrap();
    fs::write(d.join("src/utils.rs"), "pub struct Foo;\npub struct Bar;\npub struct Baz;\n").unwrap();

    // Dry run: previews the bare-use rewrite, applies nothing, warns about braces.
    let (code, out, err) = run_move(&d, "src/utils.rs=src/helpers/utils.rs", false);
    assert_eq!(code, 0, "dry run failed: {out}\n{err}");
    assert!(out.contains("crate::utils::Foo -> crate::helpers::utils::Foo"),
        "previews bare-use rewrite: {out}");
    assert!(err.contains("2 brace-import reference(s) not rewritten"),
        "reports the two brace leaves as skipped: {err}");
    // Dry run must not touch the file.
    assert!(fs::read_to_string(d.join("src/app.rs")).unwrap().contains("use crate::utils::Foo;"),
        "dry run left the file unchanged");

    // Apply: the bare use is rewritten on disk; the brace line is untouched.
    let (code, _out, _err) = run_move(&d, "src/utils.rs=src/helpers/utils.rs", true);
    assert_eq!(code, 0);
    let after = fs::read_to_string(d.join("src/app.rs")).unwrap();
    assert!(after.contains("use crate::helpers::utils::Foo;"), "bare use rewritten: {after}");
    assert!(after.contains("use crate::utils::{Bar, Baz};"), "brace line untouched: {after}");
}

#[test]
fn move_with_no_matching_refs_is_a_noop() {
    let d = sandbox("noop");
    fs::write(d.join("src/lib.rs"), "mod app;\nfn main() {}\n").unwrap();
    fs::write(d.join("src/app.rs"), "use std::collections::HashMap;\nfn go() {}\n").unwrap();
    let (code, _out, err) = run_move(&d, "src/utils.rs=src/helpers/utils.rs", false);
    assert_eq!(code, 0);
    assert!(err.contains("no use-path references to rewrite"), "no-op reported: {err}");
}
