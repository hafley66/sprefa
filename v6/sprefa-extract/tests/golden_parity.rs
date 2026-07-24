//! Tier-2 PARITY GOLDEN: v6 vs the captured v5 oracle, over a fixture set. Proves
//! v6's phase-1 TS extraction matches v5 for the ported facets — line-for-line
//! for type/call/const (v5 drops byte offsets there), byte-for-byte for df.
//!
//! The oracle is CAPTURED, not linked: `cargo run --example v5_normalize --
//! <fixture> > <name>.v5.jsonl` in the v5 crate, then commit. v6 never depends on
//! v5 (its workspace is deliberately isolated). The contract lives in
//! `examples/v5_normalize.rs`; the canonical line is tab-separated + sorted.
//!
//! Facet split:
//!   PORTED {type_node, type_sig, call_def, call_site, df_node, df_edge,
//!           const_value} — v6 emits these; the test ASSERTS their set equals
//!           v5's (empty diff = gold). Const entities flow as `type_node` kind=
//!           const; `const_value` is the resolved-string rows.
//!   DEFERRED v5-only {type_edge, doc, df_param_pos/args/fields/lits/loop/nest}
//!           — reported, not asserted. type_edge lands at Resolve<TypeF> commit
//!           4; df-aux is labels-not-graph; docs are a follow-up.
//!   v6-only {cst} — v5 has NO TS tree-sitter grammar; incomparable. Reported.

use std::collections::BTreeSet;

use sprefa_extract::{dispatch, flatten, FamilyMask, FamilyTag, FlatFact};

struct Case {
    name: &'static str,
    path: &'static str,
    fixture: &'static [u8],
    baseline: &'static str,
}

const CASES: &[Case] = &[
    Case {
        name: "sample",
        path: "sample.ts",
        fixture: include_bytes!("fixtures/ts/sample.ts"),
        baseline: include_str!("fixtures/ts/sample.v5.jsonl"),
    },
    Case {
        name: "consts",
        path: "consts.ts",
        fixture: include_bytes!("fixtures/ts/consts.ts"),
        baseline: include_str!("fixtures/ts/consts.v5.jsonl"),
    },
];

const PORTED: &[&str] = &[
    "type_node", "type_sig", "call_def", "call_site", "df_node", "df_edge", "const_value",
];

/// 1-based line containing `byte_off` (newline count + 1). Matches v5's `line_at`.
fn line_of(bytes: &[u8], byte_off: u32) -> u32 {
    bytes[..byte_off as usize].iter().filter(|&&b| b == b'\n').count() as u32 + 1
}

fn facet_of(line: &str) -> &str {
    line.split('\t').next().unwrap_or("")
}

/// v6's canonical PORTED-facet lines (cst dropped — v6-only, incomparable).
fn v6_ported(path: &str, bytes: &[u8]) -> BTreeSet<String> {
    let out = dispatch(path, bytes, FamilyMask::ALL).expect("a Source matches a .ts fixture");
    let mut set = BTreeSet::new();
    for fact in flatten(&out) {
        match fact {
            FlatFact::Node { family, span, kind, name } => match family {
                FamilyTag::Type => {
                    set.insert(format!(
                        "type_node\t{kind}\t{}\t{}",
                        name.as_deref().unwrap_or(""),
                        line_of(bytes, span.start)
                    ));
                }
                FamilyTag::Call => {
                    set.insert(format!(
                        "call_def\t{kind}\t{}\t{}",
                        name.as_deref().unwrap_or(""),
                        line_of(bytes, span.start)
                    ));
                }
                FamilyTag::Df => {
                    set.insert(format!(
                        "df_node\t{kind}\t{}\t{}",
                        name.as_deref().unwrap_or(""),
                        span.start
                    ));
                }
                FamilyTag::Cst | FamilyTag::Module => {}
            },
            FlatFact::Edge { family, from, to, .. } => match family {
                FamilyTag::Df => {
                    set.insert(format!("df_edge\t{}\t{}", from.start, to.start));
                }
                FamilyTag::Cst => {}
                _ => {}
            },
            FlatFact::Sig { owner, slot, pos, ty, .. } => {
                set.insert(format!("type_sig\t{}\t{slot}\t{pos}\t{ty}", line_of(bytes, owner.start)));
            }
            FlatFact::Site { span, callee, .. } => {
                set.insert(format!("call_site\t{callee}\t{}", line_of(bytes, span.start)));
            }
            FlatFact::Const { owner, field, text, kind, .. } => {
                set.insert(format!(
                    "const_value\t{}\t{}\t{kind}\t{text}",
                    line_of(bytes, owner.start),
                    field.as_deref().unwrap_or(""),
                ));
            }
        }
    }
    set
}

/// THE GOLD: for every fixture, the PORTED facets v6 emits must equal the v5
/// oracle. A non-empty diff is a v6 regression (port bug) or an intentional
/// rename to codify (added to the waiver list).
#[test]
fn ported_facets_match_v5() {
    for case in CASES {
        let v5_ported: BTreeSet<String> = case
            .baseline
            .lines()
            .filter(|l| PORTED.contains(&facet_of(l)))
            .map(str::to_owned)
            .collect();
        let v6 = v6_ported(case.path, case.fixture);

        let only_v5: Vec<&String> = v5_ported.difference(&v6).collect();
        let only_v6: Vec<&String> = v6.difference(&v5_ported).collect();
        if only_v5.is_empty() && only_v6.is_empty() {
            continue;
        }
        let dump = |xs: &[&String], n: usize| -> String {
            xs.iter().take(n).map(|s| format!("    {s}")).collect::<Vec<_>>().join("\n")
        };
        panic!(
            "[{}] PORTED parity diff vs v5 oracle:\n  only in v5 ({}):\n{}\n  only in v6 ({}):\n{}\n\
             Regenerate the oracle: cargo run --example v5_normalize -- \
             v6/sprefa-extract/tests/fixtures/ts/{}.ts > v6/sprefa-extract/tests/fixtures/ts/{}.v5.jsonl",
            case.name,
            only_v5.len(),
            dump(&only_v5, 50),
            only_v6.len(),
            dump(&only_v6, 50),
            case.name,
            case.name,
        );
    }
}

/// The migration ledger: the measured v5-only deferred set + the v6-only CST
/// count, per fixture. Informational (run with --nocapture). Not asserted.
#[test]
fn deferred_and_v6_only_ledger() {
    for case in CASES {
        let mut deferred: std::collections::BTreeMap<&str, usize> = Default::default();
        for line in case.baseline.lines() {
            let facet = facet_of(line);
            if !PORTED.contains(&facet) {
                *deferred.entry(facet).or_default() += 1;
            }
        }
        let cst_only = flatten(&dispatch(case.path, case.fixture, FamilyMask::ALL).expect("ts"))
            .into_iter()
            .filter(|f| {
                matches!(f, FlatFact::Node { family: FamilyTag::Cst, .. }
                    | FlatFact::Edge { family: FamilyTag::Cst, .. })
            })
            .count();
        eprintln!(
            "[{}] migration ledger: v5-only deferred {deferred:?}; v6-only cst facts {cst_only}",
            case.name
        );
    }
}
