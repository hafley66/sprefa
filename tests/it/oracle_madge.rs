//! Differential oracle: validate examples/madge.dl (module dependency graph +
//! circular detection over `module_edge`) against madge ground truth on a TS
//! fixture with a cycle, an orphan, and a leaf. Same shape as oracle_kotlin:
//! skips (does not fail) when no madge binary is found — set SPREFA_MADGE to
//! point at one explicitly (`npm i -g madge` provides it).

use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");
const V5: &str = env!("CARGO_MANIFEST_DIR");

fn find_madge() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("SPREFA_MADGE") {
        let p = PathBuf::from(p);
        if p.is_file() { return Some(p); }
    }
    let out = Command::new("which").arg("madge").output().ok()?;
    if !out.status.success() { return None; }
    let p = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    p.is_file().then_some(p)
}

/// index -> a,b ; a -> b ; b <-> c cycle ; d is an orphan (imports a, nobody
/// imports d) ; c is also a leaf-of-the-cycle counterexample (imports b).
fn write_fixture(dir: &Path) {
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("tsconfig.json"), r#"{"compilerOptions":{"module":"esnext"}}"#).unwrap();
    fs::write(dir.join("src/index.ts"), "import { a } from './a';\nimport { b } from './b';\nexport const i = a + b;\n").unwrap();
    fs::write(dir.join("src/a.ts"), "import { b } from './b';\nexport const a = b + 1;\n").unwrap();
    fs::write(dir.join("src/b.ts"), "import { c } from './c';\nexport const b = c + 1;\n").unwrap();
    fs::write(dir.join("src/c.ts"), "import { b } from './b';\nexport const c = 1;\nexport const use_b = () => b;\n").unwrap();
    fs::write(dir.join("src/d.ts"), "import { a } from './a';\nexport const d = a;\n").unwrap();
}

/// madge --json: {"src/a.ts": ["src/b.ts"], ...} relative to the scanned dir.
fn madge_edges(madge: &Path, dir: &Path) -> HashSet<(String, String)> {
    let out = Command::new(madge).args(["--extensions", "ts", "--json", "."]).current_dir(dir)
        .output().expect("run madge --json");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!(
        "madge --json unparsable: {e}\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)));
    let mut edges = HashSet::new();
    for (src, deps) in v.as_object().into_iter().flatten() {
        for d in deps.as_array().into_iter().flatten() {
            if let Some(d) = d.as_str() {
                edges.insert((src.clone(), d.to_string()));
            }
        }
    }
    edges
}

/// madge --circular --json: [["src/b.ts","src/c.ts"], ...] -> member set.
fn madge_cycle_members(madge: &Path, dir: &Path) -> BTreeSet<String> {
    let out = Command::new(madge).args(["--extensions", "ts", "--circular", "--json", "."]).current_dir(dir)
        .output().expect("run madge --circular");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_default();
    v.as_array().into_iter().flatten()
        .flat_map(|cycle| cycle.as_array().into_iter().flatten())
        .filter_map(|f| f.as_str().map(String::from))
        .collect()
}

/// Run examples/madge.dl, parse the `?` TSV blocks into (rel -> rows).
fn dl_rows(dir: &Path) -> std::collections::HashMap<String, Vec<Vec<String>>> {
    let prog = PathBuf::from(V5).join("examples/madge.dl");
    let out = Command::new(DL)
        .arg(&prog)
        .args(["--root", dir.to_str().unwrap(), "--no-daemon",
               "--db", dir.join("madge.db").to_str().unwrap()])
        .output().expect("run dl");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut rows: std::collections::HashMap<String, Vec<Vec<String>>> = Default::default();
    let mut cur: Option<String> = None;
    for line in stdout.lines() {
        // header shape: `? dep => a\tb`
        if let Some(rest) = line.strip_prefix("? ") {
            cur = rest.split(|c: char| c == ' ' || c == '(')
                .next().map(String::from);
            continue;
        }
        let t = line.trim();
        if t.is_empty() || t.starts_with('(') { continue; }
        if let Some(rel) = &cur {
            rows.entry(rel.clone()).or_default()
                .push(t.split('\t').map(String::from).collect());
        }
    }
    rows
}

#[test]
fn madge_oracle_dep_graph_and_cycles_agree() {
    let Some(madge) = find_madge() else {
        eprintln!("SKIP oracle_madge: no madge binary (npm i -g madge, or set SPREFA_MADGE)");
        return;
    };
    let dir = std::env::temp_dir().join("sprefa_oracle_madge");
    let _ = fs::remove_dir_all(&dir);
    write_fixture(&dir);

    let truth = madge_edges(&madge, &dir);
    assert!(!truth.is_empty(), "madge saw no edges — fixture broken?");

    let rows = dl_rows(&dir);
    let ours: HashSet<(String, String)> = rows.get("dep").cloned().unwrap_or_default()
        .into_iter().filter(|r| r.len() == 2)
        .map(|r| (r[0].clone(), r[1].clone()))
        .collect();

    let matched: HashSet<_> = ours.intersection(&truth).cloned().collect();
    let extra: Vec<_> = ours.difference(&truth).cloned().collect();
    let missed: Vec<_> = truth.difference(&ours).cloned().collect();
    eprintln!("[oracle:madge] ours={} madge={} matched={}", ours.len(), truth.len(), matched.len());
    eprintln!("[oracle:madge] ours not in madge: {extra:?}");
    eprintln!("[oracle:madge] madge not in ours: {missed:?}");
    assert!(!ours.is_empty(), "we produced no edges");
    assert_eq!(ours, truth, "dep graph must agree edge-for-edge on this fixture");

    // Cycle membership: {b, c} both ways.
    let truth_cycle = madge_cycle_members(&madge, &dir);
    let our_cycle: BTreeSet<String> = rows.get("cycle_member").cloned().unwrap_or_default()
        .into_iter().filter_map(|r| r.into_iter().next())
        .collect();
    assert_eq!(our_cycle, truth_cycle, "cycle members must agree");
    assert!(our_cycle.contains("src/b.ts") && our_cycle.contains("src/c.ts"),
        "the b<->c cycle is the fixture's point: {our_cycle:?}");

    // Orphans: nothing imports index.ts or d.ts.
    let orphans: BTreeSet<String> = rows.get("orphan").cloned().unwrap_or_default()
        .into_iter().filter_map(|r| r.into_iter().next())
        .collect();
    assert_eq!(orphans, BTreeSet::from(["src/index.ts".into(), "src/d.ts".into()]),
        "orphans: {orphans:?}");
}
