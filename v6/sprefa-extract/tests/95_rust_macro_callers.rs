// TEST: census class M1, the cross-file macro_rules share (rust.REPORT.md
// sec 33.2, 62 of the 1,928 rows). A `macro_rules!` body in one file mints
// fns whose bodies call corpus defs; the INVOCATION file carries none of the
// caller names, so its parse mints no def, no site, no caller. CodeQL expands
// by def-site resolution and names the minted fn as the caller at the
// invocation file.
//
// FAIL-FIRST, pre-fix: the resolve over these fixtures emits no
// resolved_edge whose caller_name is a minted fn (`alpha`/`beta`/`gamma`),
// because `rust_mbe::expand_file` collects defs from the invoking file only
// (`rust_mbe.rs` expand_pass: "a name with no local def ... is untouched").
//
// The corpus shapes this pins, one test each:
//   foreign_minted_caller_lands      the caller name + same-file callee row
//   minted_callers_bind_their_bodies two fns in ONE expansion each keep their
//                                    own sites (distinct spans, not the
//                                    collapsed first-def tie)
//   local_def_shadows_the_foreign    a file-local macro of the same name wins
//                                    and nothing double-mints

use std::path::PathBuf;

use sprefa_extract::{resolve_project, FlatFact, ResolveArms, ResolveRequest};

fn fixture(name: &str) -> PathBuf {
    let mut path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf();
    path.push("tests/fixtures/rust_macro_callers");
    path.push(name);
    path
}

/// The resolved call rows of one resolve over the whole fixture dir.
fn rows(extra: &[&str]) -> Vec<(String, Option<String>, String, Option<String>)> {
    let mut paths = vec![fixture("macros.rs"), fixture("user.rs"), fixture("decoys.rs")];
    for name in extra {
        paths.push(fixture(name));
    }
    let facts = resolve_project(&ResolveRequest {
        paths: &paths,
        arms: ResolveArms { call: true, types: false, flow: false },
        scip: Default::default(),
        project_root: None,
        scip_records: Default::default(),
        occurrence_text: false,
        rust_checker: None,
        ts_checker: None,
    })
    .expect("the fixture corpus resolves");
    facts
        .into_iter()
        .filter_map(|fact| match fact {
            FlatFact::ResolvedEdge {
                caller_path,
                caller_name,
                callee_path,
                callee_name,
                ..
            } => Some((caller_path, caller_name, callee_path, callee_name)),
            _ => None,
        })
        .collect()
}

/// `user.rs`'s rows only: the file whose invocations mint the callers.
fn user_rows(extra: &[&str]) -> Vec<(Option<String>, String, Option<String>)> {
    rows(extra)
        .into_iter()
        .filter(|(caller_path, ..)| caller_path.ends_with("user.rs"))
        .map(|(_, caller, callee_path, callee)| (caller, callee_path, callee))
        .collect()
}

/// The minted caller exists as a caller of the invocation file's own callee:
/// `alpha` (minted by the foreign `mint_helpers!`) calls `helper_one`, whose
/// def lives in the same file, with `decoys.rs` carrying the same name so a
/// corpus-unique match cannot bind it. Pre-fix this set is empty.
#[test]
fn foreign_minted_caller_lands() {
    let rows = user_rows(&[]);
    let pair = |caller: &str, callee: &str| {
        rows.iter().any(|(c, path, n)| {
            c.as_deref() == Some(caller)
                && n.as_deref() == Some(callee)
                && path.ends_with("user.rs")
        })
    };
    assert!(
        pair("alpha", "helper_one") && pair("beta", "helper_two") && pair("gamma", "helper_one"),
        "the minted callers are absent from user.rs's rows: {rows:?}"
    );
}

/// One expansion mints two fns and each body keeps its own sites: `alpha`
/// calls `helper_one`, `beta` calls `helper_two`. A collapsed-span fold ties
/// every site of the expansion to the first minted def, which names the wrong
/// caller for one of the two pairs.
#[test]
fn minted_callers_bind_their_bodies() {
    let rows = user_rows(&[]);
    let callees_of = |caller: &str| -> Vec<&str> {
        rows.iter()
            .filter(|(c, _, _)| c.as_deref() == Some(caller))
            .filter_map(|(_, _, n)| n.as_deref())
            .collect()
    };
    assert_eq!(callees_of("alpha"), vec!["helper_one"], "rows: {rows:?}");
    assert_eq!(callees_of("beta"), vec!["helper_two"], "rows: {rows:?}");
}

/// `local.rs` defines its own `mint_helpers!` whose `alpha` calls
/// `helper_two` (the foreign body calls `helper_one`): the local def wins and
/// no second expansion of the same invocation double-mints a row.
#[test]
fn local_def_shadows_the_foreign() {
    let rows = rows(&["local.rs"]);
    let local: Vec<_> = rows
        .iter()
        .filter(|(caller_path, ..)| caller_path.ends_with("local.rs"))
        .filter(|(_, c, ..)| c.as_deref() == Some("alpha"))
        .collect();
    assert!(
        local.len() == 1
            && local[0].3.as_deref() == Some("helper_two")
            && local[0].2.ends_with("local.rs"),
        "the local def must win exactly once: {local:?}"
    );
}
