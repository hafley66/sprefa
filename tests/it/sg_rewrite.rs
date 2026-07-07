//! Full ast-grep-style structural codemod in dl: an `sg` pattern binds its
//! metavars as dl vars AND the whole-match span id; `gen(:replace, ref(id))`
//! substitutes the metavars into a replacement template and applies the edit.
//!
//! Regression for the fix that made the sg match `id` cover the TRUE match-node
//! range (literal text included), not just the captures' bounding box — without
//! it, `$X.unwrap()` rewrote only `$X` and left `.unwrap()` dangling.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sg_rewrite_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

#[test]
fn sg_metavar_template_rewrites_whole_match() {
    let d = sandbox("unwrap_to_expect");
    fs::write(d.join("src/a.rs"), "let a = foo.unwrap();\nlet b = bar.baz.unwrap();\n").unwrap();

    // $X.unwrap()  ->  $X.expect("checked")
    // X binds the receiver text; id binds the whole-match span; ref resolves it;
    // gen(:replace) substitutes {x} from the bound metavar over the whole span.
    let prog = r#"
rel hit(p: file, x: text, id: text).
hit(p, X, id) <- scan("WORK","src/**/*.rs",p,rev),
  sg(p, rev, :rust, "$X.unwrap()", _l, _c, _el, _ec, id).
gen(:replace, p, lo, hi, "{x}.expect(\"checked\")") <-
  hit(p, x, id), ref(id, _s, _f, lo, hi).
"#;
    fs::write(d.join("p.dl"), prog).unwrap();

    let out = Command::new(DL)
        .arg(d.join("p.dl"))
        .args(["--db", d.join("db").to_str().unwrap(), "--no-daemon"])
        .current_dir(&d)
        .output().expect("run dl");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));

    let after = fs::read_to_string(d.join("src/a.rs")).unwrap();
    assert_eq!(
        after,
        "let a = foo.expect(\"checked\");\nlet b = bar.baz.expect(\"checked\");\n",
        "whole match rewritten, metavar receiver preserved; got:\n{after}"
    );
}
