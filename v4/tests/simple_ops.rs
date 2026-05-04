// SimpleOp surface + starter string/path std-lib.
//   1. Split    — break a term value into N children
//   2. Format   — render template with ${X} into a term
//   3. Trim     — whitespace trim
//   4. Basename — file_name on a path term
//   5. Dirname  — parent dir on a path term
//
// Each is a 1-line `step(&Cursor) -> Vec<Cursor>`. The blanket
// `impl<T: SimpleOp> Op for T` provides ident, run, telemetry.

use std::sync::Arc;
use futures::stream::StreamExt;
use tokio::sync::mpsc;
use v4::*;

fn hooks() -> Hooks {
    let (eff_tx, _eff_rx) = mpsc::unbounded_channel();
    Hooks {
        store: MemStore::new(), effects: eff_tx, gen: 0, lineage: new_lineage(),
        tele: Telemetry::new(), interner: Interner::new(),
    }
}
async fn run_one(op: Arc<dyn Op>, batch: Vec<Cursor>) -> Vec<Cursor> {
    let input = futures::stream::iter(vec![batch]).boxed();
    let mut s = op.run(hooks(), input);
    let mut out = Vec::new();
    while let Some(b) = s.next().await { out.extend(b); }
    out
}

#[tokio::test]
async fn split_emits_one_child_per_piece() {
    let mut c = Cursor::default(); c.set("RAW", "a,b,,c");
    let out = run_one(Arc::new(Split::new("RAW", ",", "X")), vec![c]).await;
    let xs: Vec<&str> = out.iter().filter_map(|c| c.get("X")).collect();
    assert_eq!(xs, vec!["a", "b", "c"]);  // empty piece dropped
    // Parent term preserved on every child.
    assert!(out.iter().all(|c| c.get("RAW") == Some("a,b,,c")));
}

#[tokio::test]
async fn format_renders_template_with_term_subs() {
    let mut c = Cursor::default();
    c.set("REPO", "alpha"); c.set("REV", "main");
    let op = Format::new("https://github.com/${REPO}/tree/${REV}", "URL");
    let out = run_one(Arc::new(op), vec![c]).await;
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].get("URL"), Some("https://github.com/alpha/tree/main"));
}

#[tokio::test]
async fn trim_in_place_collapses_whitespace_padding() {
    let mut c = Cursor::default(); c.set("S", "  hello  \n");
    let out = run_one(Arc::new(Trim::in_place("S")), vec![c]).await;
    assert_eq!(out[0].get("S"), Some("hello"));
}

#[tokio::test]
async fn basename_and_dirname_split_a_path() {
    let mut c = Cursor::default(); c.set("FS", "/a/b/file.rs");
    let chain: Vec<Arc<dyn Op>> = vec![
        Arc::new(Basename::new("FS", "FILE")),
        Arc::new(Dirname::new("FS", "DIR")),
    ];
    let input = futures::stream::iter(vec![vec![c]]).boxed();
    let mut s = chain.into_iter().fold(input, |acc, op| op.run(hooks(), acc));
    let out: Vec<Cursor> = {
        let mut o = Vec::new();
        while let Some(b) = s.next().await { o.extend(b); }
        o
    };
    assert_eq!(out[0].get("FILE"), Some("file.rs"));
    assert_eq!(out[0].get("DIR"),  Some("/a/b"));
    assert_eq!(out[0].get("FS"),   Some("/a/b/file.rs"));
}

#[tokio::test]
async fn simple_op_ident_distinguishes_instances() {
    // Two Splits with different separators should have different idents.
    let a: Arc<dyn Op> = Arc::new(Split::new("R", ",", "X"));
    let b: Arc<dyn Op> = Arc::new(Split::new("R", "|", "X"));
    let c: Arc<dyn Op> = Arc::new(Split::new("R", ",", "X"));
    assert_ne!(a.ident(), b.ident());
    assert_eq!(a.ident(), c.ident());
}

#[tokio::test]
async fn simple_op_wraps_under_op_cache() {
    // Pure SimpleOp is_pure=true ⇒ OpCache::wrap accepts it.
    let cache = OpCache::wrap(Arc::new(Trim::in_place("S")));
    let mut c1 = Cursor::default(); c1.set("S", " a ");
    let mut c2 = Cursor::default(); c2.set("S", " b ");

    let cold = run_one(cache.clone() as Arc<dyn Op>, vec![c1.clone(), c2.clone()]).await;
    assert_eq!(cold.len(), 2);
    assert_eq!(cache.misses(), 2);
    assert_eq!(cache.hits(), 0);

    // Warm: same lineage on c1/c2 ⇒ all hits, nothing emitted.
    let warm = run_one(cache.clone() as Arc<dyn Op>, vec![c1, c2]).await;
    assert_eq!(warm.len(), 0);
    assert_eq!(cache.hits(), 2);
}

#[tokio::test]
async fn end_to_end_chain_split_then_format() {
    // Show the "least var ref" point: chain reads/writes happen via
    // documented term names; never thread cursors explicitly.
    //   RAW="alpha,beta,gamma" > Split(RAW, ",", REPO) > Format("@${REPO}", URL)
    let mut seed = Cursor::default(); seed.set("RAW", "alpha,beta,gamma");
    let chain: Vec<Arc<dyn Op>> = vec![
        Arc::new(Split::new("RAW", ",", "REPO")),
        Arc::new(Format::new("git@github.com:${REPO}", "URL")),
    ];
    let input = futures::stream::iter(vec![vec![seed]]).boxed();
    let mut s = chain.into_iter().fold(input, |acc, op| op.run(hooks(), acc));
    let mut out = Vec::new();
    while let Some(b) = s.next().await { out.extend(b); }
    let urls: Vec<&str> = out.iter().filter_map(|c| c.get("URL")).collect();
    assert_eq!(urls, vec![
        "git@github.com:alpha", "git@github.com:beta", "git@github.com:gamma",
    ]);
}
