//! Layer 5b.2 smoke — `fs()` and `read()` over a non-zero `Coord.rev`.
//!
//! Exercises the end-to-end chain `repo() > rev(...) > fs() > read()`
//! against a synthetic 2-commit repo where one file is identical
//! across both revs (content dedupe via blake3 FileId) and another
//! file differs.
//!
//! Per memory `project_v4_git_blob_walk_uses_shell`, blob enumeration
//! shells out to `git ls-tree -r -z <oid>` and per-blob bytes come
//! from `git show <oid>:<path>`. libgit2 tree walk is forbidden.

use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};

use effect_runtime::v2::{
    expand, ByteRange, Component, ExpandOpts, FactStore, MemFactStore, MemQueue,
    Node, PipeInstance, QueueBackend, RenderCtx,
};

use v4::config::{RepoConfig, SprfConfig};
use v4::lower::{default_registry, LowerCtx, Value};
use v4::store::{
    SprfStore, FILES_TABLE, PATHS_TABLE, REPOS_TABLE, REVS_TABLE,
};
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
    let mut steps: Vec<Arc<dyn Component<Next = Cursor>>> =
        p.steps.iter().cloned().collect();
    steps.push(Arc::new(Sink(sink.clone())));
    let inst = PipeInstance::new(steps);
    let q: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    expand(&inst, q, vec![Arc::new(seed)], ExpandOpts::default());
    let v = sink.lock().unwrap().clone();
    v
}

/// Build a synthetic two-commit repo. `src/main.rs` is identical
/// across HEAD and HEAD~1 (one FileId on dedup). `README.md` differs
/// (`v1\n` vs `v2\n`, two FileIds).
fn build_two_commit_repo() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().to_path_buf();
    let g = |args: &[&str], date: &str| {
        let st = Command::new("git").args(args).current_dir(&p)
            .env("GIT_AUTHOR_NAME",     "t")
            .env("GIT_AUTHOR_EMAIL",    "t@t.com")
            .env("GIT_COMMITTER_NAME",  "t")
            .env("GIT_COMMITTER_EMAIL", "t@t.com")
            .env("GIT_AUTHOR_DATE",     date)
            .env("GIT_COMMITTER_DATE",  date)
            .output()
            .expect("git");
        if !st.status.success() {
            panic!("git {:?}: {}", args, String::from_utf8_lossy(&st.stderr));
        }
    };
    g(&["init", "-b", "main"], "2026-01-01T00:00:00Z");
    g(&["config", "user.email", "t@t.com"], "2026-01-01T00:00:00Z");
    g(&["config", "user.name",  "t"],       "2026-01-01T00:00:00Z");
    std::fs::create_dir_all(p.join("src")).unwrap();
    std::fs::write(p.join("src/main.rs"), b"fn main(){println!(\"hi\")}\n").unwrap();
    std::fs::write(p.join("README.md"),   b"v1\n").unwrap();
    g(&["add", "."],                       "2026-01-01T00:00:00Z");
    g(&["commit", "-m", "c1"],             "2026-01-01T00:00:00Z");
    std::fs::write(p.join("README.md"),   b"v2\n").unwrap();
    g(&["add", "."],                       "2026-01-02T00:00:00Z");
    g(&["commit", "-m", "c2"],             "2026-01-02T00:00:00Z");
    tmp
}

/// `repo() > rev(:HEAD, :HEAD~1) > fs() > read()`. The same Pipe is
/// reused across the three primary tests; building it once per test
/// keeps the lower call-site explicit.
fn build_pipe(
    ctx:      &LowerCtx,
    rev_args: Vec<(Value, ByteRange)>,
    do_fs:    bool,
    do_read:  bool,
) -> effect_runtime::v2::Pipe<Cursor> {
    let reg = default_registry();
    let p_repo = reg.lower(ctx, "repo", None, vec![],     None, None, br(0, 6))
        .expect("repo()");
    let p_rev  = reg.lower(ctx, "rev",  None, rev_args,   None, None, br(7, 30))
        .expect("rev(...)");
    let mut pipe = p_repo.extend(p_rev);
    if do_fs {
        let p_fs = reg.lower(ctx, "fs", None, vec![], None, None, br(31, 35))
            .expect("fs()");
        pipe = pipe.extend(p_fs);
    }
    if do_read {
        let p_read = reg.lower(ctx, "read", None, vec![], None, None, br(36, 42))
            .expect("read()");
        pipe = pipe.extend(p_read);
    }
    pipe
}

fn make_ctx(
    facts: Arc<dyn FactStore<Cursor>>,
    store: Arc<SprfStore>,
    cfg:   SprfConfig,
) -> LowerCtx {
    LowerCtx::new(facts, std::env::temp_dir())
        .with_sprf_store(store)
        .with_config(Arc::new(cfg))
}

/// PRIMARY ASSERTION TEST.
/// `repo() > rev(:HEAD, :HEAD~1) > fs() > read()` over the 2-commit
/// repo. Validates blob-content dedup (src/main.rs identical across
/// revs collapses to one FileId; README.md differs across revs and
/// produces two FileIds).
#[test]
fn cross_rev_dedupes_unchanged_content_across_revs() {
    let tmp  = build_two_commit_repo();
    let root: PathBuf = tmp.path().to_path_buf();
    let cfg  = SprfConfig {
        repos: vec![RepoConfig { slug: "t/r".into(), root: root.clone() }],
        ..Default::default()
    };
    let facts: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let store = SprfStore::new(facts.clone());
    let ctx   = make_ctx(facts.clone(), store.clone(), cfg);

    let pipe = build_pipe(
        &ctx,
        vec![
            (Value::atom("HEAD"),    br(8, 13)),
            (Value::atom("HEAD~1"),  br(15, 22)),
        ],
        /* fs */ true, /* read */ true,
    );

    let cursors = run(pipe, Cursor::default());
    store.flush();

    // 2 revs × 2 blobs = 4 cursor instances.
    assert_eq!(
        cursors.len(), 4,
        "2 revs × 2 files = 4 emissions; got {}",
        cursors.len(),
    );

    // _files = sentinel + 3 unique contents:
    //   src/main.rs identical across both revs → one FileId
    //   README.md "v1\n" vs "v2\n" → two distinct FileIds
    assert_eq!(
        facts.len(FILES_TABLE), 4,
        "_files: sentinel + 3 unique contents (src/main.rs dedupes; README differs across revs)",
    );

    // _paths = sentinel + 4 (one per (repo, rev, path) tuple).
    assert_eq!(
        facts.len(PATHS_TABLE), 5,
        "_paths: sentinel + 4 (HEAD,src/main.rs) (HEAD,README.md) (HEAD~1,src/main.rs) (HEAD~1,README.md)",
    );

    // _revs = sentinel + 2.
    assert_eq!(
        facts.len(REVS_TABLE), 3,
        "_revs: sentinel + HEAD + HEAD~1",
    );

    // _repos = sentinel + 1.
    assert_eq!(
        facts.len(REPOS_TABLE), 2,
        "_repos: sentinel + t/r",
    );
}

/// `fs()` over rev != 0 emits cursors with cursor.value = path string.
#[test]
fn fs_over_rev_emits_path_cursors() {
    let tmp  = build_two_commit_repo();
    let root: PathBuf = tmp.path().to_path_buf();
    let cfg  = SprfConfig {
        repos: vec![RepoConfig { slug: "t/r".into(), root: root.clone() }],
        ..Default::default()
    };
    let facts: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let store = SprfStore::new(facts.clone());
    let ctx   = make_ctx(facts.clone(), store.clone(), cfg);

    let pipe = build_pipe(
        &ctx,
        vec![(Value::atom("HEAD"), br(8, 13))],
        /* fs */ true, /* read */ false,
    );
    let cursors = run(pipe, Cursor::default());

    let mut paths: Vec<String> = cursors.iter().map(|c| c.value.to_string()).collect();
    paths.sort();
    assert_eq!(paths, vec!["README.md".to_string(), "src/main.rs".to_string()]);

    // Each cursor's coord has rev != 0, fs == 0 (read() not yet fired).
    for cursor in &cursors {
        let coord = store.coord_of(cursor.at).expect("coord_of");
        assert_ne!(coord.rev, 0, "fs() preserves upstream rev");
        assert_eq!(coord.fs, 0, "fs() does not intern files; that's read()'s job");
    }
}

/// `read()` over rev != 0 populates intern_file and flips cursor.value
/// from path to content.
#[test]
fn read_over_rev_populates_intern_file_and_flips_value() {
    let tmp  = build_two_commit_repo();
    let root: PathBuf = tmp.path().to_path_buf();
    let cfg  = SprfConfig {
        repos: vec![RepoConfig { slug: "t/r".into(), root: root.clone() }],
        ..Default::default()
    };
    let facts: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let store = SprfStore::new(facts.clone());
    let ctx   = make_ctx(facts.clone(), store.clone(), cfg);

    let pipe = build_pipe(
        &ctx,
        vec![(Value::atom("HEAD"), br(8, 13))],
        /* fs */ true, /* read */ true,
    );
    let cursors = run(pipe, Cursor::default());

    // Two cursor instances at HEAD: src/main.rs + README.md.
    assert_eq!(cursors.len(), 2);

    // Each cursor's value is the file content (not the path).
    let main_cursor = cursors.iter().find(|c| c.value.contains("fn main"))
        .expect("expected one cursor whose content contains `fn main`");
    assert_eq!(main_cursor.value.as_ref(), "fn main(){println!(\"hi\")}\n");

    let readme_cursor = cursors.iter().find(|c| c.value.starts_with("v"))
        .expect("expected one cursor whose content starts with `v`");
    assert_eq!(readme_cursor.value.as_ref(), "v2\n");

    // Each cursor's coord has fs != 0 now (intern_file fired).
    for cursor in &cursors {
        let coord = store.coord_of(cursor.at).expect("coord_of");
        assert_ne!(coord.rev, 0);
        assert_ne!(coord.fs, 0, "read() interns the file; coord.fs is the FileId");
        assert!(coord.hi > 0, "coord.hi is the file byte length");
    }
}

/// `fs()` over wt (rev == 0) preserves the WalkBuilder behavior —
/// no regression from the rev != 0 branch added in 5b.2. The rev
/// branch is gated on `Coord.rev != 0`; a bare seed (Cursor::default)
/// has rev == 0 so the existing WalkBuilder dispatch must still fire.
#[test]
fn fs_over_wt_preserves_walkbuilder_behavior() {
    // No git repo here — pure filesystem walk under tmp/. Drives the
    // bare `fs()` source-op against a Cursor::default seed; ctx.root
    // carries the path the WalkBuilder enumerates.
    let tmp = tempfile::tempdir().unwrap();
    let root: PathBuf = tmp.path().to_path_buf();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/main.rs"), b"fn main(){}\n").unwrap();
    std::fs::write(root.join("README.md"),   b"hello\n").unwrap();

    let facts: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let store = SprfStore::new(facts.clone());
    let ctx   = LowerCtx::new(facts.clone(), root.clone())
        .with_sprf_store(store.clone())
        .with_config(Arc::new(SprfConfig::empty()));

    // Bare `fs()`. With Cursor::default seed (rev == 0) the rev branch
    // skips and WalkBuilder enumerates ctx.root.
    let reg = default_registry();
    let p_fs = reg.lower(&ctx, "fs", None, vec![], None, None, br(0, 4))
        .expect("fs()");
    let cursors = run(p_fs, Cursor::default());
    store.flush();

    let mut paths: Vec<String> = cursors.iter()
        .filter_map(|c| c.get("FS").map(|s| s.to_string()))
        .collect();
    paths.sort();
    let any_main   = paths.iter().any(|p| p.ends_with("src/main.rs"));
    let any_readme = paths.iter().any(|p| p.ends_with("README.md"));
    assert!(any_main,   "expected an FS term ending in src/main.rs; got {paths:?}");
    assert!(any_readme, "expected an FS term ending in README.md; got {paths:?}");

    // No rev was resolved → _revs untouched (sentinel only).
    assert_eq!(facts.len(REVS_TABLE), 1, "_revs: only the sentinel");
}
