//! `ast_yaml`: ast-grep RuleCore (relational YAML) extraction op. Ported from
//! v3/v4. Superset of `sg` — a bare `pattern:` body matches the same surface,
//! and `inside:`/`has:`/`any:`/`all:`/`not:`/`kind:` add the structural
//! relationships a plain pattern cannot express (the N+1 "db call inside a
//! loop" shape needs `inside:`).
//!
//! Note on `inside:`/`has:`: ast-grep 0.38's default `stopBy` is `neighbor`
//! (immediate child/parent only), so relational rules over NESTED nodes must
//! set `stopBy: end` explicitly. Captures (`$NAME` / `$$$NAME`) bind as
//! variables in the .dl rule, identical to `sg`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ast_yaml_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--root", dir.to_str().unwrap(), "--db", dir.join("db").to_str().unwrap()])
        .output()
        .expect("run dl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// A bare `pattern:` body matches the same surface as `sg`, and `$N` captures
/// flow into the .dl rule as the variable `N`. `fn beta(x: i32)` has a param so
/// the empty-param pattern correctly skips it (parity with the capture probe).
#[test]
fn pattern_matches_and_captures() {
    let d = sandbox("pattern");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/a.rs"), "fn alpha() {}\nfn beta(x: i32) {}\n").unwrap();
    let prog = concat!(
        "rel hit(nm: text, line: int).\n",
        "hit(N, line) <- scan(\"WORK\", \"src/**/*.rs\", p, rev),\n",
        "  ast_yaml(p, rev, :rs, `pattern: \"fn $N() { $$$B }\"`, line).\n",
        "? hit(nm, line).\n",
    );
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "ast_yaml pattern must not error:\n{err}");
    assert!(out.contains("alpha"), "alpha capture missing:\n{out}");
    assert!(!out.contains("beta"), "beta (has param) must not match empty-param pattern:\n{out}");
    assert!(out.contains("(1 rows)"), "expected exactly one match:\n{out}");
}

/// `has:` (descendant) over NESTED nodes needs `stopBy: end` in ast-grep 0.38
/// (the default `neighbor` checks immediate children only): a function whose
/// body contains a `dbg!` call matches; one without does not.
#[test]
fn has_with_stopby_finds_container() {
    let d = sandbox("has");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(
        d.join("src/a.rs"),
        "fn with_dbg() { dbg!(1); }\nfn clean() {}\n",
    )
    .unwrap();
    let prog = concat!(
        "rel hit(nm: text, line: int).\n",
        "hit(N, line) <- scan(\"WORK\", \"src/**/*.rs\", p, rev), ast_yaml(p, rev, :rs, `\n",
        "pattern: \"fn $N() { $$$B }\"\n",
        "has:\n",
        "  stopBy: end\n",
        "  pattern: \"dbg!($X)\"\n",
        "`, line).\n",
        "? hit(nm, line).\n",
    );
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "has: must not error:\n{err}");
    assert!(out.contains("with_dbg"), "the fn containing dbg! must match:\n{out}");
    assert!(!out.contains("clean"), "the fn without dbg! must not match:\n{out}");
    assert!(out.contains("(1 rows)"), "expected exactly one match:\n{out}");
}

/// The Phase D N+1 shape: `inside:` with `stopBy: end` discriminates a call
/// inside a loop from one outside it. `helper(i)` (line 3, in the for) matches;
/// `helper(99)` (line 5, after the loop) does not. This is the structural
/// relationship `sg` patterns alone cannot express.
#[test]
fn inside_with_stopby_discriminates_calls_in_loops() {
    let d = sandbox("inside");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(
        d.join("src/a.rs"),
        "fn outer() {\n    for i in 0..3 {\n        helper(i);\n    }\n    helper(99);\n}\nfn helper(x: i32) {}\n",
    )
    .unwrap();
    let prog = concat!(
        "rel in_loop(path: file, line: int).\n",
        "in_loop(path, line) <- scan(\"WORK\", \"src/**/*.rs\", path, rev), ast_yaml(path, rev, :rs, `\n",
        "pattern: \"helper($X)\"\n",
        "inside:\n",
        "  stopBy: end\n",
        "  pattern: \"for $I in $C { $$$BODY }\"\n",
        "`, line).\n",
        "? in_loop(path, line).\n",
    );
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "inside: must not error:\n{err}");
    assert!(out.contains("(1 rows)"), "only the in-loop call matches:\n{out}");
    // line 3 = helper(i) inside the for; line 5 = helper(99) after it
    assert!(out.contains("\t3"), "in-loop call (line 3) must match:\n{out}");
    assert!(!out.contains("\t5"), "out-of-loop call (line 5) must not match:\n{out}");
}
