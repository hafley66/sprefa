//! glob op smoke. Build a tempdir with two .rs files and one .txt;
//! lower `fs > glob`**/*.rs``; assert exactly two cursors out, each
//! carrying the matched .rs file path as `cursor.value` (and as the
//! legacy FS term).
//!
//! Substrate cleanup (2026-05-07): `glob` is a pure text matcher over
//! `cursor.value`; filesystem enumeration belongs to `fs`. The old
//! `glob(:atom)` paren-atom form was removed.

use std::sync::{Arc, Mutex};

use effect_runtime::v2::{
    expand, Component, ExpandOpts, FactStore, MemFactStore,
    MemQueue, Node, Pipe, PipeInstance, QueueBackend, RenderCtx,
};

use v4::Cursor;
use v4::lower::{default_registry, LowerCtx};

#[test]
fn glob_emits_one_cursor_per_matching_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().to_path_buf();
    std::fs::write(root.join("a.rs"),  b"fn a() {}").unwrap();
    std::fs::write(root.join("b.rs"),  b"fn b() {}").unwrap();
    std::fs::write(root.join("c.txt"), b"hello").unwrap();

    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let reg   = default_registry();
    let ctx   = LowerCtx::new(store, root.clone());

    // fs emits one cursor per file (value=path); glob filters to .rs.
    let fs_pipe = reg.lower(
        &ctx, "fs", None, vec![], None, None,
        effect_runtime::v2::ByteRange { lo: 0, hi: 2 },
    ).expect("fs lowers");
    let glob_pipe = reg.lower(
        &ctx, "glob", None, vec![], None,
        Some((v4::compile::lower::op_def::DslBody {
            raw:     Arc::<str>::from("**/*.rs"),
            interps: vec![],
        }, effect_runtime::v2::ByteRange { lo: 5, hi: 14 })),
        effect_runtime::v2::ByteRange { lo: 3, hi: 14 },
    ).expect("glob lowers");

    let combined = combine(fs_pipe, glob_pipe);
    let out = collect(combined);

    assert_eq!(out.len(), 2, "expected 2 .rs matches, got {out:?}");
    let mut names: Vec<String> = out.iter().map(|c| c.value.to_string()).collect();
    names.sort();
    assert!(names[0].ends_with("a.rs"), "first value = {:?}", names[0]);
    assert!(names[1].ends_with("b.rs"), "second value = {:?}", names[1]);
    // Legacy FS term is still stamped by fs() for back-compat.
    for c in &out {
        let fs = c.get("FS").unwrap_or("").to_string();
        assert!(fs.ends_with(".rs"), "FS term = {fs:?}");
    }
}

fn combine(a: Pipe<Cursor>, b: Pipe<Cursor>) -> Pipe<Cursor> {
    let mut out = a;
    for step in b.steps.iter().cloned() { out = out.step(step); }
    out
}

fn collect(p: Pipe<Cursor>) -> Vec<Cursor> {
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
