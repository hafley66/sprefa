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
            "between: creates=0 schema_rows_parsed=0 prepared=202 prepared_bytes=67442 unfolded_bytes=67442 executions=20714 zero_row_executions=16425 vm_steps=1096396 fullscan_steps=25338 sorts=0 autoindexes=0 reprepares=24 rows_returned=47838 written_d=0 written_i=0 written_x=0 written_scratch=0 written_source=0 written_catalog=0 written_term=32210\n",
            "install: creates=2936 schema_rows_parsed=71952 prepared=3454 prepared_bytes=1066008 unfolded_bytes=1066008 executions=4731 zero_row_executions=353 vm_steps=114125 fullscan_steps=138 sorts=169 autoindexes=0 reprepares=3 rows_returned=751 written_d=0 written_i=0 written_x=0 written_scratch=0 written_source=0 written_catalog=28 written_term=722\n",
            "settle: creates=0 schema_rows_parsed=0 prepared=3143 prepared_bytes=2188755 unfolded_bytes=2203751 executions=52068 zero_row_executions=1774 vm_steps=25375656 fullscan_steps=662994 sorts=12132 autoindexes=18743 reprepares=0 rows_returned=56785 written_d=145170 written_i=88703 written_x=0 written_scratch=95433 written_source=23381 written_catalog=11 written_term=6030",
        )
    );
    assert_eq!(
        registry_sqlite_work("engine"),
        concat!(
            "between: creates=0 schema_rows_parsed=0 prepared=208 prepared_bytes=79783 unfolded_bytes=79783 executions=22244 zero_row_executions=17993 vm_steps=1184816 fullscan_steps=27299 sorts=0 autoindexes=0 reprepares=24 rows_returned=50794 written_d=0 written_i=0 written_x=0 written_scratch=0 written_source=0 written_catalog=0 written_term=35104\n",
            "install: creates=7784 schema_rows_parsed=190190 prepared=8462 prepared_bytes=2261407 unfolded_bytes=2261407 executions=9755 zero_row_executions=359 vm_steps=255275 fullscan_steps=142 sorts=361 autoindexes=0 reprepares=3 rows_returned=765 written_d=0 written_i=0 written_x=0 written_scratch=0 written_source=0 written_catalog=30 written_term=734\n",
            "settle: creates=0 schema_rows_parsed=0 prepared=9892 prepared_bytes=6875729 unfolded_bytes=6779036 executions=78696 zero_row_executions=20932 vm_steps=151749700 fullscan_steps=4190728 sorts=66698 autoindexes=79740 reprepares=0 rows_returned=82875 written_d=651184 written_i=356741 written_x=0 written_scratch=184528 written_source=25020 written_catalog=10 written_term=6030",
        )
    );
}
