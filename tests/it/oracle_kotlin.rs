//! Differential oracle: validate the Kotlin `module_edge` (imports, wildcard
//! fan-out, expect/actual twins, same-package implicit refs) against
//! `scip-java` ground truth (its semanticdb-kotlinc backend resolves Kotlin
//! with the real compiler). Same shape as tests/oracle_rust.rs: file-level
//! edges from SCIP occurrences, assert precision, report recall.
//!
//! `#[ignore]`d — no `scip-java` binary or JDK is a genuine environmental gap,
//! not something the default `cargo test` run should silently count as
//! coverage. Set SPREFA_SCIP_JAVA to point at one explicitly. The fixture is a
//! minimal Gradle Kotlin project (scip-java drives the build tool).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use protobuf::Message;
use scip::types::{Index, SymbolRole};

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn find_scip_java() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("SPREFA_SCIP_JAVA") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    let out = Command::new("which").arg("scip-java").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let p = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    p.is_file().then_some(p)
}

fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for e in std::fs::read_dir(src).unwrap().flatten() {
        let p = e.path();
        let d = dst.join(e.file_name());
        if p.is_dir() {
            copy_dir(&p, &d);
        } else {
            std::fs::copy(&p, &d).unwrap();
        }
    }
}

/// file -> file edges from a SCIP index: for each non-local symbol, the file
/// that defines it; every reference occurrence in a different file yields
/// ref_file -> def_file.
fn scip_edges(index: &Index) -> HashSet<(String, String)> {
    let mut def_file: HashMap<String, String> = HashMap::new();
    for doc in &index.documents {
        for occ in &doc.occurrences {
            if occ.symbol_roles & (SymbolRole::Definition as i32) != 0
                && !occ.symbol.starts_with("local ")
            {
                def_file
                    .entry(occ.symbol.clone())
                    .or_insert_with(|| doc.relative_path.clone());
            }
        }
    }
    let mut edges = HashSet::new();
    for doc in &index.documents {
        for occ in &doc.occurrences {
            if occ.symbol_roles & (SymbolRole::Definition as i32) != 0 {
                continue;
            }
            if let Some(def) = def_file.get(&occ.symbol) {
                if *def != doc.relative_path {
                    edges.insert((doc.relative_path.clone(), def.clone()));
                }
            }
        }
    }
    edges
}

fn our_edges(dir: &Path) -> HashSet<(String, String)> {
    let prog = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "**/*.kt", path, rev), match(path, rev, /./, line).
? module_edge(s, d).
"#;
    std::fs::write(dir.join("mg.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("mg.dl"))
        .args(["--db", dir.join("mg.db").to_str().unwrap()])
        .current_dir(dir)
        .output()
        .expect("run dl");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut edges = HashSet::new();
    let mut in_edge = false;
    for line in stdout.lines() {
        if line.starts_with("? module_edge") {
            in_edge = true;
            continue;
        }
        if line.starts_with('?') {
            in_edge = false;
            continue;
        }
        if in_edge {
            if let Some((s, d)) = line.trim().split_once('\t') {
                edges.insert((s.to_string(), d.to_string()));
            }
        }
    }
    edges
}

#[test]
#[ignore = "needs scip-java / JDK on PATH (set SPREFA_SCIP_JAVA)"]
fn kotlin_module_edge_is_subset_of_scip() {
    let scip_java = find_scip_java().expect("needs scip-java / JDK on PATH (set SPREFA_SCIP_JAVA)");
    let tmp = std::env::temp_dir().join("sprefa_oracle_kt_ws");
    let _ = std::fs::remove_dir_all(&tmp);
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/kt_ws");
    copy_dir(&fixture, &tmp);

    let scip_out = tmp.join("index.scip");
    let run = Command::new(&scip_java)
        .args(["index", "--output", scip_out.to_str().unwrap()])
        .current_dir(&tmp)
        .output()
        .expect("run scip-java index");
    assert!(
        scip_out.is_file(),
        "scip-java produced no index: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    let bytes = std::fs::read(&scip_out).unwrap();
    let index = Index::parse_from_bytes(&bytes).expect("parse scip");
    // only .kt source edges; the index also covers stdlib jars etc.
    let truth: HashSet<_> = scip_edges(&index)
        .into_iter()
        .filter(|(s, d)| s.ends_with(".kt") && d.ends_with(".kt"))
        .collect();
    let ours = our_edges(&tmp);
    let matched: HashSet<_> = ours.intersection(&truth).cloned().collect();
    let extra: Vec<_> = ours.difference(&truth).cloned().collect();
    let missed: Vec<_> = truth.difference(&ours).cloned().collect();
    eprintln!(
        "[oracle:kotlin] our edges={} scip edges={} matched={} precision={:.2} recall={:.2}",
        ours.len(),
        truth.len(),
        matched.len(),
        if ours.is_empty() {
            1.0
        } else {
            matched.len() as f64 / ours.len() as f64
        },
        if truth.is_empty() {
            1.0
        } else {
            matched.len() as f64 / truth.len() as f64
        }
    );
    eprintln!("[oracle:kotlin] ours not in scip: {extra:?}");
    eprintln!("[oracle:kotlin] scip not in ours: {missed:?}");

    assert!(!ours.is_empty(), "we produced no edges");
    assert!(
        extra.is_empty(),
        "every emitted edge must be a real scip edge; extras: {extra:?}"
    );
    // the fixture's three edge shapes must all be caught: plain import,
    // fn import (same target file), and the same-package implicit ref
    assert!(
        ours.contains(&(
            "src/main/kotlin/com/app/Main.kt".into(),
            "src/main/kotlin/com/lib/Util.kt".into()
        )),
        "{ours:?}"
    );
    assert!(
        ours.contains(&(
            "src/main/kotlin/com/lib/Helper.kt".into(),
            "src/main/kotlin/com/lib/Util.kt".into()
        )),
        "{ours:?}"
    );
}
