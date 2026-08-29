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
//!           df_field, df_lit, const_value} — v6 emits these; the test ASSERTS
//!           their set equals v5's (empty diff = gold). Const entities flow as
//!           `type_node` kind=const; `const_value` is the resolved-string rows.
//!   PORTED for ts + go + rust + python {type_edge} — phase-2 rows: `Resolve<TypeF>`
//!           over the fixture corpus, twin-normalized to the oracle's text
//!           shape (per-lang tests below: 4b-iii ts, 4d-i-go go, 4d-i-rust).
//!   PORTED for rust {doc} — `rust_doc_parity` below asserts it; the ts, go
//!           and kotlin walkers are the follow-up, which is why `doc` is not in
//!           the global PORTED list.
//!   DEFERRED v5-only {df_param_pos/args/loop/nest} —
//!           reported, not asserted. df-aux is labels-not-graph.
//!   v6-only {cst, specifier} — cst: v5 has NO TS tree-sitter grammar;
//!           incomparable. specifier: module import/export-from rows (4b-ii);
//!           v5's module_binding is a modgraph rel the captured normalize does
//!           not emit, so there is no oracle facet. The resolved span->blob
//!           type_edge legs are also v6-only (reported, never asserted). All
//!           reported, not asserted.

use std::collections::BTreeSet;
use std::sync::Arc;

use sprefa_extract::{
    build_def_index, byte_range, containing_def_site, content_id_of, covering_def, definition_of,
    dispatch, flatten, join_documents, site_occurrence, CallEdgeKind, ContentId, ExtractOutput,
    FamilyMask, FamilyTag, FileSet, FlatFact, GoSource, IndexBag, ManifestMap, ProjectCx,
    ProjectDigest, PythonSource, Resolve, RustSource, ScipGo, ScipRust, ScipSource, ScipTypescript,
    Span, TsSource, TypeF, ZERO_CONTENT_ID,
};

struct Case {
    name: &'static str,
    /// The fixture path AS THE ORACLE WAS INVOKED WITH IT (from the worktree
    /// root). v5's closure df-node name (`lam_sym`) embeds this exact string as
    /// its root segment, so byte-exact parity requires dispatching with it
    /// verbatim — not the bare file name.
    path: &'static str,
    fixture: &'static [u8],
    baseline: &'static str,
    /// The fixtures sub-directory (for the regen hint in the panic message).
    fixture_dir: &'static str,
}

const CASES: &[Case] = &[
    Case {
        name: "sample",
        path: "v6/sprefa-extract/tests/fixtures/ts/sample.ts",
        fixture: include_bytes!("fixtures/ts/sample.ts"),
        baseline: include_str!("fixtures/ts/sample.v5.jsonl"),
        fixture_dir: "ts",
    },
    Case {
        name: "consts",
        path: "v6/sprefa-extract/tests/fixtures/ts/consts.ts",
        fixture: include_bytes!("fixtures/ts/consts.ts"),
        baseline: include_str!("fixtures/ts/consts.v5.jsonl"),
        fixture_dir: "ts",
    },
    Case {
        name: "docs",
        path: "v6/sprefa-extract/tests/fixtures/ts/docs.ts",
        fixture: include_bytes!("fixtures/ts/docs.ts"),
        baseline: include_str!("fixtures/ts/docs.v5.jsonl"),
        fixture_dir: "ts",
    },
    Case {
        name: "lambdas",
        path: "v6/sprefa-extract/tests/fixtures/ts/lambdas.ts",
        fixture: include_bytes!("fixtures/ts/lambdas.ts"),
        baseline: include_str!("fixtures/ts/lambdas.v5.jsonl"),
        fixture_dir: "ts",
    },
    Case {
        name: "rust_sample",
        path: "v6/sprefa-extract/tests/fixtures/rust/sample.rs",
        fixture: include_bytes!("fixtures/rust/sample.rs"),
        baseline: include_str!("fixtures/rust/sample.v5.jsonl"),
        fixture_dir: "rust",
    },
    Case {
        name: "rust_docs",
        path: "v6/sprefa-extract/tests/fixtures/rust/docs.rs",
        fixture: include_bytes!("fixtures/rust/docs.rs"),
        baseline: include_str!("fixtures/rust/docs.v5.jsonl"),
        fixture_dir: "rust",
    },
    Case {
        name: "go_sample",
        path: "v6/sprefa-extract/tests/fixtures/go/sample.go",
        fixture: include_bytes!("fixtures/go/sample.go"),
        baseline: include_str!("fixtures/go/sample.v5.jsonl"),
        fixture_dir: "go",
    },
    Case {
        name: "go_docs",
        path: "v6/sprefa-extract/tests/fixtures/go/docs.go",
        fixture: include_bytes!("fixtures/go/docs.go"),
        baseline: include_str!("fixtures/go/docs.v5.jsonl"),
        fixture_dir: "go",
    },
    Case {
        name: "sample",
        path: "v6/sprefa-extract/tests/fixtures/python/sample.py",
        fixture: include_bytes!("fixtures/python/sample.py"),
        baseline: include_str!("fixtures/python/sample.v5.jsonl"),
        fixture_dir: "python",
    },
    Case {
        name: "docs",
        path: "v6/sprefa-extract/tests/fixtures/python/docs.py",
        fixture: include_bytes!("fixtures/python/docs.py"),
        baseline: include_str!("fixtures/python/docs.v5.jsonl"),
        fixture_dir: "python",
    },
    Case {
        name: "flow",
        path: "v6/sprefa-extract/tests/fixtures/python/flow.py",
        fixture: include_bytes!("fixtures/python/flow.py"),
        baseline: include_str!("fixtures/python/flow.v5.jsonl"),
        fixture_dir: "python",
    },
    Case {
        name: "go_edges",
        path: "v6/sprefa-extract/tests/fixtures/go/edges.go",
        fixture: include_bytes!("fixtures/go/edges.go"),
        baseline: include_str!("fixtures/go/edges.v5.jsonl"),
        fixture_dir: "go",
    },
    Case {
        name: "kotlin_sample",
        path: "v6/sprefa-extract/tests/fixtures/kotlin/sample.kt",
        fixture: include_bytes!("fixtures/kotlin/sample.kt"),
        baseline: include_str!("fixtures/kotlin/sample.v5.jsonl"),
        fixture_dir: "kotlin",
    },
];

const PORTED: &[&str] = &[
    "type_node",
    "type_sig",
    "call_def",
    "call_site",
    "df_node",
    "df_edge",
    // df_field/df_lit are graded by content in 18_df_aux_fields_lits.rs;
    // listing them here pins v6 push order to v5's node index, a coupling
    // the user declined (2026-08-16).
    "const_value",
];

/// 1-based line containing `byte_off` (newline count + 1). Matches v5's `line_at`.
fn line_of(bytes: &[u8], byte_off: u32) -> u32 {
    bytes[..byte_off as usize]
        .iter()
        .filter(|&&b| b == b'\n')
        .count() as u32
        + 1
}

fn facet_of(line: &str) -> &str {
    line.split('\t').next().unwrap_or("")
}

/// v6's canonical PORTED-facet lines (cst dropped — v6-only, incomparable).
fn v6_ported(path: &str, bytes: &[u8]) -> BTreeSet<String> {
    let out = dispatch(path, bytes, FamilyMask::ALL).expect("a Source matches the fixture");
    let facts = flatten(&out);
    // v5's doc row is keyed by the entity SYM, so the doc arm below needs the
    // kind and name of the type node at the doc's owner span.
    let entity: std::collections::BTreeMap<(u32, u32), (String, String)> = facts
        .iter()
        .filter_map(|fact| match fact {
            FlatFact::Node {
                family: FamilyTag::Type,
                span,
                kind,
                name: Some(name),
            } => Some(((span.start, span.end), (kind.clone(), name.clone()))),
            _ => None,
        })
        .collect();
    // df node index -> byte start, in push order (flatten_df emits the DfF
    // nodes contiguously); the v5 oracle keys df_fields/df_lits by this index.
    let df_index: std::collections::HashMap<u32, u32> = facts
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
    // An interface method spec is a v6-only call_def (no v5 construct mints
    // one): skip it via its method_owner naming a `kind=interface` type node.
    let interface_names: std::collections::BTreeSet<&str> = facts
        .iter()
        .filter_map(|fact| match fact {
            FlatFact::Node {
                family: FamilyTag::Type,
                kind,
                name: Some(name),
                ..
            } if kind == "interface" => Some(name.as_str()),
            _ => None,
        })
        .collect();
    let interface_spec_spans: std::collections::BTreeSet<(u32, u32)> = facts
        .iter()
        .filter_map(|fact| match fact {
            FlatFact::MethodOwnerOut {
                owner,
                self_type: Some(self_type),
                ..
            } if interface_names.contains(self_type.as_str()) => Some((owner.start, owner.end)),
            _ => None,
        })
        .collect();
    let mut set = BTreeSet::new();
    for fact in facts {
        match fact {
            FlatFact::Node {
                family,
                span,
                kind,
                name,
            } => match family {
                FamilyTag::Type => {
                    set.insert(format!(
                        "type_node\t{kind}\t{}\t{}",
                        name.as_deref().unwrap_or(""),
                        line_of(bytes, span.start)
                    ));
                }
                FamilyTag::Call => {
                    if !interface_spec_spans.contains(&(span.start, span.end)) {
                        set.insert(format!(
                            "call_def\t{kind}\t{}\t{}",
                            name.as_deref().unwrap_or(""),
                            line_of(bytes, span.start)
                        ));
                    }
                }
                FamilyTag::Df => {
                    set.insert(format!(
                        "df_node\t{kind}\t{}\t{}",
                        name.as_deref().unwrap_or(""),
                        span.start
                    ));
                }
                FamilyTag::Cst
                | FamilyTag::Module
                | FamilyTag::Flow
                | FamilyTag::Cfg
                | FamilyTag::Data => {}
            },
            FlatFact::Edge {
                family, from, to, ..
            } => match family {
                FamilyTag::Df => {
                    set.insert(format!("df_edge\t{}\t{}", from.start, to.start));
                }
                FamilyTag::Cst => {}
                _ => {}
            },
            FlatFact::DfParam { .. } | FlatFact::DfArg { .. } => {}
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
            FlatFact::Sig {
                owner,
                slot,
                pos,
                ty,
                ..
            } => {
                set.insert(format!(
                    "type_sig\t{}\t{slot}\t{pos}\t{ty}",
                    line_of(bytes, owner.start)
                ));
            }
            FlatFact::Site { span, callee, .. } => {
                set.insert(format!(
                    "call_site\t{callee}\t{}",
                    line_of(bytes, span.start)
                ));
            }
            FlatFact::Const {
                owner,
                field,
                text,
                kind,
                ..
            } => {
                set.insert(format!(
                    "const_value\t{}\t{}\t{kind}\t{text}",
                    line_of(bytes, owner.start),
                    field.as_deref().unwrap_or(""),
                ));
            }
            // v5's `doc` row is `doc\t<path>::<kind>::<name>\t<line>`, the one
            // ported facet that carries the path; a method's name is qualified
            // by its impl owner.
            FlatFact::Doc { owner, parent, .. } => {
                if let Some((kind, name)) = entity.get(&(owner.start, owner.end)) {
                    let qualified = match parent {
                        Some(owner_type) => format!("{owner_type}.{name}"),
                        None => name.clone(),
                    };
                    set.insert(format!(
                        "doc\t{path}::{kind}::{qualified}\t{}",
                        line_of(bytes, owner.start)
                    ));
                }
            }
            // The captured oracle carries no tag rows; v6-only, never asserted.
            FlatFact::DocTagOut { .. } => {}
            // Phase-2 rows: `flatten` never produces these (dispatch stays
            // phase-1); the Resolve<TypeF> twin-normalize lands with the
            // type_edge DEFERRED->PORTED flip.
            FlatFact::ProjectEdge { .. } => {}
            // FlowF join output is phase-2 (never in `flatten`); 13_flow_join.rs
            // pins its shape.
            FlatFact::FlowEdgeOut { .. } => {}
            // Module specifier rows are v6-ONLY (no v5 oracle facet): reported
            // by the ledger test below, never asserted.
            FlatFact::Specifier { .. } => {}
            // Prolog term-occurrence references are what this lane adds and are
            // not a v5 oracle facet; they are reported by the reference ledger
            // test in 1a_prolog_refs.rs, never asserted here.
            FlatFact::Reference { .. } => {}
            // Project-mode rows: `flatten` never produces these either. They
            // come out of `project::resolve_project`, and the CLI goldens in
            // 1_resolve_cli.rs pin their shapes.
            FlatFact::ResolvedEdge { .. } => {}
            FlatFact::ResolvedTypeEdge { .. } => {}
            // Opt-in modes, never in `flatten`: `--scip-facts` projects a loaded
            // SCIP index and `--file-fact` the file identity row. Both are
            // pinned by 5_scip_facts_cli.rs.
            FlatFact::ScipMetadataRow { .. } => {}
            FlatFact::ScipDocumentRow { .. } => {}
            FlatFact::ScipOccurrenceRow { .. } => {}
            FlatFact::ScipOccurrenceDocRow { .. } => {}
            FlatFact::ScipDiagnosticRow { .. } => {}
            FlatFact::ScipSymbolRow { .. } => {}
            FlatFact::ScipRelationshipRow { .. } => {}
            FlatFact::ScipDocumentationRow { .. } => {}
            FlatFact::ScipSignatureRow { .. } => {}
            FlatFact::ScipSignatureOccurrenceRow { .. } => {}
            FlatFact::FileRow { .. } => {}
            FlatFact::FileEdgeRow { .. } => {}
            // The `--family scip` rows: v5's own scip_* relation shapes,
            // projected from a real indexer's index. `flatten` never produces
            // them either (they come out of `project::scip_family`), and they
            // have no v5 EXTRACTOR facet to be at parity with — v5 produces the
            // identical relations from the identical indexer, so grading them
            // here would compare an index to itself. 8_scip_families_cli.rs
            // pins their shapes against the real indexers instead.
            FlatFact::ScipDefRow { .. } => {}
            FlatFact::ScipNameRow { .. } => {}
            FlatFact::ScipRefRow { .. } => {}
            FlatFact::ScipEdgeRow { .. } => {}
            FlatFact::ScipFnEdgeRow { .. } => {}
            FlatFact::ScipCalleeTypeRow { .. } => {}
            FlatFact::ScipLocalRow { .. } => {}
            FlatFact::ScipImplRow { .. } => {}
            FlatFact::ScipIndexRow { .. } => {}
            FlatFact::ScipSkipRow { .. } => {}
            // Unreachable today: this match is exhaustive. It exists so a new
            // FlatFact variant does not break this normalize.
            #[allow(unreachable_patterns)]
            _ => {}
        }
    }
    set
}

/// THE GOLD: for every fixture, the PORTED facets v6 emits must equal the v5
/// oracle. A non-empty diff is a v6 regression (port bug).
///
/// ZERO WAIVERS: the closure df-node NAME is asserted byte-exact — v6's df
/// walkers mint v5's `lam_sym` (`{file}::function::{fn}::closure::{coord}`;
/// coord = byte offset for ts, `{line}_{col}` 1-based/0-based for rust,
/// `{row}_{col}` 0-based for go; nesting chains) derived from the walk's
/// containment path + span data, no sym machinery.
#[test]
fn ported_facets_match_v5() {
    for case in CASES {
        let v5_ported: BTreeSet<String> = case
            .baseline
            .lines()
            .filter(|l| PORTED.contains(&facet_of(l)))
            .map(str::to_owned)
            .collect();
        // Both sides filter by PORTED. Without it, a facet v6 emits ahead of its
        // global flip (rust `doc`) reads as a diff on every other case.
        let v6: BTreeSet<String> = v6_ported(case.path, case.fixture)
            .into_iter()
            .filter(|line| PORTED.contains(&facet_of(line)))
            .collect();

        let only_v5: Vec<&String> = v5_ported.difference(&v6).collect();
        let only_v6: Vec<&String> = v6.difference(&v5_ported).collect();
        if only_v5.is_empty() && only_v6.is_empty() {
            continue;
        }
        let dump = |xs: &[&String], n: usize| -> String {
            xs.iter()
                .take(n)
                .map(|s| format!("    {s}"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        panic!(
            "[{}] PORTED parity diff vs v5 oracle:\n  only in v5 ({}):\n{}\n  only in v6 ({}):\n{}\n\
             Regenerate the oracle: cargo run --example v5_normalize -- \
             {} > v6/sprefa-extract/tests/fixtures/{}/{}.v5.jsonl",
            case.name,
            only_v5.len(),
            dump(&only_v5, 50),
            only_v6.len(),
            dump(&only_v6, 50),
            case.path,
            case.fixture_dir,
            case.name,
        );
    }
}

/// Build the phase-2 corpus: every case's ExtractOutput + its real blake3 blob
/// hash, the DefIndex folded over all of them (the resolution universe), and a
/// borrowed ProjectCx. Shared by the type_edge parity test and the ledger test.
fn with_resolve_cx<R>(
    f: impl FnOnce(&ProjectCx, &[(ContentId, Arc<ExtractOutput>, &'static Case)]) -> R,
) -> R {
    let corpus: Vec<(ContentId, Arc<ExtractOutput>, &'static Case)> = CASES
        .iter()
        .map(|case| {
            let out = dispatch(case.path, case.fixture, FamilyMask::ALL).expect("source");
            (content_id_of(case.fixture), out, case)
        })
        .collect();
    let pairs: Vec<(ContentId, &ExtractOutput)> = corpus
        .iter()
        .map(|(hash, out, _)| (hash.clone(), out.as_ref()))
        .collect();
    let file_set = FileSet;
    let manifest_map = ManifestMap;
    let cx = ProjectCx {
        files: &file_set,
        manifests: &manifest_map,
        reader: None,
        digest: ProjectDigest::default(),
        indexes: IndexBag::default(),
        own: std::cell::RefCell::new(None),
    };
    cx.indexes
        .def_index
        .set(build_def_index(&pairs))
        .expect("fresh OnceLock");
    f(&cx, &corpus)
}

/// The entity name at a candidate's owner span (the from-leg of the oracle's
/// text shape). A miss is a collection bug, rendered loud, not skipped.
fn owner_name(out: &ExtractOutput, span: Span) -> String {
    out.types
        .as_ref()
        .and_then(|types| types.nodes.iter().find(|node| node.span == span))
        .and_then(|node| node.name)
        .map(|id| out.strings.lookup(id).to_string())
        .unwrap_or_else(|| format!("<no entity at {}..{}>", span.start, span.end()))
}

/// TS type_edge PARITY (4b-iii): `Resolve<TypeF>` per ts case over the fixture
/// corpus, twin-normalized to the oracle's `type_edge\t{owner}\t{to}\t{kind}`
/// text shape, asserted equal to the oracle. The candidate row IS the parity
/// target (text dsts stay text); the resolve output pairs 1:1, in order, with
/// `TsSource::type_edge_candidates` (the zip discipline: edge i resolves
/// candidate i, so the candidate supplies the asserted text and the edge
/// proves the arm ran + carries the v6-only resolved leg). go has its own
/// twin below (4d-i-go); rust type_edge stays DEFERRED until 4d-rust.
#[test]
fn type_edge_resolve_parity_ts() {
    with_resolve_cx(|cx, corpus| {
        for (_blob, out, case) in corpus {
            if case.fixture_dir != "ts" {
                continue;
            }
            let edges = Resolve::<TypeF>::resolve(&TsSource, out, cx);
            let candidates = TsSource::type_edge_candidates(out);
            let mut v6: BTreeSet<String> = edges
                .iter()
                .zip(candidates.iter())
                .map(|(_edge, cand)| {
                    format!(
                        "type_edge\t{}\t{}\t{}",
                        owner_name(out, cand.owner),
                        out.strings.lookup(cand.to),
                        cand.kind.as_str()
                    )
                })
                .collect();
            if edges.len() != candidates.len() {
                v6.insert(format!(
                    "ZIP_MISMATCH edges={} candidates={}",
                    edges.len(),
                    candidates.len()
                ));
            }
            let v5: BTreeSet<String> = case
                .baseline
                .lines()
                .filter(|line| facet_of(line) == "type_edge")
                .map(str::to_owned)
                .collect();
            let only_v5: Vec<&String> = v5.difference(&v6).collect();
            let only_v6: Vec<&String> = v6.difference(&v5).collect();
            assert!(
                only_v5.is_empty() && only_v6.is_empty(),
                "[{}] type_edge parity diff vs v5 oracle:\n  missing from v6 ({}):\n{}\n  only in v6 ({}):\n{}",
                case.name,
                only_v5.len(),
                only_v5.iter().map(|s| format!("    {s}")).collect::<Vec<_>>().join("\n"),
                only_v6.len(),
                only_v6.iter().map(|s| format!("    {s}")).collect::<Vec<_>>().join("\n"),
            );
            eprintln!(
                "[{}] type_edge parity: {} rows compared, 0 divergence",
                case.name,
                v5.len()
            );
        }
    });
}

/// GO type_edge PARITY (4d-i-go): the exact twin of the ts test above, over
/// `GoSource`. go's v5 type_edge is shape-only (struct field / struct embed /
/// interface embed / generic constraint — NO sig-sourced param/returns), so
/// every candidate comes from `go_edge_candidates` on the type_spec walk.
/// sample/docs carry zero rows (asserted empty sets); go_edges carries the 7.
#[test]
fn type_edge_resolve_parity_go() {
    with_resolve_cx(|cx, corpus| {
        for (_blob, out, case) in corpus {
            if case.fixture_dir != "go" {
                continue;
            }
            let edges = Resolve::<TypeF>::resolve(&GoSource, out, cx);
            let candidates = GoSource::type_edge_candidates(out);
            let mut v6: BTreeSet<String> = edges
                .iter()
                .zip(candidates.iter())
                .map(|(_edge, cand)| {
                    format!(
                        "type_edge\t{}\t{}\t{}",
                        owner_name(out, cand.owner),
                        out.strings.lookup(cand.to),
                        cand.kind.as_str()
                    )
                })
                .collect();
            if edges.len() != candidates.len() {
                v6.insert(format!(
                    "ZIP_MISMATCH edges={} candidates={}",
                    edges.len(),
                    candidates.len()
                ));
            }
            let v5: BTreeSet<String> = case
                .baseline
                .lines()
                .filter(|line| facet_of(line) == "type_edge")
                .map(str::to_owned)
                .collect();
            let only_v5: Vec<&String> = v5.difference(&v6).collect();
            let only_v6: Vec<&String> = v6.difference(&v5).collect();
            assert!(
                only_v5.is_empty() && only_v6.is_empty(),
                "[{}] type_edge parity diff vs v5 oracle:\n  missing from v6 ({}):\n{}\n  only in v6 ({}):\n{}",
                case.name,
                only_v5.len(),
                only_v5.iter().map(|s| format!("    {s}")).collect::<Vec<_>>().join("\n"),
                only_v6.len(),
                only_v6.iter().map(|s| format!("    {s}")).collect::<Vec<_>>().join("\n"),
            );
            eprintln!(
                "[{}] type_edge parity: {} rows compared, 0 divergence",
                case.name,
                v5.len()
            );
        }
    });
}

/// Python type_edge PARITY: the go test above on the python cases, via
/// `PythonSource::type_edge_candidates` (impl/field/param/returns/uses).
#[test]
fn type_edge_resolve_parity_python() {
    with_resolve_cx(|cx, corpus| {
        for (_blob, out, case) in corpus {
            if case.fixture_dir != "python" {
                continue;
            }
            let edges = Resolve::<TypeF>::resolve(&PythonSource, out, cx);
            let candidates = PythonSource::type_edge_candidates(out);
            let mut v6: BTreeSet<String> = edges
                .iter()
                .zip(candidates.iter())
                .map(|(_edge, cand)| {
                    format!(
                        "type_edge\t{}\t{}\t{}",
                        owner_name(out, cand.owner),
                        out.strings.lookup(cand.to),
                        cand.kind.as_str()
                    )
                })
                .collect();
            if edges.len() != candidates.len() {
                v6.insert(format!(
                    "ZIP_MISMATCH edges={} candidates={}",
                    edges.len(),
                    candidates.len()
                ));
            }
            let v5: BTreeSet<String> = case
                .baseline
                .lines()
                .filter(|line| facet_of(line) == "type_edge")
                .map(str::to_owned)
                .collect();
            let only_v5: Vec<&String> = v5.difference(&v6).collect();
            let only_v6: Vec<&String> = v6.difference(&v5).collect();
            assert!(
                only_v5.is_empty() && only_v6.is_empty(),
                "[{}] type_edge parity diff vs v5 oracle:\n  missing from v6 ({}):\n{}\n  only in v6 ({}):\n{}",
                case.name,
                only_v5.len(),
                only_v5.iter().map(|s| format!("    {s}")).collect::<Vec<_>>().join("\n"),
                only_v6.len(),
                only_v6.iter().map(|s| format!("    {s}")).collect::<Vec<_>>().join("\n"),
            );
            eprintln!(
                "[{}] type_edge parity: {} rows compared, 0 divergence",
                case.name,
                v5.len()
            );
        }
    });
}

/// Rust type_edge PARITY (4d-i-rust): the ts test above, on the rust cases —
/// `Resolve<TypeF>` over the fixture corpus, twin-normalized to the oracle's
/// text shape via `RustSource::type_edge_candidates` (the same zip
/// discipline). v5 rust emits field/variant/generic/impl only (no
/// param/returns, no uses); the two fixtures exercise field + variant
/// (sample 3 / docs 3 rows).
#[test]
fn type_edge_resolve_parity_rust() {
    with_resolve_cx(|cx, corpus| {
        for (_blob, out, case) in corpus {
            if case.fixture_dir != "rust" {
                continue;
            }
            let edges = Resolve::<TypeF>::resolve(&RustSource, out, cx);
            let candidates = RustSource::type_edge_candidates(out);
            let mut v6: BTreeSet<String> = edges
                .iter()
                .zip(candidates.iter())
                .map(|(_edge, cand)| {
                    format!(
                        "type_edge\t{}\t{}\t{}",
                        owner_name(out, cand.owner),
                        out.strings.lookup(cand.to),
                        cand.kind.as_str()
                    )
                })
                .collect();
            if edges.len() != candidates.len() {
                v6.insert(format!(
                    "ZIP_MISMATCH edges={} candidates={}",
                    edges.len(),
                    candidates.len()
                ));
            }
            let v5: BTreeSet<String> = case
                .baseline
                .lines()
                .filter(|line| facet_of(line) == "type_edge")
                .map(str::to_owned)
                .collect();
            let only_v5: Vec<&String> = v5.difference(&v6).collect();
            let only_v6: Vec<&String> = v6.difference(&v5).collect();
            assert!(
                only_v5.is_empty() && only_v6.is_empty(),
                "[{}] type_edge parity diff vs v5 oracle:\n  missing from v6 ({}):\n{}\n  only in v6 ({}):\n{}",
                case.name,
                only_v5.len(),
                only_v5.iter().map(|s| format!("    {s}")).collect::<Vec<_>>().join("\n"),
                only_v6.len(),
                only_v6.iter().map(|s| format!("    {s}")).collect::<Vec<_>>().join("\n"),
            );
            eprintln!(
                "[{}] type_edge parity: {} rows compared, 0 divergence",
                case.name,
                v5.len()
            );
        }
    });
}

/// Whether a facet is asserted for a case: the phase-1 PORTED set everywhere,
/// plus type_edge for all three langs (phase-2: 4b-iii ts, 4d-i-go, 4d-i-rust).
fn is_asserted(case: &Case, facet: &str) -> bool {
    PORTED.contains(&facet)
        || (matches!(case.fixture_dir, "ts" | "go" | "rust" | "python") && facet == "type_edge")
}

/// The migration ledger: the measured v5-only deferred set + the v6-only CST /
/// specifier / resolved-type_edge-leg / call-edge counts, per fixture.
/// Informational (run with --nocapture). Not asserted.
#[test]
fn deferred_and_v6_only_ledger() {
    with_resolve_cx(|cx, corpus| {
        for (_blob, out, case) in corpus {
            let mut deferred: std::collections::BTreeMap<&str, usize> = Default::default();
            for line in case.baseline.lines() {
                let facet = facet_of(line);
                if !is_asserted(case, facet) {
                    *deferred.entry(facet).or_default() += 1;
                }
            }
            let facts = flatten(out);
            let cst_only = facts
                .iter()
                .filter(|f| {
                    matches!(
                        f,
                        FlatFact::Node {
                            family: FamilyTag::Cst,
                            ..
                        } | FlatFact::Edge {
                            family: FamilyTag::Cst,
                            ..
                        }
                    )
                })
                .count();
            let specifier_only = facts
                .iter()
                .filter(|f| matches!(f, FlatFact::Specifier { .. }))
                .count();
            // The genuinely-resolved span->blob type_edge legs: v6-only, never
            // asserted (the candidate row is the parity target).
            let resolved_legs = match case.fixture_dir {
                "ts" => Resolve::<TypeF>::resolve(&TsSource, out, cx)
                    .iter()
                    .filter(|edge| edge.dst_blob != ZERO_CONTENT_ID)
                    .count(),
                "go" => Resolve::<TypeF>::resolve(&GoSource, out, cx)
                    .iter()
                    .filter(|edge| edge.dst_blob != ZERO_CONTENT_ID)
                    .count(),
                "rust" => Resolve::<TypeF>::resolve(&RustSource, out, cx)
                    .iter()
                    .filter(|edge| edge.dst_blob != ZERO_CONTENT_ID)
                    .count(),
                _ => 0,
            };
            // The resolved call edges (4c-ii ts, 4d-ii-go go, 4d-ii-rust):
            // v6-only — v5's captured oracle has no call_edge facet, so scip
            // is their only oracle (the ratchets below). This corpus
            // has NO scip index loaded, so the counts are the pure name-match
            // leg (ScipOverride always 0 here). The legacy go fixtures are
            // covered HERE only: three package clauses in one dir is not a
            // go-indexable layout, so the go ratchet's universe is the scip/
            // module (see below).
            let (mut name_resolve, mut scip_override) = (0, 0);
            match case.fixture_dir {
                "ts" => {
                    for edge in Resolve::<sprefa_extract::CallF>::resolve(&TsSource, out, cx) {
                        match edge.kind {
                            CallEdgeKind::NameResolve => name_resolve += 1,
                            CallEdgeKind::ScipOverride => scip_override += 1,
                            // A value reference has no site, so no occurrence
                            // for the scip ratchet to compare against.
                            CallEdgeKind::ValueRef => {}
                            // The module plane binds a NAME, not an occurrence.
                            CallEdgeKind::ImportResolve => name_resolve += 1,
                            CallEdgeKind::Implements => {}
                        }
                    }
                }
                "go" => {
                    for edge in Resolve::<sprefa_extract::CallF>::resolve(&GoSource, out, cx) {
                        match edge.kind {
                            CallEdgeKind::NameResolve => name_resolve += 1,
                            CallEdgeKind::ScipOverride => scip_override += 1,
                            // A value reference has no site, so no occurrence
                            // for the scip ratchet to compare against.
                            CallEdgeKind::ValueRef => {}
                            // The module plane binds a NAME, not an occurrence.
                            CallEdgeKind::ImportResolve => name_resolve += 1,
                            CallEdgeKind::Implements => {}
                        }
                    }
                }
                "rust" => {
                    for edge in Resolve::<sprefa_extract::CallF>::resolve(&RustSource, out, cx) {
                        match edge.kind {
                            CallEdgeKind::NameResolve => name_resolve += 1,
                            CallEdgeKind::ScipOverride => scip_override += 1,
                            // A value reference has no site, so no occurrence
                            // for the scip ratchet to compare against.
                            CallEdgeKind::ValueRef => {}
                            // The module plane binds a NAME, not an occurrence.
                            CallEdgeKind::ImportResolve => name_resolve += 1,
                            CallEdgeKind::Implements => {}
                        }
                    }
                }
                _ => {}
            }
            eprintln!(
                "[{}] migration ledger: v5-only deferred {deferred:?}; v6-only cst facts {cst_only}; v6-only specifier facts {specifier_only}; v6-only resolved type_edge legs {resolved_legs}; v6-only call edges name_resolve {name_resolve} scip_override {scip_override} (no scip loaded)",
                case.name
            );
        }
    });
}

/// TS Resolve<CallF> RATCHET vs scip (4c-ii): occurrence/resolution parity
/// with scip as the oracle, NOT a raw symbol diff (the ORACLE entry: scip
/// is a flat exhaustive symbol table; v5/v6 model callable arrow-types in the
/// type graph and exclude value-consts, so the models differ by construction).
/// `ScipSource` runs over the ts fixture dir (the 4 parity fixtures + the
/// `scip/` trio that exercises the override); for every call SITE v6 emits,
/// scip's occurrence at that span is the compiler's word on the resolution.
///
/// THE EXACT ASSERTION (per file, per site s with callee name c):
///  1. OCCURRENCE PARITY (the subset leg): scip's document for the file
///     contains an occurrence inside s's span whose source text == c.
///     Asserted: 0 missing.
///  2. RESOLUTION PARITY: every v6 NameResolve edge whose site scip also
///     resolves to a corpus target T AGREES with T at (blob, def-name) — the
///     name-match binds the call facet (e.g. the ctor def), scip can name the
///     type facet (the class); one definition, two facet coordinates ("the
///     models differ by construction"). Asserted: 0 disagreements.
///  3. Every ScipOverride is a COUNTED, LISTED divergence: scip's corpus
///     target exists, the edge carries exactly it, and the name-match outcome
///     differs from it (else it would be no override). Asserted per edge.
///  4. NO SILENT MISS: a site scip resolves to a corpus target always has a
///     v6 edge. Asserted: 0 misses.
///  5. NO OVERBINDING: a NameResolve edge whose site scip resolves to an
///     external/none target is a v6 false binding. Asserted: 0 overbound.
///  6. Sites scip resolves externally (library symbols: Math.sqrt, Array
///     methods) get NO v6 edge — v6 models corpus call edges only. Counted,
///     not a divergence.
/// The arm's emitted edge multiset is additionally asserted equal to the
/// twin's per-site expected outcomes (orchestration check). The ratchet runs
/// the real indexer: a missing/failed scip-typescript is a loud failure here,
/// never a skipped green.
#[test]
fn call_resolve_scip_ratchet_ts() {
    let fixture_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ts");
    // Every .ts under the fixture root, recursively (the scip/ trio included).
    let mut rels: Vec<String> = Vec::new();
    let mut stack = vec![fixture_root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) == Some("ts") {
                rels.push(
                    path.strip_prefix(&fixture_root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    rels.sort();
    let reader = |p: &str| std::fs::read(fixture_root.join(p)).ok();
    let index_path = ScipTypescript
        .build(&fixture_root)
        .expect("scip-typescript build failed (the ratchet never fakes green)");
    let scip_index = ScipTypescript.load(&index_path).expect("scip load");
    let joined = join_documents(&scip_index, &reader);
    assert!(
        joined.iter().all(Option::is_some),
        "every scip document is reader-readable: the corpus and the index cover the same universe"
    );
    // The corpus: every fixture file dispatched + the DefIndex over all.
    let corpus: Vec<(String, ContentId, Arc<ExtractOutput>)> = rels
        .iter()
        .map(|rel| {
            let bytes = reader(rel).unwrap();
            (
                rel.clone(),
                content_id_of(&bytes),
                dispatch(rel, &bytes, FamilyMask::ALL).expect("a Source matches the fixture"),
            )
        })
        .collect();
    let pairs: Vec<(ContentId, &ExtractOutput)> = corpus
        .iter()
        .map(|(_, hash, out)| (hash.clone(), out.as_ref()))
        .collect();
    let file_set = FileSet;
    let manifest_map = ManifestMap;
    let cx = ProjectCx {
        files: &file_set,
        manifests: &manifest_map,
        reader: Some(&reader),
        digest: ProjectDigest::default(),
        indexes: IndexBag::default(),
        own: std::cell::RefCell::new(None),
    };
    cx.indexes
        .def_index
        .set(build_def_index(&pairs))
        .expect("fresh OnceLock");
    cx.indexes
        .scip_index
        .set(scip_index)
        .expect("fresh OnceLock");
    let scip_index = cx.indexes.scip_index.get().unwrap();
    let def_index = cx.indexes.def_index.get().unwrap();

    let mut total_sites = 0usize;
    let mut counts = RatchetCounts::default();
    let mut lines: Vec<String> = Vec::new();
    for (rel, _blob, out) in &corpus {
        let doc_ix = scip_index
            .documents
            .iter()
            .position(|d| &d.relative_path == rel)
            .expect("one scip document per fixture file");
        let doc = &scip_index.documents[doc_ix];
        let content = reader(rel).unwrap();
        let Some(call) = &out.call else { continue };
        let edges = Resolve::<sprefa_extract::CallF>::resolve(&TsSource, out, &cx);
        let mut actual: Vec<(u32, u32, u32, &'static str, ContentId)> = edges
            .iter()
            // The ratchet grades SITE outcomes against scip occurrences; a
            // value reference is not a site and has no occurrence.
            .filter(|edge| edge.kind != CallEdgeKind::ValueRef)
            .map(|edge| {
                let from = call.node(edge.src).span;
                (
                    from.start,
                    edge.dst_span.start,
                    edge.dst_span.end(),
                    edge.kind.as_str(),
                    edge.dst_blob.clone(),
                )
            })
            .collect();
        actual.sort_by_key(|t| (t.0, t.1, t.2, t.3));
        let mut expected: Vec<(u32, u32, u32, &'static str, ContentId)> = Vec::new();
        for site in &call.aux.sites {
            total_sites += 1;
            let callee = out.strings.lookup(site.callee);
            let line = line_of(&content, site.span.start);
            // scip's independent word on this site.
            let occ = site_occurrence(doc, &content, site.span, callee);
            if occ.is_none() {
                counts.missing_occurrence += 1;
                lines.push(format!("MISSING-OCCURRENCE {rel}:{line} {callee}"));
            }
            let scip_t = occ
                .and_then(|o| definition_of(scip_index, doc_ix, &o.symbol))
                .and_then(|(def_doc_ix, def_occ)| {
                    let def_doc = &scip_index.documents[def_doc_ix];
                    let (def_blob, def_content) = joined[def_doc_ix].as_ref().unwrap();
                    let ident = byte_range(def_content, def_occ.range, def_doc.position_encoding)?;
                    containing_def_site(def_index, def_blob.clone(), ident)
                        .map(|(name, s)| (def_blob.clone(), s.span, name))
                });
            let name_t = TsSource::call_name_match(out, def_index, callee);
            // The twin outcome (the same legs the arm runs; the multiset
            // comparison below is the orchestration check). Clones name_t/scip_t
            // into the closure so both stay owned for the scip-side match below.
            let twin = covering_def(call, site.span).and_then(|caller| {
                let (dst, kind) = match (name_t.clone(), scip_t.clone()) {
                    (Some(n), Some(s)) if n.0 == s.0 && callee == s.2 => {
                        (n, CallEdgeKind::NameResolve)
                    }
                    (_, Some(s)) => ((s.0, s.1), CallEdgeKind::ScipOverride),
                    (Some(n), None) => (n, CallEdgeKind::NameResolve),
                    (None, None) => return None,
                };
                Some((caller, dst, kind))
            });
            if let Some((caller, dst, kind)) = &twin {
                let from = call.node(*caller).span;
                expected.push((
                    from.start,
                    dst.1.start,
                    dst.1.end(),
                    kind.as_str(),
                    dst.0.clone(),
                ));
            }
            // The scip-side classification (assertions 2-6).
            match (twin, scip_t) {
                (Some((_, dst, CallEdgeKind::NameResolve)), Some(s)) => {
                    if !(dst.0 == s.0 && callee == s.2) {
                        counts.disagreements += 1;
                        lines.push(format!(
                            "DISAGREE {rel}:{line} {callee}: v6 NameResolve -> ({:?}, {callee}), scip -> ({:?}, {})",
                            short(&dst.0), short(&s.0), s.2
                        ));
                    } else {
                        counts.name_resolve += 1;
                    }
                }
                (Some((_, dst, CallEdgeKind::NameResolve)), None) => {
                    counts.overbound += 1;
                    lines.push(format!(
                        "OVERBOUND {rel}:{line} {callee}: v6 NameResolve -> ({:?}) but scip has no corpus target",
                        short(&dst.0)
                    ));
                }
                (Some((_, dst, CallEdgeKind::ScipOverride)), Some(s)) => {
                    assert_eq!(
                        (dst.0.clone(), dst.1),
                        (s.0.clone(), s.1),
                        "override edge carries scip's target at {rel}:{line} {callee}"
                    );
                    assert!(
                        !(name_t == Some((s.0.clone(), s.1)) && callee == s.2),
                        "override with a matching name-match is no override at {rel}:{line} {callee}"
                    );
                    counts.scip_override += 1;
                    lines.push(format!(
                        "OVERRIDE {rel}:{line} {callee}: name-match {} displaced; scip -> ({:?}, {})",
                        match name_t {
                            Some((b, _)) => format!("({:?}, {callee})", short(&b)),
                            None => "<none: ambiguous/absent>".to_string(),
                        },
                        short(&s.0),
                        s.2
                    ));
                }
                (Some((_, _, CallEdgeKind::ScipOverride)), None) => {
                    panic!("override without a scip corpus target at {rel}:{line} {callee}");
                }
                // A value reference is not a call site: no occurrence, so the
                // scip ratchet has nothing to classify it against.
                (Some((_, _, CallEdgeKind::ValueRef)), _) => {}
                // The twin re-derives the NAME-MATCH leg only, so it mints no
                // import_resolve edge for the ratchet to classify.
                (Some((_, _, CallEdgeKind::ImportResolve)), _) => {}
                (Some((_, _, CallEdgeKind::Implements)), _) => {}
                (None, Some(s)) => {
                    counts.misses += 1;
                    lines.push(format!(
                        "MISS {rel}:{line} {callee}: scip resolves to corpus ({:?}, {}) but v6 emitted no edge",
                        short(&s.0), s.2
                    ));
                }
                (None, None) => {
                    if occ.is_some() {
                        counts.external_no_edge += 1;
                    }
                }
            }
        }
        expected.sort_by_key(|t| (t.0, t.1, t.2, t.3));
        assert_eq!(
            actual, expected,
            "[{rel}] arm edges != twin expected outcomes"
        );
        eprintln!(
            "[{rel}] scip ratchet: sites {} | name_resolve {} scip_override {} external-no-edge {}",
            call.aux.sites.len(),
            counts.name_resolve,
            counts.scip_override,
            counts.external_no_edge
        );
    }
    eprintln!(
        "[ts-total] scip ratchet ({}) over {} sites: name_resolve {} scip_override {} external-no-edge {} | missing-occurrence {} disagreements {} misses {} overbound {}",
        scip_index.tool(), total_sites, counts.name_resolve, counts.scip_override,
        counts.external_no_edge, counts.missing_occurrence, counts.disagreements,
        counts.misses, counts.overbound
    );
    for line in &lines {
        eprintln!("  {line}");
    }
    assert_eq!(
        counts.missing_occurrence,
        0,
        "occurrence parity: every v6 site has a scip occurrence\n{}",
        lines.join("\n")
    );
    assert_eq!(
        counts.disagreements,
        0,
        "every NameResolve agrees with scip's corpus target\n{}",
        lines.join("\n")
    );
    assert_eq!(
        counts.misses,
        0,
        "no silent misses: every scip-corpus-resolved site has a v6 edge\n{}",
        lines.join("\n")
    );
    assert_eq!(
        counts.overbound,
        0,
        "no overbinding: every NameResolve is scip-corpus-resolved\n{}",
        lines.join("\n")
    );
}

/// GO Resolve<CallF> RATCHET vs scip-go (4d-ii-go): the 4c-ii ts ratchet's
/// exact shape, adapted honestly to scip-go's model. scip-go indexes GO
/// PACKAGES, and a package is one directory: the legacy top-level fixtures
/// (sample/docs/edges.go) carry THREE package clauses in one dir, which is
/// not a go-indexable layout by construction (`go list` rejects it) — so the
/// ratchet's universe is the self-contained module at tests/fixtures/go/scip
/// (go.mod `module example.com/fixture`, go 1.23; packages alpha + beta +
/// gamma; stdlib-only external refs, zero network). The legacy trio's sites
/// stay covered by the pure name-match leg (the ledger test above, no scip
/// loaded). THE FIXTURE (the override is honestly constructible):
///   alpha/alpha.go, beta/beta.go — two packages each exporting `func Helper`
///   gamma/gamma.go — imports alpha; `Run` calls `local()` (same-package),
///   `alpha.Helper()` (cross-package through the import), `strings.TrimSpace`
///   (stdlib, external).
/// Expected outcomes: NameResolve 1 (local), ScipOverride 1 (Helper: the
/// name-match sees alpha+beta and abstains; scip-go binds alpha.Helper
/// through the import), external-no-edge 1 (TrimSpace: scip's
/// `gomod github.com/golang/go/src` symbol has no definition in the corpus).
///
/// THE EXACT ASSERTION (per file, per site s with callee name c) — the same
/// six legs as ts:
///  1. OCCURRENCE PARITY (the subset leg): scip's document for the file
///     contains an occurrence inside s's span whose source text == c (for a
///     selector callee `pkg.F` the field occurrence inside the whole-selector
///     site span). Asserted: 0 missing.
///  2. RESOLUTION PARITY: every v6 NameResolve edge whose site scip also
///     resolves to a corpus target T AGREES with T at (blob, def-name).
///     Asserted: 0 disagreements.
///  3. Every ScipOverride is a COUNTED, LISTED divergence: scip's corpus
///     target exists, the edge carries exactly it, and the name-match outcome
///     differs from it (else it would be no override). Asserted per edge.
///  4. NO SILENT MISS: a site scip resolves to a corpus target always has a
///     v6 edge. Asserted: 0 misses.
///  5. NO OVERBINDING: a NameResolve edge whose site scip resolves to an
///     external/none target is a v6 false binding. Asserted: 0 overbound.
///  6. Sites scip resolves externally (stdlib symbols) get NO v6 edge — v6
///     models corpus call edges only. Counted, not a divergence.
/// The arm's emitted edge multiset is additionally asserted equal to the
/// twin's per-site expected outcomes (orchestration check). The ratchet runs
/// the real indexer (PATH scip-go, else the version-pinned `go run`
/// fallback): a missing/failed scip-go is a loud failure here, never a
/// skipped green.
#[test]
fn call_resolve_scip_ratchet_go() {
    let fixture_root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/go/scip");
    // Every .go under the module root, recursively (alpha/beta/gamma).
    let mut rels: Vec<String> = Vec::new();
    let mut stack = vec![fixture_root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) == Some("go") {
                rels.push(
                    path.strip_prefix(&fixture_root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    rels.sort();
    let reader = |p: &str| std::fs::read(fixture_root.join(p)).ok();
    let index_path = ScipGo
        .build(&fixture_root)
        .expect("scip-go build failed (the ratchet never fakes green)");
    let scip_index = ScipGo.load(&index_path).expect("scip load");
    let joined = join_documents(&scip_index, &reader);
    assert!(
        joined.iter().all(Option::is_some),
        "every scip document is reader-readable: the corpus and the index cover the same universe"
    );
    // The corpus: every module file dispatched + the DefIndex over all.
    let corpus: Vec<(String, ContentId, Arc<ExtractOutput>)> = rels
        .iter()
        .map(|rel| {
            let bytes = reader(rel).unwrap();
            (
                rel.clone(),
                content_id_of(&bytes),
                dispatch(rel, &bytes, FamilyMask::ALL).expect("a Source matches the fixture"),
            )
        })
        .collect();
    let pairs: Vec<(ContentId, &ExtractOutput)> = corpus
        .iter()
        .map(|(_, hash, out)| (hash.clone(), out.as_ref()))
        .collect();
    let file_set = FileSet;
    let manifest_map = ManifestMap;
    let cx = ProjectCx {
        files: &file_set,
        manifests: &manifest_map,
        reader: Some(&reader),
        digest: ProjectDigest::default(),
        indexes: IndexBag::default(),
        own: std::cell::RefCell::new(None),
    };
    cx.indexes
        .def_index
        .set(build_def_index(&pairs))
        .expect("fresh OnceLock");
    cx.indexes
        .scip_index
        .set(scip_index)
        .expect("fresh OnceLock");
    let scip_index = cx.indexes.scip_index.get().unwrap();
    let def_index = cx.indexes.def_index.get().unwrap();

    let mut total_sites = 0usize;
    let mut counts = RatchetCounts::default();
    let mut lines: Vec<String> = Vec::new();
    for (rel, _blob, out) in &corpus {
        let doc_ix = scip_index
            .documents
            .iter()
            .position(|d| &d.relative_path == rel)
            .expect("one scip document per module file");
        let doc = &scip_index.documents[doc_ix];
        let content = reader(rel).unwrap();
        let Some(call) = &out.call else { continue };
        let edges = Resolve::<sprefa_extract::CallF>::resolve(&GoSource, out, &cx);
        let mut actual: Vec<(u32, u32, u32, &'static str, ContentId)> = edges
            .iter()
            // The ratchet grades SITE outcomes against scip occurrences; a
            // value reference is not a site and has no occurrence.
            .filter(|edge| edge.kind != CallEdgeKind::ValueRef)
            .map(|edge| {
                let from = call.node(edge.src).span;
                (
                    from.start,
                    edge.dst_span.start,
                    edge.dst_span.end(),
                    edge.kind.as_str(),
                    edge.dst_blob.clone(),
                )
            })
            .collect();
        actual.sort_by_key(|t| (t.0, t.1, t.2, t.3));
        let mut expected: Vec<(u32, u32, u32, &'static str, ContentId)> = Vec::new();
        for site in &call.aux.sites {
            total_sites += 1;
            let callee = out.strings.lookup(site.callee);
            let line = line_of(&content, site.span.start);
            // scip's independent word on this site.
            let occ = site_occurrence(doc, &content, site.span, callee);
            if occ.is_none() {
                counts.missing_occurrence += 1;
                lines.push(format!("MISSING-OCCURRENCE {rel}:{line} {callee}"));
            }
            let scip_t = occ
                .and_then(|o| definition_of(scip_index, doc_ix, &o.symbol))
                .and_then(|(def_doc_ix, def_occ)| {
                    let def_doc = &scip_index.documents[def_doc_ix];
                    let (def_blob, def_content) = joined[def_doc_ix].as_ref().unwrap();
                    let ident = byte_range(def_content, def_occ.range, def_doc.position_encoding)?;
                    containing_def_site(def_index, def_blob.clone(), ident)
                        .map(|(name, s)| (def_blob.clone(), s.span, name))
                });
            let name_t = GoSource::call_name_match(out, def_index, callee);
            // The twin outcome (the same legs the arm runs; the multiset
            // comparison below is the orchestration check). Clones name_t/scip_t
            // into the closure so both stay owned for the scip-side match below.
            let twin = covering_def(call, site.span).and_then(|caller| {
                let (dst, kind) = match (name_t.clone(), scip_t.clone()) {
                    (Some(n), Some(s)) if n.0 == s.0 && callee == s.2 => {
                        (n, CallEdgeKind::NameResolve)
                    }
                    (_, Some(s)) => ((s.0, s.1), CallEdgeKind::ScipOverride),
                    (Some(n), None) => (n, CallEdgeKind::NameResolve),
                    (None, None) => return None,
                };
                Some((caller, dst, kind))
            });
            if let Some((caller, dst, kind)) = &twin {
                let from = call.node(*caller).span;
                expected.push((
                    from.start,
                    dst.1.start,
                    dst.1.end(),
                    kind.as_str(),
                    dst.0.clone(),
                ));
            }
            // The scip-side classification (assertions 2-6).
            match (twin, scip_t) {
                (Some((_, dst, CallEdgeKind::NameResolve)), Some(s)) => {
                    if !(dst.0 == s.0 && callee == s.2) {
                        counts.disagreements += 1;
                        lines.push(format!(
                            "DISAGREE {rel}:{line} {callee}: v6 NameResolve -> ({:?}, {callee}), scip -> ({:?}, {})",
                            short(&dst.0), short(&s.0), s.2
                        ));
                    } else {
                        counts.name_resolve += 1;
                    }
                }
                (Some((_, dst, CallEdgeKind::NameResolve)), None) => {
                    counts.overbound += 1;
                    lines.push(format!(
                        "OVERBOUND {rel}:{line} {callee}: v6 NameResolve -> ({:?}) but scip has no corpus target",
                        short(&dst.0)
                    ));
                }
                (Some((_, dst, CallEdgeKind::ScipOverride)), Some(s)) => {
                    assert_eq!(
                        (dst.0.clone(), dst.1),
                        (s.0.clone(), s.1),
                        "override edge carries scip's target at {rel}:{line} {callee}"
                    );
                    assert!(
                        !(name_t == Some((s.0.clone(), s.1)) && callee == s.2),
                        "override with a matching name-match is no override at {rel}:{line} {callee}"
                    );
                    counts.scip_override += 1;
                    lines.push(format!(
                        "OVERRIDE {rel}:{line} {callee}: name-match {} displaced; scip -> ({:?}, {})",
                        match name_t {
                            Some((b, _)) => format!("({:?}, {callee})", short(&b)),
                            None => "<none: ambiguous/absent>".to_string(),
                        },
                        short(&s.0),
                        s.2
                    ));
                }
                (Some((_, _, CallEdgeKind::ScipOverride)), None) => {
                    panic!("override without a scip corpus target at {rel}:{line} {callee}");
                }
                // A value reference is not a call site: no occurrence, so the
                // scip ratchet has nothing to classify it against.
                (Some((_, _, CallEdgeKind::ValueRef)), _) => {}
                // The twin re-derives the NAME-MATCH leg only, so it mints no
                // import_resolve edge for the ratchet to classify.
                (Some((_, _, CallEdgeKind::ImportResolve)), _) => {}
                (Some((_, _, CallEdgeKind::Implements)), _) => {}
                (None, Some(s)) => {
                    counts.misses += 1;
                    lines.push(format!(
                        "MISS {rel}:{line} {callee}: scip resolves to corpus ({:?}, {}) but v6 emitted no edge",
                        short(&s.0), s.2
                    ));
                }
                (None, None) => {
                    if occ.is_some() {
                        counts.external_no_edge += 1;
                    }
                }
            }
        }
        expected.sort_by_key(|t| (t.0, t.1, t.2, t.3));
        assert_eq!(
            actual, expected,
            "[{rel}] arm edges != twin expected outcomes"
        );
        eprintln!(
            "[{rel}] scip ratchet: sites {} | name_resolve {} scip_override {} external-no-edge {}",
            call.aux.sites.len(),
            counts.name_resolve,
            counts.scip_override,
            counts.external_no_edge
        );
    }
    eprintln!(
        "[go-total] scip ratchet ({}) over {} sites: name_resolve {} scip_override {} external-no-edge {} | missing-occurrence {} disagreements {} misses {} overbound {}",
        scip_index.tool(), total_sites, counts.name_resolve, counts.scip_override,
        counts.external_no_edge, counts.missing_occurrence, counts.disagreements,
        counts.misses, counts.overbound
    );
    for line in &lines {
        eprintln!("  {line}");
    }
    assert_eq!(
        counts.missing_occurrence,
        0,
        "occurrence parity: every v6 site has a scip occurrence\n{}",
        lines.join("\n")
    );
    assert_eq!(
        counts.disagreements,
        0,
        "every NameResolve agrees with scip's corpus target\n{}",
        lines.join("\n")
    );
    assert_eq!(
        counts.misses,
        0,
        "no silent misses: every scip-corpus-resolved site has a v6 edge\n{}",
        lines.join("\n")
    );
    assert_eq!(
        counts.overbound,
        0,
        "no overbinding: every NameResolve is scip-corpus-resolved\n{}",
        lines.join("\n")
    );
}

/// Rust Resolve<CallF> RATCHET vs scip (4d-ii): the ts ratchet's 6 legs on the
/// rust corpus, with rust-analyzer as the indexer. `ScipRust` runs over
/// tests/fixtures/rust (a Cargo project: Cargo.toml + lib.rs make every
/// fixture crate-reachable — rust-analyzer indexes crate-graph-reachable
/// files only, unlike scip-typescript); the `scip/` trio exercises the
/// override (same-name fns in two modules + a use, the ts trio's mirror).
///
/// THE SAME EXACT ASSERTION (per file, per site s with callee name c):
///  1. OCCURRENCE PARITY: scip's document contains an occurrence inside s's
///     span whose source text == c. Asserted: 0 missing.
///  2. RESOLUTION PARITY: every v6 NameResolve edge whose site scip also
///     resolves to a corpus target T AGREES with T at (blob, def-name).
///     Asserted: 0 disagreements.
///  3. Every ScipOverride is a COUNTED, LISTED divergence. Asserted per edge.
///  4. NO SILENT MISS: a site scip resolves to a corpus target always has a
///     v6 edge. Asserted: 0 misses.
///  5. NO OVERBINDING: a NameResolve edge whose site scip resolves to an
///     external/none target is a v6 false binding. Asserted: 0 overbound.
///  6. Sites scip resolves externally get NO v6 edge — counted.
/// RUST-ANALYZER ADAPTATION (leg 6's rust extension, the arm mirrors it): a
/// `local ` symbol at a call site is a LOCAL BINDING (`let func = |x| ..`;
/// `func(..)` in sample/docs) — df-owned, not a call-graph def. rust-analyzer
/// names no closure symbol, so scip's resolution for such a site is the
/// binding, and the 4c containing_def_site join would misroute it to the
/// ENCLOSING fn (a false self-edge). Local-symbol sites read as scip-external
/// and count HERE under external-no-edge — legs 1-5 are untouched. A
/// missing/failed rust-analyzer is a loud test failure, never a skipped
/// green.
#[test]
fn call_resolve_scip_ratchet_rust() {
    let fixture_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rust");
    // Every .rs under the fixture root, recursively (lib.rs + the scip/ trio
    // included — every one is crate-reachable, so every one gets a document).
    let mut rels: Vec<String> = Vec::new();
    let mut stack = vec![fixture_root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().unwrap() != "target" {
                    stack.push(path);
                }
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                rels.push(
                    path.strip_prefix(&fixture_root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    rels.sort();
    let reader = |p: &str| std::fs::read(fixture_root.join(p)).ok();
    let index_path = ScipRust
        .build(&fixture_root)
        .expect("rust-analyzer scip build failed (the ratchet never fakes green)");
    let scip_index = ScipRust.load(&index_path).expect("scip load");
    let joined = join_documents(&scip_index, &reader);
    assert!(
        joined.iter().all(Option::is_some),
        "every scip document is reader-readable: the corpus and the index cover the same universe"
    );
    // The corpus: every fixture file dispatched + the DefIndex over all.
    let corpus: Vec<(String, ContentId, Arc<ExtractOutput>)> = rels
        .iter()
        .map(|rel| {
            let bytes = reader(rel).unwrap();
            (
                rel.clone(),
                content_id_of(&bytes),
                dispatch(rel, &bytes, FamilyMask::ALL).expect("a Source matches the fixture"),
            )
        })
        .collect();
    let pairs: Vec<(ContentId, &ExtractOutput)> = corpus
        .iter()
        .map(|(_, hash, out)| (hash.clone(), out.as_ref()))
        .collect();
    let file_set = FileSet;
    let manifest_map = ManifestMap;
    let cx = ProjectCx {
        files: &file_set,
        manifests: &manifest_map,
        reader: Some(&reader),
        digest: ProjectDigest::default(),
        indexes: IndexBag::default(),
        own: std::cell::RefCell::new(None),
    };
    cx.indexes
        .def_index
        .set(build_def_index(&pairs))
        .expect("fresh OnceLock");
    cx.indexes
        .scip_index
        .set(scip_index)
        .expect("fresh OnceLock");
    let scip_index = cx.indexes.scip_index.get().unwrap();
    let def_index = cx.indexes.def_index.get().unwrap();

    let mut total_sites = 0usize;
    let mut counts = RatchetCounts::default();
    let mut lines: Vec<String> = Vec::new();
    for (rel, _blob, out) in &corpus {
        let doc_ix = scip_index
            .documents
            .iter()
            .position(|d| &d.relative_path == rel)
            .expect("one scip document per fixture file (every .rs is crate-reachable)");
        let doc = &scip_index.documents[doc_ix];
        let content = reader(rel).unwrap();
        let Some(call) = &out.call else { continue };
        let edges = Resolve::<sprefa_extract::CallF>::resolve(&RustSource, out, &cx);
        let mut actual: Vec<(u32, u32, u32, &'static str, ContentId)> = edges
            .iter()
            // The ratchet grades SITE outcomes against scip occurrences; a
            // value reference is not a site and has no occurrence.
            .filter(|edge| edge.kind != CallEdgeKind::ValueRef)
            .map(|edge| {
                let from = call.node(edge.src).span;
                (
                    from.start,
                    edge.dst_span.start,
                    edge.dst_span.end(),
                    edge.kind.as_str(),
                    edge.dst_blob.clone(),
                )
            })
            .collect();
        actual.sort_by_key(|t| (t.0, t.1, t.2, t.3));
        let mut expected: Vec<(u32, u32, u32, &'static str, ContentId)> = Vec::new();
        for site in &call.aux.sites {
            total_sites += 1;
            let callee = out.strings.lookup(site.callee);
            let line = line_of(&content, site.span.start);
            // scip's independent word on this site (the local guard is the
            // rust adaptation: a local binding is NOT a corpus call target).
            let occ = site_occurrence(doc, &content, site.span, callee);
            if occ.is_none() {
                counts.missing_occurrence += 1;
                lines.push(format!("MISSING-OCCURRENCE {rel}:{line} {callee}"));
            }
            let scip_t = occ
                .filter(|o| !o.symbol.starts_with("local "))
                .and_then(|o| definition_of(scip_index, doc_ix, &o.symbol))
                .and_then(|(def_doc_ix, def_occ)| {
                    let def_doc = &scip_index.documents[def_doc_ix];
                    let (def_blob, def_content) = joined[def_doc_ix].as_ref().unwrap();
                    let ident = byte_range(def_content, def_occ.range, def_doc.position_encoding)?;
                    containing_def_site(def_index, def_blob.clone(), ident)
                        .map(|(name, s)| (def_blob.clone(), s.span, name))
                });
            let name_t = RustSource::call_name_match(out, def_index, callee);
            // The twin outcome (the same legs the arm runs; the multiset
            // comparison below is the orchestration check). Clones name_t/scip_t
            // into the closure so both stay owned for the scip-side match below.
            let twin = covering_def(call, site.span).and_then(|caller| {
                let (dst, kind) = match (name_t.clone(), scip_t.clone()) {
                    (Some(n), Some(s)) if n.0 == s.0 && callee == s.2 => {
                        (n, CallEdgeKind::NameResolve)
                    }
                    (_, Some(s)) => ((s.0, s.1), CallEdgeKind::ScipOverride),
                    (Some(n), None) => (n, CallEdgeKind::NameResolve),
                    (None, None) => return None,
                };
                Some((caller, dst, kind))
            });
            if let Some((caller, dst, kind)) = &twin {
                let from = call.node(*caller).span;
                expected.push((
                    from.start,
                    dst.1.start,
                    dst.1.end(),
                    kind.as_str(),
                    dst.0.clone(),
                ));
            }
            // The scip-side classification (assertions 2-6).
            match (twin, scip_t) {
                (Some((_, dst, CallEdgeKind::NameResolve)), Some(s)) => {
                    if !(dst.0 == s.0 && callee == s.2) {
                        counts.disagreements += 1;
                        lines.push(format!(
                            "DISAGREE {rel}:{line} {callee}: v6 NameResolve -> ({:?}, {callee}), scip -> ({:?}, {})",
                            short(&dst.0), short(&s.0), s.2
                        ));
                    } else {
                        counts.name_resolve += 1;
                    }
                }
                (Some((_, dst, CallEdgeKind::NameResolve)), None) => {
                    counts.overbound += 1;
                    lines.push(format!(
                        "OVERBOUND {rel}:{line} {callee}: v6 NameResolve -> ({:?}) but scip has no corpus target",
                        short(&dst.0)
                    ));
                }
                (Some((_, dst, CallEdgeKind::ScipOverride)), Some(s)) => {
                    assert_eq!(
                        (dst.0.clone(), dst.1),
                        (s.0.clone(), s.1),
                        "override edge carries scip's target at {rel}:{line} {callee}"
                    );
                    assert!(
                        !(name_t == Some((s.0.clone(), s.1)) && callee == s.2),
                        "override with a matching name-match is no override at {rel}:{line} {callee}"
                    );
                    counts.scip_override += 1;
                    lines.push(format!(
                        "OVERRIDE {rel}:{line} {callee}: name-match {} displaced; scip -> ({:?}, {})",
                        match name_t {
                            Some((b, _)) => format!("({:?}, {callee})", short(&b)),
                            None => "<none: ambiguous/absent>".to_string(),
                        },
                        short(&s.0),
                        s.2
                    ));
                }
                (Some((_, _, CallEdgeKind::ScipOverride)), None) => {
                    panic!("override without a scip corpus target at {rel}:{line} {callee}");
                }
                // A value reference is not a call site: no occurrence, so the
                // scip ratchet has nothing to classify it against.
                (Some((_, _, CallEdgeKind::ValueRef)), _) => {}
                // The twin re-derives the NAME-MATCH leg only, so it mints no
                // import_resolve edge for the ratchet to classify.
                (Some((_, _, CallEdgeKind::ImportResolve)), _) => {}
                (Some((_, _, CallEdgeKind::Implements)), _) => {}
                (None, Some(s)) => {
                    counts.misses += 1;
                    lines.push(format!(
                        "MISS {rel}:{line} {callee}: scip resolves to corpus ({:?}, {}) but v6 emitted no edge",
                        short(&s.0), s.2
                    ));
                }
                (None, None) => {
                    if occ.is_some() {
                        counts.external_no_edge += 1;
                    }
                }
            }
        }
        expected.sort_by_key(|t| (t.0, t.1, t.2, t.3));
        assert_eq!(
            actual, expected,
            "[{rel}] arm edges != twin expected outcomes"
        );
        eprintln!(
            "[{rel}] scip ratchet: sites {} | name_resolve {} scip_override {} external-no-edge {}",
            call.aux.sites.len(),
            counts.name_resolve,
            counts.scip_override,
            counts.external_no_edge
        );
    }
    eprintln!(
        "[rust-total] scip ratchet ({}) over {} sites: name_resolve {} scip_override {} external-no-edge {} | missing-occurrence {} disagreements {} misses {} overbound {}",
        scip_index.tool(), total_sites, counts.name_resolve, counts.scip_override,
        counts.external_no_edge, counts.missing_occurrence, counts.disagreements,
        counts.misses, counts.overbound
    );
    for line in &lines {
        eprintln!("  {line}");
    }
    assert_eq!(
        counts.missing_occurrence,
        0,
        "occurrence parity: every v6 site has a scip occurrence\n{}",
        lines.join("\n")
    );
    assert_eq!(
        counts.disagreements,
        0,
        "every NameResolve agrees with scip's corpus target\n{}",
        lines.join("\n")
    );
    assert_eq!(
        counts.misses,
        0,
        "no silent misses: every scip-corpus-resolved site has a v6 edge\n{}",
        lines.join("\n")
    );
    assert_eq!(
        counts.overbound,
        0,
        "no overbinding: every NameResolve is scip-corpus-resolved\n{}",
        lines.join("\n")
    );
}

#[derive(Default)]
struct RatchetCounts {
    name_resolve: usize,
    scip_override: usize,
    external_no_edge: usize,
    missing_occurrence: usize,
    disagreements: usize,
    misses: usize,
    overbound: usize,
}

/// A short diagnostic label for a blob in divergence listings, never compared
/// against a golden fixture: `ContentId`'s Display (`git:`/`blake3:` prefixed)
/// truncated to a legible prefix.
fn short(blob: &ContentId) -> String {
    blob.to_string().chars().take(16).collect()
}

/// THE DOC FACET, RUST ONLY, ASSERTED BYTE-FOR-BYTE.
///
/// `PORTED` stays global across every case, so `"doc"` cannot join it until the
/// ts, go and kotlin walkers land. This is the same set-difference as
/// `ported_facets_match_v5`, narrowed to the one case and the one facet.
///
/// FAIL-FIRST: run before `doc_facts` existed, this reported 5 rows only in v5.
#[test]
fn rust_doc_parity() {
    let case = CASES
        .iter()
        .find(|case| case.name == "rust_docs")
        .expect("the rust_docs case");
    let v5: BTreeSet<&str> = case
        .baseline
        .lines()
        .filter(|line| facet_of(line) == "doc")
        .collect();
    assert!(
        !v5.is_empty(),
        "the oracle must carry doc rows to assert on"
    );

    let v6: BTreeSet<String> = v6_ported(case.path, case.fixture)
        .into_iter()
        .filter(|line| facet_of(line) == "doc")
        .collect();
    let v6_refs: BTreeSet<&str> = v6.iter().map(String::as_str).collect();

    let only_v5: Vec<&&str> = v5.difference(&v6_refs).collect();
    let only_v6: Vec<&&str> = v6_refs.difference(&v5).collect();
    assert!(
        only_v5.is_empty() && only_v6.is_empty(),
        "rust doc parity diff vs the v5 oracle:\n  only in v5 ({}): {only_v5:#?}\n  only in v6 ({}): {only_v6:#?}",
        only_v5.len(),
        only_v6.len()
    );
}
