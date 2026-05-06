//! Walk-lane smoke. Hand-builds a `PipeAst` (no real parser) for
//!
//!   rule(:greet) { str `hello world` }
//!
//! and asserts the full lower → expand → FactStore loop:
//!   1. walk_program returns 1 pipe and 0 diags
//!   2. expand seeds the pipe with one default cursor
//!   3. store.len("greet") == 1, row value == "hello world"
//!
//! Then a deliberately-broken OpCall (rule with no block) is walked
//! and we assert a `lower/missing-slot` diag is emitted without panic.

use std::sync::Arc;

use effect_runtime::v2::{
    expand, ByteRange, ExpandOpts, FactStore, MemFactStore, MemQueue,
    QueueBackend,
};

use v4::Cursor;
use v4::compile::ast::{DslText, OpCall, PipeAst, SlotText};
use v4::compile::walk::walk_program;
use v4::lower::{default_registry, LowerCtx};

fn br(lo: u32, hi: u32) -> ByteRange { ByteRange { lo, hi } }

#[test]
fn walk_smoke_rule_str() {
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let dir   = std::env::temp_dir();
    let reg   = default_registry();

    // ── happy path: rule(:greet) { str `hello world` } ─────────────
    let program = vec![PipeAst {
        span: br(0, 30),
        steps: vec![OpCall {
            name: Arc::<str>::from("rule"),
            predicate: false,
            span: br(0, 30),
            flow: None,
            args: vec![SlotText {
                raw:  Arc::<str>::from(":greet"),
                span: br(5, 11),
            }],
            dsl: None,
            block: Some(PipeAst {
                span: br(13, 30),
                steps: vec![OpCall {
                    name: Arc::<str>::from("str"),
                    predicate: false,
                    span: br(15, 28),
                    flow: None,
                    args: vec![],
                    dsl: Some(DslText {
                        raw:  Arc::<str>::from("hello world"),
                        span: br(19, 30),
                    }),
                    block: None,
                }],
            }),
        }],
    }];

    let mut ctx = LowerCtx::new(store.clone(), dir.clone());
    let (pipes, diags) = walk_program(&program, &reg, &mut ctx);
    assert!(diags.is_empty(), "happy path diags: {:?}",
        diags.iter().map(|d| (d.code.as_ref(), d.message.as_str())).collect::<Vec<_>>());
    assert_eq!(pipes.len(), 1, "expected one pipe");

    let pipe = pipes.into_iter().next().unwrap();
    let inst = pipe.into_instance();
    let q: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    expand(&inst, q, vec![Arc::new(Cursor::default())], ExpandOpts::default());

    assert_eq!(store.len("greet"), 1, "rule wrote one row");
    let rows = store.rows_of("greet");
    assert_eq!(&*rows[0].value, "hello world");

    // ── broken: rule with no block → missing-slot diag ─────────────
    let broken = vec![PipeAst {
        span: br(0, 12),
        steps: vec![OpCall {
            name: Arc::<str>::from("rule"),
            predicate: false,
            span: br(0, 12),
            flow: None,
            args: vec![SlotText {
                raw:  Arc::<str>::from(":greet"),
                span: br(5, 11),
            }],
            dsl: None,
            block: None,
        }],
    }];

    let store2: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let mut ctx2 = LowerCtx::new(store2, dir);
    let (pipes2, diags2) = walk_program(&broken, &reg, &mut ctx2);
    assert!(pipes2.is_empty(), "broken pipe should not lower");
    assert!(
        diags2.iter().any(|d| &*d.code == "lower/missing-slot"),
        "expected missing-slot, got: {:?}",
        diags2.iter().map(|d| (d.code.as_ref(), d.message.as_str())).collect::<Vec<_>>()
    );
}
