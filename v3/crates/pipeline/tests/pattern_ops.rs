//! End-to-end pattern-op integration: host parse → injected sub-tree →
//! op-inherent compile_from_tree → Op::pipe / capability methods, per
//! spec §14.5a / §14.6 / §14.5m.

use std::path::PathBuf;
use std::sync::Arc;

use pipeline::{Cursor, GlobOp, PatternOp, ReOp, Op};
use sprefa_parse::{host_parse_with_injections, InjectedTree};

fn fake_file() -> Arc<std::path::Path> {
    Arc::from(PathBuf::from("test.sprf").as_path())
}

/// The slot body: substring between the op's opening `(` and closing `)`.
fn slot_body<'a>(src: &'a str, inj: &InjectedTree) -> &'a str {
    let host_range = inj.host_node.byte_range.clone();
    let slice = &src[host_range];
    let open = slice.find('(').expect("op_invocation has `(`");
    let close = slice.rfind(')').expect("op_invocation has `)`");
    &slice[open + 1..close]
}

#[tokio::test]
async fn glob_end_to_end() {
    let src = "glob(a/$DIR/b)";
    let (parsed, errs) = host_parse_with_injections(
        src,
        fake_file(),
        &pipeline::op_languages::language_of,
    );

    assert!(errs.is_empty(), "unexpected parse errs: {errs:?}");
    assert_eq!(parsed.injected.len(), 1);
    assert_eq!(&*parsed.injected[0].language_name, "sprefa_glob");

    let inj = &parsed.injected[0];
    let body = slot_body(src, inj);
    let g = GlobOp::compile_from_tree(&inj.tree, body.as_bytes())
        .expect("glob compiles");
    assert!(g.regex.as_str().contains("(?P<DIR>[^/]+)"));
    assert_eq!(
        g.bound_captures.iter().map(|a| &**a).collect::<Vec<_>>(),
        vec!["DIR"]
    );

    // PatternOp::binds_captures tree-walk reports the same name set.
    let probe = GlobOp::default();
    let caps: Vec<String> = probe
        .binds_captures(&inj.tree, body.as_bytes())
        .into_iter()
        .map(|a| a.to_string())
        .collect();
    assert_eq!(caps, vec!["DIR"]);

    // Op::pipe narrows cursor + records capture.
    let op: Arc<dyn Op> = Arc::new(g);
    let ctx = effect_runtime::RtCtx::default();
    let content: Arc<[u8]> = Arc::from(b"a/middle/b".as_slice());
    let out = op.pipe(&ctx, Cursor::new(content)).await;
    assert_eq!(out.len(), 1);
    let dir = out[0].captures.iter().find(|c| &*c.name == "DIR").expect("DIR capture");
    assert_eq!(&out[0].content[dir.byte_range.clone()], b"middle");
}

#[test]
fn re_end_to_end_sugar_and_native() {
    let src = r"re(TODO\($WHO\) (?P<WHEN>\d{4}))";
    let (parsed, errs) = host_parse_with_injections(
        src,
        fake_file(),
        &pipeline::op_languages::language_of,
    );
    assert!(errs.is_empty(), "unexpected parse errs: {errs:?}");
    assert_eq!(parsed.injected.len(), 1);
    assert_eq!(&*parsed.injected[0].language_name, "sprefa_re");

    let inj = &parsed.injected[0];
    let body = slot_body(src, inj);
    let r = ReOp::compile_from_tree(&inj.tree, body.as_bytes())
        .expect("re compiles");
    let caps = r.regex.captures(b"TODO(alice) 2026").expect("matches");
    assert_eq!(caps.name("WHO").unwrap().as_bytes(), b"alice");
    assert_eq!(caps.name("WHEN").unwrap().as_bytes(), b"2026");

    let probe = ReOp::default();
    let names: Vec<String> = probe
        .binds_captures(&inj.tree, body.as_bytes())
        .into_iter()
        .map(|a| a.to_string())
        .collect();
    assert!(names.contains(&"WHO".to_string()));
    assert!(names.contains(&"WHEN".to_string()));
}

#[test]
fn str_has_no_injected_entry() {
    let src = r"str(hello $NAME world)";
    let (parsed, errs) = host_parse_with_injections(
        src,
        fake_file(),
        &pipeline::op_languages::language_of,
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
        &pipeline::op_languages::language_of,
    );
    let inj = &parsed.injected[0];
    let body = slot_body(src, inj);
    let diags = ReOp::compile_from_tree(&inj.tree, body.as_bytes())
        .expect_err("regex parse error expected");
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].code, "pattern/re-syntax");
}

#[test]
fn try_raw_regex_is_some_for_regex_and_glob() {
    let (parsed, _) = host_parse_with_injections(
        "glob(**/*.rs)",
        fake_file(),
        &pipeline::op_languages::language_of,
    );
    let inj = &parsed.injected[0];
    let body = slot_body("glob(**/*.rs)", inj);
    let g: Arc<dyn Op> = Arc::new(
        GlobOp::compile_from_tree(&inj.tree, body.as_bytes()).unwrap(),
    );
    let raw = g.try_raw_regex().expect("glob exposes raw regex");
    assert!(raw.as_str().starts_with('^') && raw.as_str().ends_with('$'));

    let (parsed2, _) = host_parse_with_injections(
        r"re(foo)",
        fake_file(),
        &pipeline::op_languages::language_of,
    );
    let inj2 = &parsed2.injected[0];
    let body2 = slot_body(r"re(foo)", inj2);
    let r: Arc<dyn Op> = Arc::new(
        ReOp::compile_from_tree(&inj2.tree, body2.as_bytes()).unwrap(),
    );
    assert!(r.try_raw_regex().is_some());
}
