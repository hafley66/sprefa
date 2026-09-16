//! Foundation guard for the embedding seam: the `similar(a, b, score)` built-in
//! over the content-addressed `_embeddings` table, default `stub` backend
//! (deterministic hashed bag-of-tokens, zero deps). Properties, all always-run:
//!
//! 1. `similar_ranks_lexical_overlap`: with the stub, two captured identifiers
//!    sharing a token ("charge_payment" / "refund_payment", both -> {.,payment})
//!    are nearer than a token-disjoint one ("unrelated_widget"). Proves the
//!    encode -> store -> KNN -> `similar` pipeline runs and orders by overlap.
//! 2. `embed_once_per_content_and_backend`: `_embeddings` holds one row per
//!    (StringId, backend); a re-tick over the unchanged repo embeds nothing new
//!    and `refresh_embed_rels` early-outs (no derived churn).
//! 3. `embed_tick_has_no_n1`: encoding many strings goes through one
//!    `insert_rows`, not a per-row write (the N+1 detector stays silent).

use sprefa_v5::{db, engine::Engine, lex, parse};
use std::fs;
use std::path::{Path, PathBuf};

// Capture identifiers (interns them into `_strings`), then resolve `similar`
// back to text. `s > 1` keeps only positive-cosine pairs out of the report.
const PROG: &str = r#"
rel tok(name: text, path: file).
tok(name, p) <- scan("WORK", "**/*.rs", p, rev),
  match(p, rev, /(?<name>[A-Za-z_][A-Za-z0-9_]{3,})/, line).
rel near(a: text, b: text, score: int).
near(ta, tb, s) <- similar(sa, sb, s), string(sa, ta, _), string(sb, tb, _), s > 1.
? near(a, b, score).
"#;

fn write_repo(tag: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("embed_similar_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    for (name, body) in files {
        fs::write(dir.join("src").join(name), body).unwrap();
    }
    fs::canonicalize(&dir).unwrap()
}

fn engine_at(root: &Path) -> Engine {
    let conn = db::open(Some(root.join("db").to_str().unwrap())).unwrap();
    Engine::new(conn, root.to_path_buf())
}

/// `near` rows (a, b, score) ordered by descending score, via the public query
/// path. `near` is a user rel, so its table is `rel_near` (the `tbl()` prefix).
fn rows(eng: &Engine) -> Vec<(String, String, i64)> {
    eng.query_sql(
        "SELECT \"a\",\"b\",\"score\" FROM rel_near_txt ORDER BY 3 DESC",
        &[],
    )
    .unwrap()
    .into_iter()
    .map(|r| (str_of(&r[0]), str_of(&r[1]), int_of(&r[2])))
    .collect()
}

/// Count rows in a meta/rel table via the public query path.
fn scalar(eng: &Engine, sql: &str) -> i64 {
    int_of(&eng.query_sql(sql, &[]).unwrap()[0][0])
}

fn str_of(v: &serde_json::Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}
fn int_of(v: &serde_json::Value) -> i64 {
    v.as_i64().unwrap_or_default()
}

#[test]
fn similar_ranks_lexical_overlap() {
    let root = write_repo(
        "rank",
        &[
            ("a.rs", "fn charge_payment() {}\n"),
            ("b.rs", "fn refund_payment() {}\n"),
            ("c.rs", "fn unrelated_widget() {}\n"),
        ],
    );
    let prog = parse::parse(lex::lex(PROG).unwrap()).unwrap();
    let mut eng = engine_at(&root);
    eng.tick(&prog, true).unwrap();

    let near = rows(&eng);
    assert!(!near.is_empty(), "similar produced no pairs");
    // charge_payment and refund_payment share the `payment` token -> a positive
    // pair must exist between them.
    let shared = near.iter().any(|(a, b, sc)| {
        ((a == "charge_payment" && b == "refund_payment")
            || (a == "refund_payment" && b == "charge_payment"))
            && *sc > 1
    });
    assert!(
        shared,
        "expected a positive charge_payment~refund_payment pair, got {near:?}"
    );
    // unrelated_widget shares no token with the payment pair, so it must not be
    // the top neighbor of either (would imply a hash collision dominating).
    let payment_top = near
        .iter()
        .filter(|(a, _, _)| a == "charge_payment")
        .max_by_key(|(_, _, sc)| *sc);
    if let Some((_, b, _)) = payment_top {
        assert_eq!(
            b, "refund_payment",
            "charge_payment's nearest should be refund_payment, got {b}"
        );
    }
}

#[test]
fn embed_once_per_content_and_backend() {
    let root = write_repo(
        "once",
        &[
            ("a.rs", "fn charge_payment() {}\n"),
            ("b.rs", "fn refund_payment() {}\n"),
        ],
    );
    let prog = parse::parse(lex::lex(PROG).unwrap()).unwrap();
    let mut eng = engine_at(&root);
    eng.tick(&prog, true).unwrap();

    // one row per (sid, backend): no duplicate sids for the stub backend.
    let total = scalar(
        &eng,
        "SELECT count(*) FROM _embeddings WHERE backend = 'stub'",
    );
    let distinct = scalar(
        &eng,
        "SELECT count(DISTINCT sid) FROM _embeddings WHERE backend = 'stub'",
    );
    assert!(total > 0, "no embeddings stored");
    assert_eq!(
        total, distinct,
        "duplicate (sid, backend) rows: embed-once broke"
    );

    // re-tick the unchanged repo: nothing new to embed.
    eng.tick(&prog, true).unwrap();
    let after = scalar(
        &eng,
        "SELECT count(*) FROM _embeddings WHERE backend = 'stub'",
    );
    assert_eq!(after, total, "re-tick re-embedded unchanged content");
}

#[test]
fn embed_tick_has_no_n1() {
    // many distinct identifiers -> many _embeddings rows in one tick.
    let mut body = String::new();
    for i in 0..200 {
        body.push_str(&format!("fn func_number_{i}() {{}}\n"));
    }
    let root = write_repo("n1", &[("big.rs", &body)]);
    let prog = parse::parse(lex::lex(PROG).unwrap()).unwrap();
    let mut eng = engine_at(&root);
    eng.tick(&prog, true).unwrap();
    let n = scalar(
        &eng,
        "SELECT count(*) FROM _embeddings WHERE backend = 'stub'",
    );
    assert!(n > 64, "fixture must exceed the N+1 threshold (got {n})");
    assert!(
        eng.last_n1.is_none(),
        "embed tick tripped the N+1 detector: {:?}",
        eng.last_n1
    );
}
