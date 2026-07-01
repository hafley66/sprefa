//! `scc(edge)` builtin: SCC membership (rep, member) from the closure
//! condensation. Mirrors `closure(edge)` but exposes the partition (component
//! representative + member) the condensation already computes — no second
//! Tarjan run. Verified end-to-end via the `dl` binary.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("dl_scc_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

fn run(dir: &PathBuf, prog: &str) -> String {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl")).arg("--root").arg(dir)
        .arg("--db").arg(dir.join("db").to_str().unwrap())
        .env("DL_NO_DAEMON", "1")
        .output().expect("run dl");
    assert!(out.status.success(),
        "dl failed; stderr={}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Pull the rows of a `? rel(...)` query out of stdout (trimmed, sorted).
fn rows_of(stdout: &str, head: &str) -> Vec<String> {
    let mut out: Vec<String> = stdout.lines()
        .skip_while(|l| !l.starts_with(&format!("? {head} =>")))
        .skip(1)
        .take_while(|l| l.starts_with("  "))
        .filter(|l| !l.trim().starts_with('(')) // drop the "(N rows)" summary
        .map(|l| l.trim().to_string())
        .collect();
    out.sort();
    out
}

/// Graph: a->b->c->a (3-cycle) + d->e (chain). The cycle is one cluster under
/// rep = min(a,b,c) = "a"; d and e are singleton clusters.
#[test]
fn scc_emits_cluster_membership() {
    let dir = sandbox("membership");
    let stdout = run(&dir, r#"rel edge(a: text, b: text).
edge("a","b"). edge("b","c"). edge("c","a").
edge("d","e").
rel g(rep: text, member: text).
g(rep, member) <- scc(edge).
? g(rep, member).
"#);
    // 3-cycle {a,b,c} -> rep "a" (three rows); d, e singletons -> rep themselves.
    assert_eq!(rows_of(&stdout, "g"),
        vec!["a\ta".to_string(), "a\tb".to_string(), "a\tc".to_string(),
             "d\td".to_string(), "e\te".to_string()],
        "cluster membership mismatch:\n{stdout}");
}

/// scc reuses the closure condensation: both `closure(edge)` and `scc(edge)`
/// over the same edge run one Tarjan, and the cycle is one cluster in the scc
/// head while closure answers seeded reachability over the same condensation.
#[test]
fn scc_shares_condensation_with_closure() {
    let dir = sandbox("shared");
    let stdout = run(&dir, r#"rel edge(a: text, b: text).
edge("a","b"). edge("b","c"). edge("c","a").
rel r(a: text, b: text).
r(a,b) <- closure(edge).
rel g(rep: text, member: text).
g(rep, member) <- scc(edge).
? g(rep, member).
"#);
    let g = rows_of(&stdout, "g");
    assert!(g == vec!["a\ta".to_string(), "a\tb".to_string(), "a\tc".to_string()],
        "shared-condensation cluster wrong:\n{stdout}");
}

/// An acyclic graph (a pure DAG) has no multi-member SCC: every node is its own
/// cluster (rep == member). Guards against accidentally treating reachability
/// as clustering.
#[test]
fn scc_dag_is_all_singletons() {
    let dir = sandbox("dag");
    let stdout = run(&dir, r#"rel edge(a: text, b: text).
edge("a","b"). edge("b","c"). edge("c","d").
rel g(rep: text, member: text).
g(rep, member) <- scc(edge).
? g(rep, member).
"#);
    // a->b->c->d, no cycle: four singleton clusters, rep == member each.
    assert_eq!(rows_of(&stdout, "g"),
        vec!["a\ta".to_string(), "b\tb".to_string(),
             "c\tc".to_string(), "d\td".to_string()],
        "DAG should be all singletons:\n{stdout}");
}

/// Regression: a derived rule that READS an scc head. The head fills in the
/// query phase (after the main fixpoint), so before the post-stratum split this
/// derived rule was lowered into the fixpoint and read the head EMPTY — silently
/// emitting zero rows. The two-stratum rebuild evaluates it AFTER the scc eval.
#[test]
fn derived_rule_can_aggregate_over_scc_head() {
    let dir = sandbox("derived_over_scc");
    let stdout = run(&dir, r#"rel edge(a: text, b: text).
edge("a","b"). edge("b","c"). edge("c","a").
edge("d","e").
rel g(rep: text, member: text).
g(rep, member) <- scc(edge).
rel csize(rep: text, n: int).
csize(rep, count(member)) <- g(rep, member).
? csize(rep, n).
"#);
    // {a,b,c} -> rep "a" size 3; d, e singletons size 1. Empty before the fix.
    assert_eq!(rows_of(&stdout, "csize"),
        vec!["a\t3".to_string(), "d\t1".to_string(), "e\t1".to_string()],
        "derived aggregate over scc head wrong (was empty pre-fix):\n{stdout}");
}

/// A second derived rule downstream of the scc-reading one (post-stratum depth
/// > 1): the partition must pull the whole transitive chain into the post
/// stratum, not just the direct reader.
#[test]
fn derived_chain_below_scc_head_fills() {
    let dir = sandbox("chain_below_scc");
    let stdout = run(&dir, r#"rel edge(a: text, b: text).
edge("a","b"). edge("b","c"). edge("c","a").
edge("d","e").
rel g(rep: text, member: text).
g(rep, member) <- scc(edge).
rel csize(rep: text, n: int).
csize(rep, count(member)) <- g(rep, member).
rel big(rep: text).
big(rep) <- csize(rep, n), n > 1.
? big(rep).
"#);
    // only {a,b,c} has size > 1, rep "a".
    assert_eq!(rows_of(&stdout, "big"), vec!["a".to_string()],
        "transitive post-stratum chain did not fill:\n{stdout}");
}
