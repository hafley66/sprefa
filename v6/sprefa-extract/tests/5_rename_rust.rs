//! `extract rename` on the Rust arm: the item ident, the `use` trailing segment,
//! the `ExprPath`/`TypePath` trailing segment, the glob stop, and this crate's
//! own tree renamed and handed to rustc.
//!
//! @comment-ok: fail-first receipt, repo law keeps these in TEST headers.
//! FAIL-FIRST, against the arc-3 binary (`RustSource` absent from `renames()`):
//!     rust_rename_matches_the_hand_written_after ... exited exit status: 2:
//!         no rename arm for src/util.rs (extract rename renames ts)
//!     glob_importer_is_a_dynamic_stop ... left: Some(2), right: Some(6)
//!     self_rename_is_judged_by_rustc ... exited exit status: 2:
//!         no rename arm for src/rename_cx.rs (extract rename renames ts)
//! FAIL-FIRST (root wins), against the arc-8 binary:
//!     shadowed_items_need_no_at ... exited 3: ambiguous Helper in src/util.rs
//!     renamed_fixture_crate_passes_cargo_check ... passes before: the check
//!         judges rustc's view of the after tree, which is what makes the diff
//!         assertion mean something

use std::path::{Path, PathBuf};
use std::process::Command;

const ANCHOR: &str = "src/util.rs";

struct Fixture {
    root: PathBuf,
    state: PathBuf,
}

fn scratch(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "extract_rename_rust_{label}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_nanos())
            .unwrap_or_default()
    ))
}

fn fixture(case: &str, label: &str) -> Fixture {
    let base = scratch(&format!("{case}_{label}"));
    let root = base.join("repo");
    let state = base.join("state");
    std::fs::create_dir_all(&state).expect("create state dir");
    copy_tree(&tree(case, "before"), &root);
    Fixture {
        root: root.canonicalize().expect("canonicalize fixture root"),
        state,
    }
}

fn tree(case: &str, side: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/rust_rename/{case}/{side}"))
}

fn copy_tree(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).expect("create target dir");
    for entry in std::fs::read_dir(source).expect("read fixture dir") {
        let entry = entry.expect("fixture entry");
        let to = target.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_tree(&entry.path(), &to);
        } else {
            std::fs::copy(entry.path(), &to).expect("copy fixture file");
        }
    }
}

fn rename_verb(fixture: &Fixture, target: &str, new: &str, extra: &[&str]) -> String {
    let output = run_rename(&fixture.root, &fixture.state, target, new, extra);
    assert!(
        output.status.success(),
        "extract rename {extra:?} exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn run_rename(
    root: &Path,
    state: &Path,
    target: &str,
    new: &str,
    extra: &[&str],
) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("rename")
        .arg(target)
        .arg(new)
        .arg("--root")
        .arg(root)
        .arg("--state")
        .arg(state)
        .args(extra)
        .output()
        .expect("extract binary runs")
}

/// `diff -rq left right`, as the arc-5 receipt spells it. Returns the entries.
fn diff_rq(left: &Path, right: &Path) -> Vec<String> {
    let output = Command::new("diff")
        .arg("-rq")
        .arg(left)
        .arg(right)
        .output()
        .expect("diff runs");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

/// The committed tree is the hand-written `after/` tree, byte for byte, which
/// pins what stays too: the `format!("Helper")` string, the `mod other` struct of
/// the same name, and the `H` alias's own body uses.
/// @comment-ok: the after/ tree is the assertion, so the case list lives here
#[test]
fn rust_rename_matches_the_hand_written_after() {
    let fixture = fixture("local", "commit");
    let stdout = rename_verb(&fixture, &format!("{ANCHOR}#Helper"), "Tool", &["--commit"]);
    for line in [
        format!("plan {ANCHOR} Helper -> Tool"),
        "  src/lib.rs  5 uses".to_string(),
        "  src/util.rs  4 uses".to_string(),
    ] {
        assert!(stdout.contains(&line), "missing {line}:\n{stdout}");
    }
    let entries = diff_rq(&fixture.root, &tree("local", "after"));
    assert!(
        entries.is_empty(),
        "committed tree differs from after/:\n{}",
        entries.join("\n")
    );
}

/// `use crate::util::*;` puts the symbol in a scope that writes the bare name
/// with no clause naming it: exit 6 at the `use` item, and the tree keeps its bytes.
#[test]
fn glob_importer_is_a_dynamic_stop() {
    let fixture = fixture("glob", "stop");
    let output = run_rename(
        &fixture.root,
        &fixture.state,
        &format!("{ANCHOR}#Helper"),
        "Tool",
        &["--commit"],
    );
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert_eq!(output.status.code(), Some(6), "Dynamic exits 6:\n{stderr}");
    let text = std::fs::read_to_string(tree("glob", "before").join("src/lib.rs"))
        .expect("glob fixture text");
    let glob_offset = text.find("use crate::util::*;").expect("glob in fixture");
    assert!(
        stderr.contains(&format!("src/lib.rs byte {glob_offset}")),
        "the stop names the use item's own offset:\n{stderr}"
    );
    assert!(
        stderr.contains("glob import"),
        "the stop names the form:\n{stderr}"
    );
    let entries = diff_rq(&fixture.root, &tree("glob", "before"));
    assert!(
        entries.is_empty(),
        "the stopped run edited the tree:\n{}",
        entries.join("\n")
    );
}

/// A root item plus a function-local struct and a `mod nested` struct of the
/// same name: the root item wins without `--at`; the other two keep their
/// spelling and their uses.
#[test]
fn shadowed_items_need_no_at() {
    let fixture = fixture("shadow", "commit");
    rename_verb(&fixture, &format!("{ANCHOR}#Helper"), "Tool", &["--commit"]);
    let entries = diff_rq(&fixture.root, &tree("shadow", "after"));
    assert!(
        entries.is_empty(),
        "committed tree differs from after/:\n{}",
        entries.join("\n")
    );
}

/// rustc judges the renamed fixture crate: `cargo check` on the committed
/// `local` tree exits 0. The crate has no dependencies, so the check is the
/// fixture's own two files and nothing else.
#[test]
fn renamed_fixture_crate_passes_cargo_check() {
    let fixture = fixture("local", "check");
    rename_verb(&fixture, &format!("{ANCHOR}#Helper"), "Tool", &["--commit"]);
    let check = Command::new("cargo")
        .args(["check", "--offline"])
        .env("CARGO_TARGET_DIR", fixture.root.join("target"))
        .current_dir(&fixture.root)
        .output()
        .expect("cargo runs");
    assert!(
        check.status.success(),
        "cargo check on the renamed fixture: {}",
        String::from_utf8_lossy(&check.stderr)
    );
}

/// The verb run against this crate's own tree, judged by rustc, not by an
/// assertion. MEASURED 2026-08-27: 25.2 s, over the 10-second cap, so it runs by
/// hand: `cargo test --features cli --test 5_rename_rust -- --ignored`.
/// @comment-ok: fail-first/measured receipt, repo law keeps these on the test
#[test]
#[ignore]
fn self_rename_is_judged_by_rustc() {
    let base = scratch("self");
    let root = base.join("sprefa-extract");
    let state = base.join("state");
    std::fs::create_dir_all(&state).expect("create state dir");
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    copy_crate(source, &root);
    re_aim_path_deps(source, &root.join("Cargo.toml"));
    let root = root.canonicalize().expect("canonicalize crate copy");

    let output = run_rename(
        &root,
        &state,
        "src/rename_cx.rs#RenameCx",
        "SymbolCx",
        &["--commit"],
    );
    assert!(
        output.status.success(),
        "extract rename exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let lib = std::fs::read_to_string(root.join("src/lib.rs")).expect("read the copy's lib.rs");
    assert!(
        lib.contains("pub use rename_cx::{SymbolCx, RenameRequest};"),
        "the re-export moved:\n{lib}"
    );

    let check = Command::new("cargo")
        .args(["check", "--features", "cli"])
        .current_dir(&root)
        .output()
        .expect("cargo runs");
    assert!(
        check.status.success(),
        "cargo check on the renamed tree: {}",
        String::from_utf8_lossy(&check.stderr)
    );
    let _ = std::fs::remove_dir_all(&base);
}

/// The crate's sources, minus the build output and the git store.
fn copy_crate(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).expect("create target dir");
    for entry in std::fs::read_dir(source).expect("read crate dir") {
        let entry = entry.expect("crate entry");
        let name = entry.file_name();
        if matches!(name.to_string_lossy().as_ref(), ".git" | "target") {
            continue;
        }
        let to = target.join(&name);
        if entry.file_type().expect("file type").is_dir() {
            copy_crate(&entry.path(), &to);
        } else {
            std::fs::copy(entry.path(), &to).expect("copy crate file");
        }
    }
}

/// A copy sits at a different depth, so its two sibling path dependencies are
/// re-pointed at the originals before cargo reads them.
fn re_aim_path_deps(source: &Path, manifest: &Path) {
    let text = std::fs::read_to_string(manifest).expect("read manifest");
    let mut out = text;
    for (rel, name) in [
        ("../../../hafley-rs/crates/soopy", "soopy"),
        ("../../../hafley-rs/crates/hafley-observe", "hafley-observe"),
    ] {
        let absolute = source.join(rel).canonicalize().expect(name);
        out = out.replace(rel, &absolute.to_string_lossy());
    }
    std::fs::write(manifest, out).expect("write manifest");
}
