//! glob op smoke. Build a tempdir with two .rs files and one .txt;
//! lower a `glob(:**/*.rs)` call; assert exactly two cursors out, each
//! carrying an `FS` term pointing at one of the .rs files.

use std::sync::{Arc, Mutex};

use effect_runtime::v2::{
    expand, ByteRange, Component, ExpandOpts, FactStore, MemFactStore,
    MemQueue, Node, Pipe, PipeInstance, QueueBackend, RenderCtx,
};

use v4::Cursor;
use v4::lower::{default_registry, LowerCtx, Value};

fn br(lo: u32, hi: u32) -> ByteRange { ByteRange { lo, hi } }

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

    let pipe = reg.lower(
        &ctx,
        "glob",
        None,
        vec![(Value::atom("**/*.rs"), br(6, 13))],
        None,
        None,
        br(0, 14),
    ).expect("glob lowers");

    let out = collect(pipe);
    assert_eq!(out.len(), 2, "expected 2 .rs matches, got {out:?}");
    let mut names: Vec<String> = out.iter()
        .filter_map(|c| c.get("FS").map(|s| s.to_string()))
        .collect();
    names.sort();
    assert!(names[0].ends_with("a.rs"), "first FS = {:?}", names[0]);
    assert!(names[1].ends_with("b.rs"), "second FS = {:?}", names[1]);
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
