//! Layer 5a smoke — bare `repo()` reads `LowerCtx::config` and emits
//! one cursor per configured repo. Args-form `repo(:slug, :path)` is
//! covered by `v4_v3_parity_ops_smoke::repo_emits_cursor_per_file_*`.
//!
//! Construct RepoComponent directly + drive `expand` with a custom
//! collecting sink. Same shape as the parity smoke; lower path is
//! exercised end-to-end by SprfState in app.rs.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use effect_runtime::v2::{
    expand, ByteRange, Component, ExpandOpts, FactStore, MemFactStore, MemQueue,
    Node, PipeInstance, QueueBackend, RenderCtx,
};

use v4::config::{RepoConfig, SprfConfig};
use v4::lower::{default_registry, LowerCtx, Value};
use v4::store::{SprfStore, REPOS_TABLE};
use v4::Cursor;

fn br(lo: u32, hi: u32) -> ByteRange { ByteRange { lo, hi } }

fn run(p: effect_runtime::v2::Pipe<Cursor>, seed: Cursor) -> Vec<Cursor> {
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
    expand(&inst, q, vec![Arc::new(seed)], ExpandOpts::default());
    let v = sink.lock().unwrap().clone();
    v
}

#[test]
fn repo_bare_form_emits_one_cursor_per_configured_repo() {
    let cfg = SprfConfig {
        repos: vec![
            RepoConfig { slug: "alpha/one".into(), root: PathBuf::from("/tmp/alpha") },
            RepoConfig { slug: "beta/two".into(),  root: PathBuf::from("/tmp/beta") },
        ],
        ..Default::default()
    };

    let facts: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let store = SprfStore::new(facts.clone());
    let reg   = default_registry();

    let ctx = LowerCtx::new(facts.clone(), std::env::temp_dir())
        .with_sprf_store(store.clone())
        .with_config(Arc::new(cfg));

    let p = reg.lower(
        &ctx, "repo", None, vec![], None, None, br(0, 6),
    ).expect("bare repo() lowers");

    let out = run(p, Cursor::default());
    assert_eq!(out.len(), 2, "one cursor per configured repo");

    let slugs: Vec<&str> = out.iter().map(|c| c.value.as_ref()).collect();
    assert!(slugs.contains(&"alpha/one"), "alpha/one slug present in {slugs:?}");
    assert!(slugs.contains(&"beta/two"),  "beta/two slug present in {slugs:?}");

    for c in &out {
        let repo = c.get("REPO").expect("REPO term set");
        assert_eq!(repo, c.value.as_ref(), "REPO term mirrors cursor.value");
        assert!(c.get("FS").is_none(), "bare form does not set FS");
    }

    // _repos table: sentinel (0) + 2 configured repos.
    store.flush();
    let repo_rows = facts.rows_of(REPOS_TABLE);
    assert_eq!(repo_rows.len(), 3, "sentinel + 2 configured repos in _repos");
}

#[test]
fn repo_bare_form_emits_zero_when_no_config() {
    let facts: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let store = SprfStore::new(facts.clone());
    let reg   = default_registry();

    // Empty config attached.
    let ctx = LowerCtx::new(facts.clone(), std::env::temp_dir())
        .with_sprf_store(store.clone())
        .with_config(Arc::new(SprfConfig::empty()));

    let p = reg.lower(
        &ctx, "repo", None, vec![], None, None, br(0, 6),
    ).expect("bare repo() lowers with empty config");

    let out = run(p, Cursor::default());
    assert!(out.is_empty(), "no configured repos => no cursors");
}

#[test]
fn repo_bare_form_emits_zero_when_ctx_lacks_config() {
    // No `with_config` call at all. The FromConfig branch silently
    // emits zero rows rather than panicking. This is the back-compat
    // path for older lower call sites that don't yet thread config.
    let facts: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let reg   = default_registry();

    let ctx = LowerCtx::new(facts.clone(), std::env::temp_dir());

    let p = reg.lower(
        &ctx, "repo", None, vec![], None, None, br(0, 6),
    ).expect("bare repo() lowers without config");

    let out = run(p, Cursor::default());
    assert!(out.is_empty(), "no config attached => no cursors");
}

#[test]
fn repo_args_form_back_compat_unchanged() {
    use std::io::Write;
    // Args form must still walk the filesystem under each (slug, path)
    // pair and emit one cursor per matching file. This pins back-compat
    // for callers like `repo(:my, :./path)` that have not yet migrated.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let mut f = std::fs::File::create(root.join("a.rs")).unwrap();
    writeln!(f, "fn a() {{}}").unwrap();

    let facts: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let reg   = default_registry();
    let ctx = LowerCtx::new(facts.clone(), std::env::temp_dir());

    let slug_arg = (Value::atom("myrepo"), br(0, 6));
    let path_arg = (Value::atom(root.display().to_string().as_str()), br(7, 20));
    let p = reg.lower(
        &ctx, "repo", None,
        vec![slug_arg, path_arg],
        None, None, br(0, 25),
    ).expect("args repo() lowers");

    let out = run(p, Cursor::default());
    assert_eq!(out.len(), 1, "one file in fixture");
    assert_eq!(out[0].get("REPO"), Some("myrepo"));
    assert!(out[0].get("FS").is_some(), "args form sets FS to file path");
}
