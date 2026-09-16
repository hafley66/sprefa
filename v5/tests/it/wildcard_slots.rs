//! Wildcard `_` in the line/out slots of the line-oriented extraction ops
//! (`match` line, `ast` line/end, `cmd` line+out, `json` out). The op still
//! produces its row; `_` just binds nothing, same as the kwarg/`_` form already
//! accepted by `sg`/`comment`. Previously these slots required a throwaway named
//! variable ("expected variable, got Wild").

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("wildcard_slots_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

fn run(dir: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap()])
        .current_dir(dir)
        .output()
        .expect("run dl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// `match(..., _)`: the line slot is `_`, only the named capture binds.
#[test]
fn match_line_slot_accepts_wildcard() {
    let d = sandbox("match");
    fs::write(d.join("src/a.txt"), "alpha\nbeta\n").unwrap();
    let prog = r#"
rel names(f: file, n: text).
names(f, n) <- scan("WORK", "src/*.txt", f, rev), match(f, rev, /(?<n>\w+)/, _).
? names(f, n).
"#;
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    assert!(
        out.contains("alpha") && out.contains("beta"),
        "both captures bind without a line var: {out}"
    );
}

/// `cmd(..., _, v)` and `cmd(..., l, _)`: drop the line, or drop the value.
#[test]
fn cmd_slots_accept_wildcard() {
    let d = sandbox("cmd");
    fs::write(d.join("src/a.txt"), "one\ntwo\n").unwrap();
    // line dropped, only the value column binds
    let prog = r#"
rel vals(f: file, v: text).
vals(f, v) <- scan("WORK", "src/*.txt", f, rev), cmd(f, rev, "cat {file}", _, v).
? vals(f, v).
"#;
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    assert!(
        out.contains("one") && out.contains("two"),
        "values bind with `_` line: {out}"
    );

    // value dropped, only the line-count rows survive (a line-existence shape)
    let prog2 = r#"
rel lns(f: file, l: int).
lns(f, l) <- scan("WORK", "src/*.txt", f, rev), cmd(f, rev, "cat {file}", l, _).
? lns(f, l).
"#;
    let (code2, out2, err2) = run(&d, prog2);
    assert_eq!(code2, 0, "{err2}");
    assert!(
        out2.contains("\t1") && out2.contains("\t2"),
        "line numbers bind with `_` value: {out2}"
    );
}
