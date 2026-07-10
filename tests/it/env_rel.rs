//! The built-in `env(name, value)` relation projects only allowlisted
//! (SPREFA_/DL_/SG_ prefix, or CI/GITHUB_ACTIONS/GITLAB_CI exact) process
//! environment variables — everything else (tokens, credentials) must stay
//! invisible to a `.dl` program.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("env_rel_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str, extra_env: &[(&str, &str)]) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let mut cmd = Command::new(DL);
    cmd.arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap()])
        .current_dir(dir);
    for (key, value) in extra_env {
        cmd.env(key, value);
    }
    let out = cmd.output().expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

/// An allowlisted var (SPREFA_ prefix) shows up in `env(name, value)`.
#[test]
fn allowlisted_prefix_var_is_visible() {
    let d = sandbox("allow");
    let (code, out, err) = run(
        &d,
        "? env(\"SPREFA_TESTVAR\", value).\n",
        &[("SPREFA_TESTVAR", "hello")],
    );
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("hello"), "allowlisted var must appear:\n{out}");
}

/// A non-allowlisted var (no matching prefix, not a CI marker) is invisible:
/// the query yields no rows and its value never reaches stdout.
#[test]
fn non_allowlisted_var_is_invisible() {
    let d = sandbox("deny");
    let (code, out, err) = run(
        &d,
        "? env(\"MY_SECRET_TOKEN\", value).\n",
        &[("MY_SECRET_TOKEN", "leaked")],
    );
    assert_eq!(code, 0, "{err}");
    assert!(!out.contains("leaked"), "secret var must not surface:\n{out}");
}
