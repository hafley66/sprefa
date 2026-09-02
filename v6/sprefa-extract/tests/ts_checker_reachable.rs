//! FAIL-PRE-FIX (failure-modes 108): held out repos clone with `git clone`
//! alone, no `npm install`, so `node_modules/typescript` never exists. Every
//! `loadTypeScript` candidate in `ts_checker.mjs` misses, the driver throws,
//! `answer_inner` returns `Err`, and `project.rs::load_ts_checker` turns that
//! into a silent `None`: the syntax leg answers, `--ts-checker` costs nothing,
//! and nothing on the process's real stderr says so unless `RUST_LOG` is set
//! (trace.rs:256 defaults the subscriber to `off`).

#![cfg(feature = "ts-checker")]

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// A project the checker cannot reach: `package.json` names `typescript`, but
/// no `node_modules` exists, the exact shape `run.py` clones for every
/// held-out repo.
fn unreachable_fixture() -> PathBuf {
    let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "sprefa_ts_checker_unreachable_{}_{seq}",
        std::process::id()
    ));
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(
        root.join("package.json"),
        r#"{"name":"fixture","version":"1.0.0","devDependencies":{"typescript":"^5.9.0"}}"#,
    )
    .expect("write package.json");
    fs::write(
        root.join("src/a.ts"),
        "export function helper(x: number): number { return x + 1; }\n\
         export function caller(): number { return helper(1); }\n",
    )
    .expect("write src/a.ts");
    root
}

#[test]
fn a_project_with_no_node_modules_names_its_own_failure() {
    let root = unreachable_fixture();
    let root = root.to_str().expect("utf-8 temp path");
    let file = format!("{root}/src/a.ts");
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .env_remove("SPREFA_TS_CHECKER_TYPESCRIPT")
        .args([
            "--resolve",
            "--family",
            "call",
            "--project-root",
            root,
            "--ts-checker",
            &file,
        ])
        .output()
        .expect("extract binary runs");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success() || stderr.contains("ts checker tier did not run"),
        "a checker that cannot reach typescript must refuse loudly or fail the \
         process, not exit 0 with an empty stderr and syntax-tier answers under \
         a checker label; rc={:?} stderr={stderr:?}",
        output.status.code()
    );
    let _ = fs::remove_dir_all(root);
}
