//! Target tests for the v4 `ast` DSL sprf carveout: `${NAME?}` and
//! `${$$$NAME}` rewriting plus sub-token capture extraction. Ported from
//! v2's `_9_ast_grep.rs::lower_sugar` semantics.

use ast_grep_language::SupportLang;

use v4::cst::diag::SilentSink;
use v4::cst::dsl::{CaptureKind, Dsl, VecCaptureSink};
use v4::cst::dsls::ast::AstDsl;

fn literal_values<'a>(sink: &'a VecCaptureSink, name: &str) -> Vec<&'a str> {
    sink.rows
        .iter()
        .filter(|r| &*r.name == name)
        .filter_map(|r| match &r.kind {
            CaptureKind::Literal { value } => std::str::from_utf8(value).ok(),
            _ => None,
        })
        .collect()
}

fn span_texts<'a>(sink: &'a VecCaptureSink, name: &str, target: &'a [u8]) -> Vec<&'a str> {
    sink.rows
        .iter()
        .filter(|r| &*r.name == name)
        .filter_map(|r| match &r.kind {
            CaptureKind::Span { byte_range } => {
                std::str::from_utf8(&target[byte_range.clone()]).ok()
            }
            _ => None,
        })
        .collect()
}

#[test]
fn ast_mid_name_capture_with_prefix_and_suffix() {
    let dsl = AstDsl::new(SupportLang::TypeScript);
    let c = dsl
        .compile(b"use${NAME?}Query($$$)", &SilentSink)
        .expect("compile");
    let target = b"useFooQuery(args); useBarQuery();";
    let mut sink = VecCaptureSink::new();
    c.match_into(target, 0, &mut sink);

    let names = literal_values(&sink, "NAME");
    assert!(names.contains(&"Foo"), "expected Foo in {names:?}");
    assert!(names.contains(&"Bar"), "expected Bar in {names:?}");
}

#[test]
fn ast_multi_token_capture() {
    let dsl = AstDsl::new(SupportLang::Rust);
    let c = dsl
        .compile(b"${$$$NAME}_helper", &SilentSink)
        .expect("compile");
    // The slot synthesis collapses the prefix/suffix run around the
    // sprf marker into a single $SPRFSLOT0 metavar with regex
    // `^(?P<NAME>.+)_helper$`. The token `some_long_name_helper` is one
    // identifier from ast-grep's perspective, so it matches as a single
    // metavar slot and the regex carves out `some_long_name`.
    let target = b"fn f() { let some_long_name_helper = 1; }";
    let mut sink = VecCaptureSink::new();
    c.match_into(target, 0, &mut sink);

    let names = literal_values(&sink, "NAME");
    assert!(
        names.contains(&"some_long_name"),
        "expected some_long_name in {names:?}",
    );
}

#[test]
fn ast_native_passthrough() {
    let dsl = AstDsl::new(SupportLang::Rust);
    let c = dsl
        .compile(b"fn $NAME($$$ARGS) {}", &SilentSink)
        .expect("compile");
    let target = b"fn alpha(x: i32, y: i32) {}";
    let mut sink = VecCaptureSink::new();
    c.match_into(target, 0, &mut sink);

    let name_spans = span_texts(&sink, "NAME", target);
    assert!(name_spans.contains(&"alpha"), "name spans = {name_spans:?}");

    let args = literal_values(&sink, "ARGS");
    assert!(
        args.iter().any(|s| s.contains('x') && s.contains('y')),
        "args literal = {args:?}",
    );
}

#[test]
fn ast_mixed_native_and_sprf() {
    let dsl = AstDsl::new(SupportLang::Rust);
    let c = dsl
        .compile(b"fn use${NAME?}Hook($$$ARGS) {}", &SilentSink)
        .expect("compile");
    let target = b"fn useFooHook(a: u32, b: u32) {}";
    let mut sink = VecCaptureSink::new();
    c.match_into(target, 0, &mut sink);

    let names = literal_values(&sink, "NAME");
    assert!(names.contains(&"Foo"), "names = {names:?}");

    let args = literal_values(&sink, "ARGS");
    assert!(
        args.iter().any(|s| s.contains('a') && s.contains('b')),
        "args = {args:?}",
    );
}

#[test]
fn ast_repeated_sprf_capture() {
    let dsl = AstDsl::new(SupportLang::Rust);
    // Two `${X?}` references to the same name should compile and bind
    // a single capture name `X`.
    let c = dsl
        .compile(b"a_${X?} == b_${X?}", &SilentSink)
        .expect("compile");
    let target = b"fn f() { let _ = a_foo == b_foo; }";
    let mut sink = VecCaptureSink::new();
    c.match_into(target, 0, &mut sink);

    let xs = literal_values(&sink, "X");
    assert!(xs.contains(&"foo"), "xs = {xs:?}");
}

#[test]
fn ast_no_surrounding_tokens_uses_bare_slot() {
    // `${NAME?}` alone has no prefix/suffix. The lower_sugar optimization
    // emits a bare `$NAME` metavar with no synthetic slot/regex; the
    // capture surfaces as a Span row.
    let dsl = AstDsl::new(SupportLang::Rust);
    let c = dsl.compile(b"fn ${NAME?}() {}", &SilentSink).expect("compile");
    let target = b"fn gamma() {}";
    let mut sink = VecCaptureSink::new();
    c.match_into(target, 0, &mut sink);

    let name_spans = span_texts(&sink, "NAME", target);
    assert!(name_spans.contains(&"gamma"), "spans = {name_spans:?}");
}
