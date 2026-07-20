//! Permanent rail: two identical full rebuilds must produce byte-identical
//! extract-family rows. Commit 80617b6b fixed extraction nondeterminism from
//! unordered file-set queries and cache-order-dependent fact emission feeding
//! first-wins dedups (see docs/rca-exe-swap-write-storm.md). This test bans a
//! regression by snapshotting the `rows:<rel>` content digests in `_reldigest`
//! after a cold full tick, wiping the extract skip-state, ticking again (so the
//! second run exercises the warm parse-cache path), and asserting every digest
//! matches.

use sprefa_v5::config::RepoConfig;
use sprefa_v5::{db, engine::Engine, lex, parse};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "extraction_determinism_{tag}_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

const PROG: &str = r#"
rel seen(path: file).
seen(p) <- scan("*", "WORK", "src/**/*.{rs,ts}", p, rev).

? type_entity(repo, sym, name, kind, parent, file, line).
? type_edge(from, to, kind, repo).
? call_def(repo, sym, kind, file, line, end).
? call_site(repo, caller, callee, file, line).
? df_node(id, kind, var, fn, file, line, _).
? df_lit(id, text, kind).
? df_param(id, pos).
? df_arg(call, pos, arg).
? df_edge(from, to).
? doc_comment(repo, sym, line, text).
? comment_node(path, line, col, end_line, end_col, text, kind).
? loop_over(file, start, end, var, collection, fn).
? nest(call_id, loop_id, depth, collection).
? doc_node(repo, file, line, kind, name, parent).
"#;

/// Two repos (`repo-a` and `repo-b`) share the same relative path
/// `src/coincide.rs` with *different* content. The df_node id is
/// `file:line:col` with no repo component, so this is the lossy-id collision
/// from the RCA; ordering must keep the winner stable across rebuilds.
fn write_fixture(root: &PathBuf) -> (PathBuf, PathBuf) {
    let a = root.join("repo-a");
    let b = root.join("repo-b");
    fs::create_dir_all(a.join("src")).unwrap();
    fs::create_dir_all(b.join("src")).unwrap();

    fs::write(
        a.join("src/a.rs"),
        r#"//! Module docs for repo-a a.rs
/// Docs for outer
pub fn outer(x: i32) -> i32 {
    let doubled = |y: i32| y * 2;
    let mut sum = 0;
    for i in 0..x {
        // accumulate
        sum += doubled(i);
    }
    sum
}
"#,
    )
    .unwrap();

    fs::write(
        a.join("src/b.rs"),
        r#"/// Beta function
pub fn beta(items: &[i32]) -> i32 {
    items.iter().fold(0, |acc, &n| acc + n)
}
"#,
    )
    .unwrap();

    fs::write(
        a.join("src/example.ts"),
        r#"/**
 * Computes a value.
 * @param values - input numbers
 * @returns sum
 */
export function compute(values: number[]): number {
    let total = 0;
    for (const v of values) {
        // inline lambda call
        total += ((x: number) => x + v)(1);
    }
    return total;
}
"#,
    )
    .unwrap();

    // The lossy-id collision: same relative path, different content.
    fs::write(
        a.join("src/coincide.rs"),
        "/// Coincide in repo A\npub fn coincide_a() -> i32 { 1 }\n",
    )
    .unwrap();
    fs::write(
        b.join("src/coincide.rs"),
        "/// Coincide in repo B\npub fn coincide_b() -> i32 { 2 }\n",
    )
    .unwrap();

    (a, b)
}

fn snapshot_rows_digests(eng: &Engine) -> BTreeMap<String, String> {
    eng.query_sql("SELECT rel, digest FROM _reldigest WHERE rel LIKE 'rows:%'", &[])
        .unwrap()
        .into_iter()
        .map(|row| {
            let rel = row[0].as_str().unwrap().to_string();
            let digest = row[1].as_str().unwrap().to_string();
            (rel, digest)
        })
        .collect()
}

fn extract_rels_present(digests: &BTreeMap<String, String>) -> Vec<String> {
    digests.keys().cloned().collect()
}

#[test]
fn extraction_is_deterministic_across_identical_rebuilds() {
    let root = sandbox("rail");
    let (repo_a, repo_b) = write_fixture(&root);

    let prog = parse::parse(lex::lex(PROG).unwrap()).unwrap();
    let conn = db::open(Some(root.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, root.clone());
    eng.set_repos(vec![
        RepoConfig {
            slug: "repo-a".into(),
            root: repo_a.clone(),
            url: None,
            allow_missing: false,
        },
        RepoConfig {
            slug: "repo-b".into(),
            root: repo_b.clone(),
            url: None,
            allow_missing: false,
        },
    ]);

    // First full tick: cold parse caches.
    eng.tick(&prog, true).unwrap();
    let first = snapshot_rows_digests(&eng);
    let rels = extract_rels_present(&first);
    assert!(!rels.is_empty(), "at least one extract-family rel must land in _reldigest");
    assert!(
        rels.iter().any(|r| r == "rows:df_node"),
        "the digested rel set must contain df_node, got: {rels:?}"
    );

    // Wipe every extract-skip digest so the next tick cannot short-circuit
    // extraction. The parse caches stay warm in memory, so the second run
    // exercises the cache-hit ordering path that the RCA fixed.
    eng.query_sql("DELETE FROM _reldigest WHERE rel LIKE 'extract:%'", &[])
        .unwrap();

    // Second full tick: forced re-extract over the identical corpus.
    eng.tick(&prog, true).unwrap();
    let second = snapshot_rows_digests(&eng);

    // The set of digested rels must be stable.
    assert_eq!(
        first.keys().collect::<Vec<_>>(),
        second.keys().collect::<Vec<_>>(),
        "the set of digested extract-family rels must not change across identical rebuilds"
    );

    // Every rel's row-content digest must be identical across the two rebuilds.
    for (rel, d1) in &first {
        let d2 = second.get(rel).unwrap();
        assert_eq!(
            d1, d2,
            "extract-family rel {rel} produced different rows across two identical rebuilds"
        );
    }
}
