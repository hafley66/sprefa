//! Layer 5b.1 smoke — `rev()` op resolves revspecs against an upstream
//! repo cursor's `Coord.repo` via libgit2 and emits oid-stamped child
//! cursors. Builds a synthetic git repo via the system `git` CLI in a
//! tempdir; drives `repo() > rev(:HEAD)` end-to-end through the lower
//! registry; asserts oid is a 40-char hex, REV term is set, _revs has
//! one row beyond the sentinel.

use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};

use effect_runtime::v2::{
    expand, ByteRange, Component, ExpandOpts, FactStore, MemFactStore, MemQueue, Node,
    PipeInstance, QueueBackend, RenderCtx,
};

use v4::compile::parse::host_parse;
use v4::compile::walk::walk_program;
use v4::config::{RepoConfig, SprfConfig};
use v4::lower::{default_registry, LowerCtx, Value};
use v4::store::{SprfStore, REVS_TABLE};
use v4::Cursor;

fn br(lo: u32, hi: u32) -> ByteRange {
    ByteRange { lo, hi }
}

fn run(p: effect_runtime::v2::Pipe<Cursor>, seed: Cursor) -> Vec<Cursor> {
    let sink: Arc<Mutex<Vec<Cursor>>> = Arc::new(Mutex::new(Vec::new()));
    struct Sink(Arc<Mutex<Vec<Cursor>>>);
    impl Component for Sink {
        type Next = Cursor;
        fn render(&self, _c: &RenderCtx, c: &Cursor) -> Node<Cursor> {
            self.0.lock().unwrap().push(c.clone());
            Node::Done
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

/// Build a synthetic single-commit git repo at `path`. Returns nothing;
/// panics if any step fails (CI must have `git`). All authorship is
/// pinned via env so commit oids are deterministic-ish per-fixture.
fn init_git(path: &std::path::Path, files: &[(&str, &[u8])]) {
    let g = |args: &[&str]| {
        let st = Command::new("git")
            .args(args)
            .current_dir(path)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t.com")
            .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00Z")
            .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00Z")
            .output()
            .expect("git");
        if !st.status.success() {
            panic!(
                "git {:?} failed: {}",
                args,
                String::from_utf8_lossy(&st.stderr)
            );
        }
    };
    g(&["init", "-b", "main"]);
    g(&["config", "user.email", "t@t.com"]);
    g(&["config", "user.name", "t"]);
    for (name, bytes) in files {
        std::fs::write(path.join(name), bytes).unwrap();
    }
    g(&["add", "."]);
    g(&["commit", "-m", "init"]);
}

fn build_pipe(
    ctx: &LowerCtx,
    rev_args: Vec<(Value, ByteRange)>,
) -> effect_runtime::v2::Pipe<Cursor> {
    let reg = default_registry();
    let p_repo = reg
        .lower(ctx, "repo", None, vec![], None, None, br(0, 6))
        .expect("repo()");
    let p_rev = reg
        .lower(ctx, "rev", None, rev_args, None, None, br(7, 30))
        .expect("rev(...)");
    p_repo.extend(p_rev)
}

#[test]
fn rev_head_resolves_oid_and_intern_rev() {
    let dir = tempfile::tempdir().unwrap();
    let root: PathBuf = dir.path().to_path_buf();
    init_git(&root, &[("README.md", b"v1")]);

    let cfg = SprfConfig {
        repos: vec![RepoConfig {
            slug: "test/repo".into(),
            root: root.clone(),
        }],
        ..Default::default()
    };

    let facts: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let store = SprfStore::new(facts.clone());
    let ctx = LowerCtx::new(facts.clone(), std::env::temp_dir())
        .with_sprf_store(store.clone())
        .with_config(Arc::new(cfg));

    // `rev(:HEAD)`
    let pipe = build_pipe(&ctx, vec![(Value::atom("HEAD"), br(8, 13))]);

    let out = run(pipe, Cursor::default());
    assert_eq!(out.len(), 1, "one cursor for one repo × one revspec");

    let oid = out[0].value.as_ref();
    assert_eq!(oid.len(), 40, "oid is 40-char hex: {oid:?}");
    assert!(oid.chars().all(|c| c.is_ascii_hexdigit()), "oid is hex");
    assert_eq!(
        out[0].get("REV"),
        Some(oid),
        "REV term mirrors cursor.value"
    );

    // _revs table: sentinel + 1 resolved rev.
    store.flush();
    let rev_rows = facts.rows_of(REVS_TABLE);
    assert_eq!(rev_rows.len(), 2, "sentinel + 1 rev in _revs");
}

#[test]
fn rev_bare_form_defaults_to_head() {
    let dir = tempfile::tempdir().unwrap();
    let root: PathBuf = dir.path().to_path_buf();
    init_git(&root, &[("README.md", b"v1")]);

    let cfg = SprfConfig {
        repos: vec![RepoConfig {
            slug: "test/repo".into(),
            root: root.clone(),
        }],
        ..Default::default()
    };

    let facts: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let store = SprfStore::new(facts.clone());
    let ctx = LowerCtx::new(facts.clone(), std::env::temp_dir())
        .with_sprf_store(store.clone())
        .with_config(Arc::new(cfg));

    // bare `rev()` defaults to :HEAD
    let pipe_bare = build_pipe(&ctx, vec![]);
    let bare_out = run(pipe_bare, Cursor::default());
    assert_eq!(bare_out.len(), 1);

    // explicit `rev(:HEAD)` should resolve to the same oid
    let pipe_head = build_pipe(&ctx, vec![(Value::atom("HEAD"), br(8, 13))]);
    let head_out = run(pipe_head, Cursor::default());
    assert_eq!(head_out.len(), 1);

    assert_eq!(
        bare_out[0].value, head_out[0].value,
        "bare rev() and rev(:HEAD) resolve to same oid"
    );
}

#[test]
fn rev_multi_spec_emits_one_cursor_per_spec() {
    let dir = tempfile::tempdir().unwrap();
    let root: PathBuf = dir.path().to_path_buf();
    // Two commits so HEAD~1 is reachable.
    init_git(&root, &[("a.txt", b"v1")]);
    let g = |args: &[&str]| {
        let st = Command::new("git")
            .args(args)
            .current_dir(&root)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t.com")
            .env("GIT_AUTHOR_DATE", "2026-01-02T00:00:00Z")
            .env("GIT_COMMITTER_DATE", "2026-01-02T00:00:00Z")
            .output()
            .expect("git");
        assert!(
            st.status.success(),
            "git {:?}: {}",
            args,
            String::from_utf8_lossy(&st.stderr)
        );
    };
    std::fs::write(root.join("b.txt"), b"v2").unwrap();
    g(&["add", "."]);
    g(&["commit", "-m", "second"]);

    let cfg = SprfConfig {
        repos: vec![RepoConfig {
            slug: "test/repo".into(),
            root: root.clone(),
        }],
        ..Default::default()
    };
    let facts: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let store = SprfStore::new(facts.clone());
    let ctx = LowerCtx::new(facts.clone(), std::env::temp_dir())
        .with_sprf_store(store.clone())
        .with_config(Arc::new(cfg));

    let pipe = build_pipe(
        &ctx,
        vec![
            (Value::atom("HEAD"), br(8, 13)),
            (Value::atom("HEAD~1"), br(15, 22)),
        ],
    );

    let out = run(pipe, Cursor::default());
    assert_eq!(out.len(), 2, "one cursor per (repo × spec)");
    assert_ne!(out[0].value, out[1].value, "two distinct oids");

    // _revs: sentinel + 2 resolved revs.
    store.flush();
    let rev_rows = facts.rows_of(REVS_TABLE);
    assert_eq!(rev_rows.len(), 3, "sentinel + 2 revs in _revs");
}

#[test]
fn rev_call_writes_builtin_relation_and_revq_queries_it() {
    let dir = tempfile::tempdir().unwrap();
    let root: PathBuf = dir.path().to_path_buf();
    init_git(&root, &[("README.md", b"v1")]);

    let src = r#"
        rule(:seen_revs, REPO?, REV?, KIND?, SPEC?);

        repo()
          > rev(:HEAD);

        rev?(REPO?, REV?, KIND?, SPEC?)
          > rule(:seen_revs, REPO, REV, KIND, SPEC);
    "#;

    let facts = run_program(&root, src);
    let rows = facts.rows_of("seen_revs");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("REPO"), Some("test/repo"));
    assert_eq!(rows[0].get("KIND"), Some("spec"));
    assert_eq!(rows[0].get("SPEC"), Some("HEAD"));
    let rev = rows[0].get("REV").unwrap();
    assert_eq!(rev.len(), 40);
}

#[test]
fn rev_glob_enumerates_refs_and_binds_captures() {
    let dir = tempfile::tempdir().unwrap();
    let root: PathBuf = dir.path().to_path_buf();
    init_git(&root, &[("README.md", b"v1")]);
    git(&root, &["tag", "support/0.1.7"]);
    git(&root, &["tag", "support/0.2.0"]);

    let src = r#"
        rule(:support_revs, REPO?, PATCH?, REV?, NAME?, KIND?);

        repo()
          > rev(glob`support/0.1.${PATCH?}`)
          > rule(:support_revs, REPO, PATCH, REV, NAME, KIND);
    "#;

    let facts = run_program(&root, src);
    let rows = facts.rows_of("support_revs");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("REPO"), Some("test/repo"));
    assert_eq!(rows[0].get("PATCH"), Some("7"));
    assert_eq!(rows[0].get("NAME"), Some("support/0.1.7"));
    assert_eq!(rows[0].get("KIND"), Some("tag"));
    let rev = rows[0].get("REV").unwrap();
    assert_eq!(rev.len(), 40);
}

fn git(root: &std::path::Path, args: &[&str]) {
    let st = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t.com")
        .env("GIT_AUTHOR_DATE", "2026-01-03T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "2026-01-03T00:00:00Z")
        .output()
        .expect("git");
    assert!(
        st.status.success(),
        "git {:?}: {}",
        args,
        String::from_utf8_lossy(&st.stderr)
    );
}

fn run_program(root: &std::path::Path, src: &str) -> Arc<dyn FactStore<Cursor>> {
    let (program, parse_diags) = host_parse(src);
    assert!(parse_diags.is_empty(), "parse diags: {parse_diags:?}");

    let cfg = SprfConfig {
        repos: vec![RepoConfig {
            slug: "test/repo".into(),
            root: root.to_path_buf(),
        }],
        ..Default::default()
    };
    let facts: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let store = SprfStore::new(facts.clone());
    let reg = default_registry();
    let mut ctx = LowerCtx::new(facts.clone(), std::env::temp_dir())
        .with_sprf_store(store)
        .with_config(Arc::new(cfg));
    let (pipes, walk_diags) = walk_program(&program, &reg, &mut ctx);
    assert!(walk_diags.is_empty(), "walk diags: {walk_diags:?}");

    let q: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    for fused in pipes {
        expand(
            &fused.into_pipe().into_instance(),
            q.clone(),
            vec![Arc::new(Cursor::default())],
            ExpandOpts::default(),
        );
    }
    facts
}
