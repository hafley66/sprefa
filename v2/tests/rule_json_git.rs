//! Integration tests: JsonOp against a real GitBlobReader.
//! Fixture setup mirrors git_read.rs: shell out to `git`, use tempfile::TempDir,
//! wire GitBlobReader via InMemoryLocator.

use v2::ops::default_registry;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use futures::executor::block_on;
use futures_util::stream::StreamExt;

use v2::{
    Config, DiagSink, GitBlobReader, InMemoryLocator,
    MemWriter, OpCtx, OperatorRegistry, ProgramCtx, host_parse, lower_rules,
};
use v2::_0_types::Cursor;

// ---------------------------------------------------------------------------
// Shared fixture helpers
// ---------------------------------------------------------------------------

fn run(dir: &std::path::Path, args: &[&str]) {
    use std::process::Command;
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

fn make_config() -> Arc<Config> {
    Arc::new(Config::test_default())
}

fn make_registry() -> Arc<OperatorRegistry> {
    Arc::new(default_registry())
}

fn run_rule_git(
    src:     &str,
    reader:  Arc<GitBlobReader>,
    cfg:     Arc<Config>,
    diag_sink: DiagSink,
) -> Vec<Cursor> {
    let invs = host_parse(src, Arc::from(PathBuf::from("<test>").as_path())).unwrap();
    let pctx = ProgramCtx::new(cfg.clone(), make_registry());
    let outcome = lower_rules(invs, pctx);
    assert!(outcome.diags.is_empty(),
        "lower diags: {:?}",
        outcome.diags.iter().map(|d| d.code()).collect::<Vec<_>>());

    let writer = Arc::new(MemWriter::new());
    let ctx = OpCtx {
        diags: diag_sink,
        ..OpCtx::for_test(cfg.clone(), reader.clone(), writer.clone())
    };
    let empty: futures_core::stream::BoxStream<'static, Arc<[Cursor]>> =
        futures_util::stream::iter(Vec::<Arc<[Cursor]>>::new()).boxed();
    let batches: Vec<Arc<[Cursor]>> =
        block_on(outcome.pipelines[0].run(empty, ctx).collect());
    batches.into_iter()
        .flat_map(|b| b.iter().cloned().collect::<Vec<_>>())
        .collect()
}

// ---------------------------------------------------------------------------
// Test 1: json via GitBlobReader with upstream fs
// ---------------------------------------------------------------------------

#[test]
fn json_via_git_reader() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();

    run(dir, &["init", "-b", "main"]);
    run(dir, &["config", "user.email", "test@example.com"]);
    run(dir, &["config", "user.name",  "test"]);

    std::fs::write(
        dir.join("package.json"),
        br#"{"version":"2.0.0","name":"pkg"}"#,
    ).unwrap();
    run(dir, &["add", "."]);
    run(dir, &["commit", "-m", "init"]);

    let repo_slug = "myorg/demo";
    let cfg = make_config();
    let locator = Arc::new(
        InMemoryLocator::new()
            .with_repo(repo_slug, dir.to_path_buf(), &["main"])
    );
    let reader = Arc::new(GitBlobReader::new(locator, cfg.clone()));

    let src = r#"
        rule(v) {
            > repo(myorg/demo) > rev(main) > fs(**/package.json)
              > json({ version: $VER })
        };
    "#;
    let out = run_rule_git(
        src,
        reader,
        cfg,
        DiagSink(Arc::new(|d| panic!("unexpected diag: {}", d.code()))),
    );

    assert_eq!(out.len(), 1, "expected 1 cursor");
    assert_eq!(
        out[0].captures.get("VER").map(|c| c.value.as_ref()),
        Some("2.0.0"),
    );
}

// ---------------------------------------------------------------------------
// Test 2: self-enum via GitBlobReader (no upstream fs)
// ---------------------------------------------------------------------------

#[test]
fn json_self_enum_via_git() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();

    run(dir, &["init", "-b", "main"]);
    run(dir, &["config", "user.email", "test@example.com"]);
    run(dir, &["config", "user.name",  "test"]);

    std::fs::create_dir_all(dir.join("pkg-a")).unwrap();
    std::fs::create_dir_all(dir.join("pkg-b")).unwrap();

    std::fs::write(dir.join("pkg-a/package.json"), br#"{"name":"a"}"#).unwrap();
    std::fs::write(dir.join("pkg-b/package.json"), br#"{"name":"b"}"#).unwrap();
    std::fs::write(dir.join("README.md"),          b"# nope").unwrap();

    run(dir, &["add", "."]);
    run(dir, &["commit", "-m", "init"]);

    let repo_slug = "myorg/multi";
    let cfg = make_config();
    let locator = Arc::new(
        InMemoryLocator::new()
            .with_repo(repo_slug, dir.to_path_buf(), &["main"])
    );
    let reader = Arc::new(GitBlobReader::new(locator, cfg.clone()));

    // No upstream fs — json self-enumerates. README.md must be skipped by ext filter.
    let src = r#"
        rule(v) {
            > repo(myorg/multi) > rev(main)
              > json({ name: $N })
        };
    "#;
    let out = run_rule_git(
        src,
        reader,
        cfg,
        DiagSink(Arc::new(|_| {})),
    );

    assert_eq!(out.len(), 2, "expected 2 cursors (one per json file)");
    let mut names: Vec<&str> = out.iter()
        .map(|c| c.captures.get("N").unwrap().value.as_ref())
        .collect();
    names.sort();
    assert_eq!(names, vec!["a", "b"]);
}

// ---------------------------------------------------------------------------
// Test 3: json/no-match diag when file structure does not match pattern
// ---------------------------------------------------------------------------

#[test]
fn json_no_match_diag_from_git() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();

    run(dir, &["init", "-b", "main"]);
    run(dir, &["config", "user.email", "test@example.com"]);
    run(dir, &["config", "user.name",  "test"]);

    // File has `name` but pattern looks for `version` — guaranteed no match.
    std::fs::write(dir.join("data.json"), br#"{"name":"hello"}"#).unwrap();
    run(dir, &["add", "."]);
    run(dir, &["commit", "-m", "init"]);

    let repo_slug = "myorg/nomatch";
    let cfg = make_config();
    let locator = Arc::new(
        InMemoryLocator::new()
            .with_repo(repo_slug, dir.to_path_buf(), &["main"])
    );
    let reader = Arc::new(GitBlobReader::new(locator, cfg.clone()));

    let collected: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
    let collected2 = collected.clone();

    let src = r#"
        rule(v) {
            > repo(myorg/nomatch) > rev(main) > fs(**/data.json)
              > json({ version: $V })
        };
    "#;
    let out = run_rule_git(
        src,
        reader,
        cfg,
        DiagSink(Arc::new(move |d| {
            collected2.lock().unwrap().push(d.code().to_owned());
        })),
    );

    // No matches emitted.
    assert_eq!(out.len(), 0);

    let codes = collected.lock().unwrap();
    assert!(
        codes.iter().any(|c| c == "json/no-match"),
        "expected json/no-match in diags, got {codes:?}",
    );
}
