use std::sync::Arc;

use ast_grep_language::SupportLang;
use effect_runtime::v2::{
    expand, Component, EventBus, ExpandOpts, FactStore, MemQueue, PipeInstance, QueueBackend,
    RenderCtx,
};
use tempfile::tempdir;
use v4::fact::FactWrite;
use v4::fact::SqliteFactStore;
use v4::mounted_query::{
    input_key_for_batch, load_mounted_sql_snapshot, mount_id_for_sql, record_sql_outputs,
};
use v4::runtime_graph::{
    table_source_uri, RuntimeGraph, SourceSubscriptionInput, SprfSubscribe, SprfSupportRows,
    RUNTIME_EDGE, RUNTIME_EVENT, RUNTIME_JOB, RUNTIME_NODE,
};
use v4::runtime_replay::GraphReplayRunner;
use v4::store::SprfStore;
use v4::v2_ops::AstNmComponent;
use v4::Cursor;

fn sqlite_graph(path: &std::path::Path) -> (Arc<dyn FactStore<Cursor>>, RuntimeGraph) {
    let facts: Arc<dyn FactStore<Cursor>> =
        Arc::new(SqliteFactStore::<Cursor>::open_file(path).unwrap());
    let store = SprfStore::new(facts.clone());
    let graph = RuntimeGraph::new(store, facts.clone());
    (facts, graph)
}

fn sqlite_compact_graph(path: &std::path::Path) -> (Arc<dyn FactStore<Cursor>>, RuntimeGraph) {
    let facts: Arc<dyn FactStore<Cursor>> =
        Arc::new(SqliteFactStore::<Cursor>::open_file(path).unwrap());
    let store = SprfStore::new(facts.clone());
    let graph = RuntimeGraph::new_with_compact_sources(store, facts.clone(), path);
    (facts, graph)
}

fn row(name: &str, value: &str) -> Cursor {
    let mut cursor = Cursor::default();
    cursor.set("name", name);
    cursor.set("value", value);
    cursor
}

fn path_cursor(path: &str) -> Cursor {
    let mut cursor = Cursor::default();
    cursor.value = Arc::from(path);
    cursor.set("FS", path);
    cursor
}

#[test]
fn compact_source_graph_exposes_write_stats_snapshot() {
    let tmp = tempdir().unwrap();
    let db = tmp.path().join("runtime.db");
    let (_facts, graph) = sqlite_compact_graph(&db);
    let input = path_cursor("a.c");

    graph.record_source_subscriptions(
        &[SourceSubscriptionInput {
            ast_uri: "sprf://ast/source/test/1/0".to_string(),
            input_key: "input:a".to_string(),
            source_uri: "sprf://source/file/a.c".to_string(),
            label: "file".to_string(),
            pipe_hash: 1,
            instance_id: 2,
            depth: 3,
            input: &input,
        }],
        4,
    );

    let stats = graph.stats().expect("compact source stats");
    assert_eq!(stats.name, "runtime_graph.compact_source_subscriptions");
    assert_eq!(stats.calls, 1);
    assert_eq!(stats.rows, 1);
    assert_eq!(stats.transactions, 1);
    assert!(stats.bytes > 0);
    assert!(stats.wall_ns > 0);
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
    // Two subscribe edges on the same owner, same source: a single wake
    // appends one event and wakes one owner regardless of edge cardinality.
    // Per-edge value slots (RUNTIME_EDGE_VALUE) were dropped in slice 2.
    let tmp = tempdir().unwrap();
    let db = tmp.path().join("runtime.db");
    let (facts, graph) = sqlite_graph(&db);

    let owner = graph.declare_owner("sprf://ast/combine", None, "input", "head", "pure", 1);
    let source = graph.declare_source("sprf://source/table/a", 1);
    let left = graph.subscribe(&owner, "left", &source, 1);
    let right = graph.subscribe(&owner, "right", &source, 1);

    let woken = graph.dispatch_wake(&source, 1);

    assert_eq!(woken, vec![owner.clone()]);
    assert_ne!(left.uri_id, right.uri_id);
    assert_eq!(facts.len(RUNTIME_EVENT), 1);
}

#[test]
fn resume_finds_unconsumed_events_and_consumption_is_durable() {
    let tmp = tempdir().unwrap();
    let db = tmp.path().join("runtime.db");
    let (_facts, graph) = sqlite_graph(&db);

    let owner = graph.declare_owner("sprf://ast/rss-rule", None, "input", "head", "impure", 1);
    let source = graph.declare_source("sprf://source/rss/feed", 1);
    graph.subscribe(&owner, "rss", &source, 1);
    graph.dispatch_wake(&source, 1);
    graph.store.flush();

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

    graph.dispatch_wake(&source, 1);
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
fn sprf_runtime_effects_can_be_declared_through_render_ctx_put() {
    let tmp = tempdir().unwrap();
    let db = tmp.path().join("runtime.db");
    let (_facts, graph) = sqlite_graph(&db);
    let graph = Arc::new(graph);
    let ctx = RenderCtx::new(1, 2, 3).with_runtime(graph.clone());

    let owner = graph.declare_owner("sprf://ast/ctx-put", None, "input", "head", "pure", 1);
    let source = graph.declare_source("sprf://source/ctx", 1);
    let table = graph.declare_output_table("sprf://table/ctx", 1);

    let sub = ctx.put(SprfSubscribe::new(owner.clone(), "left", source, 1));
    ctx.put(SprfSupportRows::new(
        owner.clone(),
        table.clone(),
        vec![row("ctx", "put")],
        1,
    ));

    assert_eq!(sub.label_id, graph.store.intern_string("left"));
    // Support edge from the put landed in the graph.
    let support_edges = graph
        .replace_supports(&owner, &table, &[row("ctx", "put")], 2)
        .inserted;
    assert!(support_edges.is_empty(), "row was already inserted via put");
}

#[test]
fn table_source_uri_is_deterministic_for_table_name() {
    // Same table -> same URI; different table -> different URI.
    let a = table_source_uri("hits");
    let b = table_source_uri("hits");
    let c = table_source_uri("misses");
    assert_eq!(a, b);
    assert_ne!(a, c);
    assert!(a.starts_with("sprf://source/table/"));
}

#[test]
fn notify_table_inserted_enqueues_jobs_for_subscribed_owners() {
    let tmp = tempdir().unwrap();
    let db = tmp.path().join("runtime.db");
    let (facts, graph) = sqlite_graph(&db);

    // Owner subscribed to a user fact table by its canonical source URI.
    let owner = graph.declare_owner("sprf://ast/rule/x", None, "input", "head", "pure", 1);
    let source = graph.declare_table_source("hits", 1);
    graph.subscribe(&owner, "hits", &source, 1);

    // Pretend a user-level FactStore::insert_batch landed rows in "hits";
    // the runtime notifies the graph that the table is dirty.
    let new_jobs = graph.notify_table_inserted("hits", 2);

    assert_eq!(new_jobs.len(), 1);
    assert_eq!(new_jobs[0].owner_uri_id, owner.uri_id);
    let pending = graph.pending_jobs();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].owner_uri_id, owner.uri_id);

    // Second notify with no new subscribers is idempotent: one event per
    // (gen, source), one job per (event, owner).
    let again = graph.notify_table_inserted("hits", 2);
    assert!(again.is_empty() || again[0].uri_id == new_jobs[0].uri_id);
    assert_eq!(facts.len(RUNTIME_JOB), 1);

    // A fresh generation creates a new event and therefore a new job.
    let next_gen_jobs = graph.notify_table_inserted("hits", 3);
    assert_eq!(next_gen_jobs.len(), 1);
    assert_ne!(next_gen_jobs[0].uri_id, new_jobs[0].uri_id);
}

#[test]
fn source_aware_ast_subscriptions_reopen_and_rerun_one_changed_file() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().join("src");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("a.c"), "int a(void) { return 1; }\n").unwrap();
    std::fs::write(root.join("b.c"), "void b(void) { printk(\"b\"); }\n").unwrap();
    let db = tmp.path().join("runtime.db");
    let (facts, graph) = sqlite_graph(&db);
    let graph = Arc::new(graph);
    facts.declare("hits", &["FS", "LO", "HI"]);

    let ast: Arc<dyn effect_runtime::v2::Component<Next = Cursor>> = Arc::new(
        AstNmComponent::new("printk($$$)".to_string(), SupportLang::Cpp)
            .with_root(root.clone())
            .with_sprf_store(graph.store.clone()),
    );
    let write: Arc<dyn effect_runtime::v2::Component<Next = Cursor>> =
        Arc::new(FactWrite::new(facts.clone(), "hits"));
    let pipe = Arc::new(PipeInstance::new(vec![ast, write]));
    let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());

    expand(
        pipe.as_ref(),
        queue.clone(),
        vec![Arc::new(path_cursor("a.c")), Arc::new(path_cursor("b.c"))],
        ExpandOpts::default()
            .with_batch_cap(10)
            .with_runtime(graph.clone()),
    );
    graph.store.flush();
    facts.commit(1, None);

    assert_eq!(facts.len("hits"), 1);
    assert_eq!(facts.len(RUNTIME_EDGE), 2);
    assert_eq!(facts.len("runtime_continuation"), 2);

    drop(facts);
    std::fs::write(root.join("a.c"), "void a(void) { printk(\"a\"); }\n").unwrap();

    let (facts, graph) = sqlite_graph(&db);
    let graph = Arc::new(graph);
    let source = graph.declare_source("sprf://source/file/a.c", 2);
    graph.dispatch_dirty(&source, 2);
    let jobs = graph.pending_jobs();
    assert_eq!(jobs.len(), 1);

    let runner = GraphReplayRunner {
        facts: facts.clone(),
        queue,
        bus: Arc::new(EventBus::new()),
        sprf_store: graph.store.clone(),
        runtime_graph: graph,
        instances: vec![pipe],
    };

    assert_eq!(runner.drain(), 1);
    assert_eq!(facts.len("hits"), 2);
    assert!(runner.runtime_graph.pending_jobs().is_empty());
}

#[test]
fn fact_write_auto_notifies_subscribed_owners() {
    // FactWrite::render_batch must wake every owner subscribed to the
    // canonical table-source for the written table without an explicit
    // notify_table_inserted call from the driver.
    let tmp = tempdir().unwrap();
    let db = tmp.path().join("runtime.db");
    let (facts, graph) = sqlite_graph(&db);
    let graph = Arc::new(graph);
    facts.declare("T", &["k", "v"]);

    let owner = graph.declare_owner("sprf://ast/sub", None, "input", "head", "pure", 1);
    let source = graph.declare_table_source("T", 1);
    graph.subscribe(&owner, "T", &source, 1);

    let before = graph.pending_jobs().len();

    let write: Arc<dyn effect_runtime::v2::Component<Next = Cursor>> =
        Arc::new(FactWrite::new(facts.clone(), "T"));
    let pipe = PipeInstance::new(vec![write]);
    let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    let mut seed = Cursor::default();
    seed.set("k", "k1");
    seed.set("v", "v1");
    expand(
        &pipe,
        queue,
        vec![Arc::new(seed)],
        ExpandOpts::default().with_runtime(graph.clone()),
    );

    let after = graph.pending_jobs();
    assert_eq!(facts.len("T"), 1);
    assert_eq!(after.len(), before + 1);
    assert_eq!(after[0].owner_uri_id, owner.uri_id);
}

#[test]
fn rule_invoke_memoizes_owner_by_input_arg_tuple() {
    use effect_runtime::v2::{MemFactStore, Pipe};
    use v4::rule::{Rule, RuleInvokeAssign, RuleInvokeComponent, RuleInvokeValue};
    use v4::runtime_graph::ArgKey;

    let tmp = tempdir().unwrap();
    let db = tmp.path().join("runtime.db");
    let (_facts, graph) = sqlite_graph(&db);
    let graph = Arc::new(graph);
    let user_store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());

    let body = Pipe::new();
    let rule = Rule::new("foo", user_store.clone(), "foo", &["A", "B"], body);

    // foo("a", X?) — A literal "a", B unbound (Term against a key that
    // isn't on the cursor).
    let assignments = vec![
        RuleInvokeAssign {
            col: Arc::<str>::from("A"),
            value: RuleInvokeValue::Literal(Arc::<str>::from("a")),
        },
        RuleInvokeAssign {
            col: Arc::<str>::from("B"),
            value: RuleInvokeValue::Term(Arc::<str>::from("MISSING")),
        },
    ];
    let call = RuleInvokeComponent::new(rule.clone(), assignments.clone(), false);

    let seed = Cursor::default();
    let ctx = RenderCtx::new(0, 0, 0).with_runtime(graph.clone());

    let _ = call.render(&ctx, &seed);
    let _ = call.render(&ctx, &seed);

    assert_eq!(graph.rule_memo_len(), 1);
    let rule_id = graph.store.intern_string("foo");
    let arg_a = ArgKey::Literal(graph.store.intern_string("a"));
    let arg_b = ArgKey::Unbound;
    let owner_first = graph
        .rule_memo_get(rule_id, &[arg_a, arg_b])
        .expect("memoized owner");

    let _ = call.render(&ctx, &seed);
    let owner_again = graph
        .rule_memo_get(rule_id, &[arg_a, arg_b])
        .expect("still memoized");
    assert_eq!(owner_first.uri_id, owner_again.uri_id);
    assert_eq!(owner_first.uri, owner_again.uri);

    let other_assignments = vec![
        RuleInvokeAssign {
            col: Arc::<str>::from("A"),
            value: RuleInvokeValue::Literal(Arc::<str>::from("b")),
        },
        RuleInvokeAssign {
            col: Arc::<str>::from("B"),
            value: RuleInvokeValue::Term(Arc::<str>::from("MISSING")),
        },
    ];
    let other_call = RuleInvokeComponent::new(rule, other_assignments, false);
    let _ = other_call.render(&ctx, &seed);
    let arg_a2 = ArgKey::Literal(graph.store.intern_string("b"));
    let owner_other = graph
        .rule_memo_get(rule_id, &[arg_a2, arg_b])
        .expect("second-tuple owner");
    assert_ne!(owner_first.uri_id, owner_other.uri_id);
    assert_eq!(graph.rule_memo_len(), 2);
}

#[test]
fn rule_invoke_body_skips_on_cache_hit_and_refires_on_source_dirty() {
    use effect_runtime::v2::{MemFactStore, Node, Pipe};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use v4::rule::{Rule, RuleInvokeAssign, RuleInvokeComponent, RuleInvokeValue};
    use v4::runtime_graph::ArgKey;

    // Body counter: how many times the rule body actually ran.
    struct CountBody {
        count: Arc<AtomicUsize>,
    }
    impl Component for CountBody {
        type Next = Cursor;
        fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
            self.count.fetch_add(1, Ordering::SeqCst);
            Node::Emit(Arc::new(c.clone()))
        }
    }

    let tmp = tempdir().unwrap();
    let db = tmp.path().join("runtime.db");
    let (_facts, graph) = sqlite_graph(&db);
    let graph = Arc::new(graph);
    let user_store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());

    let count = Arc::new(AtomicUsize::new(0));
    let body = Pipe::new().step(Arc::new(CountBody {
        count: count.clone(),
    }));
    let rule = Rule::new("r", user_store.clone(), "r", &["A"], body);

    let assignments = vec![RuleInvokeAssign {
        col: Arc::<str>::from("A"),
        value: RuleInvokeValue::Literal(Arc::<str>::from("a")),
    }];
    let call = RuleInvokeComponent::new(rule, assignments, false);

    let seed = Cursor::default();

    // First fire at gen=1.
    let ctx1 = RenderCtx::new(1, 0, 0).with_runtime(graph.clone());
    let _ = call.render(&ctx1, &seed);
    assert_eq!(count.load(Ordering::SeqCst), 1, "first fire runs body");

    // Subscribe the memoized owner to a "src" table source so a dirty
    // there will enqueue a job for the owner.
    let rule_id = graph.store.intern_string("r");
    let arg_keys_one = [ArgKey::Literal(graph.store.intern_string("a"))];
    let owner = graph
        .rule_memo_get(rule_id, &arg_keys_one)
        .expect("owner memoized after first fire");
    let src = graph.declare_table_source("src", 1);
    graph.subscribe(&owner, "src", &src, 1);

    // Second fire at gen=2, no dirty in between: body must NOT re-run.
    let ctx2 = RenderCtx::new(2, 0, 0).with_runtime(graph.clone());
    let _ = call.render(&ctx2, &seed);
    assert_eq!(count.load(Ordering::SeqCst), 1, "cache hit skips body");

    // Mutate the source table — fires notify, enqueues a job at gen=3.
    graph.notify_table_inserted("src", 3);
    assert!(graph
        .pending_jobs()
        .iter()
        .any(|job| job.owner_uri_id == owner.uri_id && job.generation == 3));

    // Third fire at gen=4: body re-runs because owner has a pending
    // dirty newer than the cached fire's gen.
    let ctx3 = RenderCtx::new(4, 0, 0).with_runtime(graph.clone());
    let _ = call.render(&ctx3, &seed);
    assert_eq!(
        count.load(Ordering::SeqCst),
        2,
        "source dirty invalidates cache"
    );
}

#[test]
fn run_to_quiescence_drains_three_level_chain() {
    // Chain: source-a inserts -> wake owner-a -> a writes b -> wake
    // owner-b -> b writes c -> wake owner-c. The handler closure
    // implements each owner's effect inline; the cascade pumps through
    // run_to_quiescence until no jobs remain.
    let tmp = tempdir().unwrap();
    let db = tmp.path().join("runtime.db");
    let (facts, graph) = sqlite_graph(&db);
    let graph = Arc::new(graph);

    facts.declare("a", &["k"]);
    facts.declare("b", &["k"]);
    facts.declare("c", &["k"]);

    let owner_a = graph.declare_owner("sprf://ast/rule/a", None, "input", "head", "pure", 1);
    let owner_b = graph.declare_owner("sprf://ast/rule/b", None, "input", "head", "pure", 1);
    let owner_c = graph.declare_owner("sprf://ast/rule/c", None, "input", "head", "pure", 1);

    let src_a = graph.declare_table_source("a", 1);
    let src_b = graph.declare_table_source("b", 1);
    let src_c = graph.declare_table_source("c", 1);
    graph.subscribe(&owner_a, "a", &src_a, 1);
    graph.subscribe(&owner_b, "b", &src_b, 1);
    graph.subscribe(&owner_c, "c", &src_c, 1);

    // Seed: write to "a", which fires notify_table_inserted -> owner_a
    // gets a job.
    let mut row_a = Cursor::default();
    row_a.set("k", "v");
    facts.insert_batch("a", vec![Arc::new(row_a)]);
    graph.notify_table_inserted("a", 2);

    let owner_a_id = owner_a.uri_id;
    let owner_b_id = owner_b.uri_id;
    let owner_c_id = owner_c.uri_id;
    let facts_for_handler = facts.clone();
    let graph_for_handler = graph.clone();

    let handled = graph
        .run_to_quiescence(10, move |job| {
            // Each owner's handler: read the upstream row count, write
            // a single derived row to the next table, notify.
            if job.owner_uri_id == owner_a_id {
                let mut row_b = Cursor::default();
                row_b.set("k", "v");
                facts_for_handler.insert_batch("b", vec![Arc::new(row_b)]);
                graph_for_handler.notify_table_inserted("b", job.generation + 1);
            } else if job.owner_uri_id == owner_b_id {
                let mut row_c = Cursor::default();
                row_c.set("k", "v");
                facts_for_handler.insert_batch("c", vec![Arc::new(row_c)]);
                graph_for_handler.notify_table_inserted("c", job.generation + 1);
            } else if job.owner_uri_id == owner_c_id {
                // Terminal — nothing more to fire.
            }
        })
        .expect("quiescence");

    assert_eq!(handled, 3, "three jobs across three chain levels");
    assert_eq!(facts.len("a"), 1);
    assert_eq!(facts.len("b"), 1);
    assert_eq!(facts.len("c"), 1);
    assert!(graph.pending_jobs().is_empty(), "no jobs pending at quiescence");
}

#[test]
fn run_to_quiescence_errors_on_iteration_cap() {
    // Handler that re-fires the same source forever. Cap kicks in.
    let tmp = tempdir().unwrap();
    let db = tmp.path().join("runtime.db");
    let (facts, graph) = sqlite_graph(&db);
    let graph = Arc::new(graph);
    facts.declare("t", &["k"]);

    let owner = graph.declare_owner("sprf://ast/cycle", None, "input", "head", "pure", 1);
    let src = graph.declare_table_source("t", 1);
    graph.subscribe(&owner, "t", &src, 1);

    graph.notify_table_inserted("t", 2);

    let graph_for_handler = graph.clone();
    let mut gen = 3u64;
    let err = graph
        .run_to_quiescence(4, move |_job| {
            graph_for_handler.notify_table_inserted("t", gen);
            gen += 1;
        })
        .unwrap_err();

    match err {
        v4::runtime_graph::QuiescenceError::IterationCap { iters, handled } => {
            assert_eq!(iters, 4);
            assert!(handled >= 1);
        }
    }
}
