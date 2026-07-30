//! `comment_node` — every comment in every parsed file as a grammar-backed
//! fact: (path, line, col, end_line, end_col, text, kind∈line|block|doc). The
//! walk is lexer/grammar-backed (oxc for TS/TSX, tree-sitter for Rust, Kotlin,
//! Python, ...), so a `//`/`#` inside a STRING literal is never a comment row —
//! the string-literal safety a naive text scan can't give. Asserted per
//! language below.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("comment_node_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
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

const PROG: &str = r#"
rel seen(p: file).
seen(p) <- scan("WORK", "src/**/*.{rs,ts,tsx,kt,py,go}", p, rev).
? comment_node(path, line, col, end_line, end_col, text, kind).
"#;

#[test]
fn rust_comments_line_block_doc_and_string_safety() {
    let d = sandbox("rust");
    fs::write(
        d.join("src/lib.rs"),
        "// a line comment\n\
         /// a doc comment\n\
         pub fn f() {\n\
             let s = \"// not a comment\"; // trailing inline\n\
             /* a block */\n\
         }\n",
    )
    .unwrap();
    let out = run(&d, PROG);
    // kinds
    assert!(
        out.contains("a line comment\tline"),
        "rust line kind:\n{out}"
    );
    assert!(out.contains("a doc comment\tdoc"), "rust doc kind:\n{out}");
    assert!(out.contains("a block\tblock"), "rust block kind:\n{out}");
    // inline trailing comment (THE previously-impossible form) — on line 4
    assert!(
        out.contains("trailing inline\tline"),
        "rust inline trailing:\n{out}"
    );
    // string-literal safety: `// not a comment` must NOT be a row.
    assert!(
        !out.contains("not a comment"),
        "rust string leaked as comment:\n{out}"
    );
}

#[test]
fn ts_comments_via_oxc_and_string_safety() {
    let d = sandbox("ts");
    fs::write(
        d.join("src/a.ts"),
        "// ts line\n\
         const s = \"// string not comment\"; // inline ts\n\
         /** jsdoc block */\n\
         export function g() {}\n",
    )
    .unwrap();
    let out = run(&d, PROG);
    assert!(out.contains("ts line\tline"), "ts line kind:\n{out}");
    assert!(
        out.contains("inline ts\tline"),
        "ts inline trailing:\n{out}"
    );
    assert!(
        out.contains("jsdoc block\tdoc"),
        "ts jsdoc doc kind:\n{out}"
    );
    assert!(
        !out.contains("string not comment"),
        "ts string leaked:\n{out}"
    );
}

#[test]
fn kotlin_comments_and_string_safety() {
    let d = sandbox("kt");
    fs::write(
        d.join("src/Svc.kt"),
        "// kt line\n\
         val s = \"// kt string\" // inline kt\n\
         /** kdoc */\n\
         class Svc {}\n",
    )
    .unwrap();
    let out = run(&d, PROG);
    assert!(out.contains("kt line\tline"), "kotlin line kind:\n{out}");
    assert!(
        out.contains("inline kt\tline"),
        "kotlin inline trailing:\n{out}"
    );
    assert!(out.contains("kdoc\tdoc"), "kotlin kdoc doc kind:\n{out}");
    assert!(!out.contains("kt string"), "kotlin string leaked:\n{out}");
}

#[test]
fn generic_treesitter_lang_python_string_safety() {
    let d = sandbox("py");
    fs::write(
        d.join("src/util.py"),
        "# python comment\n\
         x = \"# not a comment\"  # trailing py\n",
    )
    .unwrap();
    let out = run(&d, PROG);
    assert!(
        out.contains("python comment\tline"),
        "python line kind:\n{out}"
    );
    assert!(
        out.contains("trailing py\tline"),
        "python inline trailing:\n{out}"
    );
    assert!(
        !out.contains("not a comment"),
        "python string leaked:\n{out}"
    );
}
