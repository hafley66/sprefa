use std::sync::Arc;

use effect_runtime::v2::{
    expand, Component, ExpandOpts, FactStore, MemFactStore, MemQueue, NextKey,
    Node, PipeInstance, QueueBackend, RenderCtx, Wake,
};

use v4::compile::parse::host_parse;
use v4::compile::walk::walk_program;
use v4::lower::{default_registry, LowerCtx};
use v4::v2_ops::{CollectComponent, CollectMode};
use v4::Cursor;

fn run_into_store(src: &str, store: Arc<dyn FactStore<Cursor>>) {
    let (program, parse_diags) = host_parse(src);
    assert!(parse_diags.is_empty(), "parse diags: {parse_diags:?}");

    let reg = default_registry();
    let mut ctx = LowerCtx::new(store.clone(), std::env::temp_dir());
    let (pipes, walk_diags) = walk_program(&program, &reg, &mut ctx);
    assert!(walk_diags.is_empty(), "walk diags: {walk_diags:?}");

    let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    for pipe in pipes {
        expand(
            &pipe.into_instance(),
            queue.clone(),
            vec![Arc::new(Cursor::default())],
            ExpandOpts::default(),
        );
    }
}

fn run_pipes(src: &str) -> Arc<dyn FactStore<Cursor>> {
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    run_into_store(src, store.clone());
    store
}

#[test]
fn empty_rule_fully_bound_apply_sends_identity() {
    let store = run_pipes(r#"
        rule(:frontend_hooks, OP?, FILE?);
        rule(:seen, OP?, FILE?);

        `getUser`
          > term_bind(:OP)
          > `src/hooks.ts`
          > term_bind(:FILE)
          > frontend_hooks.(OP, FILE)
          > rule(:seen, OP: OP, FILE: FILE);
    "#);

    let frontend_rows = store.rows_of("frontend_hooks");
    assert_eq!(frontend_rows.len(), 1, "fully bound empty-rule apply should write one subject row");
    assert_eq!(frontend_rows[0].get("OP"), Some("getUser"));
    assert_eq!(frontend_rows[0].get("FILE"), Some("src/hooks.ts"));

    let seen_rows = store.rows_of("seen");
    assert_eq!(seen_rows.len(), 1, "send/write should pass the original cursor through 1:1");
}

#[test]
#[ignore = "target semantics: mounted anti-join should retract stale missing rows after later writes"]
fn mounted_query_reacts_to_late_relation_write() {
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());

    run_into_store(r#"
        rule(:frontend_hooks, OP?, FILE?);
        rule(:missing_hooks, OP?);

        `getUser`
          > term_bind(:OP)
          > `src/hooks.ts`
          > term_bind(:FILE)
          > sql`
              SELECT input.__cursor_idx, input.OP
              FROM input
              WHERE NOT EXISTS (
                SELECT 1
                FROM frontend_hooks
                WHERE frontend_hooks.OP = ${OP}
                  AND frontend_hooks.FILE = ${FILE}
              )
            `
          > rule(:missing_hooks, OP: OP);
    "#, store.clone());

    assert_eq!(store.rows_of("missing_hooks").len(), 1, "initial anti-join should report missing hook");

    run_into_store(r#"
        `getUser`
          > term_bind(:OP)
          > `src/hooks.ts`
          > term_bind(:FILE)
          > rule(:frontend_hooks, OP: OP, FILE: FILE);
    "#, store.clone());

    assert_eq!(
        store.rows_of("missing_hooks").len(),
        0,
        "live query mount should rerun/retract the missing row after frontend_hooks changes"
    );
}

#[test]
fn mounted_query_persists_outputs_by_mount_and_input_key() {
    let store = run_pipes(r#"
        rule(:openapi_ops, OP?);
        rule(:frontend_hooks, OP?, FILE?);
        rule(:hook_hits, OP?, FILE?);

        `getUser`
          > term_bind(:OP)
          > rule(:openapi_ops, OP: OP);

        `getUser`
          > term_bind(:OP)
          > `src/hooks.ts`
          > term_bind(:FILE)
          > rule(:frontend_hooks, OP: OP, FILE: FILE);

        openapi_ops(OP: OP?)
          > sql`
              SELECT input.__cursor_idx, input.OP, frontend_hooks.FILE AS value
              FROM input
              JOIN frontend_hooks ON frontend_hooks.OP = ${OP}
            `
          > term_bind(:FILE)
          > rule(:hook_hits, OP: OP, FILE: FILE);
    "#);

    assert_eq!(store.rows_of("hook_hits").len(), 1);
    let mounted_outputs = store.rows_of("mounted_query_output");
    assert!(
        mounted_outputs.len() >= 1,
        "mounted query output set should persist independently of the consumer rule table"
    );
    assert!(mounted_outputs[0].get("mount_id").is_some());
    assert!(mounted_outputs[0].get("input_key").is_some());
    assert!(mounted_outputs[0].get("generation").is_some());
    assert!(mounted_outputs[0].get("output_hash").is_some());
    assert!(mounted_outputs[0].get("cursor_blob").is_some());
}

#[test]
#[ignore = "target semantics: rerun should diff old/new mounted query outputs and emit only additions"]
fn mounted_query_rerun_emits_only_new_output_hashes() {
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());

    run_into_store(r#"
        rule(:frontend_hooks, OP?, FILE?);
        rule(:hook_hits, OP?, FILE?);

        `getUser`
          > term_bind(:OP)
          > `src/hooks.ts`
          > term_bind(:FILE)
          > rule(:frontend_hooks, OP: OP, FILE: FILE);

        `getUser`
          > term_bind(:OP)
          > sql`
              SELECT input.__cursor_idx, input.OP, frontend_hooks.FILE AS value
              FROM input
              JOIN frontend_hooks ON frontend_hooks.OP = ${OP}
            `
          > term_bind(:FILE)
          > rule(:hook_hits, OP: OP, FILE: FILE);
    "#, store.clone());

    assert_eq!(store.rows_of("hook_hits").len(), 1);

    run_into_store(r#"
        `getUser`
          > term_bind(:OP)
          > `src/hooks_extra.ts`
          > term_bind(:FILE)
          > rule(:frontend_hooks, OP: OP, FILE: FILE);
    "#, store.clone());

    assert_eq!(
        store.rows_of("hook_hits").len(),
        2,
        "mounted query rerun should add only the new output row while preserving the old one"
    );
}

#[test]
fn mounted_query_manual_rerun_emits_only_new_output_hashes() {
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());

    run_into_store(r#"
        rule(:frontend_hooks, OP?, FILE?);
        rule(:hook_hits, OP?, FILE?);

        `getUser`
          > term_bind(:OP)
          > `src/hooks.ts`
          > term_bind(:FILE)
          > rule(:frontend_hooks, OP: OP, FILE: FILE);

        `getUser`
          > term_bind(:OP)
          > sql`
              SELECT input.__cursor_idx, input.OP, frontend_hooks.FILE AS value
              FROM input
              JOIN frontend_hooks ON frontend_hooks.OP = ${OP}
            `
          > term_bind(:FILE)
          > rule(:hook_hits, OP: OP, FILE: FILE);
    "#, store.clone());

    assert_eq!(store.rows_of("hook_hits").len(), 1);

    run_into_store(r#"
        rule(:frontend_hooks, OP?, FILE?);
        rule(:hook_hits, OP?, FILE?);

        `getUser`
          > term_bind(:OP)
          > `src/hooks_extra.ts`
          > term_bind(:FILE)
          > rule(:frontend_hooks, OP: OP, FILE: FILE);

        `getUser`
          > term_bind(:OP)
          > sql`
              SELECT input.__cursor_idx, input.OP, frontend_hooks.FILE AS value
              FROM input
              JOIN frontend_hooks ON frontend_hooks.OP = ${OP}
            `
          > term_bind(:FILE)
          > rule(:hook_hits, OP: OP, FILE: FILE);
    "#, store.clone());

    let rows = store.rows_of("hook_hits");
    assert_eq!(
        rows.len(),
        2,
        "manual rerun should pass only the new mounted query output downstream"
    );
    let mut files: Vec<&str> = rows.iter().filter_map(|row| row.get("FILE")).collect();
    files.sort();
    assert_eq!(files, vec!["src/hooks.ts", "src/hooks_extra.ts"]);
}

#[test]
fn collect_does_not_mix_two_pipe_instances() {
    use std::sync::Mutex;

    struct ParkValue;
    impl Component for ParkValue {
        type Next = Cursor;
        fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
            if c.value.as_ref() == "park" {
                Node::Yield {
                    value: Arc::new(c.clone()),
                    wake: Wake::Key {
                        domain: "barrier-scope-test".into(),
                        key: NextKey([7; 32]),
                    },
                }
            } else {
                Node::Emit(Arc::new(c.clone()))
            }
        }
    }

    struct Sink(Arc<Mutex<Vec<Cursor>>>);
    impl Component for Sink {
        type Next = Cursor;
        fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
            self.0.lock().unwrap().push(c.clone());
            Node::Done
        }
    }

    fn cur(value: &str) -> Arc<Cursor> {
        let mut c = Cursor::default();
        c.value = Arc::from(value);
        Arc::new(c)
    }

    let collect = Arc::new(CollectComponent::new(CollectMode::Complete));
    let left_sink = Arc::new(Mutex::new(Vec::new()));
    let right_sink = Arc::new(Mutex::new(Vec::new()));

    let mut left = PipeInstance::new(vec![
        Arc::new(ParkValue) as Arc<dyn Component<Next = Cursor>>,
        collect.clone() as Arc<dyn Component<Next = Cursor>>,
        Arc::new(Sink(left_sink.clone())),
    ]);
    left.pipe_hash = 1;
    left.instance_id = 1;

    let mut right = PipeInstance::new(vec![
        collect as Arc<dyn Component<Next = Cursor>>,
        Arc::new(Sink(right_sink.clone())),
    ]);
    right.pipe_hash = 2;
    right.instance_id = 2;

    let left_queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    expand(
        &left,
        left_queue,
        vec![cur("a"), cur("park")],
        ExpandOpts::default(),
    );
    assert_eq!(left_sink.lock().unwrap().len(), 0);

    let right_queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    expand(&right, right_queue, vec![cur("b")], ExpandOpts::default());

    let got = right_sink.lock().unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(
        got[0].value.as_ref(),
        "b",
        "right mount should not flush buffered rows from left mount"
    );
}

#[test]
fn bodied_rule_apply_runs_body_and_emits_outputs() {
    let store = run_pipes(r#"
        rule(:derive_hook, OP?, FILE?) {
          `src/hooks.ts` > term_bind(:FILE)
        };
        rule(:hook_hits, OP?, FILE?);

        `getUser`
          > term_bind(:OP)
          > derive_hook.(OP)
          > rule(:hook_hits, OP: OP, FILE: FILE);
    "#);

    let rows = store.rows_of("hook_hits");
    assert_eq!(rows.len(), 1, "bodied rule apply should run the body for the input cursor");
    assert_eq!(rows[0].get("OP"), Some("getUser"));
    assert_eq!(rows[0].get("FILE"), Some("src/hooks.ts"));
}

#[test]
fn bodied_rule_query_reads_table_without_running_body() {
    let store = run_pipes(r#"
        rule(:derive_hook, OP?, FILE?) {
          `src/hooks.ts` > term_bind(:FILE)
        };
        rule(:hook_hits, OP?, FILE?);

        `getUser`
          > term_bind(:OP)
          > derive_hook(OP, FILE?)
          > rule(:hook_hits, OP: OP, FILE: FILE);
    "#);

    let rows = store.rows_of("hook_hits");
    assert_eq!(rows.len(), 0, "non-dotted rule query should not run the body");
}
