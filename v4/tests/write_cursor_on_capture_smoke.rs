//! Layer 3 smoke — `write_cursor(:replace, :NAME)` capture-internal
//! splice plus the on-disk drift-detection diag.
//!
//! Direct-component construction (no DSL pipeline harness): each test
//! builds a single-step PipeInstance over `WriteCursorComponent`, hand-
//! rolls one or more cursors with the appropriate coord-space terms,
//! and drives them through `expand`. Avoids the named-capture
//! lower-from-`re` path which is still wired bare-string in v4.

use std::sync::{Arc, Mutex};

use effect_runtime::v2::{
    expand, Component, Diag, DiagSink, ExpandOpts, FactStore, MemFactStore, MemQueue, Node,
    PipeInstance, QueueBackend, RenderCtx,
};

use v4::store::SprfStore;
use v4::v2_ops::{WriteCursorComponent, WriteMode};
use v4::{Coord, Cursor};

struct CollectorSink {
    sink: Arc<Mutex<Vec<Diag>>>,
}
impl DiagSink for CollectorSink {
    fn emit(&self, d: Diag) {
        self.sink.lock().unwrap().push(d);
    }
}

struct Collector {
    sink: Arc<Mutex<Vec<Cursor>>>,
}
impl Component for Collector {
    type Next = Cursor;
    fn render(&self, _c: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        self.sink.lock().unwrap().push(c.clone());
        Node::Done
    }
}

/// Drive a one-component pipeline over `seeds` and collect the diags
/// emitted along the way. Returns (cursors, diags).
fn run_with_diag(
    comp: Arc<dyn Component<Next = Cursor>>,
    seeds: Vec<Arc<Cursor>>,
) -> (Vec<Cursor>, Vec<Diag>) {
    let cursors: Arc<Mutex<Vec<Cursor>>> = Arc::new(Mutex::new(Vec::new()));
    let diags: Arc<Mutex<Vec<Diag>>> = Arc::new(Mutex::new(Vec::new()));
    let pipe = PipeInstance::new(vec![
        comp,
        Arc::new(Collector {
            sink: cursors.clone(),
        }),
    ]);
    let q: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    let opts = ExpandOpts::default().with_diag(Arc::new(CollectorSink {
        sink: diags.clone(),
    }));
    expand(&pipe, q, seeds, opts);
    let c = cursors.lock().unwrap().clone();
    let d = diags.lock().unwrap().clone();
    (c, d)
}

/// Helper: write `content` to `<dir>/<name>` and return the path string.
fn write_fixture(dir: &std::path::Path, name: &str, content: &[u8]) -> String {
    let p = dir.join(name);
    std::fs::write(&p, content).unwrap();
    p.display().to_string()
}

/// Build a cursor whose `terms` carries one named capture `NAME` whose
/// `at` Ref points at the byte range `[lo, hi)` of `file_id` in the
/// store. The cursor's focal `value` is the replacement payload.
fn cursor_with_capture(
    store: &SprfStore,
    name: &str,
    file_id: v4::FileId,
    lo: u32,
    hi: u32,
    payload: &str,
    capture_slice: &str,
) -> Arc<Cursor> {
    let mut c = Cursor::default();
    c.value = Arc::from(payload);
    let coord = Coord {
        repo: 0,
        rev: 0,
        fs: file_id,
        lo,
        hi,
    };
    c.set_at(name, capture_slice, coord, store);
    Arc::new(c)
}

// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
//                              tests
// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░

/// PRIMARY — `write_cursor(:replace, :NAME)` splices only the NAME
/// capture's byte range. Surrounding text untouched.
#[test]
fn write_cursor_on_capture_replaces_capture_bytes_only() {
    let tmp = tempfile::tempdir().unwrap();
    let path = write_fixture(tmp.path(), "foo.rs", b"fn old_name() {}\n");

    let facts: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let store = SprfStore::new(facts.clone());

    // Seed _files with the on-disk content so coord_of/lookup_file resolve.
    let bytes = std::fs::read(&path).unwrap();
    let file_id = store.intern_file(&bytes, &path);

    // `fn old_name() {}` — NAME captures bytes [3, 11) = "old_name".
    let seed = cursor_with_capture(&store, "NAME", file_id, 3, 11, "new_name", "old_name");

    let comp: Arc<dyn Component<Next = Cursor>> = Arc::new(
        WriteCursorComponent::new(WriteMode::Replace)
            .on_capture("NAME")
            .with_sprf_store(store.clone()),
    );
    let (_cursors, diags) = run_with_diag(comp, vec![seed]);

    let after = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        after, "fn new_name() {}\n",
        "write_cursor(:replace, :NAME) splices only the NAME capture range",
    );
    assert!(
        diags.iter().all(|d| &*d.code != "write/file-drift"),
        "no drift diag expected; got {:?}",
        diags,
    );
}

/// `:on=NAME` with multiple captures on the same cursor — write only
/// NAME's bytes. The other capture's range is unaffected.
#[test]
fn write_cursor_on_capture_leaves_other_captures_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let path = write_fixture(tmp.path(), "multi.rs", b"fn alpha(beta: u32) {}\n");

    let facts: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let store = SprfStore::new(facts.clone());
    let bytes = std::fs::read(&path).unwrap();
    let file_id = store.intern_file(&bytes, &path);

    // Two captures: FN_NAME = "alpha" at [3, 8), ARG_NAME = "beta" at [9, 13).
    let mut c = Cursor::default();
    c.value = Arc::from("renamed");
    let fn_coord = Coord {
        repo: 0,
        rev: 0,
        fs: file_id,
        lo: 3,
        hi: 8,
    };
    let arg_coord = Coord {
        repo: 0,
        rev: 0,
        fs: file_id,
        lo: 9,
        hi: 13,
    };
    c.set_at("FN_NAME", "alpha", fn_coord, &store);
    c.set_at("ARG_NAME", "beta", arg_coord, &store);
    let seed = Arc::new(c);

    let comp: Arc<dyn Component<Next = Cursor>> = Arc::new(
        WriteCursorComponent::new(WriteMode::Replace)
            .on_capture("FN_NAME")
            .with_sprf_store(store.clone()),
    );
    let (_cursors, _diags) = run_with_diag(comp, vec![seed]);

    let after = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        after, "fn renamed(beta: u32) {}\n",
        "only FN_NAME's bytes get rewritten; ARG_NAME's range is left alone",
    );
}

/// Focal default still works (no second atom). Uses the FS/LO/HI
/// term path with no SprfStore wiring.
#[test]
fn write_cursor_focal_default_replaces_match_range() {
    let tmp = tempfile::tempdir().unwrap();
    let path = write_fixture(tmp.path(), "focal.rs", b"fn target() {}\n");

    // Mimic what ReComponent / GlobComponent stamp on the cursor for
    // the focal-mode path: bare-string FS/LO/HI plus a focal value.
    let mut c = Cursor::default();
    c.set("FS", path.clone());
    c.set("LO", "0");
    c.set("HI", "9"); // "fn target"
    c.value = Arc::from("fn renamed");
    let seed = Arc::new(c);

    let comp: Arc<dyn Component<Next = Cursor>> =
        Arc::new(WriteCursorComponent::new(WriteMode::Replace));

    let (_cursors, _diags) = run_with_diag(comp, vec![seed]);

    let after = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        after, "fn renamed() {}\n",
        "focal-mode replace still operates on FS/LO/HI terms",
    );
}

/// Drift-detection — when the file on disk has been rewritten between
/// `intern_file` and `write_cursor`, the splice is skipped and a
/// `write/file-drift` diag is emitted. The on-disk content stays at
/// the externally-rewritten value.
#[test]
fn write_cursor_drift_detection_skips_and_diags() {
    let tmp = tempfile::tempdir().unwrap();
    let path = write_fixture(tmp.path(), "drift.rs", b"fn target() {}\n");

    let facts: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let store = SprfStore::new(facts.clone());

    // Step 1 — intern at the original content_hash.
    let original = std::fs::read(&path).unwrap();
    let file_id = store.intern_file(&original, &path);

    // NAME = "target" at [3, 9).
    let seed = cursor_with_capture(&store, "NAME", file_id, 3, 9, "renamed", "target");

    // Step 2 — external rewrite. Different content_hash now.
    std::fs::write(&path, b"fn other() {}\n").unwrap();

    // Step 3 — run write_cursor.
    let comp: Arc<dyn Component<Next = Cursor>> = Arc::new(
        WriteCursorComponent::new(WriteMode::Replace)
            .on_capture("NAME")
            .with_sprf_store(store.clone()),
    );
    let (_cursors, diags) = run_with_diag(comp, vec![seed]);

    // File untouched by write_cursor — external content remains.
    let after = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        after, "fn other() {}\n",
        "drift detected → write_cursor skipped → external edit preserved",
    );
    assert!(
        diags.iter().any(|d| &*d.code == "write/file-drift"),
        "expected write/file-drift diag; got {:?}",
        diags.iter().map(|d| d.code.to_string()).collect::<Vec<_>>(),
    );
}
