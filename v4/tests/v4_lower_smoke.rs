//! Four-slot lower smoke. One #[test], exercising:
//!   1. Plain `str` literal (no interps)
//!   2. `str` with a `${WHO}` interp resolved from ctx.bindings
//!   3. `rule` sink: rule(:greet) { str ` hello ${WHO} ` } — writes 1 row
//!   4. validate_call diag: pass an Atom where Pipe is required

use std::sync::Arc;

use effect_runtime::v2::{
    expand, ByteRange, Diag, ExpandOpts, FactStore, MemFactStore, MemQueue,
    Pipe, QueueBackend,
};

use v4::Cursor;
use v4::lower::{
    default_registry, str_pipe, validate_call, BlockShape, DslBody, DslInterp,
    DslShape, LowerCtx, OperatorDef, Value,
};
use v4::lower::op_def::{ArgKind, ArgSig};

fn br(lo: u32, hi: u32) -> ByteRange { ByteRange { lo, hi } }

#[test]
fn four_slot_lower_smoke() {
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let dir   = std::env::temp_dir();
    let reg   = default_registry();
    let str_def = reg.get("str").unwrap();

    // ── 1. plain literal ───────────────────────────────────────────
    let plain_dsl = DslBody { raw: Arc::from("hello world"), interps: vec![] };
    let ctx0 = LowerCtx::new(store.clone(), dir.clone());
    let plain = str_def.lower(&ctx0, None, &[], None, Some(&plain_dsl)).unwrap();
    let out = collect(plain);
    assert_eq!(out.len(), 1);
    assert_eq!(&*out[0].value, "hello world");

    // ── 2. interpolated literal ───────────────────────────────────
    // raw = "hello ${WHO}", ${WHO} occupies bytes 6..12 (the literal
    // '${WHO}'). Lower-time ctx binding grounds WHO → "world".
    let raw = "hello ${WHO}";
    let interp_dsl = DslBody {
        raw: Arc::from(raw),
        interps: vec![DslInterp { name: Arc::from("WHO"), range: br(6, 12) }],
    };
    let ctx1 = LowerCtx::new(store.clone(), dir.clone())
        .with_binding("WHO", str_pipe("world"));
    let interp = str_def.lower(&ctx1, None, &[], None, Some(&interp_dsl)).unwrap();
    let out = collect(interp);
    assert_eq!(out.len(), 1);
    assert_eq!(&*out[0].value, "hello world");

    // ── 3. rule sink ──────────────────────────────────────────────
    // Build the body via str_def.lower again, then hand to rule_def
    // through Registry::lower with byte ranges (validates clean).
    let body = str_def.lower(
        &LowerCtx::new(store.clone(), dir.clone())
            .with_binding("WHO", str_pipe("world")),
        None, &[], None, Some(&DslBody {
            raw: Arc::from("hello ${WHO}"),
            interps: vec![DslInterp { name: Arc::from("WHO"), range: br(6, 12) }],
        }),
    ).unwrap();

    let rule_pipe = reg.lower(
        &LowerCtx::new(store.clone(), dir.clone()),
        "rule",
        None,
        vec![(Value::atom("greet"), br(0, 5))],
        Some((body, br(7, 30))),
        None,
        br(0, 30),
    ).expect("rule lowers");

    let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    let inst  = rule_pipe.into_instance();
    expand(&inst, queue, vec![Arc::new(Cursor::default())], ExpandOpts::default());

    assert_eq!(store.len("greet"), 1, "rule wrote one row");
    let rows = store.rows_of("greet");
    assert_eq!(&*rows[0].value, "hello world");

    // ── 4. validate diag: arg-kind mismatch ────────────────────────
    // Synthetic def that demands one Pipe arg. Hand it an Atom, expect
    // a `lower/wrong-arg-kind` diag.
    struct PipeOnly;
    const PIPE_ONLY_SPEC: &[ArgSig] = &[ArgSig {
        kind: ArgKind::Pipe, name: "p", doc: "", required: true,
    }];
    impl OperatorDef for PipeOnly {
        fn name(&self) -> &'static str { "pipe_only" }
        fn paren_args(&self) -> &[ArgSig] { PIPE_ONLY_SPEC }
        fn lower(
            &self, _: &LowerCtx, _: Option<Value>, _: &[Value],
            _: Option<Pipe<Cursor>>, _: Option<&DslBody>,
        ) -> Result<Pipe<Cursor>, v4::lower::LowerError> {
            unreachable!()
        }
    }
    let def = PipeOnly;
    let diags: Vec<Diag> = validate_call(
        &def, &None,
        &[(Value::atom("nope"), br(10, 14))],
        &None, &None, br(0, 20),
    );
    assert!(
        diags.iter().any(|d| &*d.code == "lower/wrong-arg-kind"),
        "expected wrong-arg-kind, got: {:?}",
        diags.iter().map(|d| (d.code.as_ref(), d.message.as_str())).collect::<Vec<_>>()
    );

    // Bonus: also cover `slot-not-allowed` via a stray block on a def
    // that doesn't take one.
    let body_pipe = str_pipe("anything");
    let diags = validate_call(
        &def, &None,
        &[(Value::pipe(str_pipe("x")), br(10, 14))],
        &Some((body_pipe, br(15, 20))),
        &None, br(0, 25),
    );
    assert!(
        diags.iter().any(|d| &*d.code == "lower/slot-not-allowed"),
        "expected slot-not-allowed, got: {:?}", diags
    );

    // Block + DSL absent on a def that requires both: missing-slot
    // emitted twice for `rule` (no block) and `str` (no dsl).
    let str_def_ref: &dyn OperatorDef = &*str_def;
    let diags = validate_call(str_def_ref, &None, &[], &None, &None, br(0, 4));
    assert!(diags.iter().any(|d| &*d.code == "lower/missing-slot"));

    // BlockShape is exposed for downstream introspection.
    assert_eq!(reg.get("rule").unwrap().brace_block(), Some(BlockShape::Pipe));
    assert_eq!(reg.get("str").unwrap().dsl_body(),    Some(DslShape::Plain));
}

// helper: drain a lowered Pipe<Cursor> into Vec<Cursor>.
fn collect(p: Pipe<Cursor>) -> Vec<Cursor> {
    use std::sync::Mutex;
    use effect_runtime::v2::{Component, Node, PipeInstance, RenderCtx};
    let sink: Arc<Mutex<Vec<Cursor>>> = Arc::new(Mutex::new(Vec::new()));
    struct Sink(Arc<Mutex<Vec<Cursor>>>);
    impl Component for Sink {
        type Next = Cursor;
        fn render(&self, _c: &RenderCtx, c: &Cursor) -> Node<Cursor> {
            self.0.lock().unwrap().push(c.clone()); Node::Done
        }
    }
    let mut steps: Vec<Arc<dyn Component<Next = Cursor>>> =
        p.steps.iter().cloned().collect();
    steps.push(Arc::new(Sink(sink.clone())));
    let inst = PipeInstance::new(steps);
    let q: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    expand(&inst, q, vec![Arc::new(Cursor::default())], ExpandOpts::default());
    let v = sink.lock().unwrap().clone();
    v
}
