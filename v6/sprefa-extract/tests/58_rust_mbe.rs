//! `rust_mbe::expand_file` in isolation: no hook into `RustSource::extract`
//! exists yet (blocked on a shared `CallFAux` row, see the PR body), so this
//! drives `RustSource.extract` a second time over the spliced text to check
//! the exact "sites/defs orig -> expanded" counts the lab measured
//! (`plans/extract-macro-lab-2026-08-29/PLAN.md` Option 1 fixture table).

use sprefa_extract::lang::rust_mbe::expand_file;
use sprefa_extract::{FamilyMask, RustSource, Source};

const MBE_DIR: &str = "tests/fixtures/rust_findings/mbe";

fn counts(content: &str) -> (usize, usize) {
    let output = RustSource.extract("f.rs", content.as_bytes(), FamilyMask::ALL);
    let call = output.call.as_ref().expect("syn parse must succeed");
    (call.nodes.len(), call.aux.sites.len())
}

fn read(name: &str) -> String {
    std::fs::read_to_string(format!("{MBE_DIR}/{name}")).expect(name)
}

/// SABOTAGE RECEIPT: dropping the `collect_calls` name filter (matching every
/// `MacroCall` regardless of whether a local def exists) makes f2/f4/f5/f8
/// fail their `is_none()` check, since `include!`/attribute macros would then
/// be mistaken for local invocations.
#[test]
fn no_local_macro_leaves_file_untouched() {
    for name in [
        "f2_cross_file.rs",
        "f4_builtins.rs",
        "f5_derive.rs",
        "f6_attr_proc_macro.rs",
        "f8_include.rs",
    ] {
        let src = read(name);
        assert!(expand_file(&src).is_none(), "{name} should stay unexpanded");
    }
}

#[test]
fn f1_local_call_in_body_gains_two_sites() {
    let src = read("f1_local_call_in_body.rs");
    let (orig_defs, orig_sites) = counts(&src);
    assert_eq!((orig_defs, orig_sites), (2, 0));

    let expanded = expand_file(&src).expect("twice! is local");
    assert!(!expanded.budget_hit);
    let (defs, sites) = counts(&expanded.text);
    assert_eq!((defs, sites), (2, 2));
}

#[test]
fn f3_nested_invocations_settle_to_one_site() {
    let src = read("f3_nested.rs");
    let (orig_defs, orig_sites) = counts(&src);
    assert_eq!((orig_defs, orig_sites), (2, 0));

    let expanded = expand_file(&src).expect("outer!/inner! are both local");
    assert!(!expanded.budget_hit);
    let (defs, sites) = counts(&expanded.text);
    assert_eq!((defs, sites), (2, 1));
}

#[test]
fn f7_mints_fn_gains_a_def_and_a_site() {
    let src = read("f7_mints_fn.rs");
    let (orig_defs, orig_sites) = counts(&src);
    assert_eq!((orig_defs, orig_sites), (2, 1));

    let expanded = expand_file(&src).expect("mkfn! is local");
    assert!(!expanded.budget_hit);
    let (defs, sites) = counts(&expanded.text);
    assert_eq!((defs, sites), (3, 2));
}

/// Every def/site gained by expansion reports the ORIGINAL invocation's span,
/// never a spliced-text offset with no home in the source file.
#[test]
fn gained_site_spans_map_inside_the_invocation() {
    let src = read("f7_mints_fn.rs");
    let expanded = expand_file(&src).expect("mkfn! is local");
    let output = RustSource.extract("f.rs", expanded.text.as_bytes(), FamilyMask::ALL);
    let call = output.call.as_ref().unwrap();

    let mut saw_macro_site = false;
    for site in &call.aux.sites {
        let range = site.span.start..site.span.start + site.span.len;
        if expanded.is_macro_span(range.clone()) {
            saw_macro_site = true;
            let original = expanded.map_span(range).expect("macro span always maps");
            let text = &src[original.start as usize..(original.start + original.len) as usize];
            assert!(
                text.contains("mkfn"),
                "mapped span {original:?} does not cover the mkfn! invocation: {text:?}"
            );
        }
    }
    assert!(saw_macro_site, "f7's generated() call should be macro-origin");
}

/// A macro that keeps re-minting itself never terminates a fixpoint; the pass
/// cap is what stops it, not a growing byte count.
#[test]
fn recursive_macro_hits_the_pass_budget() {
    let src = read("f9_recursive.rs");
    let expanded = expand_file(&src).expect("spin! is local");
    assert!(expanded.budget_hit);
}
