//! Differential oracle: validate examples/madge.dl (module dependency graph +
//! circular detection over `module_edge`) against madge ground truth on a TS
//! fixture with a cycle, an orphan, and a leaf. `#[ignore]`d — no madge
//! binary is a genuine environmental gap, not something the default
//! `cargo test` run should silently count as coverage. Set SPREFA_MADGE to
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

/// index -> a,b,leaf ; a -> b ; b <-> c cycle ; d is an orphan (imports a,
/// nobody imports d) ; c is also a leaf-of-the-cycle counterexample (imports
/// b). Grown to exercise every module_edge resolver path madge covers:
/// - `leaf.ts`: no imports (a leaf), but index.ts imports it (not an orphan) —
///   proves leaf and orphan are independent conditions.
/// - `barrel.ts`: `export { a } from './a'` re-export barrel.
/// - `dyn.ts`: dynamic `import(...)` inside an async function.
/// - `helper.js` + `cjs.js`: CommonJS `require`. The require target is a
///   sibling `.js` file, not `.ts` — madge does not resolve a CJS `require`
///   from a `.js` file to a `.ts` target (verified empirically: real Node
///   `require` has no native `.ts` handling), so a cross-extension CJS case
///   would be a madge quirk, not a module-graph fact worth asserting.
/// - `npmuser.ts`: `import _ from 'lodash'` (never installed, so genuinely
///   unresolvable) alongside a real project import.
/// - `broken.ts`: a relative import to a file that doesn't exist — the
///   genuinely-broken-import case, distinct from the npm case.
/// - `data.json` + `withjson.ts`: a relative `.json` import to a file that
///   actually exists. Ground truth (verified against real madge in a scratch
///   fixture outside this test): madge resolves an existing relative `.json`
///   import to a dependency edge REGARDLESS of whether `json` is in
///   `--extensions` — `--extensions` only gates which files seed the walk as
///   entry points, not what an import can resolve to. The resolved `.json`
///   file then shows up as its own zero-dependency node in `--json`,
///   `--leaves`, and `--summary`, but never in `--orphans` (it was never an
///   entry point, so it can't be "unimported and eligible" the way an
///   extensions-matched file can).
fn write_fixture(dir: &Path) {
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("tsconfig.json"), r#"{"compilerOptions":{"module":"esnext"}}"#).unwrap();
    fs::write(dir.join("src/index.ts"), "import { a } from './a';\nimport { b } from './b';\nimport { leafVal } from './leaf';\nexport const i = a + b + leafVal;\n").unwrap();
    fs::write(dir.join("src/a.ts"), "import { b } from './b';\nexport const a = b + 1;\n").unwrap();
    fs::write(dir.join("src/b.ts"), "import { c } from './c';\nexport const b = c + 1;\n").unwrap();
    fs::write(dir.join("src/c.ts"), "import { b } from './b';\nexport const c = 1;\nexport const use_b = () => b;\n").unwrap();
    fs::write(dir.join("src/d.ts"), "import { a } from './a';\nexport const d = a;\n").unwrap();
    fs::write(dir.join("src/leaf.ts"), "export const leafVal = 42;\n").unwrap();
    fs::write(dir.join("src/barrel.ts"), "export { a } from './a';\n").unwrap();
    fs::write(dir.join("src/dyn.ts"), "export async function loadB() {\n  const mod = await import('./b');\n  return mod;\n}\n").unwrap();
    fs::write(dir.join("src/helper.js"), "module.exports = { helperVal: 7 };\n").unwrap();
    fs::write(dir.join("src/cjs.js"), "const helper = require('./helper');\nmodule.exports = { got: helper.helperVal };\n").unwrap();
    fs::write(dir.join("src/npmuser.ts"), "import _ from 'lodash';\nimport { a } from './a';\nexport const useIt = () => _.isEmpty({}) && a;\n").unwrap();
    fs::write(dir.join("src/broken.ts"), "import { missing } from './does-not-exist';\nexport const brokenVal = missing;\n").unwrap();
    fs::write(dir.join("src/data.json"), r#"{"k":1}"#).unwrap();
    fs::write(dir.join("src/withjson.ts"), "import data from './data.json';\nimport { a } from './a';\nexport const wj = a + data.k;\n").unwrap();
}

/// Drop ANSI escape sequences (`ESC [ ... <final byte @-~>`). Newer madge
/// colorizes even non-tty stdout via chalk, so the text-parsed `--warning`
/// output carries color codes; the env vars below usually suppress them, this
/// strip is the belt-and-suspenders for chalk versions that ignore NO_COLOR.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for e in chars.by_ref() {
                if ('@'..='~').contains(&e) { break; }
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// Run madge with the given extra args (always `--extensions ts,js` so the
/// grown fixture's CommonJS `.js` files are in scope), parse stdout as JSON.
/// NO_COLOR/FORCE_COLOR keep chalk from colorizing the output.
fn madge_json(madge: &Path, dir: &Path, args: &[&str]) -> serde_json::Value {
    let mut full_args = vec!["--extensions", "ts,js"];
    full_args.extend_from_slice(args);
    let out = Command::new(madge).args(&full_args).current_dir(dir)
        .env("NO_COLOR", "1").env("FORCE_COLOR", "0")
        .output().unwrap_or_else(|e| panic!("run madge {args:?}: {e}"));
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!(
        "madge {args:?} --json unparsable: {e}\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
}

/// madge --json: {"src/a.ts": ["src/b.ts"], ...} relative to the scanned dir.
fn madge_edges(madge: &Path, dir: &Path) -> HashSet<(String, String)> {
    let v = madge_json(madge, dir, &["--json", "."]);
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
    let v = madge_json(madge, dir, &["--circular", "--json", "."]);
    v.as_array().into_iter().flatten()
        .flat_map(|cycle| cycle.as_array().into_iter().flatten())
        .filter_map(|f| f.as_str().map(String::from))
        .collect()
}

/// madge --leaves --json: ["src/leaf.ts", ...].
fn madge_leaves(madge: &Path, dir: &Path) -> BTreeSet<String> {
    let v = madge_json(madge, dir, &["--leaves", "--json", "."]);
    v.as_array().into_iter().flatten()
        .filter_map(|f| f.as_str().map(String::from))
        .collect()
}

/// madge --summary --json: {"src/index.ts": 3, ...} dependency count per file.
fn madge_summary(madge: &Path, dir: &Path) -> std::collections::HashMap<String, i64> {
    let v = madge_json(madge, dir, &["--summary", "--json", "."]);
    v.as_object().into_iter().flatten()
        .filter_map(|(k, n)| n.as_i64().map(|n| (k.clone(), n)))
        .collect()
}

/// madge --depends <target> --json: direct dependents of target.
fn madge_depends(madge: &Path, dir: &Path, target: &str) -> BTreeSet<String> {
    let v = madge_json(madge, dir, &["--depends", target, "--json", "."]);
    v.as_array().into_iter().flatten()
        .filter_map(|f| f.as_str().map(String::from))
        .collect()
}

/// madge --warning: a flat, file-attribution-free, deduped list of specifiers
/// it could not resolve (relative AND bare/npm alike — this is the one place
/// madge treats them the same). Parsed from stdout text; madge has no
/// `--warning --json` combination (verified: `--json` still emits the dep
/// graph, dropping the skip list entirely).
fn madge_warnings(madge: &Path, dir: &Path) -> BTreeSet<String> {
    let out = Command::new(madge).args(["--extensions", "ts,js", "--warning", "."])
        .env("NO_COLOR", "1").env("FORCE_COLOR", "0")
        .current_dir(dir).output().expect("run madge --warning");
    let stdout = strip_ansi(&String::from_utf8_lossy(&out.stdout));
    let after = stdout.split("Skipped").nth(1).unwrap_or("");
    after.lines().skip(1) // the "N files" remainder of the "Skipped" line
        .map(str::trim).filter(|l| !l.is_empty())
        .map(String::from).collect()
}

/// Run examples/madge.dl, parse the `?` TSV blocks into (rel -> rows).
fn dl_rows(dir: &Path) -> std::collections::HashMap<String, Vec<Vec<String>>> {
    let prog = PathBuf::from(V5).join("examples/madge.dl");
    let out = Command::new(DL)
        .arg(&prog)
        .args(["--no-daemon",
               "--db", dir.join("madge.db").to_str().unwrap()])
        .current_dir(dir)
        .output().expect("run dl");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut rows: std::collections::HashMap<String, Vec<Vec<String>>> = Default::default();
    let mut cur: Option<String> = None;
    for line in stdout.lines() {
        // header shape: `? dep => a\tb`
        if let Some(rest) = line.strip_prefix("? ") {
            cur = rest.split([' ', '('])
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
#[ignore = "needs madge on PATH (npm i -g madge, or set SPREFA_MADGE)"]
fn madge_oracle_dep_graph_and_cycles_agree() {
    let madge = find_madge().expect("needs madge on PATH (npm i -g madge, or set SPREFA_MADGE)");
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
    assert!(ours.contains(&("src/withjson.ts".to_string(), "src/data.json".to_string())),
        "an existing relative .json import must resolve to a dep edge: {ours:?}");

    // Cycle membership: {b, c} both ways.
    let truth_cycle = madge_cycle_members(&madge, &dir);
    let our_cycle: BTreeSet<String> = rows.get("cycle_member").cloned().unwrap_or_default()
        .into_iter().filter_map(|r| r.into_iter().next())
        .collect();
    assert_eq!(our_cycle, truth_cycle, "cycle members must agree");
    assert!(our_cycle.contains("src/b.ts") && our_cycle.contains("src/c.ts"),
        "the b<->c cycle is the fixture's point: {our_cycle:?}");

    // Orphans: nobody imports these (madge --orphans ground truth on this
    // fixture — barrel/broken/cjs/d/dyn/index/npmuser all have zero dependents).
    let truth_orphans = madge_json(&madge, &dir, &["--orphans", "--json", "."]);
    let truth_orphans: BTreeSet<String> = truth_orphans.as_array().into_iter().flatten()
        .filter_map(|f| f.as_str().map(String::from)).collect();
    let orphans: BTreeSet<String> = rows.get("orphan").cloned().unwrap_or_default()
        .into_iter().filter_map(|r| r.into_iter().next())
        .collect();
    assert_eq!(orphans, truth_orphans, "orphans must agree with madge --orphans");
    assert!(orphans.contains("src/index.ts") && orphans.contains("src/d.ts"),
        "orphans: {orphans:?}");
    assert!(!orphans.contains("src/data.json"),
        "an imported .json target was never a `seen`/extensions-matched entry \
         file, so it must never appear as an orphan: {orphans:?}");

    // --leaves: depends on nothing. Not previously asserted against madge —
    // the original 5-file fixture had zero leaves (every file imports
    // something), so `leaf` never got proven against ground truth.
    let truth_leaves = madge_leaves(&madge, &dir);
    let leaves: BTreeSet<String> = rows.get("leaf").cloned().unwrap_or_default()
        .into_iter().filter_map(|r| r.into_iter().next())
        .collect();
    eprintln!("[oracle:madge] leaves ours={leaves:?} madge={truth_leaves:?}");
    assert_eq!(leaves, truth_leaves, "leaves must agree with madge --leaves");
    assert!(leaves.contains("src/leaf.ts"), "src/leaf.ts is the fixture's dedicated leaf: {leaves:?}");
    assert!(leaves.contains("src/data.json"),
        "an imported .json target is a leaf (zero deps of its own) even though \
         it was never a `seen` entry file: {leaves:?}");

    // --summary: dependency count per module, including the zero-count leaves.
    let truth_summary = madge_summary(&madge, &dir);
    let summary: std::collections::HashMap<String, i64> = rows.get("summary").cloned().unwrap_or_default()
        .into_iter().filter(|r| r.len() == 2)
        .map(|r| (r[0].clone(), r[1].parse().unwrap()))
        .collect();
    assert_eq!(summary, truth_summary, "summary counts must agree with madge --summary");
    assert_eq!(summary["src/index.ts"], 3, "index.ts imports a, b, leaf: {summary:?}");
    assert_eq!(summary["src/leaf.ts"], 0, "leaf.ts has zero deps, must still appear: {summary:?}");
    assert_eq!(summary["src/withjson.ts"], 2, "withjson.ts imports data.json + a: {summary:?}");
    assert_eq!(summary["src/data.json"], 0,
        "an imported .json target must appear in summary with zero deps: {summary:?}");

    // --depends src/a.ts: direct dependents (barrel, d, index, npmuser — 4, so
    // the ≥2-dependents case the task calls for).
    let truth_depends = madge_depends(&madge, &dir, "src/a.ts");
    assert!(truth_depends.len() >= 2, "fixture must give a.ts multiple direct dependents: {truth_depends:?}");
    let depends: BTreeSet<String> = rows.get("depends_on").cloned().unwrap_or_default()
        .into_iter().filter(|r| r.len() == 2 && r[0] == "src/a.ts")
        .map(|r| r[1].clone())
        .collect();
    assert_eq!(depends, truth_depends, "depends_on(src/a.ts, _) must agree with madge --depends src/a.ts");

    // npm/external: lodash is never installed, so it is genuinely unresolvable,
    // not just excluded by a madge flag. madge silently drops an unresolvable
    // bare specifier from the dep graph EVEN WITH `--include-npm` — that flag
    // only widens resolution to a package that already resolves under
    // node_modules; it does not report which bare specifiers failed to
    // resolve (confirmed against real madge in an npm-installed scratch dir
    // outside this test, since installing lodash here would defeat the
    // "never installed" fixture point). So `npm_dep` has no madge --json
    // counterpart to diff against directly; it is instead cross-checked
    // against madge's flat `--warning` skip list below, which is the one
    // place madge surfaces bare npm specifiers at all.
    let npm_deps: BTreeSet<String> = rows.get("npm_dep").cloned().unwrap_or_default()
        .into_iter().filter(|r| r.len() == 2 && r[0] == "src/npmuser.ts")
        .map(|r| r[1].clone())
        .collect();
    assert_eq!(npm_deps, BTreeSet::from(["lodash".to_string()]), "npm_dep must see the bare lodash import: {npm_deps:?}");
    assert!(!truth.contains(&("src/npmuser.ts".to_string(), "lodash".to_string())),
        "sanity: madge's dep graph must NOT contain the unresolvable lodash edge");

    // warnings/skipped: madge --warning has no --json form (verified: --json
    // still emits the dep graph and drops the skip list). Its flat,
    // file-attribution-free specifier list is the union of dl's two more
    // structured relations: `skipped` (genuinely broken — file+line, the
    // linter question) and `npm_dep` (external packages — an inventory
    // question). dl's split is strictly MORE informative, not a subset/
    // superset mismatch: the specifier sets agree, dl additionally locates them.
    let truth_warnings = madge_warnings(&madge, &dir);
    assert_eq!(truth_warnings, BTreeSet::from(["./does-not-exist".to_string(), "lodash".to_string()]),
        "madge --warning ground truth drifted: {truth_warnings:?}");
    let skipped: BTreeSet<String> = rows.get("skipped").cloned().unwrap_or_default()
        .into_iter().filter(|r| r.len() == 4)
        .map(|r| r[1].clone())
        .collect();
    assert_eq!(skipped, BTreeSet::from(["./does-not-exist".to_string()]),
        "skipped (module_unresolved) must carry the genuinely broken relative import: {skipped:?}");
    let dl_warning_union: BTreeSet<String> = skipped.union(&npm_deps).cloned().collect();
    assert_eq!(dl_warning_union, truth_warnings,
        "skipped ∪ npm_dep must equal madge's flat --warning specifier set: {dl_warning_union:?} vs {truth_warnings:?}");
}
