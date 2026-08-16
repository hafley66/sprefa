//! The `df_field` / `df_lit` rows, graded verbatim against the v5 oracle.
//!
//! `df_field` (named value flow into a composite: struct-literal field,
//! object-literal property, Kotlin named argument) and `df_lit` (one row per
//! string-carrying df node, kind lit/template/concat) were the last deferred
//! df-aux facets. This test asserts, per fixture, that the v6 flat facts fold
//! to EXACTLY the oracle's `df_fields\t{node_idx}\t{field}\t{val_idx}` and
//! `df_lits\t{node_idx}\t{kind}\t{text}` lines — the same 16 rows
//! `grep -hE '^(df_fields|df_lits)' v6/sprefa-extract/tests/fixtures/*/*.v5.jsonl`
//! prints, byte for byte.
//!
//! Three facts the oracle pins:
//!   1. `concat` is SYNTACTIC, never a type judgment: `outer + 1` is numeric and
//!      v5 still emits `concat` with the RAW source slice of the whole binary
//!      expression, nested operators intact.
//!   2. `lit` rows are STRING literals only: `/inner/x` is a string; no numeric,
//!      boolean, null or regexp literal produced a row, though v6 pushes a `lit`
//!      df NODE for all of them.
//!   3. `template`/`concat` text is the raw source slice (holes/operators
//!      intact); `lit` text is the cooked literal value.

use std::collections::{BTreeMap, BTreeSet};

use sprefa_extract::{dispatch, flatten, FamilyMask, FamilyTag, FlatFact};

struct Case {
    name: &'static str,
    path: &'static str,
    fixture: &'static [u8],
    baseline: &'static str,
}

const CASES: &[Case] = &[
    Case {
        name: "ts_consts",
        path: "v6/sprefa-extract/tests/fixtures/ts/consts.ts",
        fixture: include_bytes!("fixtures/ts/consts.ts"),
        baseline: include_str!("fixtures/ts/consts.v5.jsonl"),
    },
    Case {
        name: "ts_sample",
        path: "v6/sprefa-extract/tests/fixtures/ts/sample.ts",
        fixture: include_bytes!("fixtures/ts/sample.ts"),
        baseline: include_str!("fixtures/ts/sample.v5.jsonl"),
    },
    Case {
        name: "ts_docs",
        path: "v6/sprefa-extract/tests/fixtures/ts/docs.ts",
        fixture: include_bytes!("fixtures/ts/docs.ts"),
        baseline: include_str!("fixtures/ts/docs.v5.jsonl"),
    },
    Case {
        name: "ts_lambdas",
        path: "v6/sprefa-extract/tests/fixtures/ts/lambdas.ts",
        fixture: include_bytes!("fixtures/ts/lambdas.ts"),
        baseline: include_str!("fixtures/ts/lambdas.v5.jsonl"),
    },
    Case {
        name: "rust_sample",
        path: "v6/sprefa-extract/tests/fixtures/rust/sample.rs",
        fixture: include_bytes!("fixtures/rust/sample.rs"),
        baseline: include_str!("fixtures/rust/sample.v5.jsonl"),
    },
    Case {
        name: "rust_docs",
        path: "v6/sprefa-extract/tests/fixtures/rust/docs.rs",
        fixture: include_bytes!("fixtures/rust/docs.rs"),
        baseline: include_str!("fixtures/rust/docs.v5.jsonl"),
    },
    Case {
        name: "go_sample",
        path: "v6/sprefa-extract/tests/fixtures/go/sample.go",
        fixture: include_bytes!("fixtures/go/sample.go"),
        baseline: include_str!("fixtures/go/sample.v5.jsonl"),
    },
    Case {
        name: "go_docs",
        path: "v6/sprefa-extract/tests/fixtures/go/docs.go",
        fixture: include_bytes!("fixtures/go/docs.go"),
        baseline: include_str!("fixtures/go/docs.v5.jsonl"),
    },
    Case {
        name: "kotlin_sample",
        path: "v6/sprefa-extract/tests/fixtures/kotlin/sample.kt",
        fixture: include_bytes!("fixtures/kotlin/sample.kt"),
        baseline: include_str!("fixtures/kotlin/sample.v5.jsonl"),
    },
];

/// The `df_fields`/`df_lits` lines v6 folds to, in the oracle's dense-index
/// shape. flatten_df emits the DfF nodes as one contiguous run in bundle order,
/// so the nth Df node fact is index n — the same dense NodeIdx v5 keyed these
/// rows on.
fn v6_aux(path: &str, bytes: &[u8]) -> BTreeSet<String> {
    let out = dispatch(path, bytes, FamilyMask::ALL).expect("a Source matches the fixture");
    let facts = flatten(&out);
    let df_index: BTreeMap<u32, u32> = facts
        .iter()
        .filter_map(|fact| match fact {
            FlatFact::Node {
                family: FamilyTag::Df,
                span,
                ..
            } => Some(span.start),
            _ => None,
        })
        .enumerate()
        .map(|(ix, start)| (start, ix as u32))
        .collect();
    let mut set = BTreeSet::new();
    for fact in &facts {
        match fact {
            FlatFact::DfField {
                owner, name, value, ..
            } => {
                set.insert(format!(
                    "df_fields\t{}\t{name}\t{}",
                    df_index[&owner.start], df_index[&value.start]
                ));
            }
            FlatFact::DfLit {
                node, kind, text, ..
            } => {
                set.insert(format!(
                    "df_lits\t{}\t{kind}\t{text}",
                    df_index[&node.start]
                ));
            }
            _ => {}
        }
    }
    set
}

#[test]
fn df_aux_fields_lits_match_v5() {
    for case in CASES {
        let oracle: BTreeSet<String> = case
            .baseline
            .lines()
            .filter(|line| line.starts_with("df_fields\t") || line.starts_with("df_lits\t"))
            .map(str::to_owned)
            .collect();
        let v6 = v6_aux(case.path, case.fixture);
        let only_v5: Vec<&String> = oracle.difference(&v6).collect();
        let only_v6: Vec<&String> = v6.difference(&oracle).collect();
        assert!(
            only_v5.is_empty() && only_v6.is_empty(),
            "[{}] df_aux (fields/lits) parity diff vs v5 oracle:\n  only in v5 ({}):\n{}\n  only in v6 ({}):\n{}",
            case.name,
            only_v5.len(),
            only_v5.iter().map(|s| format!("    {s}")).collect::<Vec<_>>().join("\n"),
            only_v6.len(),
            only_v6.iter().map(|s| format!("    {s}")).collect::<Vec<_>>().join("\n"),
        );
        eprintln!(
            "[{}] df_aux fields/lits parity: {} oracle rows, 0 divergence",
            case.name,
            oracle.len()
        );
    }
}
