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
            "between: creates=0 schema_rows_parsed=0 prepared=87 prepared_bytes=27471 unfolded_bytes=27471 executions=5806 zero_row_executions=2473 vm_steps=371263 fullscan_steps=10122 sorts=0 autoindexes=0 reprepares=15 rows_returned=27626 written_d=0 written_i=0 written_x=0 written_scratch=0 written_source=0 written_catalog=0 written_term=4498\n",
            "install: creates=2426 schema_rows_parsed=54360 prepared=2801 prepared_bytes=830627 unfolded_bytes=830627 executions=3846 zero_row_executions=278 vm_steps=92156 fullscan_steps=90 sorts=238 autoindexes=0 reprepares=2 rows_returned=586 written_d=0 written_i=0 written_x=0 written_scratch=0 written_source=0 written_catalog=18 written_term=565\n",
            "settle: creates=0 schema_rows_parsed=0 prepared=3130 prepared_bytes=1873081 unfolded_bytes=2011536 executions=47605 zero_row_executions=9094 vm_steps=15289166 fullscan_steps=388219 sorts=41192 autoindexes=29147 reprepares=0 rows_returned=31765 written_d=98150 written_i=44593 written_x=0 written_scratch=28445 written_source=5620 written_catalog=44 written_term=6368",
        )
    );
    assert_eq!(
        registry_sqlite_work("engine"),
        concat!(
            "between: creates=0 schema_rows_parsed=0 prepared=93 prepared_bytes=36993 unfolded_bytes=36993 executions=6922 zero_row_executions=3577 vm_steps=431475 fullscan_steps=10122 sorts=0 autoindexes=0 reprepares=15 rows_returned=28673 written_d=0 written_i=0 written_x=0 written_scratch=0 written_source=0 written_catalog=0 written_term=6464\n",
            "install: creates=4899 schema_rows_parsed=112883 prepared=5355 prepared_bytes=1372235 unfolded_bytes=1372235 executions=6408 zero_row_executions=281 vm_steps=164314 fullscan_steps=92 sorts=340 autoindexes=0 reprepares=2 rows_returned=593 written_d=0 written_i=0 written_x=0 written_scratch=0 written_source=0 written_catalog=19 written_term=571\n",
            "settle: creates=0 schema_rows_parsed=0 prepared=5833 prepared_bytes=3544926 unfolded_bytes=3752804 executions=52978 zero_row_executions=11996 vm_steps=17862553 fullscan_steps=502949 sorts=47452 autoindexes=29147 reprepares=0 rows_returned=35281 written_d=119976 written_i=59767 written_x=0 written_scratch=35834 written_source=6851 written_catalog=44 written_term=6368",
        )
    );
}
