//! Integration test: GitBlobReader + read op against a real git repo.
//! Creates a tmp git repo with two branches, each having different file
//! content, drives the pipeline, asserts bytes differ and evidence lands.

use std::path::PathBuf;
use std::sync::Arc;

use futures::executor::block_on;
use futures_util::stream::StreamExt;

use v2::{
    Config, DiagSink, EventSink, GitBlobReader, InMemoryLocator,
    MemWriter, OpCtx, OpId, OperatorRegistry, ProgramCtx, RunId,
    RuntimeConfig, host_parse, lower_rules,
};
use v2::ops::{FsFactory, ReadFactory, RepoFactory, RevFactory, RuleFactory};
use v2::_0_types::Cursor;

// ---------------------------------------------------------------------------
// Git fixture helpers
// ---------------------------------------------------------------------------

fn setup_git_repo(dir: &std::path::Path) {
    use std::process::Command;

    fn run(dir: &std::path::Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME",     "test")
            .env("GIT_AUTHOR_EMAIL",    "test@example.com")
            .env("GIT_COMMITTER_NAME",  "test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .env("GIT_AUTHOR_DATE",     "2000-01-01T00:00:00")
            .env("GIT_COMMITTER_DATE",  "2000-01-01T00:00:00")
            .status()
            .expect("git command failed");
        assert!(status.success(), "git {:?} failed", args);
    }

    run(dir, &["init", "-b", "main"]);
    run(dir, &["config", "user.email", "test@example.com"]);
    run(dir, &["config", "user.name",  "test"]);

    // main: src/hello.rs = "fn main() { println!(\"v1\"); }"
    let src = dir.join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("hello.rs"), b"fn main() { println!(\"v1\"); }").unwrap();
    run(dir, &["add", "."]);
    run(dir, &["commit", "-m", "v1"]);

    // next: branch off main, update content
    run(dir, &["checkout", "-b", "next"]);
    std::fs::write(src.join("hello.rs"), b"fn main() { println!(\"v2\"); }").unwrap();
    run(dir, &["add", "."]);
    run(dir, &["commit", "-m", "v2"]);

    // back to main so HEAD is main
    run(dir, &["checkout", "main"]);
}

// ---------------------------------------------------------------------------
// Test
// ---------------------------------------------------------------------------

#[test]
fn git_blob_reader_read_op() {
    let tmp = tempfile::tempdir().unwrap();
    setup_git_repo(tmp.path());

    let repo_slug = "myorg/demo";
    let locator   = Arc::new(
        InMemoryLocator::new()
            .with_repo(repo_slug, tmp.path().to_path_buf(), &["main", "next"])
    );

    let cfg = Arc::new(Config {
        repos:        vec![],
        revs:         vec![],
        fs_exclude:   vec![],
        sprf_files:   vec![],
        shell_allow:  vec![],
        runtime: RuntimeConfig {
            worker_threads:    1,
            buffer_size:       256,
            flush_interval_ms: 100,
            collect_witnesses: true,
            xref_cartesian_limit: 10_000,
        },
        content_hash: 0,
    });

    let reader = Arc::new(GitBlobReader::new(locator, cfg.clone()));
    let writer = Arc::new(MemWriter::new());

    let src = r#"
        rule(code) {
            > repo(myorg/demo) > rev($V) > fs(src/*.rs) > read
        };
    "#;

    let invs = host_parse(src, Arc::from(PathBuf::from("<test>").as_path())).unwrap();

    let mut registry = OperatorRegistry::new();
    registry.register(Arc::new(RuleFactory));
    registry.register(Arc::new(RepoFactory));
    registry.register(Arc::new(RevFactory));
    registry.register(Arc::new(FsFactory));
    registry.register(Arc::new(ReadFactory));

    let pctx = ProgramCtx::new(cfg.clone(), Arc::new(registry));
    let outcome = lower_rules(invs, pctx);
    assert!(outcome.diags.is_empty(),
        "diags: {:?}", outcome.diags.iter().map(|d| d.code()).collect::<Vec<_>>());

    let (result_store, xref_seen) = OpCtx::fresh_xref_state();
    let ctx = OpCtx {
        run_id: RunId(1),
        op_id:  OpId(0),
        reader: reader.clone(),
        writer: writer.clone(),
        config: cfg.clone(),
        diags:  DiagSink(Arc::new(|d| panic!("unexpected diag: {}", d.code()))),
        events: EventSink(Arc::new(|_| {})),
        result_store,
        xref_seen,
    };

    let empty: futures_core::stream::BoxStream<'static, Cursor> =
        futures_util::stream::iter(Vec::<Cursor>::new()).boxed();
    let out: Vec<Cursor> = block_on(outcome.pipelines[0].run(empty, ctx).collect());

    // Two branches × one file each = 2 cursors.
    assert_eq!(out.len(), 2, "expected one cursor per rev");

    // Each cursor carries content.
    for c in &out {
        let content = c.content.as_ref().expect("content should be set");
        assert!(!content.is_empty(), "content empty for rev={}", c.rev);
    }

    // Content differs between revs.
    let c0 = out[0].content.as_ref().unwrap().as_ref();
    let c1 = out[1].content.as_ref().unwrap().as_ref();
    assert_ne!(c0, c1, "content should differ between main and next");

    // Concrete content check.
    let main_cursor = out.iter().find(|c| c.rev.as_ref() == "main").unwrap();
    let next_cursor = out.iter().find(|c| c.rev.as_ref() == "next").unwrap();
    assert!(std::str::from_utf8(main_cursor.content.as_ref().unwrap()).unwrap().contains("v1"));
    assert!(std::str::from_utf8(next_cursor.content.as_ref().unwrap()).unwrap().contains("v2"));

    // Evidence chain: repo(filter), rev(bind), fs(filter), read(bytes).
    for c in &out {
        let ops: Vec<&str> = c.evidence.iter().map(|e| e.op_name).collect();
        assert_eq!(ops, vec!["repo", "rev", "fs", "read"],
            "evidence chain wrong for rev={}", c.rev);
        // read witness = "<N> bytes"
        let read_ev = &c.evidence[3];
        assert!(read_ev.matched.contains("bytes"),
            "read evidence should be byte count, got: {}", read_ev.matched);
    }

    println!("cursors:");
    for c in &out {
        println!("  rev={} file={:?} len={}",
            c.rev,
            c.fs.as_ref().map(|f| f.0.display().to_string()),
            c.content.as_ref().map(|b| b.len()).unwrap_or(0));
    }
}
