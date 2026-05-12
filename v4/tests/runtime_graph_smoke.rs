use std::sync::Arc;

use effect_runtime::v2::FactStore;
use effect_runtime::v2::RenderCtx;
use tempfile::tempdir;
use v4::fact::SqliteFactStore;
use v4::mounted_query::{
    input_key_for_batch, load_mounted_sql_snapshot, mount_id_for_sql, record_sql_outputs,
};
use v4::runtime_graph::{
    RuntimeGraph, SprfActiveChild, SprfSubscribe, SprfSupportRows, RUNTIME_EDGE,
    RUNTIME_EDGE_VALUE, RUNTIME_EVENT, RUNTIME_JOB, RUNTIME_NODE, RUNTIME_VALUE,
};
use v4::store::SprfStore;
use v4::{Cursor, WhereBytes, WhereBytesId};

fn sqlite_graph(path: &std::path::Path) -> (Arc<dyn FactStore<Cursor>>, RuntimeGraph) {
    let facts: Arc<dyn FactStore<Cursor>> =
        Arc::new(SqliteFactStore::<Cursor>::open_file(path).unwrap());
    let store = SprfStore::new(facts.clone());
    let graph = RuntimeGraph::new(store, facts.clone());
    (facts, graph)
}

fn row(name: &str, value: &str) -> Cursor {
    let mut cursor = Cursor::default();
    cursor.set("name", name);
    cursor.set("value", value);
    cursor
}

#[test]
fn mounted_sql_inputs_are_durable_for_graph_job_rerun() {
    let tmp = tempdir().unwrap();
    let db = tmp.path().join("runtime.db");
    let (facts, _graph) = sqlite_graph(&db);
    let first = row("a", "1");
    let second = row("b", "2");
    let batch = vec![&first, &second];
    let sql = "SELECT input.__cursor_idx, input.name FROM input";

    record_sql_outputs(&facts, sql, 1, &batch, &["source_table".to_string()], &[]);
    let mount_id = mount_id_for_sql(sql);
    let input_key = input_key_for_batch(&batch);

    drop(facts);

    let (facts, _graph) = sqlite_graph(&db);
    let snapshot = load_mounted_sql_snapshot(facts.as_ref(), &mount_id, &input_key)
        .expect("mounted sql input snapshot should reopen");

    assert_eq!(snapshot.sql, sql);
    assert_eq!(snapshot.dep_tables, vec!["source_table".to_string()]);
    assert_eq!(snapshot.inputs, vec![first, second]);
}

#[test]
fn runtime_graph_identity_and_rows_survive_sqlite_reopen() {
    let tmp = tempdir().unwrap();
    let db = tmp.path().join("runtime.db");

    let (facts, graph) = sqlite_graph(&db);
    let owner = graph.declare_owner(
        "sprf://ast/rule/main",
        None,
        "input:a",
        "source:head",
        "pure",
        1,
    );
    let same_owner = graph.declare_owner(
        "sprf://ast/rule/main",
        None,
        "input:a",
        "source:head",
        "pure",
        2,
    );
    let other_owner = graph.declare_owner(
        "sprf://ast/rule/main",
        None,
        "input:b",
        "source:head",
        "pure",
        1,
    );
    let table = graph.declare_output_table("sprf://table/warnings", 1);
    let first_delta = graph.replace_supports(&owner, &table, &[row("a", "1")], 1);

    assert_eq!(owner.uri_id, same_owner.uri_id);
    assert_ne!(owner.uri_id, other_owner.uri_id);
    assert_eq!(first_delta.inserted.len(), 1);
    assert_eq!(first_delta.retracted.len(), 0);
    assert_eq!(facts.len(RUNTIME_NODE), 4);

    drop(graph);
    drop(facts);

    let (facts, graph) = sqlite_graph(&db);

    assert_eq!(facts.len(RUNTIME_NODE), 4);
    assert_eq!(facts.len(RUNTIME_EDGE), 2);
    assert_eq!(facts.len(RUNTIME_VALUE), 0);

    let repeat_delta = graph.replace_supports(&owner, &table, &[row("a", "1")], 2);
    assert!(repeat_delta.inserted.is_empty());
    assert!(repeat_delta.retracted.is_empty());
}

#[test]
fn support_reconciliation_keeps_shared_rows_until_final_support_retracts() {
    let tmp = tempdir().unwrap();
    let db = tmp.path().join("runtime.db");
    let (_facts, graph) = sqlite_graph(&db);

    let owner_a = graph.declare_owner("sprf://ast/a", None, "input", "head", "pure", 1);
    let owner_b = graph.declare_owner("sprf://ast/b", None, "input", "head", "pure", 1);
    let table = graph.declare_output_table("sprf://table/warnings", 1);
    let shared = row("shared", "same");

    let a_insert = graph.replace_supports(&owner_a, &table, &[shared.clone()], 1);
    let b_insert = graph.replace_supports(&owner_b, &table, &[shared.clone()], 1);
    let a_remove = graph.replace_supports(&owner_a, &table, &[], 2);
    let b_remove = graph.replace_supports(&owner_b, &table, &[], 3);

    assert_eq!(a_insert.inserted.len(), 1);
    assert!(b_insert.inserted.is_empty());
    assert!(a_remove.retracted.is_empty());
    assert_eq!(b_remove.retracted.len(), 1);
}

#[test]
fn wake_dispatch_uses_subscribe_edges_as_separate_readiness_slots() {
    let tmp = tempdir().unwrap();
    let db = tmp.path().join("runtime.db");
    let (facts, graph) = sqlite_graph(&db);

    let owner = graph.declare_owner("sprf://ast/combine", None, "input", "head", "pure", 1);
    let source = graph.declare_source("sprf://source/table/a", 1);
    let left = graph.subscribe(&owner, "left", &source, 1);
    let right = graph.subscribe(&owner, "right", &source, 1);
    let value = graph.runtime_value_cursor_blob("sprf://value/a/1", &row("a", "1"), "ready", 1);

    let woken = graph.dispatch_wake(&source, &value, 1);
    let edge_values = facts.rows_of(RUNTIME_EDGE_VALUE);
    let labels: Vec<String> = edge_values
        .iter()
        .filter_map(|row| row.get("label_id").map(str::to_string))
        .collect();

    assert_eq!(woken, vec![owner.clone()]);
    assert_ne!(left.uri_id, right.uri_id);
    assert_eq!(facts.len(RUNTIME_EVENT), 1);
    assert_eq!(edge_values.len(), 2);
    assert!(labels.contains(&left.label_id.0.to_string()));
    assert!(labels.contains(&right.label_id.0.to_string()));
}

#[test]
fn active_child_and_source_located_values_are_durable() {
    let tmp = tempdir().unwrap();
    let db = tmp.path().join("runtime.db");
    let (facts, graph) = sqlite_graph(&db);

    let parent = graph.declare_owner("sprf://ast/switch", None, "input", "head", "pure", 1);
    let old_child = graph.declare_owner(
        "sprf://ast/inner",
        Some(parent.uri.as_ref()),
        "old",
        "head",
        "pure",
        1,
    );
    let new_child = graph.declare_owner(
        "sprf://ast/inner",
        Some(parent.uri.as_ref()),
        "new",
        "head",
        "pure",
        2,
    );

    graph.replace_active_child(&parent, "active-inner", &old_child, 1);
    let active = graph.replace_active_child(&parent, "active-inner", &new_child, 2);

    let source_text = graph.store.intern_string("source bytes");
    let where_bytes = graph.store.intern_where_bytes(WhereBytes {
        string: source_text,
        repo: 1,
        rev: 2,
        file: 3,
        lo: 4,
        hi: 16,
    });
    let value = graph.runtime_value_where_bytes("sprf://value/source/a", where_bytes, "ready", 2);

    drop(graph);
    drop(facts);

    let (facts, _graph) = sqlite_graph(&db);
    let active_rows = facts.read_where(RUNTIME_EDGE, "edge_uri_id", &active.uri_id.0.to_string());
    let value_rows = facts.read_where(RUNTIME_VALUE, "value_uri_id", &value.uri_id.0.to_string());

    assert_eq!(active_rows.len(), 1);
    assert_eq!(
        active_rows[0].get("to_uri_id"),
        Some(new_child.uri_id.0.to_string().as_str())
    );
    assert_eq!(value_rows.len(), 1);
    assert_eq!(
        value_rows[0].get("value_ref_id"),
        Some(WhereBytesId(where_bytes.0).0.to_string().as_str())
    );
    assert_eq!(value_rows[0].get("value_blob"), Some(""));
}

#[test]
fn resume_finds_unconsumed_events_and_consumption_is_durable() {
    let tmp = tempdir().unwrap();
    let db = tmp.path().join("runtime.db");
    let (_facts, graph) = sqlite_graph(&db);

    let owner = graph.declare_owner("sprf://ast/rss-rule", None, "input", "head", "impure", 1);
    let source = graph.declare_source("sprf://source/rss/feed", 1);
    graph.subscribe(&owner, "rss", &source, 1);
    let value =
        graph.runtime_value_cursor_blob("sprf://value/rss/1", &row("entry", "a"), "ready", 1);
    graph.dispatch_wake(&source, &value, 1);

    drop(graph);

    let (_facts, graph) = sqlite_graph(&db);
    let events = graph.unconsumed_events();
    let owners = graph.owners_for_unconsumed_events();

    assert_eq!(events.len(), 1);
    assert_eq!(owners, vec![owner]);

    graph.mark_event_consumed(&events[0]);
    drop(graph);

    let (_facts, graph) = sqlite_graph(&db);
    assert!(graph.unconsumed_events().is_empty());
    assert!(graph.owners_for_unconsumed_events().is_empty());
}

#[test]
fn graph_jobs_are_durable_and_idempotent_work_items() {
    let tmp = tempdir().unwrap();
    let db = tmp.path().join("runtime.db");
    let (facts, graph) = sqlite_graph(&db);

    let owner = graph.declare_owner("sprf://ast/sql", None, "input", "head", "sql", 1);
    let source = graph.declare_source("sprf://source/table/hooks", 1);
    graph.subscribe(&owner, "hooks", &source, 1);
    let value = graph.runtime_value_dirty(&source, 1);

    graph.dispatch_wake(&source, &value, 1);
    graph.enqueue_jobs_for_unconsumed_events(2);
    let jobs = graph.pending_jobs();

    assert_eq!(facts.len(RUNTIME_JOB), 1);
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].owner_uri_id, owner.uri_id);

    drop(graph);
    drop(facts);

    let (facts, graph) = sqlite_graph(&db);
    graph.enqueue_jobs_for_unconsumed_events(3);
    let jobs = graph.pending_jobs();

    assert_eq!(facts.len(RUNTIME_JOB), 1);
    assert_eq!(jobs.len(), 1);

    graph.mark_job_done(&jobs[0]);
    assert!(graph.pending_jobs().is_empty());
    assert!(graph.unconsumed_events().is_empty());
}

#[test]
fn reactive_operator_harnesses_use_edge_local_state_and_active_edges() {
    let tmp = tempdir().unwrap();
    let db = tmp.path().join("runtime.db");
    let (_facts, graph) = sqlite_graph(&db);

    let owner = graph.declare_owner("sprf://ast/reactive", None, "input", "head", "pure", 1);
    let source_a = graph.declare_source("sprf://source/table/a", 1);
    let source_b = graph.declare_source("sprf://source/table/b", 1);
    let left = graph.subscribe(&owner, "left", &source_a, 1);
    let right = graph.subscribe(&owner, "right", &source_b, 1);
    let a_value = graph.runtime_value_cursor_blob("sprf://value/a", &row("a", "1"), "ready", 1);
    let b_value = graph.runtime_value_cursor_blob("sprf://value/b", &row("b", "2"), "ready", 1);

    graph.dispatch_wake(&source_a, &a_value, 1);
    assert_eq!(
        graph.edge_values(&[left.clone(), right.clone()]),
        vec![Some(a_value.clone()), None]
    );

    let merge_wake = graph.dispatch_wake(&source_b, &b_value, 2);
    assert_eq!(merge_wake, vec![owner.clone()]);
    assert_eq!(
        graph.edge_values(&[left.clone(), right.clone()]),
        vec![Some(a_value), Some(b_value)]
    );

    let table = graph.declare_output_table("sprf://table/reactive", 1);
    let old_child = graph.declare_owner(
        "sprf://ast/inner",
        Some(owner.uri.as_ref()),
        "old",
        "head",
        "pure",
        1,
    );
    let new_child = graph.declare_owner(
        "sprf://ast/inner",
        Some(owner.uri.as_ref()),
        "new",
        "head",
        "pure",
        2,
    );
    graph.replace_supports(&old_child, &table, &[row("old", "visible")], 1);
    graph.replace_active_child(&owner, "active-inner", &old_child, 1);
    graph.replace_active_child(&owner, "active-inner", &new_child, 2);

    assert_eq!(graph.active_child(&owner, "active-inner"), Some(new_child));
    assert!(graph
        .replace_supports(&old_child, &table, &[], 3)
        .retracted
        .is_empty());
}

#[test]
fn sprf_runtime_effects_can_be_declared_through_render_ctx_put() {
    let tmp = tempdir().unwrap();
    let db = tmp.path().join("runtime.db");
    let (_facts, graph) = sqlite_graph(&db);
    let graph = Arc::new(graph);
    let ctx = RenderCtx::new(1, 2, 3).with_runtime(graph.clone());

    let owner = graph.declare_owner("sprf://ast/ctx-put", None, "input", "head", "pure", 1);
    let source = graph.declare_source("sprf://source/ctx", 1);
    let table = graph.declare_output_table("sprf://table/ctx", 1);
    let child = graph.declare_owner(
        "sprf://ast/child",
        Some(owner.uri.as_ref()),
        "input",
        "head",
        "pure",
        1,
    );

    let sub = ctx.put(SprfSubscribe::new(owner.clone(), "left", source.clone(), 1));
    let delta = ctx.put(SprfSupportRows::new(
        owner.clone(),
        table,
        vec![row("ctx", "put")],
        1,
    ));
    let active = ctx.put(SprfActiveChild::new(
        owner.clone(),
        "active-inner",
        child.clone(),
        1,
    ));

    assert_eq!(sub.label_id, graph.store.intern_string("left"));
    assert_eq!(delta.inserted.len(), 1);
    assert_ne!(active.uri_id, child.uri_id);
    assert_eq!(graph.active_child(&owner, "active-inner"), Some(child));
}
