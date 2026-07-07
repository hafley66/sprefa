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
        .args(["--db", dir.join("db").to_str().unwrap(), "--no-daemon"])
        .current_dir(dir)
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
        .args(["--db", d.join("db").to_str().unwrap(), "--no-daemon"])
        .current_dir(&d)
        .output().expect("run dl");
    let out = String::from_utf8_lossy(&out.stdout).into_owned();
    // line 2 (unannotated) fires the no-unwrap WARN; line 1 (annotated) does not.
    // (line 1 still carries the shipped-on `dl-directive` info marker, so assert
    // on the specific code, not "any diag on line 1".)
    assert!(out.lines().any(|l| l.contains("\t2\t") && l.contains("no-unwrap")),
        "unannotated unwrap should fire:\n{out}");
    assert!(!out.lines().any(|l| l.contains("\t1\t") && l.contains("warn\tno-unwrap")),
        "inline dl-disable-line should suppress the no-unwrap warn on line 1:\n{out}");
    // the directive itself is now visible as a subtle info marker.
    assert!(out.lines().any(|l| l.contains("\t1\t") && l.contains("info\tdl-directive")),
        "line 1 directive should show an info marker:\n{out}");
}

/// A recognized directive earns a subtle INFO marker sitting on the comment's
/// own byte span (col > 0), code `dl-directive`, message naming the effect.
#[test]
fn directive_shows_info_marker_at_comment_span() {
    let d = sandbox("visible");
    fs::write(d.join("src/lib.rs"),
        "fn a() { foo(); } // dl-disable-line no-unwrap\n").unwrap();
    let prog = r#"
use "std/suppress.dl".
rel seen(p: file).
seen(p) <- scan("WORK", "src/**/*.rs", p, rev).
? diag(path, line, col, end_col, severity, code, msg).
"#;
    let out = run(&d, prog);
    assert!(out.contains("info\tdl-directive\tsuppresses no-unwrap on this line"),
        "info marker with precise effect msg:\n{out}");
    // the marker sits on the comment (col 18 = the `//`), not the whole line.
    assert!(out.contains("src/lib.rs\t1\t18\t"), "marker on the comment span:\n{out}");
}

/// A next-line directive names the +1 target; a block names its dl-enable line;
/// a reason is echoed in the marker.
#[test]
fn directive_marker_msg_variants() {
    let d = sandbox("variants");
    fs::write(d.join("src/lib.rs"),
        "// dl-disable-next-line no-panic\n\
         fn c() { baz(); }\n\
         // dl-disable other-code -- legacy\n\
         fn d() { qux(); }\n\
         // dl-enable other-code\n").unwrap();
    let prog = r#"
use "std/suppress.dl".
rel seen(p: file).
seen(p) <- scan("WORK", "src/**/*.rs", p, rev).
? diag(path, line, col, end_col, severity, code, msg).
"#;
    let out = run(&d, prog);
    assert!(out.contains("suppresses no-panic on the next line"), "next-line msg:\n{out}");
    assert!(out.contains("suppresses other-code until dl-enable (line 5) (reason: legacy)"),
        "block msg with enable line + reason:\n{out}");
    assert!(out.contains("ends the dl-disable block for other-code"), "enable msg:\n{out}");
}

/// A typo'd directive (`dl-disable-nextline`) is not silently ignored — it warns
/// with code `dl-directive-malformed` and never earns an info marker.
#[test]
fn malformed_directive_warns() {
    let d = sandbox("malformed");
    fs::write(d.join("src/lib.rs"),
        "fn a() { foo(); } // dl-disable-nextline no-unwrap\n").unwrap();
    let prog = r#"
use "std/suppress.dl".
rel seen(p: file).
seen(p) <- scan("WORK", "src/**/*.rs", p, rev).
? diag(path, line, col, end_col, severity, code, msg).
"#;
    let out = run(&d, prog);
    assert!(out.contains("warn\tdl-directive-malformed"), "typo should warn:\n{out}");
    assert!(!out.contains("info\tdl-directive"), "a malformed directive is not recognized:\n{out}");
}

/// The unused-suppression rail: a directive whose code caught no `rail_finding`
/// warns; one that matched a finding does not.
#[test]
fn unused_suppression_warns_used_does_not() {
    let d = sandbox("unused");
    fs::write(d.join("src/lib.rs"),
        "fn a() { foo(); } // dl-disable-line used-code\n\
         fn b() { bar(); } // dl-disable-line unused-code\n").unwrap();
    let prog = r#"
use "std/suppress.dl".
rel seen(p: file).
seen(p) <- scan("WORK", "src/**/*.rs", p, rev).
# a rail contributes a finding for used-code on line 1 only
rail_finding(p, 1, "used-code") <- seen(p).
? diag(path, line, col, end_col, severity, code, msg).
"#;
    let out = run(&d, prog);
    assert!(out.lines().any(|l| l.contains("\t2\t") && l.contains("dl-suppress-unused")),
        "unused-code on line 2 should warn:\n{out}");
    assert!(!out.lines().any(|l| l.contains("\t1\t") && l.contains("dl-suppress-unused")),
        "used-code on line 1 must not warn:\n{out}");
}

/// A bare (wildcard) directive is unused only when NO finding of ANY code fell
/// in its scope.
#[test]
fn wildcard_unused_needs_no_finding_of_any_code() {
    let d = sandbox("wildcard_unused");
    fs::write(d.join("src/lib.rs"),
        "fn a() { foo(); } // dl-disable-line\n\
         fn b() { bar(); } // dl-disable-line\n").unwrap();
    let prog = r#"
use "std/suppress.dl".
rel seen(p: file).
seen(p) <- scan("WORK", "src/**/*.rs", p, rev).
# some finding of an arbitrary code lands on line 1
rail_finding(p, 1, "whatever") <- seen(p).
? diag(path, line, col, end_col, severity, code, msg).
"#;
    let out = run(&d, prog);
    assert!(out.lines().any(|l| l.contains("\t2\t") && l.contains("dl-suppress-unused")),
        "line 2 wildcard covers no finding -> unused:\n{out}");
    assert!(!out.lines().any(|l| l.contains("\t1\t") && l.contains("dl-suppress-unused")),
        "line 1 wildcard covers a finding -> used:\n{out}");
}

/// Self-hosting: `// dl-disable-line dl-directive` silences the info marker on
/// its own line, using the very grammar it documents.
#[test]
fn self_hosting_dl_directive_disable() {
    let d = sandbox("selfhost");
    fs::write(d.join("src/lib.rs"),
        "fn a() { foo(); } // dl-disable-line no-unwrap\n\
         fn b() { bar(); } // dl-disable-line dl-directive\n").unwrap();
    let prog = r#"
use "std/suppress.dl".
rel seen(p: file).
seen(p) <- scan("WORK", "src/**/*.rs", p, rev).
? diag(path, line, col, end_col, severity, code, msg).
"#;
    let out = run(&d, prog);
    // line 1 keeps its marker; line 2 (which disables dl-directive) has none.
    assert!(out.lines().any(|l| l.contains("\t1\t") && l.contains("info\tdl-directive")),
        "line 1 marker present:\n{out}");
    assert!(!out.lines().any(|l| l.contains("\t2\t") && l.contains("info\tdl-directive")),
        "line 2 self-disables its own dl-directive marker:\n{out}");
    // and the self-disable is not itself flagged unused (meta code excluded).
    assert!(!out.lines().any(|l| l.contains("\t2\t") && l.contains("dl-suppress-unused")),
        "self-hosting disable must not warn unused:\n{out}");
}
