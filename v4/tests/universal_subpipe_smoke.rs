//! Universal `${ <pipe> }` carveouts must drain in EVERY DSL body.
//!
//! Today (red baseline):
//!   sh   — regex-based render_template ignores SubPipe interps; carveout
//!          renders as literal `${ ... }` text in the shell command.
//!   glob — GlobDef::lower rejects SubPipe with `lower/glob-subpipe-not-supported`.
//!   re   — ReDef::lower compiles body.raw straight to a regex; SubPipe text
//!          becomes `${ ... }` regex literal which never matches.
//!
//! After: every dsl body routes through a shared render_segments util that
//! resolves Term interps and drains SubPipe interps per cursor.

use effect_runtime::v2::{expand, ExpandOpts, FactStore, MemFactStore, MemQueue, QueueBackend};
use std::sync::Arc;
use v4::compile::parse::host_parse;
use v4::compile::walk::walk_program;
use v4::lower::{default_registry, LowerCtx};
use v4::Cursor;

fn run_in(root: &std::path::Path, src: &str) -> Arc<dyn FactStore<Cursor>> {
    let (program, parse_diags) = host_parse(src);
    assert!(parse_diags.is_empty(), "parse: {:?}", parse_diags);
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let reg = default_registry();
    let mut ctx = LowerCtx::new(store.clone(), root.to_path_buf());
    let (pipes, walk_diags) = walk_program(&program, &reg, &mut ctx);
    assert!(
        walk_diags.is_empty(),
        "walk: {:?}",
        walk_diags
            .iter()
            .map(|d| (d.code.as_ref(), d.message.as_str()))
            .collect::<Vec<_>>()
    );
    let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    for fused in pipes {
        let inst = fused.into_pipe().into_instance();
        expand(
            &inst,
            queue.clone(),
            vec![Arc::new(Cursor::default())],
            ExpandOpts::default(),
        );
    }
    store
}

fn run(src: &str) -> Arc<dyn FactStore<Cursor>> {
    run_in(&std::env::temp_dir(), src)
}

/// sh body subpipe — `${ str`world` }` should drain to "world" inside the
/// shell command.
#[test]
fn sh_body_renders_subpipe() {
    let src = r#"rule(:greet) { sh`echo hello-${ str`world` }` };"#;
    let store = run(src);
    assert_eq!(store.len("greet"), 1, "expected one row");
    let rows = store.rows_of("greet");
    // sh captures stdout into &.value; trailing newline trimmed.
    assert_eq!(
        &*rows[0].value, "hello-world",
        "sh subpipe should drain `world` into the command"
    );
}

/// glob body subpipe — driver pipe emits a literal stem; glob recompiles
/// per cursor and matches the file with that stem.
#[test]
fn glob_body_renders_subpipe() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("alpha.txt"), b"a").unwrap();
    std::fs::write(tmp.path().join("bravo.txt"), b"b").unwrap();

    let src = r#"rule(:hits, EXT?) { fs > glob`**/${ str`alpha` }.${EXT?}` };"#;
    let store = run_in(tmp.path(), src);
    let rows = store.rows_of("hits");
    assert_eq!(
        rows.len(),
        1,
        "expected exactly one alpha hit, got {:?}",
        rows.iter().map(|r| &*r.value).collect::<Vec<_>>()
    );
    assert_eq!(rows[0].get("EXT"), Some("txt"));
}

/// re body subpipe — pattern is built per-cursor by draining the inner
/// `str` pipe; matches the surrounding cursor.value text.
#[test]
fn re_body_renders_subpipe() {
    // Post-matcher-purification: re reads cursor.value bytes. `read`
    // is the gate that materializes file content into cursor.value.
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("hay.txt");
    std::fs::write(&path, b"the needle is here").unwrap();

    let src =
        format!(r#"rule(:hits) {{ fs > glob`**/hay.txt` > read > re`${{ str`needle` }}` }};"#);
    let store = run_in(tmp.path(), &src);
    let rows = store.rows_of("hits");
    assert_eq!(
        rows.len(),
        1,
        "re subpipe should match `needle` in haystack"
    );
}

/// Regression — str template subpipe still works (today's path).
#[test]
fn str_template_subpipe_regression() {
    let src = r#"rule(:greet) { `out=${ str`hi` }` };"#;
    let store = run(src);
    assert_eq!(store.len("greet"), 1);
    assert_eq!(&*store.rows_of("greet")[0].value, "out=hi");
}
