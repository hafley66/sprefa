use std::sync::Arc;

use effect_runtime::v2::{expand, ExpandOpts, FactStore, MemFactStore, MemQueue, QueueBackend};

use v4::compile::parse::host_parse;
use v4::compile::walk::walk_program;
use v4::lower::{default_registry, LowerCtx};
use v4::Cursor;

fn walk_diags(src: &str) -> Vec<effect_runtime::v2::Diag> {
    let (program, parse_diags) = host_parse(src);
    assert!(parse_diags.is_empty(), "parse diags: {parse_diags:?}");

    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let reg = default_registry();
    let mut ctx = LowerCtx::new(store, std::env::temp_dir());
    let (_pipes, walk_diags) = walk_program(&program, &reg, &mut ctx);
    walk_diags
}

fn run_pipes(src: &str) -> Arc<dyn FactStore<Cursor>> {
    let (program, parse_diags) = host_parse(src);
    assert!(parse_diags.is_empty(), "parse diags: {parse_diags:?}");

    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
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

    store
}

#[test]
fn declared_rule_force_call_requires_bodied_rule() {
    let diags = walk_diags(r#"
        rule(:frontend_hooks);
        frontend_hooks!();
    "#);

    assert!(
        diags.iter().any(|d| d.code.as_ref() == "lower/rule-force"),
        "expected lower/rule-force diag, got {diags:?}"
    );
}

#[test]
fn sql_anti_join_finds_missing_frontend_hook() {
    let src = r#"
        rule(:frontend_hooks, OP?);
        rule(:missing_frontend_hooks, OP?);

        `getUser` > split(OP?)`!` > rule(:frontend_hooks);

        `getUser`
          > split(OP?)`!`
          > sql`
              SELECT input.__cursor_idx, input.OP
              FROM input
              WHERE NOT EXISTS (
                SELECT 1
                FROM frontend_hooks
                WHERE frontend_hooks.OP = ${OP}
              )
            `
          > rule(:missing_frontend_hooks);

        `listPets`
          > split(OP?)`!`
          > sql`
              SELECT input.__cursor_idx, input.OP
              FROM input
              WHERE NOT EXISTS (
                SELECT 1
                FROM frontend_hooks
                WHERE frontend_hooks.OP = ${OP}
              )
            `
          > rule(:missing_frontend_hooks);
    "#;

    let store = run_pipes(src);
    let rows = store.rows_of("missing_frontend_hooks");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("OP"), Some("listPets"));
}

#[test]
fn sql_anti_join_handles_declared_empty_rule_table() {
    let src = r#"
        rule(:frontend_hooks, OP?);
        rule(:missing_frontend_hooks, OP?);

        `listPets`
          > split(OP?)`!`
          > sql`
              SELECT input.__cursor_idx, input.OP
              FROM input
              WHERE NOT EXISTS (
                SELECT 1
                FROM frontend_hooks
                WHERE frontend_hooks.OP = ${OP}
              )
            `
          > rule(:missing_frontend_hooks);
    "#;

    let store = run_pipes(src);
    let rows = store.rows_of("missing_frontend_hooks");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("OP"), Some("listPets"));
}

#[test]
fn declared_rule_call_projects_matching_rows() {
    let src = r#"
        rule(:frontend_hooks, OP?, FILE?);
        rule(:hook_hits, OP?, FILE?);

        `getUser`
          > term_bind(:OP)
          > `src/hooks.ts`
          > term_bind(:FILE)
          > rule(:frontend_hooks);

        `getUser`
          > term_bind(:OP)
          > frontend_hooks(OP, FILE?)
          > rule(:hook_hits);
    "#;

    let store = run_pipes(src);
    let rows = store.rows_of("hook_hits");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("OP"), Some("getUser"));
    assert_eq!(rows[0].get("FILE"), Some("src/hooks.ts"));
}

#[test]
fn declared_rule_dotted_apply_projects_matching_rows_with_kwargs() {
    let src = r#"
        rule(:frontend_hooks, OP!, FILE?);
        rule(:hook_hits, OP?, FILE?);

        `getUser`
          > term_bind(:OP)
          > `src/hooks.ts`
          > term_bind(:FILE)
          > rule(:frontend_hooks, OP: OP, FILE: FILE);

        `getUser`
          > term_bind(:OP)
          > frontend_hooks.(OP: OP, FILE: FILE?)
          > rule(:hook_hits, OP: OP, FILE: FILE);
    "#;

    let store = run_pipes(src);
    let frontend_rows = store.rows_of("frontend_hooks");
    assert_eq!(frontend_rows.len(), 1);
    assert_eq!(frontend_rows[0].get("OP"), Some("getUser"));
    assert_eq!(frontend_rows[0].get("FILE"), Some("src/hooks.ts"));
    assert!(frontend_rows[0].get("_id").is_some());

    let rows = store.rows_of("hook_hits");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("OP"), Some("getUser"));
    assert_eq!(rows[0].get("FILE"), Some("src/hooks.ts"));
}

#[test]
fn declared_rule_predicate_call_filters_fully_bound_input() {
    let src = r#"
        rule(:frontend_hooks, OP?, FILE?);
        rule(:checked_hooks, OP?, FILE?);

        `getUser`
          > term_bind(:OP)
          > `src/hooks.ts`
          > term_bind(:FILE)
          > rule(:frontend_hooks);

        `getUser`
          > term_bind(:OP)
          > `src/hooks.ts`
          > term_bind(:FILE)
          > frontend_hooks?(OP, FILE)
          > rule(:checked_hooks);
    "#;

    let store = run_pipes(src);
    let rows = store.rows_of("checked_hooks");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("OP"), Some("getUser"));
    assert_eq!(rows[0].get("FILE"), Some("src/hooks.ts"));
}

#[test]
fn declared_rule_predicate_call_does_not_duplicate_matching_input() {
    let src = r#"
        rule(:frontend_hooks, OP?, FILE?);
        rule(:checked_hooks, OP?, FILE?);

        `getUser`
          > term_bind(:OP)
          > `src/hooks.ts`
          > term_bind(:FILE)
          > rule(:frontend_hooks);

        `getUser`
          > term_bind(:OP)
          > `src/hooks.ts`
          > term_bind(:FILE)
          > rule(:frontend_hooks);

        `getUser`
          > term_bind(:OP)
          > `src/hooks.ts`
          > term_bind(:FILE)
          > frontend_hooks?(OP, FILE)
          > rule(:checked_hooks);
    "#;

    let store = run_pipes(src);
    let rows = store.rows_of("checked_hooks");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("OP"), Some("getUser"));
    assert_eq!(rows[0].get("FILE"), Some("src/hooks.ts"));
}

#[test]
fn declared_rule_dotted_predicate_empty_args_is_table_nonempty_gate() {
    let src = r#"
        rule(:frontend_hooks, OP!);
        rule(:checked_hooks, OP?);

        `getUser`
          > term_bind(:OP)
          > rule(:frontend_hooks, OP: OP);

        `listPets`
          > term_bind(:OP)
          > frontend_hooks?.()
          > rule(:checked_hooks, OP: OP);
    "#;

    let store = run_pipes(src);
    let rows = store.rows_of("checked_hooks");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("OP"), Some("listPets"));
}
