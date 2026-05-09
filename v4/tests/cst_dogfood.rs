//! Cross-DSL trait-surface dogfood. Holds shipped DSLs behind
//! `Vec<Box<dyn Dsl>>` and runs compile + match_into via dynamic dispatch.
//!
//! Catches trait-surface drift: object-safety, Send+Sync+'static bounds,
//! `&dyn DiagSink`, span arithmetic with non-zero `target_off`. Does NOT
//! test individual DSL semantics — those live in each DSL's unit tests.

use ast_grep_language::SupportLang;
use v4::cst::{Compiled, Dsl, SilentSink, VecCaptureSink};
use v4::cst::dsl::CaptureKind;

use v4::cst::dsls::ast::AstDsl;
use v4::cst::dsls::glob::GlobDsl;
use v4::cst::dsls::json::JsonDsl;
use v4::cst::dsls::markdown::MarkdownDsl;
use v4::cst::dsls::re::ReDsl;
use v4::cst::dsls::sql::SqlDsl;

struct Case {
    dsl:           Box<dyn Dsl>,
    body:          &'static [u8],
    target:        &'static [u8],
    target_off:    usize,
    expected_caps: &'static [&'static str],
    expect_match:  bool,
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            dsl:           Box::new(ReDsl::new()),
            body:          br"TODO\($WHO\)",
            target:        b"prefix TODO(alice) suffix",
            target_off:    1000,
            expected_caps: &["WHO"],
            expect_match:  true,
        },
        Case {
            dsl:           Box::new(GlobDsl::new()),
            body:          b"**/$DIR/file.txt",
            target:        b"a/b/middle/file.txt",
            target_off:    2000,
            expected_caps: &["DIR"],
            expect_match:  true,
        },
        Case {
            dsl:           Box::new(JsonDsl::new()),
            body:          b"{ name: $N }",
            target:        br#"{"name":"alice"}"#,
            target_off:    3000,
            expected_caps: &["N"],
            expect_match:  true,
        },
        Case {
            dsl:           Box::new(AstDsl::new(SupportLang::Rust)),
            body:          b"fn $NAME() {}",
            target:        b"fn alpha() {}",
            target_off:    4000,
            expected_caps: &["NAME"],
            expect_match:  true,
        },
        Case {
            dsl:           Box::new(SqlDsl::new()),
            body:          b"SELECT input.__cursor_idx, input.OP FROM input",
            target:        b"",
            target_off:    0,
            expected_caps: &[],
            expect_match:  false,
        },
        Case {
            dsl:           Box::new(MarkdownDsl::new()),
            body:          b"## ${TITLE}",
            target:        b"",
            target_off:    0,
            expected_caps: &[],
            expect_match:  false,
        },
    ]
}

#[test]
fn shipped_dsls_compile_via_dyn_dispatch() {
    for case in cases() {
        let dsl: &dyn Dsl = case.dsl.as_ref();
        // compile via the trait object — proves Dsl is object-safe and that
        // the boxed Compiled can be returned across the dyn boundary.
        let _ = dsl
            .compile(case.body, &SilentSink)
            .unwrap_or_else(|e| panic!("{} compile failed: {:?}", dsl.id(), e));
    }
}

#[test]
fn shipped_dsls_match_via_dyn_dispatch_with_offset() {
    for case in cases() {
        let dsl: &dyn Dsl = case.dsl.as_ref();
        let compiled = dsl.compile(case.body, &SilentSink).unwrap();
        let compiled: &dyn Compiled = compiled.as_ref();

        let mut sink = VecCaptureSink::new();
        compiled.match_into(case.target, case.target_off, &mut sink);

        if !case.expect_match {
            assert!(sink.rows.is_empty(), "{}: expected no rows", dsl.id());
            continue;
        }

        // Capture names emerge at match time — verify the expected names
        // appear in the emitted rows.
        for cap_name in case.expected_caps {
            let row = sink
                .rows
                .iter()
                .find(|r| &*r.name == *cap_name)
                .unwrap_or_else(|| panic!("{}: no row for capture {}", dsl.id(), cap_name));

            // Span ranges must be lifted into target_off space; literals are
            // legal too (ast multi-metavars, future synthesized rows).
            match &row.kind {
                CaptureKind::Span { byte_range } => {
                    assert!(
                        byte_range.start >= case.target_off,
                        "{}: span {:?} below target_off {}",
                        dsl.id(),
                        byte_range,
                        case.target_off,
                    );
                    assert!(
                        byte_range.end <= case.target_off + case.target.len(),
                        "{}: span {:?} past target end",
                        dsl.id(),
                        byte_range,
                    );
                }
                CaptureKind::Literal { .. } => {}
            }
        }
    }
}
