//! `dl8 build`: the `fs.file` rows of the settled comptime fixpoint written
//! to disk, the ordered join `doc.join`, and the generated string kernel
//! files kept current by `std/str_ops_emit.dl7`.

use super::_9e_generic_rules::ENGINES;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn build(source: &Path, out: &Path, engine: &str, check: bool) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_dl8"));
    command
        .current_dir(root())
        .arg("build")
        .arg(source)
        .arg("--out")
        .arg(out)
        .env("DL8_ENGINE", engine);
    if check {
        command.arg("--check");
    }
    command.output().unwrap()
}

fn report(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&output.stderr)))
}

fn scratch(tag: &str) -> PathBuf {
    let directory = root().join(format!("target/build-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

/// Every engine joins the lines in int key order and writes both paths;
/// `--check` then holds, and fails once a file is edited.
#[test]
fn build_writes_joined_files_on_every_engine() {
    let directory = scratch("join");
    let source = root().join("fixtures/build/0_join.dl7");
    for engine in ENGINES {
        let out = directory.join(engine.replace(':', "_"));
        let built = build(&source, &out, engine, false);
        assert!(
            built.status.success(),
            "{engine}: {}",
            String::from_utf8_lossy(&built.stderr)
        );
        assert_eq!(
            report(&built),
            json!({"diagnostics": [], "files": [
                {"path": "a.txt", "bytes": 13, "changed": true},
                {"path": "nested/b.txt", "bytes": 4, "changed": true}
            ]}),
            "{engine}"
        );
        assert_eq!(
            std::fs::read_to_string(out.join("a.txt")).unwrap(),
            "zero\ntwo\nten\n"
        );
        assert_eq!(
            std::fs::read_to_string(out.join("nested/b.txt")).unwrap(),
            "one\n"
        );
        assert!(
            build(&source, &out, engine, true).status.success(),
            "{engine}"
        );
        std::fs::write(out.join("a.txt"), "edited\n").unwrap();
        let checked = build(&source, &out, engine, true);
        assert_eq!(checked.status.code(), Some(1), "{engine}");
        assert_eq!(
            std::fs::read_to_string(out.join("a.txt")).unwrap(),
            "edited\n"
        );
    }
    std::fs::remove_dir_all(directory).unwrap();
}

/// Two texts for one path write nothing and name the path.
#[test]
fn build_reports_a_path_with_two_texts() {
    let directory = scratch("conflict");
    let built = build(
        &root().join("fixtures/build/1_conflict.dl7"),
        &directory,
        "ir:dd",
        false,
    );
    assert_eq!(built.status.code(), Some(1));
    assert_eq!(
        report(&built),
        json!({"diagnostics": [{"f": "fs_file_conflict", "args": [{"s": "same.txt"}]}], "files": []})
    );
    assert!(!directory.join("same.txt").exists());
    std::fs::remove_dir_all(directory).unwrap();
}

/// The two generated string kernel files are what `std/str_ops_emit.dl7`
/// builds, byte for byte.
#[test]
fn string_kernel_files_are_current() {
    for engine in ENGINES {
        let built = build(&root().join("std/str_ops_emit.dl7"), root(), engine, true);
        assert!(
            built.status.success(),
            "{engine}: {}",
            String::from_utf8_lossy(&built.stdout)
        );
        assert_eq!(
            report(&built)["files"],
            json!([
                {"path": "sqlite_ivm/crates/ivm-ir/src/3_str_ops.rs", "bytes": 9043, "changed": false},
                {"path": "src/_6_eval/_4a_str_ops.rs", "bytes": 5313, "changed": false}
            ]),
            "{engine}"
        );
    }
}

/// `std/tsi_registry_emit.dl7` writes hafley_scm's `registry.rs` byte for
/// byte. The file lives in the linked `hafley-rs` checkout.
#[test]
fn tsi_registry_matches_hafley_scm() {
    let oracle = root().join("hafley-rs/crates/hafley_scm/src/read/tsi/registry.rs");
    let Ok(expected) = std::fs::read_to_string(&oracle) else {
        println!("SKIP: {} is absent", oracle.display());
        return;
    };
    let directory = scratch("registry");
    let source = root().join("std/tsi_registry_emit.dl7");
    for engine in ENGINES {
        let out = directory.join(engine.replace(':', "_"));
        let built = build(&source, &out, engine, false);
        assert!(
            built.status.success(),
            "{engine}: {}",
            String::from_utf8_lossy(&built.stdout)
        );
        let written = std::fs::read_to_string(out.join("target/tsi/registry.rs")).unwrap();
        assert!(written == expected, "{engine}: registry.rs differs");
    }
    std::fs::remove_dir_all(directory).unwrap();
}


