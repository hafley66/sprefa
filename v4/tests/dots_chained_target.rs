//! Chained dot access — `x.a.b.c`.
//!
//! `resolve_dot` projects one segment; before this patch it dropped the
//! projected value's type, so the SECOND hop had no `.ty` and the chain
//! died. With a per-`(table,col)` type map, a projected column carries
//! its own declared type forward, so a multi-segment path keeps
//! resolving until it hits a scalar (terminal) column. An unknown
//! segment anywhere in the chain is still a loud miss.

use std::sync::Arc;

use effect_runtime::v2::{FactStore, MemFactStore};
use v4::compile::lower::value::Value;
use v4::lower::LowerCtx;
use v4::Cursor;

fn ctx() -> LowerCtx {
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let c = LowerCtx::new(store, std::env::temp_dir());
    // Outer.m : Mid ; Mid.leaf : Leaf ; Leaf.v : scalar
    c.store.declare("Outer", &["m"]);
    c.store.declare("Mid", &["leaf"]);
    c.store.declare("Leaf", &["v"]);
    c.set_col_type("Outer", "m", "Mid");
    c.set_col_type("Mid", "leaf", "Leaf");
    c
}

#[test]
fn chain_resolves_through_typed_columns() {
    let c = ctx();
    let x = Value::atom("seed").typed("Outer");

    let m = c.resolve_dot(&x, "m").expect("Outer.m resolves");
    assert_eq!(m.dots.ty.as_deref().map(|s| s.as_ref()), Some("Mid"));

    let leaf = c.resolve_dot(&m, "leaf").expect("Outer.m.leaf resolves");
    assert_eq!(leaf.dots.ty.as_deref().map(|s| s.as_ref()), Some("Leaf"));

    let v = c.resolve_dot(&leaf, "v").expect("Outer.m.leaf.v resolves");
    assert!(v.dots.ty.is_none(), "scalar column is terminal (no type)");
}

#[test]
fn unknown_segment_mid_chain_is_loud() {
    let c = ctx();
    let x = Value::atom("seed").typed("Outer");
    let m = c.resolve_dot(&x, "m").expect("Outer.m resolves");
    // `Mid` has column `leaf`, not `bogus`.
    assert!(
        c.resolve_dot(&m, "bogus").is_err(),
        "Outer.m.bogus must be a loud dot-miss"
    );
}

#[test]
fn scalar_then_dot_is_loud() {
    let c = ctx();
    let x = Value::atom("seed").typed("Outer");
    let m = c.resolve_dot(&x, "m").unwrap();
    let leaf = c.resolve_dot(&m, "leaf").unwrap();
    let v = c.resolve_dot(&leaf, "v").unwrap();
    // `v` is a scalar projection: no type, so any further dot is a miss.
    assert!(
        c.resolve_dot(&v, "anything").is_err(),
        "dotting past a scalar must be loud, not silent"
    );
}
