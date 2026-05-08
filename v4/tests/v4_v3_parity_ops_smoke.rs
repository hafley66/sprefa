//! Smoke for v3-parity ops added to v4: void, print, sh, sh!, repo.
//!
//! Each op lowers cleanly via the registry and produces the expected
//! cursor stream when expanded against a seed.

use std::sync::Arc;

use effect_runtime::v2::{
    expand, ByteRange, Component, ExpandOpts, FactStore, MemFactStore, MemQueue,
    Node, PipeInstance, QueueBackend, RenderCtx,
};

use v4::Cursor;
use v4::lower::{default_registry, DslBody, LowerCtx, Value};

fn br(lo: u32, hi: u32) -> ByteRange { ByteRange { lo, hi } }

fn run(p: effect_runtime::v2::Pipe<Cursor>, seed: Cursor) -> Vec<Cursor> {
    run_many(p, vec![seed])
}

fn run_many(p: effect_runtime::v2::Pipe<Cursor>, seed: Vec<Cursor>) -> Vec<Cursor> {
    use std::sync::Mutex;
    let sink: Arc<Mutex<Vec<Cursor>>> = Arc::new(Mutex::new(Vec::new()));
    struct Sink(Arc<Mutex<Vec<Cursor>>>);
    impl Component for Sink {
        type Next = Cursor;
        fn render(&self, _c: &RenderCtx, c: &Cursor) -> Node<Cursor> {
            self.0.lock().unwrap().push(c.clone()); Node::Done
        }
    }
    let mut steps: Vec<Arc<dyn Component<Next = Cursor>>> = p.steps.iter().cloned().collect();
    steps.push(Arc::new(Sink(sink.clone())));
    let inst = PipeInstance::new(steps);
    let q: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    let seed = seed.into_iter().map(Arc::new).collect();
    expand(&inst, q, seed, ExpandOpts::default());
    let v = sink.lock().unwrap().clone();
    v
}

#[test]
fn void_drops_cursor() {
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let reg   = default_registry();
    let p = reg.lower(
        &LowerCtx::new(store.clone(), std::env::temp_dir()),
        "void", None, vec![], None, None, br(0, 4),
    ).unwrap();
    let mut seed = Cursor::default();
    seed.set("X", "1");
    let out = run(p, seed);
    assert!(out.is_empty(), "void must drop");
}

#[test]
fn print_passes_through() {
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let reg   = default_registry();
    // print`hello ${WHO}`
    let dsl = DslBody { raw: Arc::from("hello ${WHO}"), interps: vec![] };
    let p = reg.lower(
        &LowerCtx::new(store.clone(), std::env::temp_dir()),
        "print", None, vec![], None, Some((dsl, br(0, 12))), br(0, 12),
    ).unwrap();
    let mut seed = Cursor::default();
    seed.set("WHO", "world");
    let out = run(p, seed);
    assert_eq!(out.len(), 1, "print is a side-tap, cursor passes through");
    assert_eq!(out[0].get("WHO"), Some("world"));
}

#[test]
fn sh_filter_drops_on_nonzero_exit() {
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let reg   = default_registry();
    // sh(:filter)`false` — explicit filter mode (default is now capture).
    let dsl = DslBody { raw: Arc::from("false"), interps: vec![] };
    let p = reg.lower(
        &LowerCtx::new(store.clone(), std::env::temp_dir()),
        "sh", None,
        vec![(Value::atom("filter"), br(3, 10))],
        None, Some((dsl, br(0, 5))), br(0, 5),
    ).unwrap();
    let out = run(p, Cursor::default());
    assert!(out.is_empty(), "sh(:filter) with failing command drops cursor");
}

#[test]
fn sh_default_captures_stdout_into_value() {
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let reg   = default_registry();
    // sh`printf hello` — default mode now captures into &.value.
    let dsl = DslBody { raw: Arc::from("printf hello"), interps: vec![] };
    let p = reg.lower(
        &LowerCtx::new(store.clone(), std::env::temp_dir()),
        "sh", None, vec![], None, Some((dsl, br(0, 12))), br(0, 12),
    ).unwrap();
    let out = run(p, Cursor::default());
    assert_eq!(out.len(), 1);
    assert_eq!(&*out[0].value, "hello");
}

#[test]
fn sh_capture_binds_stdout_to_term() {
    use v4::lower::Value as V;
    use v4::pipeline::str_pipe;
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let reg   = default_registry();
    // sh(OUT?)`printf alpha`
    // OUT? lowers (in real walk) to Value::Pipe(term_bind(:OUT)). We
    // synthesize that here with a Pipe that binds OUT.
    use v4::term::Term;
    let bind_pipe = effect_runtime::v2::Pipe::new()
        .step(Arc::new(Term::bind(Arc::<str>::from("OUT"))));
    let dsl = DslBody { raw: Arc::from("printf alpha"), interps: vec![] };
    let p = reg.lower(
        &LowerCtx::new(store.clone(), std::env::temp_dir()),
        "sh", None,
        vec![(V::pipe(bind_pipe), br(3, 7))],
        None,
        Some((dsl, br(8, 22))),
        br(0, 22),
    ).unwrap();
    let _ = str_pipe; // silence unused
    let out = run(p, Cursor::default());
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].get("OUT"), Some("alpha"));
}

#[test]
fn sh_bang_passes_through_without_capture() {
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let reg   = default_registry();
    let dsl = DslBody { raw: Arc::from("true"), interps: vec![] };
    let p = reg.lower(
        &LowerCtx::new(store.clone(), std::env::temp_dir()),
        "sh!", None, vec![], None, Some((dsl, br(0, 4))), br(0, 4),
    ).unwrap();
    let out = run(p, Cursor::default());
    assert_eq!(out.len(), 1, "sh! passes cursor through");
}

#[test]
fn split_on_value_emits_cursor_per_piece() {
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let reg   = default_registry();
    let dsl = DslBody { raw: Arc::from("\n"), interps: vec![] };
    let p = reg.lower(
        &LowerCtx::new(store.clone(), std::env::temp_dir()),
        "split", None, vec![], None, Some((dsl, br(0, 1))), br(0, 1),
    ).unwrap();
    let mut seed = Cursor::default();
    seed.value = Arc::from("alpha\nbeta\n\ngamma");
    let out = run(p, seed);
    let vals: Vec<&str> = out.iter().map(|c| c.value.as_ref()).collect();
    assert_eq!(vals, vec!["alpha", "beta", "gamma"]);
}

#[test]
fn read_loads_file_bytes_into_value() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hello.txt");
    let mut f = std::fs::File::create(&path).unwrap();
    write!(f, "alpha-bravo").unwrap();

    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let reg   = default_registry();
    let p = reg.lower(
        &LowerCtx::new(store.clone(), std::env::temp_dir()),
        "read", None, vec![], None, None, br(0, 4),
    ).unwrap();
    // Substrate cleanup (2026-05-07): `read` reads `cursor.value` as
    // the path source. The legacy `cursor.get("FS")` fallback was
    // removed; producers (fs / glob) now set value directly.
    let mut seed = Cursor::default();
    seed.value = Arc::<str>::from(path.display().to_string().as_str());
    let out = run(p, seed);
    assert_eq!(out.len(), 1);
    assert_eq!(&*out[0].value, "alpha-bravo");
}

#[test]
fn comment_sequential_narrows_value_between_markers() {
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let reg   = default_registry();
    let dsl = DslBody { raw: Arc::from(r"# SECTION:.*\n"), interps: vec![] };
    let p = reg.lower(
        &LowerCtx::new(store.clone(), std::env::temp_dir()),
        "comment", None, vec![], None, Some((dsl, br(0, 14))), br(0, 14),
    ).unwrap();
    let mut seed = Cursor::default();
    seed.value = Arc::from("# SECTION: a\nlineA\n# SECTION: b\nlineB\n");
    let out = run(p, seed);
    assert_eq!(out.len(), 2);
    assert_eq!(&*out[0].value, "lineA\n");
    assert_eq!(&*out[1].value, "lineB\n");
}

#[test]
fn write_cursor_replace_splices_value_into_byte_range() {
    use std::io::Write;
    let dir  = tempfile::tempdir().unwrap();
    let path = dir.path().join("w.txt");
    let mut f = std::fs::File::create(&path).unwrap();
    write!(f, "abcdefghij").unwrap();

    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let reg   = default_registry();
    let p = reg.lower(
        &LowerCtx::new(store.clone(), std::env::temp_dir()),
        "write_cursor", None, vec![], None, None, br(0, 12),
    ).unwrap();
    // two cursors targeting the same file. Right-to-left order means
    // the second splice (HI=8) lands first, so LO=2 stays valid.
    let mk = |lo: usize, hi: usize, v: &str| -> Cursor {
        let mut c = Cursor::default();
        c.set("FS", path.display().to_string());
        c.set("LO", lo.to_string());
        c.set("HI", hi.to_string());
        c.value = Arc::from(v);
        c
    };
    use std::sync::Mutex;
    use effect_runtime::v2::{MemQueue, PipeInstance, QueueBackend};
    let sink: Arc<Mutex<Vec<Cursor>>> = Arc::new(Mutex::new(Vec::new()));
    struct Sink(Arc<Mutex<Vec<Cursor>>>);
    impl Component for Sink {
        type Next = Cursor;
        fn render(&self, _c: &RenderCtx, c: &Cursor) -> Node<Cursor> {
            self.0.lock().unwrap().push(c.clone()); Node::Done
        }
    }
    let mut steps: Vec<Arc<dyn Component<Next = Cursor>>> = p.steps.iter().cloned().collect();
    steps.push(Arc::new(Sink(sink.clone())));
    let inst = PipeInstance::new(steps);
    let q: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    expand(&inst, q,
        vec![Arc::new(mk(2, 4, "XX")), Arc::new(mk(7, 9, "YY"))],
        ExpandOpts::default());
    let written = std::fs::read_to_string(&path).unwrap();
    assert_eq!(written, "abXXefgYYj");
}

#[test]
fn write_file_writes_value_to_path_arg() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.txt");
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let reg   = default_registry();
    let p = reg.lower(
        &LowerCtx::new(store.clone(), std::env::temp_dir()),
        "write_file", None,
        vec![(Value::atom(path.display().to_string().as_str()), br(11, 30))],
        None, None, br(0, 32),
    ).unwrap();
    let mut seed = Cursor::default();
    seed.value = Arc::from("payload");
    let _ = run(p, seed);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "payload");
}

#[test]
fn collect_emits_one_cursor_after_batch_completion() {
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let reg   = default_registry();
    let p = reg.lower(
        &LowerCtx::new(store.clone(), std::env::temp_dir()),
        "collect", None,
        vec![],
        None, None, br(0, 9),
    ).unwrap();

    let mut a = Cursor::default();
    a.value = Arc::from("a");
    let mut b = Cursor::default();
    b.value = Arc::from("b");
    b.set("K", "from-b");

    let out = run_many(p, vec![a, b]);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].value.as_ref(), "ab");
}

#[test]
fn collect_then_write_file_writes_aggregate_value_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.txt");
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let reg   = default_registry();
    let collect = reg.lower(
        &LowerCtx::new(store.clone(), std::env::temp_dir()),
        "collect", None,
        vec![],
        None, None, br(0, 9),
    ).unwrap();
    let write_file = reg.lower(
        &LowerCtx::new(store.clone(), std::env::temp_dir()),
        "write_file", None,
        vec![(Value::atom(path.display().to_string().as_str()), br(11, 30))],
        None, None, br(0, 32),
    ).unwrap();

    let p = collect.extend(write_file);
    let mut a = Cursor::default();
    a.value = Arc::from("hello ");
    let mut b = Cursor::default();
    b.value = Arc::from("world");

    let out = run_many(p, vec![a, b]);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].value.as_ref(), "hello world");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello world");
}

#[test]
fn collect_ready_accepts_snapshot_and_append_modes() {
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let reg   = default_registry();
    for mode in ["snapshot", "append"] {
        let p = reg.lower(
            &LowerCtx::new(store.clone(), std::env::temp_dir()),
            "collect_ready", None,
            vec![(Value::atom(mode), br(14, 22))],
            None, None, br(0, 23),
        ).unwrap();

        let mut a = Cursor::default();
        a.value = Arc::from("x");
        let mut b = Cursor::default();
        b.value = Arc::from("y");

        let out = run_many(p, vec![a, b]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].value.as_ref(), "xy");
    }
}

#[test]
fn repo_emits_cursor_per_file_with_repo_and_fs_terms() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let mut f = std::fs::File::create(root.join("a.rs")).unwrap();
    writeln!(f, "fn a() {{}}").unwrap();
    let mut f = std::fs::File::create(root.join("b.rs")).unwrap();
    writeln!(f, "fn b() {{}}").unwrap();

    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let reg   = default_registry();
    let slug_arg = (Value::atom("myrepo"), br(0, 6));
    let path_arg = (Value::atom(root.display().to_string().as_str()), br(7, 20));
    let p = reg.lower(
        &LowerCtx::new(store.clone(), std::env::temp_dir()),
        "repo", None,
        vec![slug_arg, path_arg],
        None, None, br(0, 25),
    ).unwrap();
    let out = run(p, Cursor::default());
    assert_eq!(out.len(), 2, "two files in fixture");
    for c in &out {
        assert_eq!(c.get("REPO"), Some("myrepo"));
        assert!(c.get("FS").is_some());
    }
}
