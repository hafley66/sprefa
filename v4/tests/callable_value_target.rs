//! RED SPEC — Callable as a Value (plan: plans/2026-05-18-callable-value.md,
//! "Corrected (2026-05-19)"). Rust-API level: there is intentionally NO
//! sprf surface yet (#9), this is internal plumbing that the grammar +
//! ty→Value work later point at.
//!
//! RED today: `CallableRef`, `CallableKind`, `Value::callable`,
//! `v4::lower::apply`, `ArgKind::Callable`, and the `resolve_dot`
//! Callable(Rule) arm do not exist — this file fails to COMPILE. That
//! compile failure IS the red state. GREEN = the patched plan built.
//!
//! Each test pins one corrected hole:
//!   H6  value_callable_ctor_and_identity   — variant, kind_str, (kind,name) eq
//!   H5  argkind_callable_matches            — ArgKind::Callable + Any accepts
//!   H1/2 apply_op_returns_pipe_value        — apply via real Registry::lower
//!   H3  apply_rule_returns_pipe_value       — apply via real invoke path
//!   H4  resolve_dot_on_callable_rule        — Callable(Rule) projects cols
//!   H7  apply_rule_uses_existing_invoke     — NO new subscription path

use std::sync::Arc;

use effect_runtime::v2::{Component, FactStore, MemFactStore, Pipe};
use v4::lower::{
    apply, default_registry, ArgKind, CallableKind, CallableRef, LowerCtx, Value, ValueKind,
};
use v4::rule::{Rule, RuleInvokeComponent};
use v4::Cursor;

fn ctx() -> (LowerCtx, Arc<dyn FactStore<Cursor>>) {
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let c = LowerCtx::new(store.clone(), std::env::temp_dir());
    (c, store)
}

// ── H6: the variant exists; identity is (kind, name) ──────────────────────
#[test]
fn value_callable_ctor_and_identity() {
    let a = Value::callable(CallableKind::Op, "split");
    let b = Value::callable(CallableKind::Op, "split");
    let c = Value::callable(CallableKind::Rule, "split");

    assert_eq!(a.kind_str(), "callable", "new kind_str arm");
    assert!(matches!(a.kind(), ValueKind::Callable(_)));

    // name-based eq/hash by (kind, name) — feeds the existing blake3 _id.
    let key = |v: &Value| match v.kind() {
        ValueKind::Callable(r) => format!("{:?}:{}", r.kind, r.name),
        _ => unreachable!(),
    };
    assert_eq!(key(&a), key(&b), "same (kind,name) ⇒ equal identity");
    assert_ne!(key(&a), key(&c), "kind discriminates Op vs Rule");
}

// ── H5: ArgKind::Callable; Any still accepts; Pipe/Atom reject ────────────
#[test]
fn argkind_callable_matches() {
    let f = Value::callable(CallableKind::Op, "fs");
    let atom = Value::atom("x");

    assert!(ArgKind::Callable.matches(&f), "Callable slot accepts callable");
    assert!(!ArgKind::Callable.matches(&atom), "Callable slot rejects atom");
    assert!(ArgKind::Any.matches(&f), "Any still accepts a callable");
    assert!(!ArgKind::Pipe.matches(&f), "Pipe slot rejects a callable");
    assert!(!ArgKind::Atom.matches(&f), "Atom slot rejects a callable");
}

// ── H1/H2: apply(Op) routes through the REAL Registry::lower ──────────────
#[test]
fn apply_op_returns_pipe_value() {
    let (c, _s) = ctx();
    let reg = default_registry();

    // `print` is a registered op taking one optional atom (no required
    // dsl/block); applying it must yield an APPLIED callable == a Pipe
    // value (the §Premise collapse). (`str` is unusable here: it
    // requires a `` dsl body and takes no paren args.)
    let cref = CallableRef {
        kind: CallableKind::Op,
        name: "print".into(),
    };
    let out = apply(&c, &reg, &cref, vec![Value::atom("hello")])
        .expect("apply(Op print) ok");
    assert_eq!(out.kind_str(), "pipe", "apply output reuses Pipe");

    // unknown op ⇒ loud error, never a silent empty pipe.
    let bad = CallableRef {
        kind: CallableKind::Op,
        name: "nope_not_an_op".into(),
    };
    assert!(
        apply(&c, &reg, &bad, vec![]).is_err(),
        "apply of an unknown op must error"
    );
}

// ── H3: apply(Rule) routes through the existing invoke path ───────────────
#[test]
fn apply_rule_returns_pipe_value() {
    let (c, store) = ctx();
    let reg = default_registry();

    let r = Rule::new("R", store.clone(), "R", &["x"], Pipe::new());
    c.register_rule(r);

    let cref = CallableRef {
        kind: CallableKind::Rule,
        name: "R".into(),
    };
    let out = apply(&c, &reg, &cref, vec![Value::atom("1")])
        .expect("apply(Rule R) ok");
    assert_eq!(out.kind_str(), "pipe", "apply(Rule) output is a Pipe");
}

// ── H4: resolve_dot must inspect ValueKind::Callable(Rule) ────────────────
#[test]
fn resolve_dot_on_callable_rule() {
    let (c, store) = ctx();
    // A decl-only type: rule(:Pt, x?, y?) ⇒ schema only.
    store.declare("Pt", &["x", "y"]);

    let pt = Value::callable(CallableKind::Rule, "Pt");

    // `.x` is a declared column ⇒ projects (no DotMiss), even though
    // pt.dots.ty == None (the whole point of H4).
    let proj = c
        .resolve_dot(&pt, "x")
        .expect("Callable(Rule Pt).x must project a column");
    assert_eq!(proj.kind_str(), "pipe", "projected column is a Pipe");

    // `.z` is not a column ⇒ loud DotMiss, never empty.
    assert!(
        c.resolve_dot(&pt, "z").is_err(),
        "Callable(Rule Pt).z must be a loud dot-miss"
    );
}

// ── H7: apply(Rule) must reuse the EXISTING invoke component ──────────────
// The recursion arc's owner-subscribe + self/SCC wake-guard is keyed to
// RuleInvokeComponent. apply(Rule) must NOT introduce a second
// subscription path; structurally, the produced pipe's step is exactly
// a RuleInvokeComponent (same type a normal rule invoke lowers to).
#[test]
fn apply_rule_uses_existing_invoke_component() {
    let (c, store) = ctx();
    let reg = default_registry();

    let r = Rule::new("Self", store.clone(), "Self", &["n"], Pipe::new());
    c.register_rule(r);

    let cref = CallableRef {
        kind: CallableKind::Rule,
        name: "Self".into(),
    };
    let out = apply(&c, &reg, &cref, vec![Value::atom("0")])
        .expect("apply(Rule Self) ok");

    let steps: &[Arc<dyn Component<Next = Cursor>>] = match out.kind() {
        ValueKind::Pipe(p) => &p.steps,
        _ => panic!("apply(Rule) must yield a Pipe"),
    };
    let last = steps.last().expect("non-empty pipe");
    let is_invoke = last
        .describe()
        .and_then(|d| d.downcast_ref::<RuleInvokeComponent>().map(|_| ()))
        .is_some();
    assert!(
        is_invoke,
        "apply(Rule) must step the EXISTING RuleInvokeComponent (no new \
         subscription path) so it inherits the recursion wake-guard"
    );
}
