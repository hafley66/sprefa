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
        "ir:sqlite",
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
                {"path": "sqlite_ivm/crates/ivm-ir/src/3_str_ops.rs", "bytes": 9037, "changed": false},
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

/// The ir:sqlite work counts (`ivm_sqlite::work` events, summed per phase) of the registry
/// build under one `DL8_MACROTIME` mode.
fn registry_sqlite_work(mode: &str) -> String {
    let directory = scratch(&format!("registry-work-{mode}"));
    let built = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .current_dir(root())
        .arg("build")
        .arg(root().join("std/tsi_registry_emit.dl7"))
        .arg("--out")
        .arg(&directory)
        .env("DL8_ENGINE", "ir:sqlite")
        .env("DL8_MACROTIME", mode)
        .env("DL8_RULES", "compile")
        .env("DL8_ONE_ENGINE", "0")
        .env("RUST_LOG", "ivm_sqlite::work=info")
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
    let mut phases: std::collections::BTreeMap<String, Vec<(String, u64)>> = Default::default();
    for line in String::from_utf8_lossy(&built.stderr).lines() {
        let Some((_, fields)) = line.split_once("ivm_sqlite::work: sqlite work ") else { continue };
        let mut fields = fields.split(' ').filter_map(|field| field.split_once('='));
        let phase = fields.next().unwrap().1.trim_matches('"').to_owned();
        let sums = phases.entry(phase).or_default();
        for (i, (name, value)) in fields.enumerate() {
            if sums.len() <= i {
                sums.push((name.to_owned(), 0));
            }
            sums[i].1 += value.parse::<u64>().unwrap();
        }
    }
    let got = phases
        .iter()
        .map(|(phase, sums)| {
            let fields = sums.iter().map(|(name, value)| format!("{name}={value}")).collect::<Vec<_>>();
            format!("{phase}: {}", fields.join(" "))
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::remove_dir_all(directory).unwrap();
    got
}

/// A change in the work SQLite does shows up here as a count change, per macrotime mode.
#[test]
fn tsi_registry_sqlite_work_counts() {
    assert_eq!(
        registry_sqlite_work("rust"),
        concat!(
            "between: creates=0 schema_rows_parsed=0 prepared=229 prepared_bytes=70527 unfolded_bytes=70527 executions=20823 zero_row_executions=16718 vm_steps=1108210 fullscan_steps=25338 sorts=0 autoindexes=0 reprepares=3 rows_returned=47599 written_d=0 written_i=0 written_x=0 written_scratch=0 written_source=0 written_catalog=0 written_term=32796\n",
            "install: creates=4963 schema_rows_parsed=106435 prepared=5411 prepared_bytes=1705656 unfolded_bytes=1705656 executions=5411 zero_row_executions=58 vm_steps=149691 fullscan_steps=138 sorts=358 autoindexes=0 reprepares=0 rows_returned=170 written_d=0 written_i=0 written_x=0 written_scratch=0 written_source=0 written_catalog=28 written_term=138\n",
            "settle: creates=0 schema_rows_parsed=0 prepared=5694 prepared_bytes=3734868 unfolded_bytes=3987655 executions=57781 zero_row_executions=3409 vm_steps=40949771 fullscan_steps=2481233 sorts=15963 autoindexes=23363 reprepares=0 rows_returned=57918 written_d=221502 written_i=108610 written_x=0 written_scratch=95433 written_source=23381 written_catalog=11 written_term=6030",
        )
    );
    assert_eq!(
        registry_sqlite_work("engine"),
        concat!(
            "between: creates=0 schema_rows_parsed=0 prepared=236 prepared_bytes=83527 unfolded_bytes=83527 executions=22355 zero_row_executions=18290 vm_steps=1196766 fullscan_steps=27299 sorts=0 autoindexes=0 reprepares=3 rows_returned=50552 written_d=0 written_i=0 written_x=0 written_scratch=0 written_source=0 written_catalog=0 written_term=35698\n",
            "install: creates=11199 schema_rows_parsed=249743 prepared=11847 prepared_bytes=3261737 unfolded_bytes=3261737 executions=11847 zero_row_executions=60 vm_steps=329849 fullscan_steps=142 sorts=786 autoindexes=0 reprepares=0 rows_returned=176 written_d=0 written_i=0 written_x=0 written_scratch=0 written_source=0 written_catalog=30 written_term=142\n",
            "settle: creates=0 schema_rows_parsed=0 prepared=14451 prepared_bytes=9272635 unfolded_bytes=9562150 executions=91282 zero_row_executions=27480 vm_steps=181853207 fullscan_steps=7064819 sorts=76557 autoindexes=100881 reprepares=0 rows_returned=84323 written_d=763858 written_i=398945 written_x=0 written_scratch=184528 written_source=25020 written_catalog=10 written_term=6030",
        )
    );
}
