//! `$NAME` sugar in /regex/ literals (the v4 `re` DSL surface): `$ident`
//! desugars to a lazy named group `(?P<ident>.*?)` at parse time, so the
//! capture binds a variable exactly like a hand-written `(?<ident>...)`.
//! Bare `$` stays the EOL anchor; `\$` stays a literal dollar.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("regex_sugar_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--root", dir.to_str().unwrap(), "--db", dir.join("db").to_str().unwrap()])
        .output().expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn dollar_hole_binds_a_variable() {
    let d = sandbox("hole");
    fs::write(d.join("a.rs"), "// TODO(alice) fix\n// TODO(bob) drop\n").unwrap();
    let (code, out, err) = run(&d, concat!(
        "rel todo(p: file, who: text).\n",
        "todo(p, who) <- scan(\"WORK\", \"*.rs\", p, rev), ",
        "match(p, rev, /TODO\\($who\\)/, l).\n",
        "? todo(p, who).\n"));
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("alice") && out.contains("bob"), "{out}");
}

#[test]
fn bare_dollar_is_still_the_eol_anchor() {
    let d = sandbox("anchor");
    fs::write(d.join("a.rs"), "end}\nmid} tail\n").unwrap();
    let (code, out, err) = run(&d, concat!(
        "rel hit(p: file, l: int).\n",
        "hit(p, l) <- scan(\"WORK\", \"*.rs\", p, rev), match(p, rev, /\\}$/, l).\n",
        "? hit(p, l).\n"));
    assert_eq!(code, 0, "{err}");
    // Only line 1 ends in `}`.
    assert!(out.contains("\t1"), "{out}");
    assert!(!out.contains("\t2"), "anchor must not match mid-line:\n{out}");
}

#[test]
fn escaped_dollar_is_a_literal() {
    let d = sandbox("escaped");
    fs::write(d.join("a.rs"), "price $5 each\n").unwrap();
    let (code, out, err) = run(&d, concat!(
        "rel hit(p: file, l: int).\n",
        "hit(p, l) <- scan(\"WORK\", \"*.rs\", p, rev), match(p, rev, /\\$5/, l).\n",
        "? hit(p, l).\n"));
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("a.rs"), "{out}");
}
