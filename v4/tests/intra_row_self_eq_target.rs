//! Target test (RED first) for the `?`-then-same-name-ref intra-row
//! self-equality desugar.
//!
//! When a rule-read call BINDS a term with `?` and then, LATER IN THE
//! SAME CALL, READS that same term name, the read must mean
//! "this row's other column equals this row's bound column"
//! (intra-row self-equality: `__rule.colB = __rule.colA`), NOT a
//! correlated read against the streaming `input` cursor
//! (`__rule.colB = input.term`). The latter is what the pre-desugar
//! lowering emits and it fails at SQL time with
//! `no such column: input.<term>` because the term is born in this
//! very read — there is no prior cursor column to correlate against.
//!
//! Pins:
//!   A. positional  `reaches?(N?, N)`  ⇒ rows where SRC == DST.
//!   B. kwarg        `pair?(X: V?, Y: V)` ⇒ rows where X == Y
//!      (map keys on the resolved TERM name `V`, not the column).
//!   C. inert        `pair?(X?, Y: "2")` literal RHS untouched — the
//!      BoundLiteral path must not be perturbed by the new branch.
//!
//! The desugar is strictly additive: A/B currently produce
//! `no such column: input.N` / `input.V`; C already works and must
//! keep working byte-for-byte.

use std::fs;
use std::sync::Arc;

use v4::app::{
    build_router, GetFactTableReq, InProcessClient, RunReq, SprfClient, SprfState,
};

fn col1(t: &v4::app::FactTable, c: &str) -> Vec<String> {
    let mut v: Vec<String> = t
        .rows
        .iter()
        .map(|r| {
            r.fields
                .iter()
                .find(|(k, _)| k == c)
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        })
        .collect();
    v.sort();
    v.dedup();
    v
}

async fn run(src: &str) -> InProcessClient {
    let dir = tempfile::tempdir().unwrap();
    let sprf = dir.path().join("g.sprf");
    let fact_db = dir.path().join("facts.db");
    fs::write(&sprf, src).unwrap();
    let client = InProcessClient::new(build_router(Arc::new(
        SprfState::new_with_sqlite_facts(dir.path().to_path_buf(), &fact_db),
    )));
    client
        .run(RunReq {
            path: sprf.clone(),
            root: Some(dir.path().to_path_buf()),
        })
        .await
        .expect("run ok");
    // Keep `dir` alive for the duration of the client by leaking it; the
    // test process is short-lived and the temp dir is cleaned by the OS.
    std::mem::forget(dir);
    client
}

async fn table(client: &InProcessClient, name: &str) -> v4::app::FactTable {
    client
        .get_fact_table(GetFactTableReq {
            name: name.into(),
            limit: None,
        })
        .await
        .unwrap()
}

const SEED_REACHES: &str = r#"
rule(:reaches, SRC?, DST?);
`a|a
a|b
b|c
c|c`
  > split`
`
  > re`(?P<SRC>[^|]+)\|(?P<DST>.+)`
  > reaches(SRC, DST);
"#;

const SEED_PAIR: &str = r#"
rule(:pair, X?, Y?);
`1|1
1|2
3|3`
  > split`
`
  > re`(?P<X>[^|]+)\|(?P<Y>.+)`
  > pair(X, Y);
"#;

// A. positional `?`-then-ref → intra-row self-equality.
#[tokio::test]
async fn positional_qmark_then_ref_is_intra_row_self_eq() {
    let src = format!(
        "{SEED_REACHES}\nrule(:selfreach, N?);\nreaches?(N?, N) > selfreach(N);\n"
    );
    let client = run(&src).await;
    let got = col1(&table(&client, "selfreach").await, "N");
    assert_eq!(
        got,
        vec!["a".to_string(), "c".to_string()],
        "reaches?(N?, N) must keep only rows where SRC == DST (a|a, c|c)"
    );
}

// B. kwarg `?`-then-ref keys on the resolved TERM name, not the column.
#[tokio::test]
async fn kwarg_qmark_then_ref_keys_on_term_name() {
    let src = format!(
        "{SEED_PAIR}\nrule(:eqp, V?);\npair?(X: V?, Y: V) > eqp(V);\n"
    );
    let client = run(&src).await;
    let got = col1(&table(&client, "eqp").await, "V");
    assert_eq!(
        got,
        vec!["1".to_string(), "3".to_string()],
        "pair?(X: V?, Y: V) must keep only rows where X == Y (1|1, 3|3)"
    );
}

// C. inert: a literal RHS is unaffected by the new branch.
#[tokio::test]
async fn literal_rhs_unaffected() {
    let src = format!(
        "{SEED_PAIR}\nrule(:litp, X?);\npair?(X?, Y: \"2\") > litp(X);\n"
    );
    let client = run(&src).await;
    let got = col1(&table(&client, "litp").await, "X");
    assert_eq!(
        got,
        vec!["1".to_string()],
        "pair?(X?, Y: \"2\") must keep only the row whose Y literal is 2 (1|2)"
    );
}
