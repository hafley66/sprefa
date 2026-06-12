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

    // Dry run: previews BOTH the bare use and the brace head (F1b), applies nothing.
    let (code, out, err) = run_move(&d, "src/utils.rs=src/helpers/utils.rs", false);
    assert_eq!(code, 0, "dry run failed: {out}\n{err}");
    assert!(out.contains("crate::utils::Foo -> crate::helpers::utils::Foo"),
        "previews bare-use rewrite: {out}");
    // The brace head `crate::utils` rewrites once, covering both {Bar, Baz}.
    assert!(out.contains("crate::utils -> crate::helpers::utils"),
        "previews brace head rewrite (F1b): {out}");
    // Dry run must not touch the file.
    assert!(fs::read_to_string(d.join("src/app.rs")).unwrap().contains("use crate::utils::Foo;"),
        "dry run left the file unchanged");

    // Apply: both the bare use AND the brace import are rewritten on disk.
    let (code, _out, _err) = run_move(&d, "src/utils.rs=src/helpers/utils.rs", true);
    assert_eq!(code, 0);
    let after = fs::read_to_string(d.join("src/app.rs")).unwrap();
    assert!(after.contains("use crate::helpers::utils::Foo;"), "bare use rewritten: {after}");
    assert!(after.contains("use crate::helpers::utils::{Bar, Baz};"), "brace head rewritten: {after}");
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

/// `--repo "*"` fans the move out over every config repo, rewriting each repo's
/// own references against its own root; `--repo <slug>` scopes to one.
#[test]
fn move_repo_star_fans_out_and_slug_scopes() {
    let d = sandbox("repos");
    for slug in ["ra", "rb"] {
        let r = d.join(slug);
        fs::create_dir_all(r.join("src")).unwrap();
        fs::write(r.join("src/lib.rs"), "pub mod clk;\npub mod cpufreq;\n").unwrap();
        fs::write(r.join("src/clk.rs"), "pub struct Clk;\n").unwrap();
        fs::write(r.join("src/cpufreq.rs"), "use crate::clk::Clk;\npub fn f(_: Clk) {}\n").unwrap();
    }
    fs::write(d.join("cfg.toml"), format!("\
        [[repos]]\nslug = \"ra\"\nroot = \"{a}\"\n\
        [[repos]]\nslug = \"rb\"\nroot = \"{b}\"\n",
        a = d.join("ra").display(), b = d.join("rb").display())).unwrap();

    let run = |args: &[&str]| {
        let out = Command::new(DL).args(args)
            .env("SPREFA_CONFIG", d.join("cfg.toml"))
            .output().expect("run dl --move");
        (out.status.code().unwrap_or(-1),
         String::from_utf8_lossy(&out.stderr).into_owned())
    };

    // --repo ra --fix: only ra is rewritten, rb untouched.
    let (code, _err) = run(&["--repo", "ra", "--move", "src/clk.rs=src/hw/clk.rs", "--fix"]);
    assert_eq!(code, 0);
    assert!(fs::read_to_string(d.join("ra/src/cpufreq.rs")).unwrap().contains("crate::hw::clk::Clk"),
        "ra rewritten");
    assert!(fs::read_to_string(d.join("rb/src/cpufreq.rs")).unwrap().contains("crate::clk::Clk"),
        "rb untouched by --repo ra");

    // --repo "*" --fix: rb now rewritten too (ra already done, idempotent).
    let (code, err) = run(&["--repo", "*", "--move", "src/clk.rs=src/hw/clk.rs", "--fix"]);
    assert_eq!(code, 0, "{err}");
    assert!(err.contains("repo ra") && err.contains("repo rb"), "fan-out names both repos: {err}");
    assert!(fs::read_to_string(d.join("rb/src/cpufreq.rs")).unwrap().contains("crate::hw::clk::Clk"),
        "rb rewritten by --repo *");
}

/// Kotlin: `--move OLD.kt=NEW.kt` derives the package change from the moved
/// file's own `package` decl + the directory delta under its source root, then
/// rewrites import specifiers at their ref-spine spans. Wildcard imports and
/// same-package bare uses are counted loudly, not rewritten.
#[test]
fn move_rewrites_kotlin_imports() {
    let d = sandbox("kotlin");
    fs::create_dir_all(d.join("src/com/lib")).unwrap();
    fs::create_dir_all(d.join("src/com/app")).unwrap();
    fs::write(d.join("src/com/lib/Util.kt"),
        "package com.lib\n\nclass Util\nfun helper() = 1\n").unwrap();
    fs::write(d.join("src/com/lib/Peer.kt"),
        "package com.lib\n\nclass Peer {\n    val u = Util()\n}\n").unwrap();
    fs::write(d.join("src/com/app/Main.kt"), concat!(
        "package com.app\n\n",
        "import com.lib.Util\n",
        "import com.lib.helper\n",
        "import com.lib.*\n",
        "fun main() { Util(); helper() }\n")).unwrap();

    // Dry run previews the rewrites and counts the wildcard + same-package uses.
    let (code, out, err) = run_move(&d, "src/com/lib/Util.kt=src/com/core/Util.kt", false);
    assert_eq!(code, 0, "dry run failed: {out}\n{err}");
    assert!(out.contains("com.lib.Util -> com.core.Util"), "previews class import: {out}");
    assert!(out.contains("com.lib.helper -> com.core.helper"), "previews fn import: {out}");
    assert!(err.contains("wildcard import(s) of com.lib"), "wildcard counted: {err}");
    assert!(err.contains("same-package bare use(s)"), "Peer.kt's bare Util counted: {err}");

    // Apply: import lines rewritten on disk; wildcard left alone.
    let (code, _out, _err) = run_move(&d, "src/com/lib/Util.kt=src/com/core/Util.kt", true);
    assert_eq!(code, 0);
    let after = fs::read_to_string(d.join("src/com/app/Main.kt")).unwrap();
    assert!(after.contains("import com.core.Util\n"), "{after}");
    assert!(after.contains("import com.core.helper\n"), "{after}");
    assert!(after.contains("import com.lib.*\n"), "wildcard untouched: {after}");
}

/// A Kotlin file whose directory disagrees with its package declaration is a
/// loud skip — the new package cannot be inferred from the path delta.
#[test]
fn move_kotlin_layout_mismatch_is_loud() {
    let d = sandbox("ktmismatch");
    fs::write(d.join("src/Util.kt"), "package com.lib\nclass Util\n").unwrap();
    let (code, _out, err) = run_move(&d, "src/Util.kt=src/core/Util.kt", false);
    assert_eq!(code, 0);
    assert!(err.contains("does not match its package"), "{err}");
}

/// An unknown `--repo` slug is a loud error, not a silent no-op.
#[test]
fn move_unknown_repo_slug_errors() {
    let d = sandbox("badrepo");
    fs::write(d.join("cfg.toml"), "[[repos]]\nslug = \"ra\"\nroot = \"/tmp/ra\"\n").unwrap();
    let out = Command::new(DL)
        .args(["--repo", "nope", "--move", "a=b"])
        .env("SPREFA_CONFIG", d.join("cfg.toml"))
        .output().expect("run dl --move");
    assert!(!out.status.success(), "unknown slug fails");
    assert!(String::from_utf8_lossy(&out.stderr).contains("not a configured repo slug"));
}
