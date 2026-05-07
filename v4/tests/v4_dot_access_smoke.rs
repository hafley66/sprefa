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

use effect_runtime::v2::{FactStore, MemFactStore};

use v4::{Coord, Cursor};
use v4::lower::op_def::{default_plain_dsl_parse, InterpMode};
use v4::pipeline::StrTemplateComponent;
use v4::store::SprfStore;
use v4::v2_ops::ReComponent;

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

#[test]
fn term_dot_access_lo_hi_fs_resolve_via_raw_terms() {
    // Replicates the emit-site pattern: set_at + parallel <NAME>_LO/HI/FS
    // raw_terms writes. Verifies Cursor::get dot-access dispatches to the
    // backfilled columns.
    let inner: Arc<dyn FactStore<Cursor>> =
        Arc::new(MemFactStore::<Cursor>::new());
    let store = SprfStore::new(inner);
    let file_id = store.intern_file(b"hello world", "/tmp/x.rs");
    let coord = Coord { repo: 0, rev: 0, fs: file_id, lo: 10, hi: 15 };

    let mut cur = Cursor::default();
    cur.set_at("NAME", "value", coord, &store);
    cur.set("NAME_LO", coord.lo.to_string());
    cur.set("NAME_HI", coord.hi.to_string());
    if let Some(path) = store.path_of(coord.fs) {
        cur.set("NAME_FS", &*path);
    }

    assert_eq!(cur.get("NAME.lo"), Some("10"));
    assert_eq!(cur.get("NAME.hi"), Some("15"));
    assert_eq!(cur.get("NAME.fs"), Some("/tmp/x.rs"));
    // Bare term value still readable via legacy raw_terms (set_at does
    // NOT write the bare <NAME> column; emitters separately call set).
    assert!(cur.get("NAME.zorp").is_none());
}

#[test]
fn re_component_backfills_named_group_lo_hi_fs() {
    use std::io::Write;
    use effect_runtime::v2::{
        expand, ExpandOpts, MemQueue, PipeInstance, QueueBackend,
    };

    // Tempfile holding `fn foo` and `fn bar`. ReComponent named group X
    // captures the identifier after `fn `.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.rs");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(b"fn foo() {}\nfn bar() {}\n").unwrap();
    drop(f);

    let inner: Arc<dyn FactStore<Cursor>> =
        Arc::new(MemFactStore::<Cursor>::new());
    let store = SprfStore::new(inner);

    let re = Arc::new(
        ReComponent::new(r"fn (?P<X>\w+)", &["X"])
            .with_sprf_store(store.clone()),
    );

    // Substrate purification: ReComponent consumes cursor.value bytes.
    // Upstream of `re` would normally be `... > read`; here we seed
    // value directly to the file content.
    let mut seed = Cursor::default();
    seed.set("FS", path.to_string_lossy().as_ref());
    seed.value = std::sync::Arc::<str>::from(
        std::fs::read_to_string(&path).unwrap().as_str(),
    );

    let sink: Arc<Mutex<Vec<Cursor>>> = Arc::new(Mutex::new(Vec::new()));
    struct Sink(Arc<Mutex<Vec<Cursor>>>);
    impl Component for Sink {
        type Next = Cursor;
        fn render(&self, _c: &RenderCtx, c: &Cursor) -> Node<Cursor> {
            self.0.lock().unwrap().push(c.clone()); Node::Done
        }
    }
    let steps: Vec<Arc<dyn Component<Next = Cursor>>> =
        vec![re, Arc::new(Sink(sink.clone()))];
    let inst = PipeInstance::new(steps);
    let q: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    expand(&inst, q, vec![Arc::new(seed)], ExpandOpts::default());

    let hits = sink.lock().unwrap().clone();
    assert_eq!(hits.len(), 2, "two `fn <name>` matches");

    // First match: `fn foo` at bytes 0..6, group X at 3..6.
    let h0 = &hits[0];
    assert_eq!(h0.get("X"),    Some("foo"));
    assert_eq!(h0.get("X.lo"), Some("3"));
    assert_eq!(h0.get("X.hi"), Some("6"));
    assert_eq!(h0.get("X.fs"), Some(path.to_string_lossy().as_ref()));

    // MATCH backfill too.
    assert_eq!(h0.get("MATCH.lo"), Some("0"));
    assert_eq!(h0.get("MATCH.hi"), Some("6"));
    assert_eq!(h0.get("MATCH.fs"), Some(path.to_string_lossy().as_ref()));

    // Second match: `fn bar` at bytes 12..18, group X at 15..18.
    let h1 = &hits[1];
    assert_eq!(h1.get("X.lo"), Some("15"));
    assert_eq!(h1.get("X.hi"), Some("18"));

    // Negative: an unstamped sub-field stays None.
    assert!(h0.get("X.zorp").is_none());
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
