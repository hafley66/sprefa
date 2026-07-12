//! Regression coverage for two extractor gaps: a Rust trait's default method
//! (a body inside the `trait` block, not an `impl`) and a Kotlin `object`
//! declaration (top-level or a nested `companion object`) both now mint a
//! `type_entity` row, matching how a regular method / class already does.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("type_entity_extractor_gaps_{tag}"));
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

const RUST_PROG: &str = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /./, line).
rel trait_method(name: text, kind: text, owner: text).
trait_method(name, kind, owner) <- type_entity(_, sym, name, kind, owner, file, line),
  owner =~ /Greeter/.
? trait_method(name, kind, owner).
"#;

#[test]
fn rust_trait_default_method_gets_type_entity_row() {
    let d = sandbox("rust_trait_default");
    // `name` is a bare signature (no body): stays entity-less, matching a
    // regular trait method requirement. `greet` has a default body and
    // should mint a `type_entity` row parented to the trait, just like an
    // `impl` block's method does today.
    fs::write(
        d.join("src/lib.rs"),
        "pub trait Greeter {\n\
         \x20   fn name(&self) -> String;\n\
         \n\
         \x20   fn greet(&self) -> String {\n\
         \x20       format!(\"hi {}\", self.name())\n\
         \x20   }\n\
         }\n",
    )
    .unwrap();

    let (code, out, err) = run(&d, RUST_PROG);
    assert_eq!(code, 0, "run failed:\nstdout={out}\nstderr={err}");
    assert!(
        out.contains("greet\tmethod\ttype_entity_extractor_gaps_rust_trait_default::src/lib.rs::trait::Greeter"),
        "default method row missing: {out}"
    );
    assert!(!out.contains("\nname\t") && !out.contains("name\tmethod"), "bare signature must NOT mint a row: {out}");
}

const KOTLIN_PROG: &str = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "src/**/*.kt", path, rev), match(path, rev, /./, line).
rel object_entity(name: text, kind: text).
object_entity(name, kind) <- type_entity(_, sym, name, kind, parent, file, line), name =~ /Singleton|Factory/.
? object_entity(name, kind).
"#;

#[test]
fn kotlin_object_declarations_get_type_entity_rows() {
    let d = sandbox("kotlin_object");
    // top-level `object` AND a nested `companion object` (a distinct grammar
    // node, `companion_object`, from `object_declaration`) both mint rows.
    fs::write(
        d.join("src/Model.kt"),
        "package com.app\n\
         \n\
         class Repo {\n\
         \x20   companion object Factory {\n\
         \x20       val x = 1\n\
         \x20   }\n\
         }\n\
         \n\
         object Singleton {\n\
         \x20   val y = 2\n\
         }\n",
    )
    .unwrap();

    let (code, out, err) = run(&d, KOTLIN_PROG);
    assert_eq!(code, 0, "run failed:\nstdout={out}\nstderr={err}");
    assert!(out.contains("Singleton\tclass"), "top-level object row missing: {out}");
    assert!(out.contains("Factory\tclass"), "companion object row missing: {out}");
}
