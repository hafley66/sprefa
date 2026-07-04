//! `std/suppress.dl` — the eslint/biome comment-directive suppression grammar
//! as pure dl over `comment_node`. Proves inline `dl-disable-line`,
//! `dl-disable-next-line`, block `dl-disable`/`dl-enable` pairing, code scoping,
//! and reason capture, plus the converted `examples/lint-unwrap.dl` rail (the
//! inline trailing form that a whole-line-only comment op could never express).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");
const REPO: &str = env!("CARGO_MANIFEST_DIR");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("suppress_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    // Copy the std/ dir so `use "std/suppress.dl"` resolves from the program dir.
    let std_src = Path::new(REPO).join("std");
    let std_dst = dir.join("std");
    fs::create_dir_all(&std_dst).unwrap();
    for e in fs::read_dir(&std_src).unwrap() {
        let e = e.unwrap();
        if e.path().extension().is_some_and(|x| x == "dl") {
            fs::copy(e.path(), std_dst.join(e.file_name())).unwrap();
        }
    }
    dir
}

fn run(dir: &Path, prog: &str) -> String {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--root", dir.to_str().unwrap(),
               "--db", dir.join("db").to_str().unwrap(), "--no-daemon"])
        .output()
        .expect("run dl");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Drive the suppress lib directly: scan a Rust file full of directives and
/// query the exported rels.
#[test]
fn directive_grammar_line_next_block_scope_reason() {
    let d = sandbox("grammar");
    // 1: inline disable-line (scoped) + reason
    // 3: disable-next-line -> covers 4
    // 5: block open (scoped) ; 7: block close -> covers 5..7
    fs::write(d.join("src/lib.rs"),
        "fn a() { foo(); } // dl-disable-line no-unwrap -- allowed here\n\
         fn b() { bar(); }\n\
         // dl-disable-next-line no-unwrap\n\
         fn c() { baz(); }\n\
         // dl-disable other-code\n\
         fn d() { qux(); }\n\
         // dl-enable other-code\n").unwrap();
    let prog = r#"
use "std/suppress.dl".
rel seen(p: file).
seen(p) <- scan("WORK", "src/**/*.rs", p, rev).
# feed candidate lines so block ranges close over them
lint_candidate(p, l) <- comment_node(p, l, _, _, _, _, _).
? suppressed(path, line, code).
? suppress_reason(path, line, reason).
? suppress_span(path, code, start, endl).
"#;
    let out = run(&d, prog);
    // exact-line: disable-line on line 1, disable-next-line target line 4
    assert!(out.contains("src/lib.rs\t1\tno-unwrap"), "disable-line:\n{out}");
    assert!(out.contains("src/lib.rs\t4\tno-unwrap"), "disable-next-line -> +1:\n{out}");
    // code scoping: line 1 is "no-unwrap", never "other-code"
    assert!(!out.contains("src/lib.rs\t1\tother-code"), "scope leaked:\n{out}");
    // block: disable line 5, enable line 7 -> span 5..7 for other-code
    assert!(out.contains("src/lib.rs\tother-code\t5\t7"), "block span:\n{out}");
    // reason captured
    assert!(out.contains("src/lib.rs\t1\tallowed here"), "reason:\n{out}");
}

/// Unscoped `dl-disable-line` (no codes) yields the `"*"` row.
#[test]
fn unscoped_directive_is_wildcard() {
    let d = sandbox("wildcard");
    fs::write(d.join("src/lib.rs"),
        "fn a() { foo(); } // dl-disable-line\n").unwrap();
    let prog = r#"
use "std/suppress.dl".
rel seen(p: file).
seen(p) <- scan("WORK", "src/**/*.rs", p, rev).
? suppressed(path, line, code).
"#;
    let out = run(&d, prog);
    assert!(out.contains("src/lib.rs\t1\t*"), "wildcard row:\n{out}");
}

/// The converted rail: an inline trailing `// dl-disable-line no-unwrap` on the
/// SAME line as the `.unwrap()` offense silences just that diagnostic, while an
/// un-annotated `.unwrap()` on another line still fires.
#[test]
fn lint_unwrap_rail_inline_suppression() {
    let d = sandbox("rail");
    fs::write(d.join("src/lib.rs"),
        "pub fn a() -> i32 { let o: Option<i32> = None; o.unwrap() } // dl-disable-line no-unwrap\n\
         pub fn b() -> i32 { let o: Option<i32> = None; o.unwrap() }\n").unwrap();
    // Copy the real rail from examples/ so the test tracks the shipped file.
    let rail = fs::read_to_string(Path::new(REPO).join("examples/lint-unwrap.dl")).unwrap();
    fs::write(d.join("p.dl"), rail).unwrap();
    let out = Command::new(DL)
        .arg(d.join("p.dl"))
        .args(["--root", d.to_str().unwrap(),
               "--db", d.join("db").to_str().unwrap(), "--no-daemon"])
        .output().expect("run dl");
    let out = String::from_utf8_lossy(&out.stdout).into_owned();
    // line 2 (unannotated) fires; line 1 (annotated) is suppressed.
    assert!(out.contains("\t2\t"), "unannotated unwrap should fire:\n{out}");
    assert!(!out.contains("\t1\t"), "inline dl-disable-line should suppress line 1:\n{out}");
}
