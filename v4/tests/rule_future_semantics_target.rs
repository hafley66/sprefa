use std::sync::Arc;

use effect_runtime::v2::{expand, ExpandOpts, FactStore, MemFactStore, MemQueue, QueueBackend};

use v4::compile::parse::host_parse;
use v4::compile::walk::walk_program;
use v4::lower::{default_registry, LowerCtx};
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
#[ignore = "target semantics: empty-rule fully-bound apply should send/write identity"]
fn empty_rule_fully_bound_apply_sends_identity() {
    let store = run_pipes(r#"
        rule(:frontend_hooks, OP!, FILE!);
        rule(:seen, OP?, FILE?);

        `getUser`
          > term_bind(:OP)
          > `src/hooks.ts`
          > term_bind(:FILE)
          > frontend_hooks(OP, FILE)
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
        rule(:frontend_hooks, OP!, FILE!);
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
#[ignore = "target semantics: bodied rule apply should run body, cache outputs, and emit them"]
fn bodied_rule_apply_runs_body_and_emits_outputs() {
    let store = run_pipes(r#"
        rule(:derive_hook, OP!, FILE?) {
          `src/hooks.ts` > term_bind(:FILE)
        };
        rule(:hook_hits, OP?, FILE?);

        `getUser`
          > term_bind(:OP)
          > derive_hook(OP, FILE?)
          > rule(:hook_hits, OP: OP, FILE: FILE);
    "#);

    let rows = store.rows_of("hook_hits");
    assert_eq!(rows.len(), 1, "bodied rule apply should run the body for the input cursor");
    assert_eq!(rows[0].get("OP"), Some("getUser"));
    assert_eq!(rows[0].get("FILE"), Some("src/hooks.ts"));
}
