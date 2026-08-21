//! Rust inline-`mod` scope + method-owner test: drives `RustSource` directly.
//! Expected values are hand-derived from the two fixtures, never copied from the
//! extractor's output.
//!
//! FAIL-FIRST RECEIPT, all four red before the fix:
//!   rust_call_defs_reach_inline_mod_bodies
//!     left: {"helper_a", "top_level"}
//!     right: {"deep_fn", "helper_a", "inner_method", "nested_fn",
//!             "nested_trait_method", "setup", "top_level"}
//!   rust_type_entities_reach_inline_mod_bodies
//!     left: {"ROOT_CONST", "helper_a", "top_level"}
//!     right: {"Inner", "MOD_CONST", "Nested", "ROOT_CONST", "deep_fn",
//!             "helper_a", "inner_method", "nested_fn", "setup", "top_level"}
//!   rust_df_reaches_inline_mod_bodies
//!     the `let closure = ..` bind inside inner::deeper::deep_fn is lifted: {}
//!   rust_method_defs_carry_their_owner
//!     every Method def has an owner row
//!
//! SABOTAGE RECEIPT: dropping the `syn::Item::Mod` arm from `call_defs_in_items`
//! restores the first failure verbatim; reading `i.self_ty` instead of `i.trait_`
//! for the trait seat collapses rows 3 and 5 of `rust_method_defs_carry_their_owner`.

use std::collections::BTreeSet;

use sprefa_extract::{CallKind, FamilyMask, RustSource, Source};

const NESTED_PATH: &str = "tests/fixtures/rust_scopes/nested_mods.rs";
const NESTED: &[u8] = include_bytes!("fixtures/rust_scopes/nested_mods.rs");
const OWNERS_PATH: &str = "tests/fixtures/rust_scopes/impl_owners.rs";
const OWNERS: &[u8] = include_bytes!("fixtures/rust_scopes/impl_owners.rs");
const CFG_PATH: &str = "tests/fixtures/rust_scopes/cfg_scopes.rs";
const CFG: &[u8] = include_bytes!("fixtures/rust_scopes/cfg_scopes.rs");
const CALL_SITES_PATH: &str = "tests/fixtures/rust_scopes/cfg_call_sites.rs";
const CALL_SITES: &[u8] = include_bytes!("fixtures/rust_scopes/cfg_call_sites.rs");

/// A callable declared inside an inline `mod` is a def of the file. The site half
/// already walks those bodies, so without this the extractor reports uses whose
/// definitions it never emits.
#[test]
fn rust_call_defs_reach_inline_mod_bodies() {
    let output = RustSource.extract(NESTED_PATH, NESTED, FamilyMask::ALL);
    let call = output.call.as_ref().unwrap();

    let named: BTreeSet<&str> = call
        .nodes
        .iter()
        .filter_map(|node| node.name.map(|id| output.strings.lookup(id)))
        .collect();

    assert_eq!(
        named,
        BTreeSet::from([
            "deep_fn",
            "helper_a",
            "inner_method",
            "nested_fn",
            "nested_trait_method",
            "setup",
            "top_level",
        ])
    );

    // The closure inside `deeper::deep_fn` is reached through the same descent.
    let lambdas = call
        .nodes
        .iter()
        .filter(|node| node.kind == CallKind::Lambda)
        .count();
    assert_eq!(lambdas, 1, "the nested closure is a Lambda def");
}

/// The TypeF plane descends the same bodies: entities, and the const facet.
#[test]
fn rust_type_entities_reach_inline_mod_bodies() {
    let output = RustSource.extract(NESTED_PATH, NESTED, FamilyMask::ALL);
    let types = output.types.as_ref().unwrap();

    let named: BTreeSet<&str> = types
        .nodes
        .iter()
        .filter_map(|node| node.name.map(|id| output.strings.lookup(id)))
        .collect();

    assert_eq!(
        named,
        BTreeSet::from([
            "Inner",
            "MOD_CONST",
            "Nested",
            "ROOT_CONST",
            "deep_fn",
            "helper_a",
            "inner_method",
            "nested_fn",
            "setup",
            "top_level",
        ])
    );

    let consts: BTreeSet<&str> = types
        .aux
        .consts
        .iter()
        .map(|value| output.strings.lookup(value.text))
        .collect();
    assert_eq!(consts, BTreeSet::from(["root", "scoped"]));
}

/// The df plane lifts a body inside an inline `mod`, and its fn sym carries the
/// module path so two same-named callables in sibling mods stay apart.
#[test]
fn rust_df_reaches_inline_mod_bodies() {
    let output = RustSource.extract(NESTED_PATH, NESTED, FamilyMask::ALL);
    let df = output.df.as_ref().unwrap();

    let named: BTreeSet<&str> = df
        .nodes
        .iter()
        .filter_map(|node| node.name.map(|id| output.strings.lookup(id)))
        .collect();

    assert!(
        named.contains("closure"),
        "the `let closure = ..` bind inside inner::deeper::deep_fn is lifted: {named:?}"
    );
}

/// A `Method` def names the declaration it belongs to. Self type and trait are
/// two facts: `impl Draw for Alpha` and `impl Erase for Alpha` agree on the self
/// type and differ only in the trait.
#[test]
fn rust_method_defs_carry_their_owner() {
    let output = RustSource.extract(OWNERS_PATH, OWNERS, FamilyMask::ALL);
    let call = output.call.as_ref().unwrap();

    let by_span: std::collections::BTreeMap<u32, (Option<&str>, Option<&str>)> = call
        .aux
        .method_owners
        .iter()
        .map(|owner| {
            (
                owner.span.start,
                (
                    owner.self_type.map(|id| output.strings.lookup(id)),
                    owner.trait_name.map(|id| output.strings.lookup(id)),
                ),
            )
        })
        .collect();

    let mut rows: Vec<(&str, Option<&str>, Option<&str>)> = call
        .nodes
        .iter()
        .filter(|node| node.kind == CallKind::Method)
        .map(|node| {
            let owner = by_span
                .get(&node.span.start)
                .copied()
                .expect("every Method def has an owner row");
            (
                output.strings.lookup(node.name.expect("named method")),
                owner.0,
                owner.1,
            )
        })
        .collect();
    rows.sort();

    assert_eq!(
        rows,
        [
            ("draw", None, Some("Draw")),
            ("draw", None, Some("Erase")),
            ("draw", Some("Alpha"), None),
            ("draw", Some("Alpha"), Some("Draw")),
            ("draw", Some("Alpha"), Some("Erase")),
            ("draw", Some("Beta"), Some("Draw")),
        ]
    );
}

/// A cfg predicate naming `test` guards a def wherever it sits: on the item, on
/// an enclosing module, on an ancestor of one, or inside an `any(..)` arm. The
/// two negative rows matter more than the positives: `feature = "testing"`
/// contains the letters of `test` and must NOT match, which is why the
/// predicate is split on non-word characters and compared whole.
///
/// FAIL-FIRST RECEIPT, before `cfg_test_predicate` existed: cfg_scopes was
/// empty, so `left: []`, `right: ["guarded_by_an_ancestor", "guarded_by_an_any_arm",
/// "guarded_by_the_module", "guarded_directly"]`.
///
/// SABOTAGE RECEIPT: matching with `text.contains("test")` instead of the word
/// split adds "not_guarded_the_token_is_a_substring" to the left side; passing
/// `None` instead of `active` in the `Item::Mod` arm drops
/// "guarded_by_the_module" and "guarded_by_an_ancestor".
#[test]
fn rust_cfg_test_defs_are_marked_wherever_the_predicate_sits() {
    let output = RustSource.extract(CFG_PATH, CFG, FamilyMask::ALL);
    let call = output.call.as_ref().unwrap();
    let guarded: BTreeSet<&str> = call
        .aux
        .cfg_scopes
        .iter()
        .map(|scope| {
            let node = call
                .nodes
                .iter()
                .find(|node| node.span.start == scope.span.start)
                .expect("every cfg scope joins a def node by span");
            output.strings.lookup(node.name.expect("named def"))
        })
        .collect();

    assert_eq!(
        guarded,
        BTreeSet::from([
            "guarded_directly",
            "guarded_by_the_module",
            "guarded_by_an_ancestor",
            "guarded_by_an_any_arm",
        ])
    );
}

/// A callee named ONLY from cfg-guarded sites, at any item depth, with every
/// shipped name left out. `shared_step` is called from a test AND from a
/// shipped fn, so it must NOT appear: the consumer subtracts the name, and a
/// file naming it in both places stays a caller of it. `only_shipped` is called
/// from a `feature = "testing"` fn, which is not a test cfg, so it ships too.
///
/// FAIL-FIRST RECEIPT, before the site plane read cfg at all:
///   left: {}, right: {("deeply_tested", "test"), ("only_tested", "test")}
///
/// SABOTAGE RECEIPT: dropping the `shipped` filter in `test_only_calls` adds
/// ("shared_step", "test") to the left side, and the dead-module rail then
/// reports mixed_call_sites.rs dead against two shipped call sites
/// (`v6/dl/deadcode/oracle-rustc.sh`). Dropping the `visit_item` override
/// empties the left side.
#[test]
fn rust_test_only_callees_leave_out_every_shipped_name() {
    let output = RustSource.extract(CALL_SITES_PATH, CALL_SITES, FamilyMask::ALL);
    let call = output.call.as_ref().unwrap();

    let guarded: BTreeSet<(&str, &str)> = call
        .aux
        .test_only_calls
        .iter()
        .map(|row| {
            (
                output.strings.lookup(row.callee),
                output.strings.lookup(row.cfg),
            )
        })
        .collect();

    assert_eq!(
        guarded,
        BTreeSet::from([("deeply_tested", "test"), ("only_tested", "test")])
    );
}
