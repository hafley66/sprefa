use super::*;

use crate::ast::{Col, RelDecl, Type};
use std::path::PathBuf;

fn engine() -> Engine {
    let mut engine = Engine::new(crate::db::open(None).unwrap(), PathBuf::new());
    engine.ensure_meta().unwrap();
    engine
        .declare(&RelDecl {
            name: "staged_fact".into(),
            cols: vec![
                Col::raw("name", Type::Text),
                Col::plain("count".into(), Type::Int),
            ],
            ..RelDecl::default()
        })
        .unwrap();
    engine
        .declare(&RelDecl {
            name: "keyed_fact".into(),
            cols: vec![
                Col::raw("key", Type::Text),
                Col::plain("value".into(), Type::Int),
            ],
            key: Some(vec!["key".into()]),
            ..RelDecl::default()
        })
        .unwrap();
    engine
}

fn live_rows(engine: &Engine) -> i64 {
    engine
        .db
        .conn()
        .query_row("SELECT count(*) FROM rel_staged_fact", [], |row| row.get(0))
        .unwrap()
}

fn prepared(engine: &Engine, base: [u8; 32]) -> PreparedSourceFacts {
    let mut stage = FullSourceStageBuilder::new(engine.db.conn(), 1, base).unwrap();
    stage
        .push(
            "staged_fact",
            "repo",
            "a.rs",
            0,
            &[Value::Text("alpha".into()), Value::Int(1)],
        )
        .unwrap();
    stage
        .push(
            "staged_fact",
            "repo",
            "a.rs",
            1,
            &[Value::Text("beta".into()), Value::Int(2)],
        )
        .unwrap();
    stage
        .push_where(
            "repo",
            "a.rs",
            0,
            crate::spine::WhereBytes {
                string: crate::spine::StringId::of("prepared-span"),
                repo: crate::spine::RepoId(1),
                rev: crate::spine::RevId(2),
                file: crate::spine::FileId(3),
                lo: 1,
                hi: 4,
            },
            "prepared-span".into(),
        )
        .unwrap();
    stage.complete_owner("staged_fact", "repo", "a.rs").unwrap();
    stage.complete_where_owner("repo", "a.rs").unwrap();
    stage
        .complete_owner("staged_fact", "repo", "empty.rs")
        .unwrap();
    stage.seal().unwrap()
}

#[test]
fn prepared_rows_apply_with_provenance_then_cleanup() {
    let mut engine = engine();
    let base = [7; 32];
    let prepared = prepared(&engine, base);
    let inserted = engine
        .with_semantic_generation(|engine| prepared.apply(engine, base))
        .unwrap();
    assert_eq!(inserted, 2);
    assert_eq!(live_rows(&engine), 2);
    let provenance: i64 = engine
        .db
        .conn()
        .query_row(
            "SELECT count(*) FROM _prov WHERE rel='staged_fact' AND repo='repo' AND path='a.rs'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(provenance, 2);
    prepared.discard(engine.db.conn()).unwrap();
    let staged: i64 = engine
        .db
        .conn()
        .query_row("SELECT count(*) FROM _source_stage_row", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(staged, 0);
}

#[test]
fn apply_error_rolls_back_live_rows_and_stale_base_is_refused() {
    let mut engine = engine();
    let base = [8; 32];
    let prepared = prepared(&engine, base);
    let result: anyhow::Result<()> = engine.with_semantic_generation(|engine| {
        prepared.apply(engine, base)?;
        anyhow::bail!("stop after staged insert")
    });
    assert!(result.is_err());
    assert_eq!(live_rows(&engine), 0);
    let spans: i64 = engine
        .db
        .conn()
        .query_row(
            "SELECT count(*) FROM _where_bytes WHERE repo='repo' AND path='a.rs'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let strings: i64 = engine
        .db
        .conn()
        .query_row(
            "SELECT count(*) FROM _strings WHERE content='prepared-span'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!((spans, strings), (0, 0));
    assert!(prepared.apply(&engine, [9; 32]).is_err());
    assert_eq!(live_rows(&engine), 0);
    prepared.discard(engine.db.conn()).unwrap();
}

#[test]
fn failed_builder_is_cleaned_up_on_drop() {
    let engine = engine();
    {
        let mut stage = FullSourceStageBuilder::new(engine.db.conn(), 1, [3; 32]).unwrap();
        stage
            .push("staged_fact", "repo", "a.rs", 0, &[Value::Int(1)])
            .unwrap();
    }
    let staged: i64 = engine
        .db
        .conn()
        .query_row("SELECT count(*) FROM _source_stage_row", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(staged, 0);
}

#[test]
fn located_spans_share_the_sealed_stage_and_apply_in_batches() {
    let mut engine = engine();
    let base = [5; 32];
    let mut stage = FullSourceStageBuilder::new(engine.db.conn(), 1, base).unwrap();
    stage
        .push_where(
            "repo",
            "a.rs",
            0,
            crate::spine::WhereBytes {
                string: crate::spine::StringId::of("alpha"),
                repo: crate::spine::RepoId(1),
                rev: crate::spine::RevId(2),
                file: crate::spine::FileId(3),
                lo: 4,
                hi: 9,
            },
            "alpha".into(),
        )
        .unwrap();
    stage.complete_where_owner("repo", "a.rs").unwrap();
    let prepared = stage.seal().unwrap();
    engine
        .with_semantic_generation(|engine| prepared.apply(engine, base))
        .unwrap();
    let spans: i64 = engine
        .db
        .conn()
        .query_row(
            "SELECT count(*) FROM _where_bytes WHERE repo='repo' AND path='a.rs'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let strings: i64 = engine
        .db
        .conn()
        .query_row(
            "SELECT count(*) FROM _strings WHERE content='alpha'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!((spans, strings), (1, 1));
    prepared.discard(engine.db.conn()).unwrap();
}

#[test]
fn reserved_span_rows_are_typed_and_count_toward_stage_bytes() {
    let engine = engine();
    let mut stage = FullSourceStageBuilder::new(engine.db.conn(), 1, [6; 32]).unwrap();
    assert!(stage
        .push("@where", "repo", "a.rs", 0, &[Value::Int(1)])
        .is_err());
    let result = stage.push_where(
        "repo",
        "a.rs",
        0,
        crate::spine::WhereBytes::default(),
        "x".repeat(300 * 1024),
    );
    assert!(result.is_err());
}

#[test]
fn apply_pages_past_cursor_and_keeps_extraction_order_for_keyed_facts() {
    let mut engine = engine();
    let base = [4; 32];
    let mut stage = FullSourceStageBuilder::new(engine.db.conn(), 1, base).unwrap();
    for ordinal in 0..4101u64 {
        stage
            .push(
                "staged_fact",
                "repo",
                "many.rs",
                ordinal,
                &[
                    Value::Text(format!("n{ordinal}")),
                    Value::Int(ordinal as i64),
                ],
            )
            .unwrap();
    }
    stage
        .complete_owner("staged_fact", "repo", "many.rs")
        .unwrap();
    stage
        .push(
            "keyed_fact",
            "repo",
            "z-first.rs",
            0,
            &[Value::Text("same".into()), Value::Int(1)],
        )
        .unwrap();
    stage
        .complete_owner("keyed_fact", "repo", "z-first.rs")
        .unwrap();
    stage
        .push(
            "keyed_fact",
            "repo",
            "a-second.rs",
            1,
            &[Value::Text("same".into()), Value::Int(2)],
        )
        .unwrap();
    stage
        .complete_owner("keyed_fact", "repo", "a-second.rs")
        .unwrap();
    let prepared = stage.seal().unwrap();
    engine
        .with_semantic_generation(|engine| prepared.apply(engine, base))
        .unwrap();
    assert_eq!(live_rows(&engine), 4101);
    let selected: i64 = engine
        .db
        .conn()
        .query_row(
            "SELECT value FROM rel_keyed_fact WHERE key='same'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(selected, 1);
    prepared.discard(engine.db.conn()).unwrap();
}
