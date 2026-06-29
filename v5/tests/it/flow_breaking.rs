//! Breaking-change detection on the spec-as-artifact (bench/flow/breaking.dl).
//! The OpenAPI spec is a codegen input, not source, so the meaningful event is
//! "a change REMOVED an operation that consumers still depend on". We commit the
//! spec with two ops (HEAD baseline), then in the working tree delete one op while
//! the generated lib still exports it and the app still calls it. The detector
//! diffs HEAD vs WORK, finds the removed op, and joins it to the consumers still
//! referencing its generated symbol — the blast radius of the break.
//!
//! Uses a hand-built SCIP index (the `scip` crate) so the test is deterministic
//! and toolchain-free; the resolution shape matches real scip-typescript output.

use protobuf::Message;
use scip::types::{Document, Index, Occurrence, SymbolRole};
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");
const FLOW: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/bench/flow");

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().expect("git");
    assert!(out.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
}

fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for e in std::fs::read_dir(src).unwrap().flatten() {
        let p = e.path();
        let d = dst.join(e.file_name());
        if p.is_dir() { copy_dir(&p, &d); } else { std::fs::copy(&p, &d).unwrap(); }
    }
}

fn occ(line0: i32, col0: i32, end_col: i32, symbol: &str, def: bool) -> Occurrence {
    Occurrence {
        range: vec![line0, col0, end_col],
        symbol: symbol.to_string(),
        symbol_roles: if def { SymbolRole::Definition as i32 } else { 0 },
        ..Default::default()
    }
}

/// Generated lib defs (getPet, createPet) + app refs to BOTH — the lib still
/// exports createPet and the app still calls it (codegen has not re-run).
fn build_index() -> Index {
    let lib_get = "scip-typescript npm . . lib/`petclient.ts`/getPet().";
    let lib_create = "scip-typescript npm . . lib/`petclient.ts`/createPet().";
    let lib = Document {
        relative_path: "ts/lib/petclient.ts".into(),
        occurrences: vec![
            occ(4, 16, 22, lib_get, true),
            occ(8, 16, 25, lib_create, true),
        ],
        ..Default::default()
    };
    let app = Document {
        relative_path: "ts/app/pages.ts".into(),
        occurrences: vec![
            occ(5, 9, 15, lib_get, false),       // import
            occ(5, 17, 26, lib_create, false),
            occ(8, 9, 15, lib_get, false),       // callsites
            occ(11, 9, 15, lib_get, false),
            occ(15, 9, 18, lib_create, false),   // createPet callsite
        ],
        ..Default::default()
    };
    Index { documents: vec![lib, app], ..Default::default() }
}

/// rows of a `? rel` block from dl stdout (tab-split values).
fn query_block<'a>(stdout: &'a str, header: &str) -> Vec<Vec<&'a str>> {
    let mut rows = Vec::new();
    let mut in_q = false;
    for line in stdout.lines() {
        if line.starts_with(header) { in_q = true; continue; }
        if line.starts_with('?') { in_q = false; continue; }
        if in_q {
            let t = line.trim();
            if t.is_empty() || t.starts_with('(') { continue; }
            rows.push(t.split('\t').collect());
        }
    }
    rows
}

#[test]
fn removed_spec_op_flags_breaking_consumers() {
    let root = std::env::temp_dir().join(format!("flow_breaking_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    copy_dir(&PathBuf::from(FLOW), &root);
    std::fs::write(root.join("index.scip"), build_index().write_to_bytes().unwrap()).unwrap();

    // HEAD baseline: the spec declares both getPet and createPet.
    git(&root, &["init", "-q"]);
    git(&root, &["config", "user.email", "t@t"]);
    git(&root, &["config", "user.name", "t"]);
    git(&root, &["config", "commit.gpgsign", "false"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "baseline spec with getPet + createPet"]);

    // working-tree change: remove the createPet operation from the spec artifact.
    // (The generated lib + app are unchanged: codegen has not re-run yet.)
    let spec = root.join("spec/openapi.yaml");
    let kept = "openapi: 3.0.0\ninfo:\n  title: Pet API\n  version: 1.0.0\npaths:\n  /pets/{id}:\n    get:\n      operationId: getPet\n      responses:\n        '200':\n          description: a pet\n";
    std::fs::write(&spec, kept).unwrap();

    let out = Command::new(DL)
        .arg(root.join("breaking.dl"))
        .args(["--root", root.to_str().unwrap(), "--db", root.join("b.db").to_str().unwrap()])
        .output().expect("run dl");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "dl failed: {}", String::from_utf8_lossy(&out.stderr));

    let removed = query_block(&stdout, "? removed_op");
    assert_eq!(removed, vec![vec!["createPet"]], "createPet removed from the spec: {removed:?}\n{stdout}");

    let breaking = query_block(&stdout, "? breaking");
    assert!(breaking.iter().any(|r| r == &["createPet", "ts/app/pages.ts"]),
        "createPet break must name the consuming app: {breaking:?}\n{stdout}");
    // getPet is still in the spec, so it is not breaking even though it is used.
    assert!(!breaking.iter().any(|r| r.first() == Some(&"getPet")),
        "getPet still in spec, not breaking: {breaking:?}");
}
