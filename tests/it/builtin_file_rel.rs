//! Proof of the built-in `file` relation (data-model migration, Stage 1): any
//! rule can join the file set without a `scan`; the reserved names error if a
//! program redeclares them; and a cross-file reference validated by joining
//! `file` drops when the target file is deleted (FS-as-facts) — the primitive
//! the cross-codebase module graph is built on.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("builtin_file_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str, extra: &[&str]) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap()])
        .args(extra)
        .current_dir(dir)
        .output().expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn file_relation_is_queryable_without_scan() {
    let d = sandbox("queryable");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/x.rs"), "fn a() {}\n").unwrap();
    fs::write(d.join("src/y.rs"), "fn b() {}\n").unwrap();
    // `seen` scans (populates the file set); `known` joins the built-in `file`
    // relation with no scan of its own.
    let prog = r#"
rel seen(path: file).
rel known(path: file).
seen(path) <- scan("WORK", "src/**/*.rs", path, rev), sg(path, rev, :rust, "fn $N() {}", line).
known(p) <- file(_, rev, p, _).
? known(p).
"#;
    let (code, out, _) = run(&d, prog, &[]);
    assert_eq!(code, 0);
    assert!(out.contains("src/x.rs") && out.contains("src/y.rs"), "file set must be queryable: {out}");
    assert!(out.contains("(2 rows)"), "expected both files: {out}");
}

/// The walk runs with hidden(false) (dotfiles are scannable), which un-hides
/// `.git` too — it must be filter_entry'd out explicitly or big-repo scans
/// crawl the object store.
#[test]
fn scan_never_enters_dot_git() {
    let d = sandbox("dotgit");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::create_dir_all(d.join(".git/objects")).unwrap();
    fs::create_dir_all(d.join(".hidden")).unwrap();
    fs::write(d.join("src/x.txt"), "real\n").unwrap();
    fs::write(d.join(".hidden/y.txt"), "dotdir, scannable\n").unwrap();
    fs::write(d.join(".git/objects/z.txt"), "object store, never\n").unwrap();
    let prog = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "**/*.txt", path, rev).
? seen(p).
"#;
    let (code, out, err) = run(&d, prog, &[]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("src/x.txt"), "{out}");
    assert!(out.contains(".hidden/y.txt"), "hidden dirs other than .git scan: {out}");
    assert!(!out.contains(".git/objects/z.txt"), ".git must be skipped: {out}");
}

/// A submodule worktree is not gitignored — its files sit inside the parent
/// repo's tree with nothing on disk marking them ignored — so it must be
/// pruned as a foreign repo the same way `.git` itself is pruned. Covers
/// both `.git` forms: a plain directory (an ordinary nested checkout) and a
/// FILE containing `gitdir: ...` (the real submodule-worktree form).
#[test]
fn scan_prunes_nested_repo_dir_and_file_git_forms() {
    let d = sandbox("nested_repo");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/main.rs"), "fn main() {}\n").unwrap();

    // Directory-form `.git` (an ordinary nested checkout under vendor/).
    fs::create_dir_all(d.join("vendor/nested_dir/.git")).unwrap();
    fs::write(d.join("vendor/nested_dir/lib.rs"), "fn nested_dir_fn() {}\n").unwrap();

    // File-form `.git` (the submodule-worktree form: a text file pointing at
    // the real gitdir, not a directory).
    fs::create_dir_all(d.join("vendor/nested_submodule")).unwrap();
    fs::write(d.join("vendor/nested_submodule/.git"), "gitdir: /elsewhere/modules/nested_submodule\n").unwrap();
    fs::write(d.join("vendor/nested_submodule/lib.rs"), "fn nested_submodule_fn() {}\n").unwrap();

    let prog = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "**/*.rs", path, rev).
? seen(p).
"#;
    let (code, out, err) = run(&d, prog, &[]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("src/main.rs"), "sibling source file must be scanned: {out}");
    assert!(!out.contains("vendor/nested_dir/lib.rs"),
        "dir-form nested repo must be pruned: {out}");
    assert!(!out.contains("vendor/nested_submodule/lib.rs"),
        "file-form (submodule) nested repo must be pruned: {out}");
}

#[test]
fn declaring_a_builtin_name_errors() {
    let d = sandbox("collision");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/x.rs"), "fn a() {}\n").unwrap();
    let prog = "rel file(x: text).\nfile(a) <- scan(\"WORK\", \"src/**/*.rs\", a, r).\n";
    let (code, _, err) = run(&d, prog, &[]);
    assert_ne!(code, 0, "redeclaring `file` must fail");
    assert!(err.contains("reserved-name") && err.contains("relation `file`")
        && err.contains("write to the built-in directly"), "earlier reserved-name fix, got: {err}");
}

#[test]
fn cross_file_ref_drops_when_target_deleted() {
    let d = sandbox("xref");
    fs::write(d.join("a.txt"), "ref b.txt\n").unwrap();
    fs::write(d.join("b.txt"), "hello\n").unwrap();
    // holds(a,b) only if `a` references `b` AND `b` is a real file (the join).
    let prog = r#"
rel link(src: file, dst: text).
rel holds(src: file, dst: text).
link(src, dst) <- scan("WORK", "*.txt", src, rev), match(src, rev, /ref (?<dst>\S+)/, line).
holds(src, dst) <- link(src, dst), file(_, rev, dst, _).
? holds(src, dst).
"#;
    let (_, out, _) = run(&d, prog, &[]);
    assert!(out.contains("a.txt\tb.txt"), "cold: holds(a.txt,b.txt) expected: {out}");

    // Delete the target and tick only that path. a.txt is NOT reparsed, yet the
    // edge must drop because the `file` join no longer finds b.txt.
    fs::remove_file(d.join("b.txt")).unwrap();
    let (_, _, err) = run(&d, prog, &["--changed", "b.txt"]);
    assert!(err.contains("rebuilt derived: holds") || err.contains("holds"),
        "the holds rule (joins file) must rebuild on the file-set change: {err}");

    let (_, out2, _) = run(&d, prog, &[]);
    // holds is now empty: the reference survives but its target file is gone.
    assert!(out2.contains("(0 rows)"), "holds must drop when b.txt is gone: {out2}");
}
