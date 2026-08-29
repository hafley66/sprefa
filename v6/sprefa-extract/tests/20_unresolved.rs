//! The `unresolved` runtime-computed edge markers (v5 `UnresolvedRef`),
//! graded by hand against the fixture `ts_unresolved/unresolved.ts`.
//!
//! v5 emits a marker row wherever an edge EXISTS in the source but its target
//! is computed at runtime: a dynamic `import(expr)`, a bare `require(expr)` with
//! a non-literal argument, a computed-member call `obj[key]()`, and a spread
//! call argument `f(...args)`. v6 had none; these rows are the port. `span` is
//! the computed expression itself, so `detail` is exactly the source text at
//! `span` (recoverable by containment for the enclosing statement).
//!
//! The fixture covers all four positive rules plus their negatives (a static
//! `import("./x")`, a `require("./y")` with a literal, a plain `obj.method()`,
//! a call with only ordinary arguments), which must produce NO row.
//!
//! It lives OUTSIDE `tests/fixtures/ts/` on purpose: the scip ratchet in
//! `golden_parity.rs` walks that root recursively and demands a scip
//! occurrence per call site, which undeclared callees like `loadPath` cannot
//! have.

use sprefa_extract::{dispatch, flatten, FamilyMask, FamilyTag, FlatFact, SCHEMA};

/// The `(reason, detail, start, end)` rows v6 emits for `ts_unresolved/unresolved.ts`.
/// Every span is the computed expression's own byte range (hand-derived from
/// the fixture text):
///   `loadPath()`   -> dynamic-import (import(loadPath());  line 2)
///   `...args`      -> spread-call-args (f(...args);  line 4)
///   `o[key]`       -> computed-member-call (o[key]();  line 6)
///   `somePath`     -> dynamic-import (require(somePath);  line 9)
const EXPECTED: &[(&str, &str, u32, u32)] = &[
    ("dynamic-import", "loadPath()", 31, 41),
    ("spread-call-args", "...args", 73, 80),
    ("computed-member-call", "o[key]", 116, 122),
    ("dynamic-import", "somePath", 170, 178),
];

fn unresolved_rows(bytes: &[u8]) -> Vec<(String, String, u32, u32)> {
    let out = dispatch(
        "v6/sprefa-extract/tests/fixtures/ts_unresolved/unresolved.ts",
        bytes,
        FamilyMask::ALL,
    )
    .expect("a Source matches the fixture");
    let mut rows = Vec::new();
    for fact in flatten(&out) {
        if let FlatFact::Unresolved {
            family,
            path,
            span,
            reason,
            detail,
        } = fact
        {
            assert_eq!(
                family,
                FamilyTag::Call,
                "unresolved rows live on the Call plane"
            );
            assert_eq!(path, None, "a phase-1 row's file is the caller's own");
            rows.push((reason, detail, span.start, span.end));
        }
    }
    rows.sort();
    rows
}

#[test]
fn unresolved_exact_rows() {
    let fixture = include_bytes!("fixtures/ts_unresolved/unresolved.ts");
    let mut got = unresolved_rows(fixture);
    got.sort();
    let mut expected: Vec<(String, String, u32, u32)> = EXPECTED
        .iter()
        .map(|(r, d, s, e)| (r.to_string(), d.to_string(), *s, *e))
        .collect();
    expected.sort();
    assert_eq!(
        got, expected,
        "unresolved rows must match the hand-derived set exactly"
    );
}

#[test]
fn unresolved_negatives_absent() {
    let fixture = include_bytes!("fixtures/ts_unresolved/unresolved.ts");
    let rows = unresolved_rows(fixture);
    // The four negatives in the fixture must contribute nothing: the static
    // import and the literal require are resolvable, the plain member call is
    // not a computed callee, and the ordinary-arg call has no spread.
    for (_, detail, _, _) in &rows {
        assert_ne!(detail, "./static-mod", "static import() must not be a row");
        assert_ne!(
            detail, "./static-require",
            "literal require() must not be a row"
        );
        assert_ne!(detail, "o.m", "plain member call must not be a row");
        assert_ne!(detail, "1", "ordinary arg must not be a spread row");
    }
    assert_eq!(
        rows.len(),
        EXPECTED.len(),
        "only the four positive rows may exist"
    );
}

#[test]
fn unresolved_jsonl_line_byte_exact() {
    let fixture = include_bytes!("fixtures/ts_unresolved/unresolved.ts");
    let out = dispatch(
        "v6/sprefa-extract/tests/fixtures/ts_unresolved/unresolved.ts",
        fixture,
        FamilyMask::ALL,
    )
    .expect("a Source matches the fixture");
    let line = flatten(&out)
        .into_iter()
        .find_map(|fact| match &fact {
            FlatFact::Unresolved { reason, .. } if reason == "computed-member-call" => {
                Some(serde_json::to_string(&fact).expect("fact is serializable"))
            }
            _ => None,
        })
        .expect("the computed-member-call row exists");
    assert_eq!(
        line,
        r#"{"record":"unresolved","family":"call","span":{"start":116,"end":122},"reason":"computed-member-call","detail":"o[key]"}"#
    );
}

#[test]
fn unresolved_in_schema() {
    assert!(
        SCHEMA.contains("record=unresolved  family=call"),
        "schema must declare the unresolved record line:\n{}",
        SCHEMA
    );
    assert!(
        SCHEMA.contains(
            "unresolved reason  dynamic-import | computed-member-call | spread-call-args"
        ),
        "schema must declare the unresolved reason vocabulary:\n{}",
        SCHEMA
    );
}
