//! Smoke test for the inlay-hint pipeline.
//!
//! `inlays_for` lives behind `mod inlay` in the binary crate, so tests
//! under `tests/` can't import it directly. The logic is replicated
//! inline here. If `inlay::inlays_for` ever moves to a lib target,
//! collapse this test back onto the public API.

use std::collections::HashMap;
use std::sync::Arc;

use effect_runtime::v2::{
    expand, BufferProbeSink, ExpandOpts, FactStore, MemFactStore,
    MemQueue, ProbeSink, QueueBackend,
};

use v4::compile::parse::host_parse;
use v4::compile::walk::walk_program;
use v4::lower::{default_registry, LowerCtx};
use v4::Cursor;

#[test]
fn inlay_smoke_rule_str_emits_one_cursor_per_op() {
    let src = "rule(:greet) { str `hello world` }";

    let (program, parse_diags) = host_parse(src);
    assert!(parse_diags.is_empty(), "parse diags: {:?}", parse_diags);

    let probe: Arc<BufferProbeSink<Cursor>> = Arc::new(BufferProbeSink::new());
    let probe_dyn: Arc<dyn ProbeSink<Cursor>> = probe.clone();
    let store: Arc<dyn FactStore<Cursor>> =
        Arc::new(MemFactStore::<Cursor>::new());
    let reg = default_registry();
    let mut ctx = LowerCtx::new(store, std::env::temp_dir())
        .with_probe(probe_dyn);

    let (pipes, walk_diags) = walk_program(&program, &reg, &mut ctx);
    assert!(walk_diags.is_empty(), "walk diags: {:?}", walk_diags);
    assert_eq!(pipes.len(), 1, "expected one lowered pipe");

    for pipe in pipes {
        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let inst = pipe.into_instance();
        expand(
            &inst,
            queue,
            vec![Arc::new(Cursor::default())],
            ExpandOpts::default(),
        );
    }

    let probes = probe.drain();
    assert!(!probes.is_empty(), "probe buffer must capture emits");

    // Group by span. Both `str` and `rule` should appear.
    let mut by_span: HashMap<(u32, u32), u32> = HashMap::new();
    for p in &probes {
        *by_span.entry((p.span.lo, p.span.hi)).or_insert(0) += 1;
    }
    assert!(
        by_span.len() >= 2,
        "expected at least 2 distinct op spans (str + rule), got {}: {:?}",
        by_span.len(), by_span,
    );

    // Sanity: every probe carries a non-empty span (lo < hi).
    for p in &probes {
        assert!(p.span.lo < p.span.hi, "degenerate span: {:?}", p.span);
    }
}
