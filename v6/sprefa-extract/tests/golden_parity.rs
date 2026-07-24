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
    /// The fixtures sub-directory (for the regen hint in the panic message).
    fixture_dir: &'static str,
}

const CASES: &[Case] = &[
    Case {
        name: "sample",
        path: "sample.ts",
        fixture: include_bytes!("fixtures/ts/sample.ts"),
        baseline: include_str!("fixtures/ts/sample.v5.jsonl"),
        fixture_dir: "ts",
    },
    Case {
        name: "consts",
        path: "consts.ts",
        fixture: include_bytes!("fixtures/ts/consts.ts"),
        baseline: include_str!("fixtures/ts/consts.v5.jsonl"),
        fixture_dir: "ts",
    },
    Case {
        name: "rust_sample",
        path: "sample.rs",
        fixture: include_bytes!("fixtures/rust/sample.rs"),
        baseline: include_str!("fixtures/rust/sample.v5.jsonl"),
        fixture_dir: "rust",
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
    let out = dispatch(path, bytes, FamilyMask::ALL).expect("a Source matches the fixture");
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
///
/// One documented waiver: the Rust closure df-node NAME. v5 stored `lam_sym`
/// (the closure-to-body join key, encoding the file path + enclosing fn) as the
/// `closure` node's `var`; v6 drops it (the join is span-containment, not a sym
/// — same call the TS DfF port makes, which is why TS has no closure df-nodes at
/// all). The closure node's KIND + byte offset still match exactly, so the waiver
/// normalizes only the name field. The waiver is SELF-VERIFYING: the test
/// asserts that every line it removes is a `df_node closure` row, so a future
/// real regression cannot hide behind it.
#[test]
fn ported_facets_match_v5() {
    for case in CASES {
        let v5_raw: BTreeSet<String> = case
            .baseline
            .lines()
            .filter(|l| PORTED.contains(&facet_of(l)))
            .map(str::to_owned)
            .collect();
        // Apply the documented closure-name waiver (no-op for TS: zero closure
        // df-nodes; see strip_closure_name).
        let v5_ported: BTreeSet<String> =
            v5_raw.iter().map(|line| strip_closure_name(line)).collect();
        let v6 = v6_ported(case.path, case.fixture);

        // Self-verify the waiver: every v5 line it changed must be a closure
        // df-node. If a non-closure line ever differs, that is a real regression
        // and the waiver must not mask it.
        let unwaivered_only_v5: Vec<&String> = v5_raw.difference(&v6).collect();
        let hidden: Vec<&&String> = unwaivered_only_v5
            .iter()
            .filter(|line| !is_closure_df_node(line))
            .collect();
        assert!(
            hidden.is_empty(),
            "[{}] a divergence outside the closure-name waiver would be hidden by it:\n{}",
            case.name,
            hidden.iter().map(|s| format!("    {s}")).collect::<Vec<_>>().join("\n"),
        );

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
             v6/sprefa-extract/tests/fixtures/{}/{} > v6/sprefa-extract/tests/fixtures/{}/{}.v5.jsonl",
            case.name,
            only_v5.len(),
            dump(&only_v5, 50),
            only_v6.len(),
            dump(&only_v6, 50),
            case.fixture_dir,
            case.path,
            case.fixture_dir,
            case.name,
        );
    }
}

/// Whether a canonical line is a `df_node closure` row (the closure-name waiver
/// target). Fields: `df_node\tclosure\t<name>\t<byte>`.
fn is_closure_df_node(line: &str) -> bool {
    let mut parts = line.split('\t');
    parts.next() == Some("df_node") && parts.next() == Some("closure")
}

/// Normalize the closure df-node NAME to "" (v6 drops v5's lam_sym; see the
/// waiver note on `ported_facets_match_v5`). No-op for non-closure lines.
fn strip_closure_name(line: &str) -> String {
    if !is_closure_df_node(line) {
        return line.to_string();
    }
    let mut parts: Vec<&str> = line.split('\t').collect();
    if parts.len() > 2 {
        parts[2] = "";
    }
    parts.join("\t")
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
        let cst_only = flatten(&dispatch(case.path, case.fixture, FamilyMask::ALL).expect("source"))
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
