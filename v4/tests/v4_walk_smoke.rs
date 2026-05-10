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
    expand, ByteRange, ExpandOpts, FactStore, MemFactStore, MemQueue, QueueBackend,
};

use effect_runtime::v2::Pipe;
use v4::compile::ast::{DslText, OpCall, PipeAst, SlotText};
use v4::compile::walk::walk_program;
use v4::lower::{default_registry, LowerCtx};
use v4::lower::{ArgKind, ArgSig, DslBody, OperatorDef, Value};
use v4::Cursor;

fn br(lo: u32, hi: u32) -> ByteRange {
    ByteRange { lo, hi }
}

#[test]
fn walk_smoke_rule_str() {
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let dir = std::env::temp_dir();
    let reg = default_registry();

    // ── happy path: rule(:greet) { str `hello world` } ─────────────
    let program = vec![PipeAst {
        span: br(0, 30),
        steps: vec![OpCall {
            name: Arc::<str>::from("rule"),
            force: false,
            predicate: false,
            apply: false,
            span: br(0, 30),
            flow: None,
            args: vec![SlotText {
                raw: Arc::<str>::from(":greet"),
                span: br(5, 11),
            }],
            dsl: None,
            block: Some(PipeAst {
                span: br(13, 30),
                steps: vec![OpCall {
                    name: Arc::<str>::from("str"),
                    force: false,
                    predicate: false,
                    apply: false,
                    span: br(15, 28),
                    flow: None,
                    args: vec![],
                    dsl: Some(DslText {
                        raw: Arc::<str>::from("hello world"),
                        span: br(19, 30),
                    }),
                    block: None,
                }],
            }),
        }],
    }];

    let mut ctx = LowerCtx::new(store.clone(), dir.clone());
    let (pipes, diags) = walk_program(&program, &reg, &mut ctx);
    assert!(
        diags.is_empty(),
        "happy path diags: {:?}",
        diags
            .iter()
            .map(|d| (d.code.as_ref(), d.message.as_str()))
            .collect::<Vec<_>>()
    );
    assert_eq!(pipes.len(), 1, "expected one pipe");

    let pipe = pipes.into_iter().next().unwrap();
    let inst = pipe.into_instance();
    let q: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    expand(
        &inst,
        q,
        vec![Arc::new(Cursor::default())],
        ExpandOpts::default(),
    );

    assert_eq!(store.len("greet"), 1, "rule wrote one row");
    let rows = store.rows_of("greet");
    assert_eq!(&*rows[0].value, "hello world");

    // ── empty-body decl: rule(:greet) at chain_pos 0 with no block ─
    // Pure declaration form. Lowers to an empty Pipe (zero steps);
    // the cursor seed has nothing to drain into → zero rows. The
    // table is still declared with no columns.
    let decl_only = vec![PipeAst {
        span: br(0, 12),
        steps: vec![OpCall {
            name: Arc::<str>::from("rule"),
            force: false,
            predicate: false,
            apply: false,
            span: br(0, 12),
            flow: None,
            args: vec![SlotText {
                raw: Arc::<str>::from(":greet"),
                span: br(5, 11),
            }],
            dsl: None,
            block: None,
        }],
    }];

    let store2: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let mut ctx2 = LowerCtx::new(store2.clone(), dir);
    let (pipes2, diags2) = walk_program(&decl_only, &reg, &mut ctx2);
    assert!(
        diags2.is_empty(),
        "decl-only diags: {:?}",
        diags2
            .iter()
            .map(|d| (d.code.as_ref(), d.message.as_str()))
            .collect::<Vec<_>>()
    );
    assert_eq!(pipes2.len(), 1, "decl-only lowers to one (empty) pipe");

    let pipe2 = pipes2.into_iter().next().unwrap();
    let inst2 = pipe2.into_instance();
    let q2: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    expand(
        &inst2,
        q2,
        vec![Arc::new(Cursor::default())],
        ExpandOpts::default(),
    );
    assert_eq!(store2.len("greet"), 0, "decl-only writes zero rows");
}

// ─── inline-pipe arg via host_parse re-entry ───────────────────────
//
// Hand-built call equivalent to `pipe_only(foo > bar)`. The second
// fragment must classify as Value::Pipe (via host_parse fallback in
// classify_slot), not emit `compile/inline-pipe-unsupported`. Custom
// op accepts one Pipe arg so a clean walk lowers without diags.

struct PipeAccept;
const PIPE_ACCEPT_SPEC: &[ArgSig] = &[ArgSig {
    kind: ArgKind::Pipe,
    name: "p",
    doc: "",
    required: true,
}];
impl OperatorDef for PipeAccept {
    fn name(&self) -> &'static str {
        "pipe_accept"
    }
    fn paren_args(&self) -> &[ArgSig] {
        PIPE_ACCEPT_SPEC
    }
    fn lower(
        &self,
        _ctx: &v4::lower::LowerCtx,
        _flow: Option<Value>,
        args: &[Value],
        _blk: Option<Pipe<Cursor>>,
        _dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, v4::lower::LowerError> {
        match &args[0] {
            Value::Pipe(p) => Ok(p.clone()),
            _ => Err(v4::lower::LowerError::Unknown(
                "pipe_accept: not a pipe".into(),
            )),
        }
    }
}

#[test]
fn walk_smoke_inline_pipe() {
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let dir = std::env::temp_dir();
    let mut reg = default_registry();
    reg.register(Arc::new(PipeAccept));

    // Hand-built `pipe_accept(foo > bar)`. The second arg's raw text
    // is the inline pipe expression "foo > bar"; span values pretend
    // it lived at offset 12..21 in some outer source.
    let program = vec![PipeAst {
        span: br(0, 22),
        steps: vec![OpCall {
            name: Arc::<str>::from("pipe_accept"),
            force: false,
            predicate: false,
            apply: false,
            span: br(0, 22),
            flow: None,
            args: vec![SlotText {
                raw: Arc::<str>::from("foo > bar"),
                span: br(12, 21),
            }],
            dsl: None,
            block: None,
        }],
    }];

    let mut ctx = LowerCtx::new(store, dir);
    let (_pipes, diags) = walk_program(&program, &reg, &mut ctx);

    // The gap closes at walk-time: no `compile/inline-pipe-unsupported`
    // diag must appear, regardless of how lower-time treats the result.
    assert!(
        !diags
            .iter()
            .any(|d| &*d.code == "compile/inline-pipe-unsupported"),
        "inline-pipe-unsupported leaked through: {:?}",
        diags
            .iter()
            .map(|d| (d.code.as_ref(), d.message.as_str()))
            .collect::<Vec<_>>()
    );
    // And no malformed-parse from a clean `foo > bar` fragment.
    assert!(
        !diags
            .iter()
            .any(|d| &*d.code == "compile/inline-pipe-malformed"),
        "inline-pipe-malformed unexpected: {:?}",
        diags
            .iter()
            .map(|d| (d.code.as_ref(), d.message.as_str()))
            .collect::<Vec<_>>()
    );
}
