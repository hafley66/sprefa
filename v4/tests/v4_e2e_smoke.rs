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
    // Rule = function: `rule(:greet){…}` declares (inert); the bare
    // `greet()` call runs the body. Two statements ⇒ two pipes; the
    // call pipe is what writes the row. Bare backtick at pipe-step
    // lowers to `str` sugar.
    let src = "rule(:greet) { `hello world` }; greet();";

    // ── 2. parse ──────────────────────────────────────────────────────
    let (program, parse_diags) = host_parse(src);
    assert!(parse_diags.is_empty(), "parse diags: {:?}", parse_diags);
    assert_eq!(program.len(), 2, "decl + call = two statements");

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
    assert_eq!(pipes.len(), 2, "decl pipe + call pipe");

    // ── 4. expand ─────────────────────────────────────────────────────
    // Run the program in statement order: the decl pipe is inert, the
    // call pipe runs `body > FactWrite(greet)`.
    let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    for fused in pipes {
        let inst = fused.into_pipe().into_instance();
        expand(
            &inst,
            queue.clone(),
            vec![Arc::new(Cursor::default())],
            ExpandOpts::default(),
        );
    }

    // ── 5. assert FactStore writes ────────────────────────────────────
    assert_eq!(store.len("greet"), 1, "rule should have written one row");
    let rows = store.rows_of("greet");
    assert_eq!(&*rows[0].value, "hello world");
}
