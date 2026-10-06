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
            "between: creates=0 schema_rows_parsed=0 prepared=202 prepared_bytes=67408 unfolded_bytes=67408 executions=20714 zero_row_executions=16425 vm_steps=1101817 fullscan_steps=26478 sorts=0 autoindexes=0 reprepares=24 rows_returned=48978 written_d=0 written_i=0 written_x=0 written_scratch=0 written_source=0 written_catalog=0 written_term=32210\n",
            "install: creates=3015 schema_rows_parsed=66154 prepared=3538 prepared_bytes=1009854 unfolded_bytes=1009854 executions=4815 zero_row_executions=353 vm_steps=115415 fullscan_steps=138 sorts=289 autoindexes=0 reprepares=3 rows_returned=750 written_d=0 written_i=0 written_x=0 written_scratch=0 written_source=0 written_catalog=28 written_term=722\n",
            "settle: creates=0 schema_rows_parsed=0 prepared=3957 prepared_bytes=2197614 unfolded_bytes=2345761 executions=77117 zero_row_executions=10169 vm_steps=31141342 fullscan_steps=947811 sorts=44288 autoindexes=54149 reprepares=0 rows_returned=67104 written_d=219128 written_i=123827 written_x=0 written_scratch=98658 written_source=23381 written_catalog=51 written_term=6600",
        )
    );
    assert_eq!(
        registry_sqlite_work("engine"),
        concat!(
            "between: creates=0 schema_rows_parsed=0 prepared=208 prepared_bytes=79749 unfolded_bytes=79749 executions=22244 zero_row_executions=17993 vm_steps=1190237 fullscan_steps=28439 sorts=0 autoindexes=0 reprepares=24 rows_returned=51934 written_d=0 written_i=0 written_x=0 written_scratch=0 written_source=0 written_catalog=0 written_term=35104\n",
            "install: creates=7997 schema_rows_parsed=183901 prepared=8683 prepared_bytes=2107495 unfolded_bytes=2107495 executions=9976 zero_row_executions=359 vm_steps=260543 fullscan_steps=142 sorts=497 autoindexes=0 reprepares=3 rows_returned=764 written_d=0 written_i=0 written_x=0 written_scratch=0 written_source=0 written_catalog=30 written_term=734\n",
            "settle: creates=0 schema_rows_parsed=0 prepared=10757 prepared_bytes=6121100 unfolded_bytes=6497545 executions=111618 zero_row_executions=33741 vm_steps=250652101 fullscan_steps=7337967 sorts=115799 autoindexes=160312 reprepares=0 rows_returned=93228 written_d=1070422 written_i=640813 written_x=0 written_scratch=256662 written_source=25020 written_catalog=50 written_term=6600",
        )
    );
}
