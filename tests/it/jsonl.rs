//! JSONL (JSON Lines / NDJSON) support in `datapath.rs`. Each line is an
//! independent JSON document; `jsonp` and `json` pattern ops walk each line
//! separately, with byte spans offset to the full file.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("jsonl_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &std::path::Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap(), "--no-daemon"])
        .current_dir(dir)
        .output().expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn jsonp_extracts_from_each_jsonl_line() {
    let d = sandbox("jsonp");
    fs::write(d.join("events.jsonl"),
        "{\"name\":\"alice\",\"age\":30}\n\
         {\"name\":\"bob\",\"age\":25}\n\
         {\"name\":\"carol\",\"age\":40}\n").unwrap();
    let prog = r#"
rel names(name: text).
names(name) <- scan("WORK", "events.jsonl", p, rev), jsonp(p, rev, "name", name).
? names(name).
"#;
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "dl failed:\n{err}");
    assert!(out.contains("alice"), "alice:\n{out}");
    assert!(out.contains("bob"), "bob:\n{out}");
    assert!(out.contains("carol"), "carol:\n{out}");
    let block = out.split("? names").nth(1).unwrap_or("");
    assert!(block.contains("(3 rows)"), "exactly 3 rows:\n{block}");
}

#[test]
fn json_pattern_matches_each_jsonl_line() {
    let d = sandbox("jsonpat");
    fs::write(d.join("users.jsonl"),
        "{\"id\":1,\"role\":\"admin\"}\n\
         {\"id\":2,\"role\":\"user\"}\n\
         {\"id\":3,\"role\":\"admin\"}\n").unwrap();
    let prog = r#"
rel admin(id: text).
admin(id) <-
    scan("WORK", "users.jsonl", p, rev),
    json(p, rev, q:{ "id": $id, "role": "admin" }).
? admin(id).
"#;
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "dl failed:\n{err}");
    assert!(out.contains("1") && out.contains("3"), "both admin ids:\n{out}");
    assert!(!out.contains("\t2\t") && !out.contains("\t2\n"), "user id 2 is not admin:\n{out}");
    let block = out.split("? admin").nth(1).unwrap_or("");
    assert!(block.contains("(2 rows)"), "exactly 2 admins:\n{block}");
}

#[test]
fn jsonl_skips_blank_lines() {
    let d = sandbox("blanks");
    fs::write(d.join("data.jsonl"),
        "{\"v\":1}\n\
         \n\
         {\"v\":2}\n\
         \n\
         {\"v\":3}\n").unwrap();
    let prog = r#"
rel vals(v: text).
vals(v) <- scan("WORK", "data.jsonl", p, rev), jsonp(p, rev, "v", v).
? vals(v).
"#;
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "dl failed:\n{err}");
    let block = out.split("? vals").nth(1).unwrap_or("");
    assert!(block.contains("(3 rows)"), "blank lines skipped, 3 values:\n{block}");
}
