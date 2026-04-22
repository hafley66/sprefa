//! Acceptance tests for sprefa-4m7.14: Pattern values as first-class.
//!
//! Each test calls the post-14 factory API: `Op::from_values(Vec<Value>)`
//! (or the registry's `Vec<Value>`-taking overload). That API does not
//! exist yet; these tests are red until the parallel Rust sonnet lands
//! the factory signature flip described in the epic notes.
//!
//! ## API requests for the Rust sonnet
//!
//! Every op needs a `from_values` constructor (or the registry needs a
//! `build_from_values(name, Vec<Value>)` overload). Specifically:
//!
//! - `RevOp::from_values(values: Vec<Value>) -> Result<RevOp, Vec<PatternDiagnostic>>`
//! - `FsOp::from_values(values: Vec<Value>) -> Result<FsOp, Vec<PatternDiagnostic>>`
//! - `RepoOp::from_values(values: Vec<Value>) -> Result<RepoOp, Vec<PatternDiagnostic>>`
//! - `CommentOp::from_values(values: Vec<Value>) -> Result<CommentOp, Vec<PatternDiagnostic>>`
//! - `Registry::build_from_values(name: &str, values: Vec<Value>) -> Option<Result<Box<dyn Op>, Vec<PatternDiagnostic>>>`
//!
//! Public mode fields needed for inspection (or a `mode()` accessor):
//!
//! - `RevOp::mode: RevMode` — already `pub`; no change needed.
//! - `FsOp::mode: FsMode` — already `pub`; no change needed.
//! - `RepoOp::mode: RepoMode` — currently private field; needs `pub` or
//!   an accessor `pub fn mode(&self) -> &RepoMode`.
//!
//! ## Expected compile failure mode
//!
//! `cargo build -p pipeline --tests` will fail with:
//!   - `error[E0599]: no method named 'from_values' found` on RevOp/FsOp/RepoOp/CommentOp
//!   - `error[E0599]: no method named 'build_from_values' found` on Registry
//!
//! Once the Rust sonnet lands those constructors, all tests here should pass.

use std::sync::Arc;

use pipeline::ops::{CommentOp, FsOp, FsMode, RepoOp, RevOp, RevMode};
use pipeline::value::Value;
use pipeline::registry::Registry;
use pipeline::ops::GlobOp;
use pipeline::Op as _;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a `Value::Pattern(Pattern::Glob(_))` by running `GlobOp::compile`
/// on a source string. Panics on compile error (invalid glob source is a
/// test-authoring problem, not a runtime scenario).
fn glob_value(src: &str) -> Value {
    use sprefa_parse::host_parse_with_injections;
    use pipeline::op_languages::language_of;
    use std::path::PathBuf;

    let wrapped = format!("glob({src})");
    let file: Arc<std::path::Path> = Arc::from(PathBuf::from("test.sprf").as_path());
    let (parsed, errs) = host_parse_with_injections(&wrapped, file, &language_of);
    assert!(errs.is_empty(), "glob parse errors for '{src}': {errs:?}");
    let inj = &parsed.injected[0];
    // Body is the slice between '(' and ')'.
    let host_range = inj.host_node.byte_range.clone();
    let slice = &wrapped[host_range];
    let open  = slice.find('(').unwrap();
    let close = slice.rfind(')').unwrap();
    let body  = &slice[open + 1..close];
    let pat = GlobOp::compile_from_tree(&inj.tree, body.as_bytes())
        .unwrap_or_else(|e| panic!("glob compile failed for '{src}': {e:?}"));
    Value::Op(Arc::new(pat))
}

/// Build a `Value::Pattern(Pattern::Regex(_))` for a re source string.
fn re_value(src: &str) -> Value {
    use sprefa_parse::host_parse_with_injections;
    use pipeline::op_languages::language_of;
    use pipeline::ops::ReOp;
    use std::path::PathBuf;

    let wrapped = format!("re({src})");
    let file: Arc<std::path::Path> = Arc::from(PathBuf::from("test.sprf").as_path());
    let (parsed, errs) = host_parse_with_injections(&wrapped, file, &language_of);
    assert!(errs.is_empty(), "re parse errors for '{src}': {errs:?}");
    let inj = &parsed.injected[0];
    let host_range = inj.host_node.byte_range.clone();
    let slice = &wrapped[host_range];
    let open  = slice.find('(').unwrap();
    let close = slice.rfind(')').unwrap();
    let body  = &slice[open + 1..close];
    let pat = ReOp::compile_from_tree(&inj.tree, body.as_bytes())
        .unwrap_or_else(|e| panic!("re compile failed for '{src}': {e:?}"));
    Value::Op(Arc::new(pat))
}

// ---------------------------------------------------------------------------
// RevOp tests
// ---------------------------------------------------------------------------

/// §14.5c: `rev(:main)` — atom value lowers to filter mode with equality
/// on "main". RevMode::Filter; the compiled glob matches exactly "main".
#[test]
fn lower_rev_atom_emits_filter() {
    let values = vec![Value::Atom(Arc::from("main"))];
    let op = RevOp::from_values(values).expect("rev(:main) should construct without error");
    match &op.mode {
        RevMode::Filter(pat) => {
            let re = pat.try_raw_regex().expect("pattern exposes regex");
            assert!(re.is_match(b"main"), "pattern should match 'main'");
            assert!(!re.is_match(b"develop"), "pattern should not match 'develop'");
        }
        RevMode::Bind(_) => panic!("rev(:main) must be filter mode, got Bind"),
    }
}

/// §14.5c.1: `rev($V)` — bare Term in filter position is rejected at
/// factory time with diagnostic code "rev/unbounded-capture".
#[test]
fn lower_rev_term_rejects() {
    let values = vec![Value::Term(Arc::from("V"))];
    let errs = RevOp::from_values(values).expect_err("rev($V) must return diagnostic");
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].code, "rev/unbounded-capture");
}

/// §14.2 / §14.5c.1: `rev(re($V))` — the explicit opt-in form. A
/// `Value::Pattern(Pattern::Regex(_))` is accepted; the op enters filter
/// mode driven by the regex. No diagnostic.
#[test]
fn lower_rev_re_term_accepts() {
    // re($V) compiles to a regex that matches anything and binds into V.
    let pat_value = re_value("$V");
    let values = vec![pat_value];
    let op = RevOp::from_values(values).expect("rev(re($V)) should be accepted");
    match &op.mode {
        RevMode::Filter(_) => { /* expected */ }
        RevMode::Bind(_) => panic!("rev(re($V)) must be filter mode"),
    }
}

/// §14.2: Bare identifier in slot position yields `value/bare-ident`
/// diagnostic at lower time. The lowerer classifies unrecognized identifier
/// nodes, pushes the diagnostic, and emits a best-effort `Value::Atom` so
/// downstream factories can still run.
#[test]
fn lower_bare_ident_diagnostic() {
    use sprefa_parse::host_parse;
    use pipeline::registry::lower_paren_slot;
    use std::path::PathBuf;

    let src = "rev(main)";
    let file: Arc<std::path::Path> = Arc::from(PathBuf::from("test.sprf").as_path());
    let (parsed, errs) = host_parse(src, file);
    assert!(errs.is_empty(), "parse errs: {errs:?}");

    let inv = &parsed.pipes[0].ops[0];
    let paren = inv.node()
        .child_by_field_name("paren")
        .expect("rev has paren_slot");

    let r = Registry::with_stdlib();
    let mut diags = Vec::new();
    let _values = lower_paren_slot(paren, src.as_bytes(), &r, &mut diags);

    assert_eq!(diags.len(), 1, "expected exactly one diag, got {diags:?}");
    assert_eq!(diags[0].code, "value/bare-ident");
}

// ---------------------------------------------------------------------------
// FsOp tests
// ---------------------------------------------------------------------------

/// §14.5c: `fs(glob(**/*.rs))` — nested glob arg lowers to FsMode::Filter
/// with a GlobPattern whose regex covers Rust source files.
#[test]
fn lower_fs_glob_emits_pattern_value() {
    let values = vec![glob_value("**/*.rs")];
    let op = FsOp::from_values(values).expect("fs(glob(**/*.rs)) should construct");
    match &op.mode {
        FsMode::Filter(pat) => {
            let re = pat.try_raw_regex().expect("glob exposes regex");
            assert!(re.is_match(b"src/lib.rs"), "glob should match 'src/lib.rs'");
            assert!(re.is_match(b"crates/pipeline/src/lower.rs"), "glob should match deep path");
            assert!(!re.is_match(b"src/main.go"), "glob should not match .go files");
        }
        FsMode::Bind { .. } => panic!("fs(glob(**/*.rs)) must be Filter mode"),
    }
}

/// §14.5c: `fs($P)` — Term value enters bind mode; the term name is
/// stored for synthesized capture emission.
#[test]
fn lower_fs_term_binds() {
    let values = vec![Value::Term(Arc::from("P"))];
    let op = FsOp::from_values(values).expect("fs($P) should construct in bind mode");
    match &op.mode {
        FsMode::Bind { name, .. } => {
            assert_eq!(&**name, "P");
        }
        FsMode::Filter(_) => panic!("fs($P) must be Bind mode"),
    }
}

// ---------------------------------------------------------------------------
// RepoOp tests
// ---------------------------------------------------------------------------

/// §14.5c: `repo(glob(myorg/*))` — glob value enters filter mode via
/// the GlobPattern; cursors whose repo slug does not match are dropped.
#[test]
fn lower_repo_glob_filters() {
    let values = vec![glob_value("myorg/*")];
    let op = RepoOp::from_values(values).expect("repo(glob(myorg/*)) should construct");
    // Inspect mode. RepoOp::mode must be pub or accessible via accessor.
    // (API request: pub field or `pub fn mode(&self) -> &RepoMode`.)
    match op.mode() {
        pipeline::ops::RepoMode::Filter(pat) => {
            let re = pat.try_raw_regex().expect("glob exposes regex");
            assert!(re.is_match(b"myorg/backend"), "should match myorg/backend");
            assert!(!re.is_match(b"otherorg/frontend"), "should not match otherorg");
        }
        pipeline::ops::RepoMode::Bind(_) => panic!("repo(glob(myorg/*)) must be Filter"),
    }
}

// ---------------------------------------------------------------------------
// CommentOp tests
// ---------------------------------------------------------------------------

/// §14.2: `comment(re(START), re(END))` — two Pattern args.
/// Both compile to regex patterns stored in the CommentOp. The test
/// verifies both open and close patterns are wired by inspecting the
/// constructed op via a helper accessor.
///
/// API request: `CommentOp::open_re() -> &regex::bytes::Regex` and
/// `CommentOp::close_re() -> Option<&regex::bytes::Regex>`.
#[test]
fn lower_comment_two_regexes() {
    let start_value = re_value("START");
    let end_value   = re_value("END");
    let values = vec![start_value, end_value];
    let op = CommentOp::from_values(values).expect("comment(re(START), re(END)) should construct");

    // The open regex must match the literal bytes "START".
    assert!(
        op.open_re().is_match(b"// START of section"),
        "open regex must match 'START'"
    );
    // The close regex must match "END".
    let close = op.close_re().expect("two-arg comment has a close regex");
    assert!(
        close.is_match(b"// END of section"),
        "close regex must match 'END'"
    );
}
