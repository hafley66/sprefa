//! Path-qualified call binding for the rust arm, classes 7/8/9/11 of the
//! receiver census (`plans/extract-crawl-2026-08-29/rust.REPORT.md` 18.2):
//! a module-qualified `mod::f()` resolves its prefix through the module plane
//! and binds a def in that module only; `T::f()` and a named receiver with
//! 2+ corpus impls pick the inherent impl first, else the one trait impl
//! whose trait is in scope; a 0-impl `T::f()` whose segment is an enum
//! variant binds the enum's def; an external prefix (`std::mem::take`) drops
//! `external`, never `ambiguous`.
//!
//! SABOTAGE RECEIPT (fail-pre-fix, whole file, 6 of 7 red): the edge asserts
//! panicked `assertion failed` (`inherent_impl_beats_trait_impl`,
//! `trait_impl_binds_only_when_the_trait_is_in_scope`,
//! `variant_constructor_binds_the_variant_def`,
//! `prelude_trait_counts_as_in_scope`,
//! `module_qualified_prefixes_bind_the_declared_module`) and
//! `external_module_prefixes_drop_external` failed its reason asserts; only
//! the `two_in_scope_traits_stay_ambiguous` pin was green.
//!
//! Fixtures: `tests/fixtures/rust_findings/paths3/` (two crates so a bare
//! `helpers` suffix is corpus-ambiguous; `crate_a/src/deep/helpers.rs` so a
//! `crate::helpers` prefix needs the crate-anchored exact match).

use std::process::Command;

use serde_json::Value;

const CRATE_A: &str = "tests/fixtures/rust_findings/paths3/crate_a/src";
const CRATE_B: &str = "tests/fixtures/rust_findings/paths3/crate_b/src";

const FILES: &[&str] = &[
    "{CRATE_A}/lib.rs",
    "{CRATE_A}/alpha.rs",
    "{CRATE_A}/cycle.rs",
    "{CRATE_A}/gadget.rs",
    "{CRATE_A}/gem.rs",
    "{CRATE_A}/helpers.rs",
    "{CRATE_A}/other.rs",
    "{CRATE_A}/traits_mod.rs",
    "{CRATE_A}/widget.rs",
    "{CRATE_A}/deep/helpers.rs",
    "{CRATE_B}/lib.rs",
    "{CRATE_B}/helpers.rs",
];

fn run() -> Vec<Value> {
    let mut args: Vec<String> = vec![
        "--resolve".to_string(),
        "--family".to_string(),
        "call,type".to_string(),
    ];
    args.extend(FILES.iter().map(|tpl| {
        tpl.replace("{CRATE_A}", CRATE_A)
            .replace("{CRATE_B}", CRATE_B)
    }));
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(&args)
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "{args:?} stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("stdout is UTF-8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("a flat fact is JSON"))
        .collect()
}

fn text(row: &Value, key: &str) -> String {
    row[key].as_str().unwrap_or("").to_string()
}

/// (caller_name, callee_name, callee file stem) per `resolved_edge`.
fn edges() -> Vec<(String, String, String)> {
    let mut rows: Vec<(String, String, String)> = run()
        .iter()
        .filter(|row| row["record"] == "resolved_edge")
        .map(|row| {
            (
                text(row, "caller_name"),
                text(row, "callee_name"),
                text(row, "callee_path")
                    .rsplit('/')
                    .next()
                    .unwrap_or("")
                    .trim_end_matches(".rs")
                    .to_string(),
            )
        })
        .collect();
    rows.sort();
    rows
}

/// (detail, reason) per unresolved call row.
fn drops() -> Vec<(String, String)> {
    run()
        .iter()
        .filter(|row| row["record"] == "unresolved" && row["family"] == "call")
        .map(|row| (text(row, "detail"), text(row, "reason")))
        .collect()
}

/// HEAD: w.reset() drops `ambiguous` (impl_target sees 2 impls of
/// (Widget, reset) and declines); no edge (widget_user, reset, widget).
#[test]
fn inherent_impl_beats_trait_impl() {
    assert!(edges().iter().any(|(caller, callee, stem)| {
        caller == "widget_user" && callee == "reset" && stem == "widget"
    }));
}

/// HEAD: g.polish() in other.rs drops `ambiguous` (2 trait impls of
/// (Gadget, polish)); Sand is not imported there, so only Polish can bind.
#[test]
fn trait_impl_binds_only_when_the_trait_is_in_scope() {
    assert!(edges().iter().any(|(caller, callee, stem)| {
        caller == "other_user" && callee == "polish" && stem == "gadget"
    }));
}

/// Both Polish and Sand are imported in gadget.rs, so g.polish() stays
/// `ambiguous` however good the tiebreak gets.
#[test]
fn two_in_scope_traits_stay_ambiguous() {
    assert!(
        drops()
            .iter()
            .any(|(detail, reason)| detail == "polish" && reason == "ambiguous"),
        "gadget.rs g.polish() must stay ambiguous"
    );
}

/// HEAD: Gem::from(3) drops `ambiguous` (From-impl and Sand-impl of
/// (Gem, from)); From is in the prelude, Sand is not imported in lib.rs.
#[test]
fn prelude_trait_counts_as_in_scope() {
    assert!(
        edges().iter().any(|(caller, callee, stem)| {
            caller == "lib_user" && callee == "from" && stem == "gem"
        }),
        "Gem::from(3) must bind the From impl"
    );
}

/// HEAD: Alpha::First(3) drops `no_corpus_def` (no fn named First exists);
/// First is a variant of the corpus enum Alpha, so the call binds it. The
/// callee NAME moved from the enum to the variant with `76_rust_variant_names`
/// (the ra_ap_ide oracle spells the edge `First`).
#[test]
fn variant_constructor_binds_the_variant_def() {
    assert!(
        edges().iter().any(|(caller, callee, stem)| {
            caller == "alpha_user" && callee == "First" && stem == "alpha"
        }),
        "Alpha::First(3) must bind the First variant in alpha.rs"
    );
}

/// HEAD: every `util_fn` shape in lib.rs drops `ambiguous` the bare
/// suffix `helpers` covers crate_a, crate_a/deep and crate_b.
#[test]
fn module_qualified_prefixes_bind_the_declared_module() {
    let rows = edges();
    let bound = rows
        .iter()
        .filter(|(caller, callee, stem)| {
            caller == "lib_user" && callee == "util_fn" && stem == "helpers"
        })
        .count();
    assert!(bound >= 4, "glob, bare, alias and crate:: shapes: {rows:?}");
    assert!(
        rows.iter().any(|(caller, callee, stem)| {
            caller == "crate_b_user" && callee == "util_fn" && stem == "helpers"
        }),
        "crate_b's own helpers must not leak into crate_a"
    );
}

/// HEAD: std::mem::take and mem::replace drop `no_corpus_def`; the prefix
/// names an external module, so the reason must say `external`.
#[test]
fn external_module_prefixes_drop_external() {
    let rows = drops();
    assert!(
        rows.iter()
            .any(|(detail, reason)| detail == "std::mem::take" && reason == "external"),
        "{rows:?}"
    );
    assert!(
        rows.iter()
            .any(|(detail, reason)| detail == "mem::replace" && reason == "external"),
        "{rows:?}"
    );
}

/// HEAD (pre-guard): the `use alpha as beta; use beta as alpha;` pair in
/// cycle.rs sent bound_home -> resolve_qualified -> home_file around forever
/// and the extract binary died with `fatal runtime error: stack overflow`
/// (reproduced on the whole rust-analyzer corpus, rc=134). The run must
/// complete and the cycle-bound name must simply stay unresolved.
#[test]
fn use_binding_cycles_terminate() {
    let rows = drops();
    assert!(
        rows.iter().any(|(detail, _)| detail == "deep"),
        "deep binds through nothing, so it must drop"
    );
}
