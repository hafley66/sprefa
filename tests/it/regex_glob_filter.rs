//! Proof of the `=~` (regex) and `~~` (glob) constraint operators. `=~` lowers to
//! SQLite `REGEXP` (registered `regexp()` function over the `regex` crate); `~~`
//! lowers to native SQLite `GLOB`. Filtering is done by NESTING: a derived rule
//! carries the constraint in its body, then a query reads that relation. (The
//! `where` query clause was removed; see
//! plans/2026-06-02-kill-where-seed-closures-by-nesting.md.)

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("regex_glob_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str) -> String {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap()])
        .current_dir(dir)
        .output().expect("run dl");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn regex_and_glob_filter_derived_columns() {
    let d = sandbox("derived");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/a.rs"), "fn tick() {}\nfn tick_paths() {}\nfn other() {}\n").unwrap();
    fs::write(d.join("src/b.rs"), "fn refresh_rel() {}\n").unwrap();

    let prog = r#"
rel fn_def(name: text, path: file).
fn_def(name, path) <- scan("WORK","src/**/*.rs",path,rev),
  ast(path, rev, :rust, "(function_item name: (identifier) @name) @fn", line).
rel tickish(name: text, path: file).
tickish(name, path) <- fn_def(name, path), name =~ /tick/.
rel refreshes(name: text, path: file).
refreshes(name, path) <- fn_def(name, path), name =~ /^refresh_/.
rel in_b(name: text, path: file).
in_b(name, path) <- fn_def(name, path), path ~~ "*b.rs".
? tickish(name, path).
? refreshes(name, path).
? in_b(name, path).
"#;
    let out = run(&d, prog);

    // `=~ /tick/` matches both tick and tick_paths, not `other`.
    assert!(out.contains("tick"), "regex substring match: {out}");
    assert!(out.contains("tick_paths"), "regex substring match: {out}");
    // anchored regex matches only refresh_rel.
    assert!(out.contains("refresh_rel"), "anchored regex: {out}");
    // glob on the path column selects only src/b.rs (refresh_rel lives there).
    let glob_block = out.rsplit("? in_b").next().unwrap();
    assert!(glob_block.contains("b.rs"), "glob path match: {glob_block}");
    assert!(!glob_block.contains("a.rs"), "glob must exclude a.rs: {glob_block}");
}
