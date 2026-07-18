//! Lexer preserves UTF-8 in string and regex literals. The original byte loop
//! pushed each byte via `b[i] as char`, which independently Latin-1-promotes
//! each byte of a multibyte sequence and double-encodes it (`—` -> `ââ`).
//! The fix slices runs of boring bytes via `src[start..i]`.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn run(tag: &str, prog: &str) -> (i32, String, String) {
    let dir: PathBuf = std::env::temp_dir().join(format!("lex_utf8_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap()])
        .current_dir(&dir)
        .output().expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

/// Plain string literal with a multibyte char survives intact.
#[test]
fn string_literal_preserves_em_dash() {
    let (code, out, err) = run("str", r#"
rel x(s: text).
x("em—dash").
? x(s).
"#);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("em—dash"), "expected em-dash intact in: {out}");
}

/// Regex body with a multibyte char survives intact (no double-encoding when
/// the regex is forwarded to SQLite REGEXP).
#[test]
fn regex_body_preserves_em_dash() {
    let (code, out, err) = run("re", r#"
rel x(s: text).
x("foo—bar").
rel hit(s: text).
hit(s) <- x(s), s =~ /—/.
? hit(s).
"#);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("foo—bar"), "regex with em-dash should match: {out}");
}

/// Interpolation literal preserves multibyte chars in both the literal chunks
/// and the rendered output.
#[test]
fn interp_str_preserves_em_dash() {
    let (code, out, err) = run("interp", r#"
rel x(a: text, b: text).
x("left—", "—right").
rel j(s: text).
j("${a}${b}") <- x(a, b).
? j(s).
"#);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("left——right"), "interp chunks keep em-dashes: {out}");
}
