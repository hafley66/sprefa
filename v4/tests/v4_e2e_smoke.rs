//! End-to-end smoke: source string → host_parse → walk_program → expand → FactStore.
//!
//! ONE #[test] proving the whole seam from parser to FactStore is intact.

use std::sync::Arc;

use effect_runtime::v2::{expand, ExpandOpts, FactStore, MemFactStore, MemQueue, QueueBackend};

use v4::compile::parse::host_parse;
use v4::compile::walk::walk_program;
use v4::lower::{default_registry, LowerCtx};
use v4::Cursor;

#[test]
fn e2e_source_to_fact_store() {
    // ── 1. source ─────────────────────────────────────────────────────
    // Mirrors v4_parse_smoke / v4_walk_smoke shape. `rule(:greet)` opens
    // the sink, the `{ str `hello world` }` body emits one row.
    // Bare backtick at pipe-step lowers to `str` sugar (no whitespace
    // permitted between an op name and its `` ` `` slot opener).
    let src = "rule(:greet) { `hello world` };";

    // ── 2. parse ──────────────────────────────────────────────────────
    let (program, parse_diags) = host_parse(src);
    assert!(parse_diags.is_empty(), "parse diags: {:?}", parse_diags);
    assert_eq!(program.len(), 1, "expected one pipe");

    // ── 3. walk ───────────────────────────────────────────────────────
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let dir = std::env::temp_dir();
    let reg = default_registry();
    let mut ctx = LowerCtx::new(store.clone(), dir);

    let (pipes, walk_diags) = walk_program(&program, &reg, &mut ctx);
    assert!(
        walk_diags.is_empty(),
        "walk diags: {:?}",
        walk_diags
            .iter()
            .map(|d| (d.code.as_ref(), d.message.as_str()))
            .collect::<Vec<_>>()
    );
    assert_eq!(pipes.len(), 1, "expected one lowered pipe");

    // ── 4. expand ─────────────────────────────────────────────────────
    let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    let inst = pipes.into_iter().next().unwrap().into_pipe().into_instance();
    expand(
        &inst,
        queue,
        vec![Arc::new(Cursor::default())],
        ExpandOpts::default(),
    );

    // ── 5. assert FactStore writes ────────────────────────────────────
    assert_eq!(store.len("greet"), 1, "rule should have written one row");
    let rows = store.rows_of("greet");
    assert_eq!(&*rows[0].value, "hello world");
}
