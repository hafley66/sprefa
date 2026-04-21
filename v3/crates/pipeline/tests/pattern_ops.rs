//! End-to-end pattern-op integration: host parse → injected sub-tree →
//! PatternOp::compile + binds_captures, per spec §14.5a / §14.6.
//!
//! Wires `pipeline::op_languages::language_of` into
//! `sprefa_parse::host_parse_with_injections` and exercises each
//! pattern op that shipped in this Phase 1 slice.

use std::path::PathBuf;
use std::sync::Arc;

use pipeline::ops::glob::CompiledGlob;
use pipeline::ops::re::CompiledRe;
use pipeline::{op_languages, GlobOp, PatternOp, ReOp};
use sprefa_parse::{host_parse_with_injections, InjectedTree};

fn fake_file() -> Arc<std::path::Path> {
    Arc::from(PathBuf::from("test.sprf").as_path())
}

/// Substring of `src` corresponding to an injected body. The injected
/// tree carries substring-relative byte offsets; the host_node
/// parse_site points at the *paren_slot* span, so we re-derive the
/// slot-interior by trimming the enclosing parens.
fn slot_body<'a>(src: &'a str, inj: &InjectedTree) -> &'a str {
    let host_range = inj.host_node.byte_range.clone();
    // host_range is the full op_invocation span; find the paren body
    // via the host tree… simpler in integration tests: assume the
    // op name precedes a single `(` and extract between `(` and the
    // final `)`.
    let slice = &src[host_range];
    let open = slice.find('(').expect("op_invocation has `(`");
    let close = slice.rfind(')').expect("op_invocation has `)`");
    &slice[open + 1..close]
}

#[test]
fn glob_end_to_end() {
    let src = "glob(a/$DIR/b)";
    let (parsed, errs) = host_parse_with_injections(
        src,
        fake_file(),
        &op_languages::language_of,
    );
    assert!(errs.is_empty(), "unexpected parse errs: {errs:?}");
    assert_eq!(parsed.injected.len(), 1);
    let inj = &parsed.injected[0];
    assert_eq!(&*inj.language_name, "sprefa_glob");

    let body = slot_body(src, inj);
    let compiled = GlobOp
        .compile(&inj.tree, body.as_bytes())
        .expect("glob compiles");
    let g = compiled.downcast::<CompiledGlob>().unwrap();
    assert!(matches!(
        g.segments[2],
        pipeline::ops::glob::Segment::Hole { .. }
    ));

    let caps: Vec<String> = GlobOp
        .binds_captures(&inj.tree, body.as_bytes())
        .into_iter()
        .map(|a| a.to_string())
        .collect();
    assert_eq!(caps, vec!["DIR"]);
}

#[test]
fn re_end_to_end_binds_sugar_and_native() {
    let src = r"re(TODO\($WHO\) (?P<WHEN>\d{4}))";
    let (parsed, errs) = host_parse_with_injections(
        src,
        fake_file(),
        &op_languages::language_of,
    );
    assert!(errs.is_empty(), "unexpected parse errs: {errs:?}");
    assert_eq!(parsed.injected.len(), 1);
    let inj = &parsed.injected[0];
    assert_eq!(&*inj.language_name, "sprefa_re");

    let body = slot_body(src, inj);
    let compiled = ReOp
        .compile(&inj.tree, body.as_bytes())
        .expect("re compiles");
    let c = compiled.downcast::<CompiledRe>().unwrap();
    let captures = c.regex.captures("TODO(alice) 2026").expect("matches");
    assert_eq!(captures.name("WHO").unwrap().as_str(), "alice");
    assert_eq!(captures.name("WHEN").unwrap().as_str(), "2026");

    let caps: Vec<String> = ReOp
        .binds_captures(&inj.tree, body.as_bytes())
        .into_iter()
        .map(|a| a.to_string())
        .collect();
    assert!(caps.contains(&"WHO".to_string()));
    assert!(caps.contains(&"WHEN".to_string()));
}

#[test]
fn str_has_no_injected_entry() {
    // str is a plain Op. op_languages::language_of returns None for it
    // (no grammar.js folder contributed). The paren body survives as
    // opaque host bytes; no InjectedTree is produced.
    let src = r"str(hello $NAME world)";
    let (parsed, errs) = host_parse_with_injections(
        src,
        fake_file(),
        &op_languages::language_of,
    );
    assert!(errs.is_empty());
    assert_eq!(parsed.pipes[0].ops.len(), 1);
    assert!(parsed.injected.is_empty());
}

#[test]
fn bad_regex_surfaces_as_compile_error() {
    let src = "re([unclosed)";
    let (parsed, _) = host_parse_with_injections(
        src,
        fake_file(),
        &op_languages::language_of,
    );
    let inj = &parsed.injected[0];
    let body = slot_body(src, inj);
    let diags = ReOp
        .compile(&inj.tree, body.as_bytes())
        .expect_err("regex parse error expected");
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].code, "pattern/re-syntax");
}
