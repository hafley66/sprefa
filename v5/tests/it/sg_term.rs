//! Term-form `sg(:lang, bound_str, "pattern", spans...)` — the embedded-language
//! seam. A string bound earlier in the rule (here a ground fact) is re-parsed
//! with an ast-grep grammar; metavar captures bind by name and the span outputs
//! are RELATIVE to the bound string. Mirrors the term-form `json`/`jsonp` pass
//! (`eval_extract_rules`). Also guards the one-rel-one-rule-kind bail.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sg_term_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap(), "--no-daemon"])
        .current_dir(dir)
        .output()
        .expect("run dl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// A css body held in a relation is parsed with the css grammar; each
/// declaration binds property + value, and the span output is the declaration's
/// line WITHIN the body (relative: line 1 = the first line of the bound string).
#[test]
fn css_declarations_out_of_a_bound_string() {
    let d = sandbox("css");
    // A backtick string is raw + multiline (a `"..."` literal does not turn
    // `\n` into a newline — it drops the backslash), so the css body carries a
    // real line break and the relative span spans two lines.
    let (code, out, err) = run(
        &d,
        concat!(
            "rel style(name: text, body: text).\n",
            "style(\"overlay\", `position: fixed;\ncolor: red;`).\n",
            "rel decl(name: text, prop: text, value: text, rel_line: int).\n",
            "decl(name, PROP, VAL, rel_line) <-\n",
            "  style(name, body),\n",
            "  sg(:css, body, \"$PROP: $VAL;\", rel_line).\n",
            "? decl(name, prop, value, rel_line).\n"
        ),
    );
    assert_eq!(code, 0, "{err}");
    // Two declarations, captures bound, relative line 1 then 2.
    assert!(
        out.contains("overlay\tposition\tfixed\t1"),
        "position decl, relative line 1:\n{out}"
    );
    assert!(
        out.contains("overlay\tcolor\tred\t2"),
        "color decl, relative line 2:\n{out}"
    );
}

/// Dispatch by `:lang`: the SAME term-form op runs the rust grammar over a rust
/// snippet, capturing the receiver of a `.unwrap()` method call.
#[test]
fn rust_grammar_over_a_bound_snippet() {
    let d = sandbox("rust");
    let (code, out, err) = run(
        &d,
        concat!(
            "rel snippet(id: text, code: text).\n",
            "snippet(\"a\", \"parse(raw).unwrap()\").\n",
            "rel uw(id: text, receiver: text).\n",
            "uw(id, RECEIVER) <- snippet(id, code), sg(:rust, code, \"$RECEIVER.unwrap()\").\n",
            "? uw(id, receiver).\n"
        ),
    );
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("a\tparse(raw)"), "receiver captured:\n{out}");
}

/// A rel headed by BOTH a term-extract `sg` rule and a plain derived rule now
/// desugars into twin rels + a union (was a loud bail before the mixed-rel
/// desugar); both rule kinds' rows must survive the tick.
#[test]
fn term_extract_and_derived_co_head_unions() {
    let d = sandbox("guard");
    let (code, out, err) = run(
        &d,
        concat!(
            "rel style(name: text, body: text).\n",
            "style(\"x\", \"position: fixed;\").\n",
            "rel other(prop: text).\n",
            "other(\"margin\").\n",
            "rel decl(prop: text).\n",
            "decl(PROP) <- style(_, body), sg(:css, body, \"$PROP: $VAL;\").\n",
            "decl(prop) <- other(prop).\n",
            "? decl(prop).\n"
        ),
    );
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("position"), "extracted row survives:\n{out}");
    assert!(out.contains("margin"), "derived row survives:\n{out}");
}
