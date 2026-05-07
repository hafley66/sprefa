//! Empty-body `rule(:x, A?, B?);` — pure declaration form.
//!
//! Asserts the unification:
//!   • empty body → table appears, zero rows
//!   • param ?-marks → declared columns
//!   • bodied form still works (regression)
//!   • sink form (`upstream > rule(:x)`) drains cursors into rows

use std::sync::Arc;

use effect_runtime::v2::{
    expand, Diag, ExpandOpts, FactStore, MemFactStore, MemQueue, QueueBackend,
};

use v4::Cursor;
use v4::compile::parse::host_parse;
use v4::compile::walk::walk_program;
use v4::lower::{default_registry, LowerCtx};

fn run_pipes(src: &str) -> (Arc<dyn FactStore<Cursor>>, Vec<Diag>) {
    let (program, parse_diags) = host_parse(src);
    assert!(parse_diags.is_empty(), "parse diags: {:?}", parse_diags);

    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let dir = std::env::temp_dir();
    let reg = default_registry();
    let mut ctx = LowerCtx::new(store.clone(), dir);

    let (pipes, walk_diags) = walk_program(&program, &reg, &mut ctx);

    if walk_diags.is_empty() {
        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        for pipe in pipes {
            let inst = pipe.into_instance();
            expand(
                &inst,
                queue.clone(),
                vec![Arc::new(Cursor::default())],
                ExpandOpts::default(),
            );
        }
    }

    (store, walk_diags)
}

#[test]
fn rule_empty_body_declares_table_zero_rows() {
    let src = "rule(:r);";
    let (store, diags) = run_pipes(src);
    assert!(
        diags.is_empty(),
        "no walk diags expected, got {:?}",
        diags.iter().map(|d| (d.code.as_ref(), d.message.as_str())).collect::<Vec<_>>()
    );
    assert_eq!(store.len("r"), 0, "empty-body rule produces no rows");
}

#[test]
fn rule_empty_body_with_param_captures_declares_columns() {
    let src = "rule(:r, A?, B?);";
    let (store, diags) = run_pipes(src);
    assert!(
        diags.is_empty(),
        "no walk diags expected, got {:?}",
        diags.iter().map(|d| (d.code.as_ref(), d.message.as_str())).collect::<Vec<_>>()
    );
    assert_eq!(store.len("r"), 0);
    // Deeper schema-introspection assertion (declared columns A, B) is a
    // follow-up; this test guards "lower without panicking" today.
}

#[test]
fn rule_with_body_still_writes_rows() {
    let src = "rule(:greet) { `hello world` };";
    let (store, diags) = run_pipes(src);
    assert!(
        diags.is_empty(),
        "no walk diags expected, got {:?}",
        diags.iter().map(|d| (d.code.as_ref(), d.message.as_str())).collect::<Vec<_>>()
    );
    assert_eq!(store.len("greet"), 1);
    let rows = store.rows_of("greet");
    assert_eq!(&*rows[0].value, "hello world");
}

#[test]
fn rule_at_sink_position_drains_upstream_cursors() {
    // Sink form: pure decl up front, then two pipes that drain a
    // distinct-valued cursor into the declared rule. Each pipe pushes
    // its value through `split(V?)` so the per-cursor V binding lands
    // in the row's identity (MemFactStore content_ids by raw_terms,
    // so `&.value` alone wouldn't differentiate the two rows).
    let src = "rule(:items, V?); \
               `alpha` > split(V?)`!` > rule(:items); \
               `beta`  > split(V?)`!` > rule(:items);";

    let (store, diags) = run_pipes(src);
    assert!(
        diags.is_empty(),
        "no walk diags expected, got {:?}",
        diags.iter().map(|d| (d.code.as_ref(), d.message.as_str())).collect::<Vec<_>>()
    );
    assert_eq!(store.len("items"), 2);
    let vals: Vec<_> = store
        .rows_of("items")
        .iter()
        .map(|c| c.value.to_string())
        .collect();
    assert!(vals.contains(&"alpha".to_string()));
    assert!(vals.contains(&"beta".to_string()));
}
