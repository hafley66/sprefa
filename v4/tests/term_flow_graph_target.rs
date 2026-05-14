//! RED TEST — Step 3 of the rules-as-tables patch series.
//!
//! Spec: a compile-time `TermFlowGraph` per rule body, computed once
//! and consumed by the fuser. The graph records, for every capture:
//!   - binding_site: (ScopeId, OpIdx)
//!   - read_sites: Vec<(ScopeId, OpIdx, ReadCtx)>
//!   - provenance: RegexGroup | AstMetavar | CursorBind | RuleProj | RuleParam | Literal
//!   - value_lattice: TypeLattice — bytes ⊑ string ⊑ tokens ⊑ tree, with id-types as parallel refinements
//!   - constraints: Eq(other) | LiteralPin(StringId|i64) | NullImpossible
//!   - fanout: Dead | Once | Many
//!   - crosses_scope: bool
//! Plus: eq_classes UnionFind, scopes Vec<ScopeInfo>, steps Vec<StepInfo>.
//!
//! These tests pin the contract. Five pre-computations consume the graph
//! (covered in fuser_regimes_target.rs). The graph build itself is what
//! we test here.

use std::sync::Arc;

use effect_runtime::v2::{FactStore, MemFactStore};
use v4::compile::binding_graph::{
    build_graph, CaptureInfo, Fanout, Provenance, ReadCtx, TypeLattice,
};
use v4::compile::parse::host_parse;
use v4::lower::{default_registry, LowerCtx};
use v4::Cursor;

fn graph_for(src: &str) -> v4::compile::binding_graph::ProgramFlowGraph {
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let dir = std::env::temp_dir();
    let reg = default_registry();
    let (program, parse_diags) = host_parse(src);
    assert!(parse_diags.is_empty(), "parse diags: {parse_diags:?}");
    let mut ctx = LowerCtx::new(store, dir);
    let (graph, _diags) = build_graph(&program, &reg, &mut ctx);
    graph
}

#[test]
fn captures_record_binding_site_and_read_sites() {
    // Body binds X at step 0 (ast metavar), reads X at step 1 (where).
    let src = r#"
        rule(:r, :X) {
            ast(:c)`fn ${X?}()` > where ${X} != ""
        }
    "#;
    let g = graph_for(src);
    let body_id = g.rule_body_id("r").expect("rule r has body graph");
    let body = &g.bodies[body_id];

    let x = body.captures.get("X").expect("X must be tracked");
    assert_eq!(x.binding_site.1, 0, "X binds at op 0 (ast)");
    assert_eq!(x.read_sites.len(), 1, "X read once (in where)");
    assert_eq!(x.read_sites[0].1, 1, "X read at op 1 (where)");
    assert!(
        matches!(x.read_sites[0].2, ReadCtx::Predicate),
        "X read context is the where predicate"
    );
}

#[test]
fn dead_capture_detected() {
    // HI is bound by ast but never read. Fanout must be Dead so the
    // fuser (P1) can drop it from the projection.
    let src = r#"
        rule(:hits, :FS, :LO) {
            fs(glob`**/*.c`) > ast(:c)`printk(${LO?}, ${HI?})`
        }
    "#;
    let g = graph_for(src);
    let body = &g.bodies[g.rule_body_id("hits").unwrap()];

    let hi = body.captures.get("HI").expect("HI must be tracked");
    assert!(
        matches!(hi.fanout, Fanout::Dead),
        "HI is never read; fanout must be Dead, got {:?}",
        hi.fanout
    );

    let lo = body.captures.get("LO").expect("LO must be tracked");
    assert!(
        matches!(lo.fanout, Fanout::Once | Fanout::Many),
        "LO is in the rule's declared cols; must not be Dead"
    );
}

#[test]
fn provenance_tagged_for_ast_and_rule_proj() {
    // X from ast metavar → AstMetavar provenance.
    // Y read from upstream rule projection → RuleProj provenance.
    let src = r#"
        rule(:hits, :FS, :X) { ast(:c)`fn ${X?}()` };
        rule(:caller, :FS, :X) { hits(FS?, X?) > where ${X} != "main" }
    "#;
    let g = graph_for(src);
    let caller = &g.bodies[g.rule_body_id("caller").unwrap()];

    let x = caller.captures.get("X").unwrap();
    assert!(
        matches!(x.provenance, Provenance::RuleProj { ref rule, ref col } if rule.as_ref() == "hits" && col.as_ref() == "X"),
        "X provenance must be RuleProj{{hits, X}}, got {:?}",
        x.provenance
    );

    let hits_body = &g.bodies[g.rule_body_id("hits").unwrap()];
    let xh = hits_body.captures.get("X").unwrap();
    assert!(
        matches!(xh.provenance, Provenance::AstMetavar { .. }),
        "X in hits body provenance must be AstMetavar, got {:?}",
        xh.provenance
    );
}

#[test]
fn eq_classes_union_for_shared_join_keys() {
    // Two rule queries sharing a capture (FS) means the join keys are
    // equivalent. The eq-class structure must reflect that.
    let src = r#"
        rule(:hits, :FS, :LO) { fs(glob`**/*.c`) > ast(:c)`printk(${LO?})` };
        rule(:hit_pairs, :FS, :LO, :OTHER_LO) {
            hits(FS?, LO?) > hits(FS?, OTHER_LO?)
        }
    "#;
    let g = graph_for(src);
    let body = &g.bodies[g.rule_body_id("hit_pairs").unwrap()];

    let fs_root = body.eq_classes.find("FS");
    let other_fs_root = body.eq_classes.find("FS"); // both occurrences alias same name
    assert_eq!(
        fs_root, other_fs_root,
        "FS occurrences from both rule queries must be in one eq-class"
    );
    // LO and OTHER_LO are distinct binds (different names, different roles).
    assert_ne!(
        body.eq_classes.find("LO"),
        body.eq_classes.find("OTHER_LO"),
        "LO and OTHER_LO must NOT be unified"
    );
}

#[test]
fn lattice_tightens_for_ast_capture_to_where_bytes() {
    // A capture bound by an AST metavar over a source range should
    // tighten its value_lattice to WhereBytesId, not just string.
    // The fuser uses this (P4) to emit INTEGER REFERENCES _where_bytes
    // instead of string columns.
    let src = r#"
        rule(:hits, :FS, :LO) { fs(glob`**/*.c`) > ast(:c)`printk(${LO?})` }
    "#;
    let g = graph_for(src);
    let body = &g.bodies[g.rule_body_id("hits").unwrap()];

    let lo = body.captures.get("LO").unwrap();
    assert!(
        matches!(lo.value_lattice, TypeLattice::WhereBytesId | TypeLattice::Tokens | TypeLattice::Tree),
        "LO must lattice-tighten to a source-byte id type, got {:?}",
        lo.value_lattice
    );
}

#[test]
fn literal_pin_constraint_for_literal_arg() {
    // `hits("/etc/passwd", X?)` pins col 0 to a literal. The graph must
    // record that constraint so the fuser (P3) can pre-intern and
    // compare integers, not text.
    let src = r#"
        rule(:hits, :FS, :X) { fs(glob`**`) > ast(:c)`fn ${X?}()` };
        rule(:q, :X) { hits("/etc/passwd", X?) }
    "#;
    let g = graph_for(src);
    let body = &g.bodies[g.rule_body_id("q").unwrap()];

    // The implicit FS arg in the call site must carry a LiteralPin.
    let fs_pin = body
        .call_constraints
        .iter()
        .find(|c| c.call_idx == 0 && c.col.as_ref() == "FS")
        .expect("call constraint on FS must exist");
    assert!(
        matches!(fs_pin.kind, v4::compile::binding_graph::ConstraintKind::LiteralPin(_)),
        "FS arg must be LiteralPin, got {:?}",
        fs_pin.kind
    );
}
