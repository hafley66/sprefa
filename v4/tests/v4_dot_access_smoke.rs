//! Layer 0c.3 — dot-access carveout smoke.
//!
//! Two ends of the dispatch:
//!
//!   1. `default_plain_dsl_parse` recognises `${&.field}` (focal-cursor
//!      self-op) and `${X.field}` (term-property) alongside the existing
//!      `${X}` / `${X?}` forms; illegal combos (`${&}`, `${&?...}`,
//!      `${X?.field}`) are skipped (treated as literal text).
//!
//!   2. `StrTemplateComponent` renders a template against a Cursor that
//!      holds raw_terms `FS` and `NAME`; the dotted form `${&.fs}` reads
//!      focal `FS`, `${NAME.value}` reads the bare `NAME` term, and
//!      `${&.value}` reads `cursor.value`.
//!
//! Cursor::get's dot-access dispatch (lib.rs) is verified end-to-end via
//! the rendered output.

use std::sync::{Arc, Mutex};

use effect_runtime::v2::{
    expand, ExpandOpts, Component, MemQueue, Node, Pipe, PipeInstance,
    QueueBackend, RenderCtx,
};

use v4::Cursor;
use v4::lower::op_def::{default_plain_dsl_parse, InterpMode};
use v4::pipeline::StrTemplateComponent;

#[test]
fn scanner_parses_dot_access_and_focal_forms() {
    let raw = "X=${X} Xv=${X.value} Xfs=${X.fs} F=${&.value} Ffs=${&.fs} B=${X?}";
    let interps = default_plain_dsl_parse(raw);

    // Six legal interps in declaration order.
    assert_eq!(interps.len(), 6, "interps: {:?}", interps);

    // ${X}
    assert_eq!(&*interps[0].name, "X");
    assert!(interps[0].field.is_none());
    assert!(matches!(interps[0].mode, InterpMode::Read));

    // ${X.value}
    assert_eq!(&*interps[1].name, "X");
    assert_eq!(interps[1].field.as_deref(), Some("value"));
    assert!(matches!(interps[1].mode, InterpMode::Read));

    // ${X.fs}
    assert_eq!(&*interps[2].name, "X");
    assert_eq!(interps[2].field.as_deref(), Some("fs"));

    // ${&.value}
    assert_eq!(&*interps[3].name, "&");
    assert_eq!(interps[3].field.as_deref(), Some("value"));

    // ${&.fs}
    assert_eq!(&*interps[4].name, "&");
    assert_eq!(interps[4].field.as_deref(), Some("fs"));

    // ${X?}
    assert_eq!(&*interps[5].name, "X");
    assert!(interps[5].field.is_none());
    assert!(matches!(interps[5].mode, InterpMode::Bind));
}

#[test]
fn scanner_skips_illegal_dot_access_forms() {
    // `${&}` (bare focal), `${&?.value}` (focal + bind), `${X?.value}`
    // (bind + field) — all illegal, treated as literal text and not
    // emitted as interps. `${legal}` is the lone legal form here.
    let raw = "${&} ${&?.value} ${X?.value} ${legal}";
    let interps = default_plain_dsl_parse(raw);
    assert_eq!(interps.len(), 1, "only ${{legal}} should parse, got {:?}", interps);
    assert_eq!(&*interps[0].name, "legal");
    assert!(interps[0].field.is_none());
}

#[test]
fn template_renders_focal_fs_and_term_value() {
    // Build a Cursor with the same raw_terms shape that fs+re emitters
    // produce: FS for focal coord file path, NAME for a re named-group
    // capture, and cursor.value for `&.value`.
    let mut cur = Cursor::default();
    cur.value = Arc::<str>::from("focal-text");
    cur.set("FS", "/tmp/foo.rs");
    cur.set("NAME", "foobar");

    // Direct Cursor::get dispatch.
    assert_eq!(cur.get("&.value"), Some("focal-text"));
    assert_eq!(cur.get("&.fs"),    Some("/tmp/foo.rs"));
    assert_eq!(cur.get("NAME"),    Some("foobar"));
    assert_eq!(cur.get("NAME.value"), Some("foobar"));
    // X.lo not stamped to raw_terms by 0c.2 emitters; resolves to None.
    assert!(cur.get("NAME.lo").is_none());

    // End-to-end: scan a template, lower into StrTemplateComponent,
    // expand and assert the rendered focal value.
    let raw = "${&.fs} :: ${NAME.value}";
    let interps = default_plain_dsl_parse(raw);
    let comp = Arc::new(StrTemplateComponent {
        raw:     Arc::<str>::from(raw),
        interps: Arc::new(interps),
    });

    let pipe = Pipe::<Cursor>::new().step(comp);
    let out = collect(pipe, cur);
    assert_eq!(out.len(), 1);
    assert_eq!(&*out[0].value, "/tmp/foo.rs :: foobar");
}

fn collect(p: Pipe<Cursor>, seed: Cursor) -> Vec<Cursor> {
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
    expand(&inst, q, vec![Arc::new(seed)], ExpandOpts::default());
    let v = sink.lock().unwrap().clone();
    v
}
