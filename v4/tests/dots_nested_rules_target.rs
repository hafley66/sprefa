//! RED TEST — dots/types/nested-rules patch series, Task 5.
//!
//! A TYPE is a rule whose columns are its fields. The canonical
//! imported-type shape is a decl-only `rule(:Ty, f?...)` (no body —
//! just a schema). Addressing `Ty.field`:
//!
//!   - `Ty.field`  where `field` is a declared column => projection
//!     (resolves; no `lang/dot-miss`).
//!   - `Ty.bogus`  where `bogus` is NOT a column        => loud
//!     `lang/dot-miss` at compile, never silently empty rows.
//!
//! This proves `LowerCtx::resolve_dot` + the de-greed dotted-head
//! classification in `walk::classify_slot` (positional: a dotted head
//! that names a declared type is a projection, not an op).

use std::sync::Arc;

use effect_runtime::v2::{Diag, FactStore, MemFactStore, Severity};
use v4::compile::parse::host_parse;
use v4::compile::walk::walk_program;
use v4::lower::{default_registry, LowerCtx};
use v4::Cursor;

fn diags(src: &str) -> Vec<Diag> {
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let dir = std::env::temp_dir();
    let reg = default_registry();
    let (program, parse_diags) = host_parse(src);
    assert!(parse_diags.is_empty(), "parse diags: {parse_diags:?}");
    let mut ctx = LowerCtx::new(store, dir);
    let (_rules, walk_diags) = walk_program(&program, &reg, &mut ctx);
    walk_diags
}

fn has_dot_miss(ds: &[Diag]) -> bool {
    ds.iter()
        .any(|d| d.code.as_ref() == "lang/dot-miss" && d.severity == Severity::Error)
}

#[test]
fn type_field_projection_resolves() {
    // `Person` is a decl-only type with columns name, age. `Person.name`
    // in an arg slot must resolve as a projection — no dot-miss.
    let src = r#"
        rule(:Person, name?, age?);
        tag(:people, Person.name);
    "#;
    let ds = diags(src);
    assert!(
        !has_dot_miss(&ds),
        "Person.name must project (no lang/dot-miss), got: {ds:?}"
    );
}

#[test]
fn unknown_field_is_loud_dot_miss() {
    // `Person.bogus` — no such column. Must be a loud compile error,
    // not silent empty rows.
    let src = r#"
        rule(:Person, name?, age?);
        tag(:people, Person.bogus);
    "#;
    let ds = diags(src);
    assert!(
        has_dot_miss(&ds),
        "Person.bogus must raise lang/dot-miss, got: {ds:?}"
    );
}
