//! RED TEST — Step 4 of the rules-as-tables patch series.
//!
//! Spec: every op implements `Op::classify() -> Liftable` returning one
//! of three variants:
//!   - Pure       — pure SQL projection or filter fragment
//!   - Stream     — runs in Rust during streamed-Rust regime; provides
//!                  row schema (column → IdKind) so the rule's prepared
//!                  INSERT can be built once
//!   - RuleQuery  — query against another rule's `<rule>_facts` table
//!
//! The fuser uses these classifications to pick the regime (full-SQL vs
//! streamed-Rust) and to compose fused SQL.
//!
//! Adding a new lifting op = implementing Op::classify on the op type.
//! No central registry edit. No library change.

use v4::compile::lower::{IdKind, Liftable, Op};

#[test]
fn fs_op_classifies_as_stream() {
    // fs(glob`...`) walks the filesystem in Rust. Cannot be lifted to
    // SQL. Must return Stream with a row schema of (path: FileId-or-PathId).
    let fs_op = v4::compile::lower::ops::FsOp::new_for_test_glob("**/*.c");
    let lift = fs_op.classify();

    assert!(
        matches!(lift, Liftable::Stream { .. }),
        "fs must classify as Stream, got {:?}",
        std::mem::discriminant(&lift)
    );

    if let Liftable::Stream { schema, .. } = &lift {
        let has_path_col = schema.iter().any(|(_, kind)| {
            matches!(kind, IdKind::FileId | IdKind::PathId)
        });
        assert!(has_path_col, "fs Stream schema must include FileId/PathId column");
    }
}

#[test]
fn ast_op_classifies_as_stream() {
    // ast(:c)`pattern` runs tree-sitter. Stream variant. Output cols
    // must be id-typed; LO/HI must use WhereBytesId so source byte
    // spans are first-class.
    let ast_op = v4::compile::lower::ops::AstOp::new_for_test("c", "printk($$$)");
    let lift = ast_op.classify();

    assert!(
        matches!(lift, Liftable::Stream { .. }),
        "ast must classify as Stream"
    );

    if let Liftable::Stream { schema, .. } = &lift {
        let has_where_bytes = schema.iter().any(|(_, kind)| {
            matches!(kind, IdKind::WhereBytesId)
        });
        assert!(
            has_where_bytes,
            "ast Stream schema must include WhereBytesId for the matched span"
        );
    }
}

#[test]
fn re_op_classifies_as_pure() {
    // re`(\w+)` over an existing string column is pure. Lifts to SQL
    // via the sprf_re_match UDF or planner-supported REGEXP.
    let re_op = v4::compile::lower::ops::ReOp::new_for_test(r"(\w+)");
    let lift = re_op.classify();

    assert!(
        matches!(lift, Liftable::Pure { .. }),
        "re must classify as Pure, got {:?}",
        std::mem::discriminant(&lift)
    );
}

#[test]
fn bare_rule_call_classifies_as_rule_query() {
    // After step 1: bare `r(...)` is a RuleQuery against r_facts.
    let call = v4::compile::lower::ops::RuleCallOp::new_for_test_bare("hits", &["FS?", "LO?"]);
    let lift = call.classify();

    match lift {
        Liftable::RuleQuery { table, binds } => {
            assert_eq!(table.as_ref(), "hits_facts");
            assert_eq!(binds.len(), 2);
        }
        other => panic!("expected RuleQuery, got {:?}", std::mem::discriminant(&other)),
    }
}

#[test]
fn where_op_classifies_as_pure_with_filter() {
    // where `${A} != ${B}` is a pure WHERE-clause contribution.
    let where_op = v4::compile::lower::ops::WhereOp::new_for_test("${A} != ${B}");
    let lift = where_op.classify();

    match lift {
        Liftable::Pure { filter, .. } => {
            assert!(filter.is_some(), "where must contribute a filter expression");
        }
        other => panic!("expected Pure, got {:?}", std::mem::discriminant(&other)),
    }
}

#[test]
fn apply_form_does_not_classify_as_rule_query() {
    // `r.(X, Y)` is the apply form (run body). It is NOT a RuleQuery.
    // It runs as today through its body's pipe. Defensive test so the
    // fuser never tries to JOIN against an apply call.
    let call = v4::compile::lower::ops::RuleCallOp::new_for_test_apply("hits", &["\"foo.c\"", "1"]);
    let lift = call.classify();

    assert!(
        !matches!(lift, Liftable::RuleQuery { .. }),
        "apply form must NOT classify as RuleQuery"
    );
}
