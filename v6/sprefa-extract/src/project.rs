//! Phase 2 as a LIBRARY capability: the whole-project resolve recipe.
//!
//! Why this module exists at all. The recipe (dispatch every path -> one
//! `build_def_index` over all of them -> a borrowed `ProjectCx` -> `Resolve<F>`
//! per file) used to live inside `src/bin/extract.rs`. That placement is what
//! let the binary fall behind the library: the CLI reached exactly the arms its
//! own private adapter happened to dispatch (`CallF`, no SCIP), and nothing
//! asserted the difference. Moving the recipe here makes the parity structural
//! rather than aspirational: the binary now holds argument parsing and printing
//! and nothing else, so any capability the library gains is a flag away instead
//! of a reimplementation away.
//!
//! What crosses the seam is the same flat JSONL envelope as phase 1
//! (`crate::wire::FlatFact`), with flat top-level path and name fields on the
//! resolved arms. The v6 host decodes top-level keys, so nesting the target
//! coordinate would make the rows unusable.
//!
//! SCIP is optional and additive. With no index the call arm is pure name match
//! (the v5-shaped resolution); with an index plus a rev-correct reader the arms
//! take their SCIP leg and can emit `ScipOverride` rows. Both legs are the
//! language `Resolve` impls' own code, unchanged: this module only assembles the
//! context they read.

use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use rayon::prelude::*;

use crate::lang::go_modules::{GoModuleFacts, GoModuleIndex};
use crate::lang::kotlin_modules::{kt_module_facts, KtModuleFacts, KtModuleIndex};
use crate::lang::python::{py_module_facts, PyModuleFacts, PyModuleIndex};
use crate::lang::rust_modules::{RustModuleFacts, RustModuleIndex};
use crate::lang::ts_resolve::{ModuleFacts, TsModuleIndex};
use crate::lang::{
    source_for, DlSource, GoSource, KotlinSource, MarkdownSource, PrologSource, PythonSource,
    RustSource, TsSource,
};
use crate::rows::FamilyBundle;
use crate::scip::{ScipGo, ScipRust, ScipTypescript};
use crate::scip_ensure::IndexBudget;
use crate::scip_rows::ScipRecords;
use crate::seams::{
    build_def_index, BlobSource, FileSet, IndexBag, ManifestMap, ProjectCx, ProjectDigest,
};
use crate::shape::{content_id_of, ContentId, Span};
use crate::source::{ExtractOutput, FamilyMask, Resolve, Source};
use crate::tsi::types::{CoverageOut, Mode, RunOut, WitnessOut, PROTOCOL_VERSION};
use crate::types::{
    flow_edges, CallF, ProjectEdge, ResolutionOrigin, ScipError, ScipIndex, ScipSource, TypeF,
    UnresolvedReason,
};
use crate::wire::{flatten_flow, FlatFact};

/// Which phase-2 arms to run. All default off at the type level so a caller
/// states its intent; the CLI defaults `call` on for backward compatibility.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct ResolveArms {
    /// `Resolve<CallF>`: resolved caller-to-callee edges. Implemented for every
    /// source in the roster except the ast-grep CST fallback.
    pub call: bool,
    /// `Resolve<TypeF>`: resolved type reference edges. Implemented for TS, Go,
    /// Rust, dl6 and Kotlin; Prolog has no arm and is skipped, never dispatched.
    pub types: bool,
    /// `FlowF`: the inter-procedural value-flow join over resolved call edges.
    /// A pure join, so it needs the `call` resolve to have run and emits
    /// whole-project edges rather than per-file rows.
    pub flow: bool,
}

/// Where the Tier-1 SCIP index comes from, if anywhere.
#[derive(Copy, Clone, Debug, Default)]
pub enum ScipMode<'a> {
    /// No index. The resolve arms run their name-match leg only.
    #[default]
    Off,
    /// Load an index the caller already built.
    Load(&'a Path),
    /// Run the language's own indexer over `project_root` first, then load it.
    /// One index means one indexer, so every supplied path must be the same
    /// language.
    Build,
}

impl<'a> ScipMode<'a> {
    /// The one place a (index path, build flag) pair becomes a mode. An
    /// explicit index wins over the build flag.
    pub fn from_flags(index: Option<&'a Path>, build: bool) -> ScipMode<'a> {
        match index {
            Some(path) => ScipMode::Load(path),
            None if build => ScipMode::Build,
            None => ScipMode::Off,
        }
    }
}

/// One whole-project resolve. `paths` is the resolution universe: a name that
/// resolves outside it is simply not emitted.
pub struct ResolveRequest<'a> {
    pub paths: &'a [PathBuf],
    pub arms: ResolveArms,
    pub scip: ScipMode<'a>,
    /// The directory SCIP document paths are relative to, and the root the
    /// indexer runs over under `ScipMode::Build`. Required by both SCIP modes
    /// and ignored by `ScipMode::Off`; without it there is no rev-correct
    /// reader, and the resolve arms' SCIP leg needs one to join documents to
    /// content.
    pub project_root: Option<&'a Path>,
    /// Which SCIP record kinds `scip_facts` produces. Full passthrough by
    /// default; narrowing is the demand-side lever for its measured cost.
    pub scip_records: ScipRecords,
    /// Whether `scip_occurrence` rows also carry the source slice at their
    /// span. Off by default so a plain `--scip-facts` run stays byte-identical.
    pub occurrence_text: bool,
    /// The cargo workspace root the rust CHECKER tier loads. Its own field
    /// because `project_root` also adopts a fresh SCIP index by freshness.
    pub rust_checker: Option<&'a Path>,
    /// The project root the ts CHECKER tier loads a `ts.Program` over. Its
    /// own field for the same reason `rust_checker` has one.
    pub ts_checker: Option<&'a Path>,
    /// Wrap the answer in the TSI envelope: protocol, one run per tier that
    /// ran, `fact` ordinals, one witness per leg, partial coverage per family.
    pub witness: bool,
}

/// Why a project resolve could not run. Distinct from a resolve that ran and
/// found nothing, which is an empty fact list and a success.
#[derive(Debug)]
pub enum ProjectError {
    Read(PathBuf, std::io::Error),
    Scip(ScipError),
    /// A SCIP mode was requested without `project_root`.
    ScipNeedsRoot,
    /// `ScipMode::Build` over paths spanning more than one language, or a
    /// language with no indexer in the roster.
    ScipIndexerUnavailable(String),
    /// Diet module resolution was requested without `project_root`.
    DepsNeedRoot,
    /// A supplied path does not sit under `project_root`, so it has no
    /// project-relative name and cannot join a module graph.
    DepsPathOutsideRoot(PathBuf),
    /// Manifest edges were requested without `project_root`.
    ManifestsNeedRoot,
}

impl std::fmt::Display for ProjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read(path, err) => write!(f, "read {}: {err}", path.display()),
            Self::Scip(err) => write!(f, "scip: {err:?}"),
            Self::ScipNeedsRoot => {
                write!(f, "a scip mode needs --project-root: scip document paths are project-relative and the resolve arms need a reader to join them to content")
            }
            Self::ScipIndexerUnavailable(detail) => write!(f, "no scip indexer: {detail}"),
            Self::DepsNeedRoot => write!(
                f,
                "diet module resolution needs --project-root: a module graph's node names are project-relative paths"
            ),
            Self::DepsPathOutsideRoot(path) => write!(
                f,
                "{} is outside --project-root, so it has no project-relative name",
                path.display()
            ),
            Self::ManifestsNeedRoot => write!(
                f,
                "package edges need --project-root: a package graph's node names are project-relative manifest paths"
            ),
        }
    }
}

impl std::error::Error for ProjectError {}

/// Why a requested checker tier answered nothing. Off `--witness` the string is
/// built and dropped, which is cheaper than a second code path.
struct TierDecline {
    tool: &'static str,
    detail: String,
}

/// One numbered row's legs, and the language whose checker tier can own the
/// semantic run a `Checker` leg is filed under.
struct RowLegs {
    legs: Vec<ResolutionOrigin>,
    lang: &'static str,
}

/// The legs behind every numbered row, in the order the rows were produced.
/// Off `--witness` nothing is collected and nothing allocates.
#[derive(Default)]
struct LegTrail {
    on: bool,
    lang: &'static str,
    rows: Vec<RowLegs>,
}

impl LegTrail {
    /// Every row pushed from here on belongs to this language.
    fn at(&mut self, lang: &'static str) {
        self.lang = lang;
    }

    fn push<F: crate::family::Family>(&mut self, edge: &ProjectEdge<F>) {
        if self.on {
            self.rows.push(RowLegs {
                legs: edge.legs(),
                lang: self.lang,
            });
        }
    }
}

/// One supplied file, extracted once, kept for the whole resolve.
pub(crate) struct ProjectInput {
    pub(crate) path: String,
    blob: ContentId,
    pub(crate) output: Arc<ExtractOutput>,
    /// This file's module facts, built while its bytes are in hand so the
    /// plane costs no second read. `None` outside a module-plane run.
    module: Option<ModuleFacts>,
    /// The rust module plane's own facts, same discipline as `module`.
    rust_module: Option<RustModuleFacts>,
    /// The go module plane's own facts, same discipline as `module`.
    go_module: Option<GoModuleFacts>,
    /// The python module plane's own facts, same discipline as `module`.
    py_module: Option<PyModuleFacts>,
    /// The kotlin module plane's own facts, same discipline as `module`.
    kt_module: Option<KtModuleFacts>,
}

/// Run the requested arms over the whole supplied file set and return the flat
/// facts, sorted by their serialized form so callers get a byte-stable stream.
pub fn resolve_project(request: &ResolveRequest) -> Result<Vec<FlatFact>, ProjectError> {
    let inputs = read_inputs_with_modules(request.paths)?;
    let scip_index = load_scip(request, &inputs)?;

    let pairs: Vec<(ContentId, &ExtractOutput)> = inputs
        .iter()
        .map(|input| (input.blob.clone(), input.output.as_ref()))
        .collect();
    let files = FileSet;
    let manifests = ManifestMap;
    // The reader is what makes a SCIP index usable: `join_documents` maps every
    // project-relative document path to its bytes, and the arms find their own
    // document by content hash off that join. It reads exactly the index's
    // documents, not the whole repository.
    let blobs: Option<SourceTreeBlobSource> = match (request.project_root, scip_index.as_ref()) {
        (Some(root), Some(index)) => {
            let files: Vec<&str> = index
                .documents
                .iter()
                .map(|doc| doc.relative_path.as_str())
                .collect();
            Some(
                SourceTreeBlobSource::open_files(root, &files).map_err(|err| {
                    ProjectError::Read(
                        root.to_path_buf(),
                        std::io::Error::new(std::io::ErrorKind::Other, err),
                    )
                })?,
            )
        }
        _ => None,
    };
    let reader = move |relative: &str| -> Option<Vec<u8>> { blobs.as_ref()?.blob(relative) };
    let cx = ProjectCx {
        files: &files,
        manifests: &manifests,
        reader: scip_index.is_some().then_some(&reader),
        digest: ProjectDigest::default(),
        indexes: IndexBag::default(),
        witness: request.witness,
    };
    cx.indexes
        .def_index
        .set(build_def_index(&pairs))
        .expect("fresh project definition index");
    cx.indexes
        .kinds
        .set(crate::types::build_kind_index(&pairs))
        .expect("fresh project kind index");
    cx.indexes
        .paths
        .set(crate::types::build_path_index(
            inputs
                .iter()
                .map(|input| (input.blob.clone(), input.path.as_str())),
        ))
        .expect("fresh project path index");
    if let Some(index) = scip_index {
        cx.indexes
            .scip_index
            .set(index)
            .expect("fresh project scip index");
    }
    // The module plane reads the def index (an export's identifier span joins
    // to the def node containing it), so it is built after it, never beside it.
    let corpus: Vec<(String, ContentId)> = inputs
        .iter()
        .map(|input| (input.path.clone(), input.blob.clone()))
        .collect();
    let module_files: Vec<(String, ModuleFacts)> = inputs
        .iter()
        .filter_map(|input| Some((input.path.clone(), input.module.clone()?)))
        .collect();
    cx.indexes
        .ts_modules
        .set(TsModuleIndex::build(
            module_files,
            &corpus,
            cx.indexes.def_index.get().expect("the def index is set"),
        ))
        .ok()
        .expect("fresh project module plane");
    let rust_module_files: Vec<(String, RustModuleFacts)> = inputs
        .iter()
        .filter_map(|input| Some((input.path.clone(), input.rust_module.clone()?)))
        .collect();
    cx.indexes
        .rust_modules
        .set(RustModuleIndex::build(
            rust_module_files,
            &corpus,
            cx.indexes.def_index.get().expect("the def index is set"),
        ))
        .ok()
        .expect("fresh project module plane (rust)");
    let go_module_files: Vec<(String, GoModuleFacts)> = inputs
        .iter()
        .filter_map(|input| Some((input.path.clone(), input.go_module.clone()?)))
        .collect();
    // The go resolve arms read their per-file facts from this publish step
    // (computed in the module plane's shared parse); the parse fallback in
    // go.rs only serves library/test paths with no module plane.
    for input in inputs.iter() {
        if let Some(facts) = input.go_module.as_ref().and_then(GoModuleFacts::file_facts) {
            crate::lang::go::go_publish_file_facts(&input.path, Some(&input.blob), facts.clone());
        }
    }
    cx.indexes
        .go_modules
        .set(GoModuleIndex::build(go_module_files))
        .ok()
        .expect("fresh project module plane (go)");
    let py_module_files: Vec<(String, PyModuleFacts)> = inputs
        .iter()
        .filter_map(|input| Some((input.path.clone(), input.py_module.clone()?)))
        .collect();
    cx.indexes
        .py_modules
        .set(PyModuleIndex::build(py_module_files))
        .ok()
        .expect("fresh project module plane (python)");
    let kt_module_files: Vec<(String, KtModuleFacts)> = inputs
        .iter()
        .filter_map(|input| Some((input.path.clone(), input.kt_module.clone()?)))
        .collect();
    cx.indexes
        .kt_modules
        .set(KtModuleIndex::build(kt_module_files))
        .ok()
        .expect("fresh project module plane (kotlin)");

    let mut declines: Vec<TierDecline> = Vec::new();
    if let Some(checker_root) = request.rust_checker {
        match load_rust_checker(checker_root, &inputs, &corpus, &cx) {
            Ok(index) => cx
                .indexes
                .rust_checker
                .set(index)
                .ok()
                .expect("fresh project checker tier (rust)"),
            Err(detail) => declines.push(TierDecline {
                tool: "rust-analyzer",
                detail,
            }),
        }
    }

    if let Some(checker_root) = request.ts_checker {
        match load_ts_checker(checker_root, &inputs, &corpus, &cx) {
            Ok(index) => cx
                .indexes
                .ts_checker
                .set(index)
                .ok()
                .expect("fresh project checker tier (ts)"),
            Err(detail) => declines.push(TierDecline {
                tool: "tsc",
                detail,
            }),
        }
    }

    // One resolve per input, shared by the `call` arm and the `flow` join: the
    // N+1 law applied to work rather than to rows.
    let mut resolved_calls: Vec<(ContentId, Vec<ProjectEdge<CallF>>)> =
        if request.arms.call || request.arms.flow {
            use rayon::prelude::*;
            EXTRACT_POOL.install(|| {
                inputs
                    .par_iter()
                    .map(|input| {
                        // The per-file identity the resolve seam carries: each
                        // arm reads its own blob off the thread-local pin
                        // instead of guessing it from span matches (a wrong
                        // guess when two files share a named span).
                        crate::types::set_own(Some(input.blob.clone()));
                        let edges = resolve_call_edges(&input.path, &input.output, &cx);
                        crate::types::set_own(None);
                        (input.blob.clone(), edges)
                    })
                    .collect()
            })
        } else {
            Vec::new()
        };

    // The scip macro post-pass: call edges for calls written inside macro
    // invocations, which the per-file resolve never sees (the parse mints no
    // site there). No scip index -> a no-op that emits nothing. The rows land
    // per FILE, inside that file's block of the stream, so a row-line consumer
    // can attribute each one to the caller_path the block's edges carry.
    let mut macro_rows: Vec<Vec<crate::lang::rust_scip_macros::MacroSiteRow>> = Vec::new();
    if request.arms.call {
        let macro_files: Vec<crate::lang::rust_scip_macros::ScipMacroFile> = inputs
            .iter()
            .map(|input| crate::lang::rust_scip_macros::ScipMacroFile {
                path: &input.path,
                blob: &input.blob,
                output: input.output.as_ref(),
            })
            .collect();
        macro_rows =
            crate::lang::rust_scip_macros::mint_macro_edges(&macro_files, &cx, &mut resolved_calls);
    }

    let mut facts = Vec::new();
    let mut trail = LegTrail {
        on: request.witness,
        ..LegTrail::default()
    };
    let targets = TargetIndex::build(&inputs);
    if request.arms.call {
        for ((input, (_, edges)), rows) in inputs
            .iter()
            .zip(resolved_calls.iter())
            .zip(macro_rows.into_iter().chain(std::iter::repeat(Vec::new())))
        {
            crate::types::set_own(Some(input.blob.clone()));
            trail.at(arm_for(&input.path).map_or("", |arm| arm.name));
            facts.extend(call_facts(input, &targets, edges, &mut trail));
            facts.extend(call_drop_facts(input, &cx, edges));
            for row in rows {
                facts.push(FlatFact::MacroSiteOut {
                    family: crate::shape::FamilyTag::Call,
                    span: crate::wire::SpanOut::new(row.span.start, row.span.end()),
                    macro_name: row.macro_name,
                    source: row.source.to_string(),
                });
            }
        }
    }

    if request.arms.call || request.arms.types {
        for input in &inputs {
            crate::types::set_own(Some(input.blob.clone()));
            facts.extend(import_facts(input, &cx));
        }
    }
    for input in &inputs {
        if request.arms.types {
            crate::types::set_own(Some(input.blob.clone()));
            trail.at(arm_for(&input.path).map_or("", |arm| arm.name));
            facts.extend(type_facts(input, &targets, &cx, &mut trail));
        }
    }
    if request.arms.flow {
        facts.extend(flatten_flow(&flow_edges(&pairs, &resolved_calls)));
    }
    if request.witness {
        let syntax_tsi = syntax_tsi_rows(request, &inputs, &cx);
        let relations = tsi_relations(&syntax_tsi.rows);
        facts.extend(syntax_tsi.rows.into_iter().map(FlatFact::Fact));
        return Ok(envelope(Envelope {
            facts,
            trail,
            inputs: &inputs,
            cx: &cx,
            declines,
            tsi_relations: relations,
            tsi_ids: syntax_tsi.next_id,
        }));
    }
    Ok(facts)
}

/// The syntax tier's TSI rows for one resolve, and the first id free after them.
struct SyntaxTsi {
    rows: Vec<crate::tsi::FactOut>,
    next_id: u32,
}

/// Rides the stream for every language whose checker tier did not answer:
/// beside a loaded tier the two id spaces name two types with one number.
fn syntax_tsi_rows(
    request: &ResolveRequest,
    inputs: &[ProjectInput],
    cx: &ProjectCx,
) -> SyntaxTsi {
    let mut out = SyntaxTsi {
        rows: Vec::new(),
        next_id: 0,
    };
    if !request.arms.types {
        return out;
    }
    for input in inputs {
        let answered = match arm_for(&input.path).map_or("", |arm| arm.name) {
            "ts" => cx.indexes.ts_checker.get().is_some(),
            "rust" => cx.indexes.rust_checker.get().is_some(),
            _ => false,
        };
        if answered {
            continue;
        }
        let Some(bundle) = input.output.types.as_ref() else {
            continue;
        };
        let (rows, next) = crate::wire::tsi_rows_rebased(
            &bundle.aux.tsi,
            &input.blob.to_string(),
            out.next_id,
        );
        out.rows.extend(rows);
        out.next_id = next;
    }
    out
}

/// Every relation the syntax tier's rows name, in walk order: the coverage rows
/// the envelope files beside them.
fn tsi_relations(rows: &[crate::tsi::FactOut]) -> Vec<String> {
    let mut named: Vec<String> = Vec::new();
    for row in rows {
        if !named.contains(&row.relation) {
            named.push(row.relation.clone());
        }
    }
    named
}

/// The syntax run is always 0; a checker tier that LOADED takes the next id, so
/// a stream carries a semantic run only where one actually answered.
fn semantic_runs(cx: &ProjectCx, inputs: &[ProjectInput]) -> Vec<(&'static str, RunOut)> {
    let scope: Vec<String> = inputs.iter().map(|input| input.blob.to_string()).collect();
    let tiers = [
        ("ts", cx.indexes.ts_checker.get().is_some(), "tsc"),
        (
            "rust",
            cx.indexes.rust_checker.get().is_some(),
            "rust-analyzer",
        ),
    ];
    tiers
        .into_iter()
        .filter(|(_, loaded, _)| *loaded)
        .enumerate()
        .map(|(rank, (lang, _, tool))| {
            (
                lang,
                RunOut {
                    run: rank as u32 + 1,
                    mode: Mode::Semantic,
                    tool: tool.to_string(),
                    // The tier reports no version of its own, and a run row
                    // that borrows this crate's would name the wrong compiler.
                    version: String::new(),
                    scope: scope.clone(),
                },
            )
        })
        .collect()
}

/// Everything the envelope files beside the rows it numbers.
struct Envelope<'a> {
    facts: Vec<FlatFact>,
    trail: LegTrail,
    inputs: &'a [ProjectInput],
    cx: &'a ProjectCx<'a>,
    declines: Vec<TierDecline>,
    tsi_relations: Vec<String>,
    tsi_ids: u32,
}

/// The TSI envelope over a resolve: protocol, one run per tier that ran, a
/// `fact` ordinal on every resolved row, one witness per leg, coverage.
fn envelope(input: Envelope) -> Vec<FlatFact> {
    const SYNTAX_RUN: u32 = 0;
    let Envelope {
        facts,
        trail,
        inputs,
        cx,
        declines,
        tsi_relations,
        tsi_ids,
    } = input;
    let semantic = semantic_runs(cx, inputs);
    let mut rows: Vec<FlatFact> = vec![
        FlatFact::Protocol {
            version: PROTOCOL_VERSION,
        },
        FlatFact::Run(RunOut {
            run: SYNTAX_RUN,
            mode: Mode::Syntax,
            tool: "extract".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            scope: inputs.iter().map(|input| input.blob.to_string()).collect(),
        }),
    ];
    rows.extend(semantic.iter().map(|(_, run)| FlatFact::Run(run.clone())));
    let mut witnesses: Vec<WitnessOut> = Vec::new();
    let mut numbered = 0u32;
    let mut trail = trail.rows.into_iter();
    for mut fact in facts {
        // A TSI `fact` row numbers itself in a required field and answers no
        // resolve site, so it takes the counter without consuming a leg.
        if let FlatFact::Fact(row) = &mut fact {
            numbered += 1;
            row.fact = numbered;
            witnesses.push(WitnessOut {
                fact: numbered,
                run: SYNTAX_RUN,
                method: crate::tsi::Method::Parse,
            });
            rows.push(fact);
            continue;
        }
        let ordinal = fact.fact_slot().map(|slot| {
            numbered += 1;
            *slot = Some(numbered);
            numbered
        });
        if let Some(numbered) = ordinal {
            if let Some(row) = trail.next() {
                let mut legs = row.legs;
                legs.sort();
                witnesses.extend(legs.into_iter().map(|leg| WitnessOut {
                    fact: numbered,
                    // A checker leg is the semantic run's answer; every other
                    // leg is the parse's.
                    run: match leg {
                        ResolutionOrigin::Checker => semantic
                            .iter()
                            .find(|(lang, _)| *lang == row.lang)
                            .map_or(SYNTAX_RUN, |(_, run)| run.run),
                        _ => SYNTAX_RUN,
                    },
                    method: leg.method(),
                }));
            }
        }
        rows.push(fact);
    }
    rows.extend(witnesses.into_iter().map(FlatFact::Witness));
    // The tsc walk enumerates relations rather than answering sites, so its rows
    // arrive whole and take ordinals after the resolve's.
    let mut ids = tsi_ids;
    if let Some((_, run)) = semantic.iter().find(|(lang, _)| *lang == "ts") {
        if let Some(index) = cx.indexes.ts_checker.get() {
            ids = crate::tsi::emit_semantic(run.run, index, ids, &mut rows);
        }
    }
    if let Some((_, run)) = semantic.iter().find(|(lang, _)| *lang == "rust") {
        if let Some(index) = cx.indexes.rust_checker.get() {
            crate::tsi::emit_semantic(run.run, index, ids, &mut rows);
            // The tier LOADED and enumerated nothing in this file, so the row is
            // the semantic run's news rather than a decline on the syntax run.
            for path in &index.unmodulated {
                rows.push(FlatFact::Diagnostic(crate::tsi::DiagnosticOut {
                    run: run.run,
                    relation: "tier.rust-analyzer".to_string(),
                    detail: format!(
                        "{path}: owns no module in the loaded crate graph (cfg-gated, or outside every crate root)"
                    ),
                }));
            }
        }
    }
    // A resolve enumerates no relation exhaustively, so both families are
    // partial; a checker WALK is the only leg that claims complete.
    for relation in ["extract.call", "extract.type"]
        .into_iter()
        .map(str::to_string)
        .chain(tsi_relations)
    {
        rows.push(FlatFact::Coverage(CoverageOut {
            run: SYNTAX_RUN,
            relation,
            complete: false,
        }));
    }
    // A tier that was asked and answered nothing says so in the stream: off
    // `--witness` the reason reaches a `tracing` line and nowhere else.
    for decline in declines {
        rows.push(FlatFact::Diagnostic(crate::tsi::DiagnosticOut {
            run: SYNTAX_RUN,
            relation: format!("tier.{}", decline.tool),
            detail: decline.detail,
        }));
    }
    rows
}

/// The workspace load is an index-build-class cost, so it carries the SCIP
/// exception to the 10-second law rather than the per-run ceiling.
const CHECKER_BUDGET: std::time::Duration = std::time::Duration::from_secs(900);

/// Every failure returns its reason: the syntax leg answers every site as
/// before, and under `--witness` the reason is a `diagnostic` record.
fn load_rust_checker(
    root: &Path,
    inputs: &[ProjectInput],
    corpus: &[(String, ContentId)],
    cx: &ProjectCx,
) -> Result<crate::lang::rust_checker::RustCheckerIndex, String> {
    // A relative root reaches rust-analyzer as a relative `AbsPathBuf` and its
    // workspace discovery then finds only part of the crate graph.
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let files: Vec<(String, PathBuf)> = inputs
        .iter()
        .filter(|input| input.path.ends_with(".rs"))
        .map(|input| {
            let absolute = std::fs::canonicalize(&input.path)
                .unwrap_or_else(|_| PathBuf::from(&input.path));
            (input.path.clone(), absolute)
        })
        .collect();
    let answers = match crate::lang::rust_checker::answer(&root, &files, CHECKER_BUDGET, cx.witness)
    {
        Ok(answers) => answers,
        Err(err) => {
            tracing::warn!("rust checker tier off, syntax tier answers alone: {err}");
            return Err(err.to_string());
        }
    };
    let index = crate::lang::rust_checker::RustCheckerIndex::build(
        answers,
        corpus,
        cx.indexes.def_index.get().expect("the def index is set"),
    );
    if index.files_answered == 0 {
        let detail = format!(
            "loaded a workspace containing NONE of the supplied files ({} unjoined) under {}; is --project-root the right Cargo workspace?",
            index.unjoined,
            root.display()
        );
        tracing::warn!(
            root = %root.display(),
            unjoined = index.unjoined,
            "rust checker tier loaded a workspace containing NONE of the supplied files; every answer falls to syntax — is --project-root the right Cargo workspace?"
        );
        return Err(detail);
    }
    tracing::info!(
        load_ms = index.load.as_millis() as u64,
        walk_ms = index.walk.as_millis() as u64,
        files = index.files_answered,
        unjoined = index.unjoined,
        external = index.external,
        type_ambiguous = index.type_ambiguous,
        method_sites = index.method_sites,
        method_unresolved = index.method_unresolved,
        "rust checker tier loaded"
    );
    if !index.unmodulated.is_empty() {
        tracing::warn!(
            files = ?index.unmodulated,
            "the loaded crate graph declares no module for these supplied files; they are cfg-gated out or outside every crate root"
        );
    }
    Ok(index)
}

/// The ts twin of `load_rust_checker`, same decline discipline.
fn load_ts_checker(
    root: &Path,
    inputs: &[ProjectInput],
    corpus: &[(String, ContentId)],
    cx: &ProjectCx,
) -> Result<crate::lang::ts_checker::TsCheckerIndex, String> {
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let files: Vec<(String, PathBuf)> = inputs
        .iter()
        .filter(|input| {
            let path = input.path.as_str();
            [".ts", ".tsx", ".mts", ".cts"]
                .iter()
                .any(|suffix| path.ends_with(suffix))
        })
        .map(|input| {
            let absolute =
                std::fs::canonicalize(&input.path).unwrap_or_else(|_| PathBuf::from(&input.path));
            (input.path.clone(), absolute)
        })
        .collect();
    if files.is_empty() {
        return Err("no .ts, .tsx, .mts or .cts path in the supplied file set".to_string());
    }
    let answers = match crate::lang::ts_checker::answer(&root, &files, cx.witness) {
        Ok(answers) => answers,
        Err(err) => {
            tracing::info!("ts checker tier off: {err}");
            return Err(err.to_string());
        }
    };
    let index = crate::lang::ts_checker::TsCheckerIndex::build(
        answers,
        corpus,
        cx.indexes.def_index.get().expect("the def index is set"),
    );
    tracing::info!(
        load_ms = index.load.as_millis() as u64,
        walk_ms = index.walk.as_millis() as u64,
        files = index.files_answered,
        unjoined = index.unjoined,
        external = index.external,
        "ts checker tier loaded"
    );
    Ok(index)
}

/// Load the SCIP index the request names and flatten it to raw index facts:
/// occurrences, symbol information, relationships. No resolve arm runs.
///
/// This is the whole v5 `scip_*` relation family's input in one call. Every one
/// of those ten relations is a filter or a join over these three rows, and the
/// joins live in the dl layer by standing law.
pub fn scip_facts(request: &ResolveRequest) -> Result<Vec<FlatFact>, ProjectError> {
    let Some(root) = request.project_root else {
        return Err(ProjectError::ScipNeedsRoot);
    };
    let inputs = read_inputs(request.paths)?;
    let Some(index) = load_scip(request, &inputs)? else {
        return Err(ProjectError::ScipIndexerUnavailable(
            "scip facts need --scip-index or --scip-build".to_string(),
        ));
    };
    let files: Vec<&str> = index
        .documents
        .iter()
        .map(|doc| doc.relative_path.as_str())
        .collect();
    let blobs = SourceTreeBlobSource::open_files(root, &files).map_err(|err| {
        ProjectError::Read(
            root.to_path_buf(),
            std::io::Error::new(std::io::ErrorKind::Other, err),
        )
    })?;
    let reader = blobs.reader();
    Ok(crate::scip_rows::flatten_scip_records(
        &index,
        &reader,
        &request.scip_records,
        request.occurrence_text,
    ))
}

/// Serialize scip facts to sorted JSONL lines.
pub fn scip_facts_jsonl(request: &ResolveRequest) -> Result<Vec<String>, ProjectError> {
    Ok(sorted_lines(scip_facts(request)?))
}

/// Load the SCIP index the request names and fold it to file-to-file dependency
/// edges. This is v6's answer to the missing TypeScript module resolver: the
/// indexer already resolved every reference, so the module graph falls out of
/// the index without a resolver existing at all.
///
/// Unlike `scip_facts` this needs no reader: the fold joins symbols to the
/// documents that define them, and both sides are in the index.
pub fn scip_file_edges(request: &ResolveRequest) -> Result<Vec<FlatFact>, ProjectError> {
    let inputs = read_inputs(request.paths)?;
    let Some(index) = load_scip(request, &inputs)? else {
        return Err(ProjectError::ScipIndexerUnavailable(
            "file edges need --scip-index or --scip-build".to_string(),
        ));
    };
    Ok(crate::wire::scip_file_edges(&index))
}

/// Serialize file edges to sorted JSONL lines.
pub fn scip_file_edges_jsonl(request: &ResolveRequest) -> Result<Vec<String>, ProjectError> {
    Ok(sorted_lines(scip_file_edges(request)?))
}

/// Serialize resolved facts to sorted JSONL lines, the byte-stable form the CLI
/// prints and the goldens pin.
pub fn resolve_project_jsonl(request: &ResolveRequest) -> Result<Vec<String>, ProjectError> {
    let facts = resolve_project(request)?;
    // The envelope header is ORDER, not content: a consumer reads the protocol
    // before it can read anything, so it never joins the sort.
    let (header, body): (Vec<FlatFact>, Vec<FlatFact>) = facts
        .into_iter()
        .partition(|fact| matches!(fact, FlatFact::Protocol { .. } | FlatFact::Run(_)));
    let mut lines: Vec<String> = header
        .iter()
        .map(|fact| serde_json::to_string(fact).expect("flat fact is serializable"))
        .collect();
    lines.extend(sorted_lines(body));
    Ok(lines)
}

// ════════════════════════════════════════════════════════════════════════════
// THE TWO NAMED FAMILIES
// ════════════════════════════════════════════════════════════════════════════

/// One `--family scip` request: a root, where its index cache lives, and the
/// budget one indexer run may spend.
pub struct ScipFamilyRequest<'a> {
    /// The project root. Its marker files pick the indexer, and every document
    /// path in the answer is relative to it.
    pub root: &'a Path,
    /// Where a built index is placed and found again. `None` is v5's
    /// `<root>/.dl/.state`; a caller states its own so a test never writes into
    /// a committed fixture.
    pub cache_dir: Option<&'a Path>,
    /// The per-indexer wall budget (the timeout-gun law).
    pub budget: IndexBudget,
    /// The repo id for a document with no ancestor `.git`, in the `repo`
    /// column. Defaults to the root's basename.
    pub slug: Option<&'a str>,
    /// Run ONE named roster row instead of every row whose markers match.
    /// `None` is the marker-detected set. A polyglot root matches several rows
    /// and runs them all, so a caller who wants one indexer says which.
    pub indexer: crate::scip_ensure::IndexerPick<'a>,
}

/// The `scip` FAMILY: REAL SCIP INDEX DATA.
///
/// Ensure the root has a loadable index (v5's contract: an existing index wins,
/// otherwise the detected and installed indexer runs once under the budget),
/// then project it to v5's `scip_*` relation shapes.
///
/// This is the family whose rows are compiler-resolved. Every fact in the
/// answer came out of a real type checker's own index, which is exactly what
/// `diet_scip` cannot do and why the two carry different names.
///
/// A root that cannot be indexed yields NAMED SKIP rows rather than an error or
/// an empty stream, because a missing toolchain must skip a repo without
/// killing its caller (v5's law) and an empty stream reads as "this project has
/// no symbols", which is a worse lie than a failure.
pub fn scip_family(request: &ScipFamilyRequest) -> Result<Vec<FlatFact>, ProjectError> {
    let cache = match request.cache_dir {
        Some(dir) => dir.to_path_buf(),
        None => crate::scip_ensure::default_cache_dir(request.root),
    };
    let report = crate::scip_ensure::ensure_index_picked(
        request.root,
        &cache,
        request.budget,
        None,
        request.indexer,
    );
    let mut facts: Vec<FlatFact> = report
        .skips
        .iter()
        .map(|skip| FlatFact::ScipSkipRow {
            lang: skip.lang.to_string(),
            bin: skip.bin.to_string(),
            reason: skip.reason.slug().to_string(),
            detail: skip.reason.detail(),
        })
        .collect();
    let Some(index_path) = report.index.as_ref() else {
        return Ok(facts);
    };
    // The decode is indexer-agnostic (one prost decode serves every indexer),
    // so any roster entry loads any index, including a merged multi-language one.
    let index = ScipTypescript
        .load(index_path)
        .map_err(ProjectError::Scip)?;
    let slug = match request.slug {
        Some(slug) => slug.to_string(),
        None => request
            .root
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default(),
    };
    facts.push(FlatFact::ScipIndexRow {
        reused: report.reused,
        tool_name: index.metadata.tool_name.clone(),
        tool_version: index.metadata.tool_version.clone(),
        documents: index.documents.len() as u32,
    });
    facts.extend(crate::scip_v5_rels::v5_rel_rows(
        &index,
        request.root,
        &slug,
    ));
    Ok(facts)
}

/// Where `scip_family` put or found the index, for the human line the CLI
/// prints to stderr. Runs the same ensure with the same budget, so a caller
/// that wants both the rows and the path calls `scip_family` and this in the
/// same process: the second call takes the reuse branch by construction.
pub fn scip_index_location(request: &ScipFamilyRequest) -> Option<PathBuf> {
    let cache = match request.cache_dir {
        Some(dir) => dir.to_path_buf(),
        None => crate::scip_ensure::default_cache_dir(request.root),
    };
    crate::scip_ensure::index_path(request.root, &cache)
}

/// Serialize the `scip` family to sorted JSONL lines.
pub fn scip_family_jsonl(request: &ScipFamilyRequest) -> Result<Vec<String>, ProjectError> {
    Ok(sorted_lines(scip_family(request)?))
}

/// The `diet_scip` FAMILY: the tree-sitter parse plus heuristic resolution,
/// under an honest label.
///
/// DIET MEANS PARSE TECHNIQUE AND HEURISTICS, NEVER ACTUAL SCIP DATA. Nothing
/// in this answer came from a SCIP index or a type checker. The rows are the
/// crate's own front-ends' output resolved by name match across the supplied
/// file set, which is fast, needs no toolchain, and is WRONG wherever a name is
/// ambiguous corpus-wide: two files defining `helper` make every unqualified
/// call to `helper` unresolvable here, and a real index resolves it through the
/// import. That difference is the whole reason these are two names.
///
/// The call and types arms run. `--resolve` remains the pre-existing spelling
/// of the same pass and is byte-unchanged, including its narrower `call`-only
/// default; this family is the labelled entry, not a replacement.
// @comment-ok: one pre-existing diet_scip design note, edited by one line.
pub fn diet_scip(paths: &[PathBuf]) -> Result<Vec<FlatFact>, ProjectError> {
    resolve_project(&ResolveRequest {
        paths,
        arms: ResolveArms {
            call: true,
            types: true,
            flow: false,
        },
        scip: ScipMode::Off,
        project_root: None,
        scip_records: ScipRecords::all(),
        occurrence_text: false,
        rust_checker: None,
        ts_checker: None,
        witness: false,
    })
}

/// Serialize the `diet_scip` family to sorted JSONL lines.
pub fn diet_scip_jsonl(paths: &[PathBuf]) -> Result<Vec<String>, ProjectError> {
    Ok(sorted_lines(diet_scip(paths)?))
}

pub(crate) fn sorted_lines(facts: Vec<FlatFact>) -> Vec<String> {
    let mut lines: Vec<String> = facts
        .iter()
        .map(|fact| serde_json::to_string(fact).expect("flat fact is serializable"))
        .collect();
    lines.sort();
    lines
}

pub(crate) fn read_inputs(paths: &[PathBuf]) -> Result<Vec<ProjectInput>, ProjectError> {
    read_inputs_inner(paths, false)
}

/// The same read, plus each ts/js input's module facts. Split off because the
/// facts cost one extra parse per file and only `--resolve` reads them.
pub(crate) fn read_inputs_with_modules(
    paths: &[PathBuf],
) -> Result<Vec<ProjectInput>, ProjectError> {
    read_inputs_inner(paths, true)
}

fn read_inputs_inner(paths: &[PathBuf], modules: bool) -> Result<Vec<ProjectInput>, ProjectError> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    // A corpus inside a Git repository is read in one batched read_many of the
    // worktree (current disk, so untracked and dirty files are visible), keyed
    // by repo-relative paths internally while ProjectInput.path keeps the
    // caller's spelling byte-for-byte. A corpus outside any repository has no
    // revision coordinate for soopy to enumerate, so it falls back to the plain
    // per-path filesystem read, which is what the previous implementation did
    // for every input.
    match soopy::discover(&paths[0]) {
        Ok(repository) => read_inputs_batched(&repository, paths, modules),
        Err(_) => read_inputs_plain(paths, modules),
    }
}

/// The module facts of one file, when this run wants them.
fn module_facts_of(path: &str, content: &[u8], wanted: bool) -> Option<ModuleFacts> {
    wanted.then(|| crate::lang::ts_resolve::module_facts(path, content))?
}

/// The rust module plane's own facts, same discipline as `module_facts_of`.
fn rust_module_facts_of(path: &str, content: &[u8], wanted: bool) -> Option<RustModuleFacts> {
    wanted.then(|| crate::lang::rust_modules::rust_module_facts(path, content))?
}

/// The go module plane's own facts, same discipline as `module_facts_of`.
fn go_module_facts_of(path: &str, content: &[u8], wanted: bool) -> Option<GoModuleFacts> {
    wanted.then(|| crate::lang::go_modules::go_module_facts(path, content))?
}

/// The python module plane's own facts, same discipline as `module_facts_of`.
fn py_module_facts_of(path: &str, content: &[u8], wanted: bool) -> Option<PyModuleFacts> {
    wanted.then(|| py_module_facts(path, content))?
}

/// The kotlin module plane's own facts, same discipline as `module_facts_of`.
fn kt_module_facts_of(path: &str, content: &[u8], wanted: bool) -> Option<KtModuleFacts> {
    wanted.then(|| kt_module_facts(path, content))?
}

/// Extraction thread budget. One worker is held back below the clamp so the
/// machine stays usable while a corpus extracts.
fn extract_thread_cap() -> usize {
    let requested = std::env::var("SPREFA_EXTRACT_THREADS").ok();
    let cores = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1);
    thread_cap_from(requested.as_deref(), cores)
}

/// The cap rule with its two inputs passed in, so it is testable without
/// mutating process environment from a threaded test binary.
fn thread_cap_from(requested: Option<&str>, cores: usize) -> usize {
    if let Some(raw) = requested {
        if let Ok(value) = raw.trim().parse::<usize>() {
            if value != 0 {
                return value;
            }
        }
    }
    cores.min(8).saturating_sub(1).max(1)
}

/// The dedicated extraction pool. Never rayon's global pool, so nothing else in
/// the process inherits this cap.
static EXTRACT_POOL: LazyLock<rayon::ThreadPool> = LazyLock::new(|| {
    rayon::ThreadPoolBuilder::new()
        .num_threads(extract_thread_cap())
        .thread_name(|index| format!("extract-{index}"))
        .build()
        .expect("extract thread pool builds")
});

/// The extraction pool, for corpus walks outside this module. Handed out so no
/// caller reaches for rayon's global pool and escapes the cap.
pub fn extract_pool() -> &'static rayon::ThreadPool {
    &EXTRACT_POOL
}

/// Flatten per-path results in path order, dropping the skipped files and
/// surfacing the first error by path order.
fn flatten_inputs(
    results: Vec<Result<Option<ProjectInput>, ProjectError>>,
) -> Result<Vec<ProjectInput>, ProjectError> {
    let mut inputs = Vec::with_capacity(results.len());
    for result in results {
        if let Some(input) = result? {
            inputs.push(input);
        }
    }
    Ok(inputs)
}

fn read_inputs_plain(paths: &[PathBuf], modules: bool) -> Result<Vec<ProjectInput>, ProjectError> {
    let results: Vec<Result<Option<ProjectInput>, ProjectError>> = EXTRACT_POOL.install(|| {
        paths
            .par_iter()
            .map(|path| {
                let content =
                    std::fs::read(path).map_err(|err| ProjectError::Read(path.clone(), err))?;
                let path = path.to_string_lossy().to_string();
                let output = crate::dispatch(&path, &content, resolve_mask(&path));
                let module = module_facts_of(&path, &content, modules);
                let rust_module = rust_module_facts_of(&path, &content, modules);
                let go_module = go_module_facts_of(&path, &content, modules);
                let py_module = py_module_facts_of(&path, &content, modules);
                let kt_module = kt_module_facts_of(&path, &content, modules);
                Ok(output.map(|output| ProjectInput {
                    blob: content_id_of(&content),
                    path,
                    output,
                    module,
                    rust_module,
                    go_module,
                    py_module,
                    kt_module,
                }))
            })
            .collect()
    });
    flatten_inputs(results)
}

fn read_inputs_batched(
    repository: &soopy::Repository,
    paths: &[PathBuf],
    modules: bool,
) -> Result<Vec<ProjectInput>, ProjectError> {
    let keys: Vec<String> = paths
        .iter()
        .map(|path| repo_relative(&repository.root, path))
        .collect::<Result<_, _>>()?;
    let key_refs: Vec<&str> = keys.iter().map(String::as_str).collect();
    let source = SourceTreeBlobSource::open_files(&repository.root, &key_refs).map_err(|err| {
        ProjectError::Read(
            paths[0].clone(),
            std::io::Error::new(std::io::ErrorKind::Other, err),
        )
    })?;
    let answers = source.read_many(&key_refs);
    let results: Vec<Result<Option<ProjectInput>, ProjectError>> = EXTRACT_POOL.install(|| {
        paths
            .par_iter()
            .enumerate()
            .map(|(index, path)| {
                let Some(content) = answers[index].as_ref() else {
                    return Err(ProjectError::Read(
                        path.clone(),
                        std::io::Error::new(
                            std::io::ErrorKind::NotFound,
                            "not in the worktree snapshot",
                        ),
                    ));
                };
                let path = path.to_string_lossy().to_string();
                let output = crate::dispatch(&path, content, resolve_mask(&path));
                let module = module_facts_of(&path, content, modules);
                let rust_module = rust_module_facts_of(&path, content, modules);
                let go_module = go_module_facts_of(&path, content, modules);
                let py_module = py_module_facts_of(&path, content, modules);
                let kt_module = kt_module_facts_of(&path, content, modules);
                Ok(output.map(|output| ProjectInput {
                    blob: content_id_of(content),
                    path,
                    output,
                    module,
                    rust_module,
                    go_module,
                    py_module,
                    kt_module,
                }))
            })
            .collect()
    });
    flatten_inputs(results)
}

fn repo_relative(repo_root: &Path, path: &Path) -> Result<String, ProjectError> {
    let absolute =
        std::fs::canonicalize(path).map_err(|err| ProjectError::Read(path.to_path_buf(), err))?;
    let relative = absolute.strip_prefix(repo_root).map_err(|_| {
        ProjectError::Read(
            path.to_path_buf(),
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("{} is outside the repository", path.display()),
            ),
        )
    })?;
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

fn load_scip(
    request: &ResolveRequest,
    inputs: &[ProjectInput],
) -> Result<Option<ScipIndex>, ProjectError> {
    let source = match request.scip {
        ScipMode::Off => {
            // Informed-by-default: a resolve with no explicit SCIP flags still
            // adopts a FRESH index (one whose recorded set matches this file
            // set) so the scip leg pays for itself; anything else stays plain.
            if let Some(root) = request.project_root {
                if let Some(path) =
                    crate::scip_ensure::fresh_index_for_set(root, &index_set_of(inputs).digest())
                {
                    tracing::info!(
                        "scip-informed resolve: fresh index {} (plain flags, adopted by freshness)",
                        path.display()
                    );
                    return crate::scip_decode::load_index(&path)
                        .map(Some)
                        .map_err(ProjectError::Scip);
                }
                tracing::info!(
                    "scip-informed resolve: no fresh index under {}, plain name-match leg",
                    root.display()
                );
            }
            return Ok(None);
        }
        ScipMode::Load(path) => {
            let Some(_) = request.project_root else {
                return Err(ProjectError::ScipNeedsRoot);
            };
            // `load` is indexer-agnostic (one prost decode serves every
            // indexer), so any roster entry decodes any index.
            return ScipTypescript
                .load(path)
                .map(Some)
                .map_err(ProjectError::Scip);
        }
        ScipMode::Build => scip_source_for(inputs)?,
    };
    let Some(root) = request.project_root else {
        return Err(ProjectError::ScipNeedsRoot);
    };
    // `build` alone staged the index into a temp dir and dropped it, so every
    // ask paid a whole index build again; `ensure_index_for_set` places it in
    // the cache dir with its set sidecar and reuses it while the set holds.
    let set = index_set_of(inputs);
    // OUTSIDE the root, keyed by it: `--scip-build` resolves an arbitrary path
    // list and its roots include this crate's own committed fixture trees, which
    // reading must never write to. `--family scip ROOT` and the engine's scip
    // hosts are the callers that mean "index this repository" and they keep
    // `default_cache_dir`.
    let cache = crate::scip_ensure::external_cache_dir(root);
    let report =
        crate::scip_ensure::ensure_index_for_set(root, &cache, IndexBudget::from_env(), Some(&set));
    let index_path = report.index.ok_or_else(|| {
        ProjectError::ScipIndexerUnavailable(
            report
                .skips
                .iter()
                .map(|skip| format!("{}: {}", skip.lang, skip.reason.detail()))
                .collect::<Vec<_>>()
                .join("; "),
        )
    })?;
    source
        .load(&index_path)
        .map(Some)
        .map_err(ProjectError::Scip)
}

/// The freshness set for one resolve: the supplied paths and their content ids.
pub(crate) fn index_set_of(inputs: &[ProjectInput]) -> crate::scip_ensure::IndexSet {
    crate::scip_ensure::IndexSet::new(
        inputs
            .iter()
            .map(|input| (input.path.clone(), content_id_text(&input.blob))),
    )
}

fn content_id_text(blob: &ContentId) -> String {
    match blob {
        ContentId::GitBlob(oid) => oid.0.to_string(),
        ContentId::Blake3(bytes) => bytes.iter().map(|byte| format!("{byte:02x}")).collect(),
    }
}

/// The indexer for the supplied file set. One index means one indexer, so a
/// mixed-language path list is a named refusal rather than a silent pick.
fn scip_source_for(inputs: &[ProjectInput]) -> Result<&'static dyn ScipSource, ProjectError> {
    let mut languages: Vec<&'static str> = inputs
        .iter()
        .filter_map(|input| source_for(&input.path).map(Source::name))
        .collect();
    languages.sort_unstable();
    languages.dedup();
    match languages.as_slice() {
        ["ts"] => Ok(&ScipTypescript),
        ["go"] => Ok(&ScipGo),
        ["rust"] => Ok(&ScipRust),
        [one] => Err(ProjectError::ScipIndexerUnavailable(format!(
            "{one} has no indexer in the roster (ts, go and rust do)"
        ))),
        [] => Err(ProjectError::ScipIndexerUnavailable(
            "no supplied path matched a source".to_string(),
        )),
        many => Err(ProjectError::ScipIndexerUnavailable(format!(
            "one index means one indexer, but the paths span {many:?}"
        ))),
    }
}

/// Which types plane an arm's `Resolve<TypeF>` reads. Picks BOTH the phase-1
/// mask that produces the plane and the vector `src` indexes.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TypePlane {
    /// `src` indexes `FamilyBundle::nodes`; the plane rides `FamilyMask::ALL`.
    Nodes,
    /// `src` indexes `TypeFAux::doc_nodes` (`lang/markdown/_0_source.rs:266`),
    /// projected only with cst OFF (`lang/markdown/_0_source.rs:142`).
    DocNodes,
}

impl TypePlane {
    /// The phase-1 mask that produces this plane. `FamilyMask::ALL` leaves the
    /// doc plane empty, so a resolve read of it has to state its own mask.
    pub fn mask(self) -> FamilyMask {
        match self {
            TypePlane::Nodes => FamilyMask::ALL,
            TypePlane::DocNodes => FamilyMask {
                cst: false,
                types: true,
                call: false,
                df: false,
                data: false,
            },
        }
    }
}

/// One roster entry's phase-2 arms. `None` means the language has no impl:
/// `Resolve::resolve` is non-defaulted, so a missing arm cannot be dispatched.
pub struct ResolveArm {
    pub name: &'static str,
    pub call: Option<fn(&ExtractOutput, &ProjectCx) -> Vec<ProjectEdge<CallF>>>,
    pub types: Option<fn(&ExtractOutput, &ProjectCx) -> Vec<ProjectEdge<TypeF>>>,
    /// The `call` arm's non-edge channel: one row per site it dropped. `None`
    /// leaves an arm's output byte-identical to the era before the channel.
    pub drops: Option<fn(&ExtractOutput, &ProjectCx, &[ProjectEdge<CallF>]) -> Vec<ResolveDrop>>,
    /// Which types plane the `types` arm reads. Also the phase-1 mask
    /// `read_inputs` dispatches this language under.
    pub type_plane: TypePlane,
}

/// One call site a `Resolve<CallF>` arm dropped: where, why, and the callee as
/// written. `Vec<ProjectEdge>` has no seat for a non-edge, so the arm says here.
pub struct ResolveDrop {
    pub span: Span,
    pub reason: UnresolvedReason,
    pub detail: String,
}

/// One row per `Source` in `lang::sources()`; an impl with no row here is
/// unreachable from the binary. Checked both ways by `tests/1_resolve_cli.rs`.
pub static RESOLVE_ARMS: &[ResolveArm] = &[
    ResolveArm {
        name: "ts",
        call: Some(|out, cx| Resolve::<CallF>::resolve(&TsSource, out, cx)),
        types: Some(|out, cx| Resolve::<TypeF>::resolve(&TsSource, out, cx)),
        drops: Some(crate::lang::ts::call_drops),
        type_plane: TypePlane::Nodes,
    },
    ResolveArm {
        name: "rust",
        call: Some(|out, cx| Resolve::<CallF>::resolve(&RustSource, out, cx)),
        types: Some(|out, cx| Resolve::<TypeF>::resolve(&RustSource, out, cx)),
        drops: Some(crate::lang::rust::call_drops),
        type_plane: TypePlane::Nodes,
    },
    ResolveArm {
        name: "go",
        call: Some(|out, cx| Resolve::<CallF>::resolve(&GoSource, out, cx)),
        types: Some(|out, cx| Resolve::<TypeF>::resolve(&GoSource, out, cx)),
        drops: Some(crate::lang::go::call_drops),
        type_plane: TypePlane::Nodes,
    },
    ResolveArm {
        name: "dl6",
        call: Some(|out, cx| Resolve::<CallF>::resolve(&DlSource, out, cx)),
        types: Some(|out, cx| Resolve::<TypeF>::resolve(&DlSource, out, cx)),
        drops: None,
        type_plane: TypePlane::Nodes,
    },
    ResolveArm {
        name: "kotlin",
        call: Some(|out, cx| Resolve::<CallF>::resolve(&KotlinSource, out, cx)),
        types: Some(|out, cx| Resolve::<TypeF>::resolve(&KotlinSource, out, cx)),
        drops: None,
        type_plane: TypePlane::Nodes,
    },
    ResolveArm {
        name: "prolog",
        call: Some(|out, cx| Resolve::<CallF>::resolve(&PrologSource, out, cx)),
        types: None,
        drops: None,
        type_plane: TypePlane::Nodes,
    },
    ResolveArm {
        name: "python",
        call: Some(|out, cx| Resolve::<CallF>::resolve(&PythonSource, out, cx)),
        types: Some(|out, cx| Resolve::<TypeF>::resolve(&PythonSource, out, cx)),
        drops: None,
        type_plane: TypePlane::Nodes,
    },
    ResolveArm {
        name: "markdown",
        call: None,
        types: Some(|out, cx| Resolve::<TypeF>::resolve(&MarkdownSource, out, cx)),
        drops: None,
        type_plane: TypePlane::DocNodes,
    },
    // Nothing on the data plane names another file, so it resolves nothing.
    ResolveArm {
        name: "data",
        call: None,
        types: None,
        drops: None,
        type_plane: TypePlane::Nodes,
    },
    ResolveArm {
        name: "astgrep",
        call: None,
        types: None,
        drops: None,
        type_plane: TypePlane::Nodes,
    },
];

fn arm_for(path: &str) -> Option<&'static ResolveArm> {
    let name = source_for(path).map(Source::name)?;
    RESOLVE_ARMS.iter().find(|arm| arm.name == name)
}

/// The phase-1 mask one path is extracted under for a project resolve: whatever
/// its arm's types plane needs, and `FamilyMask::ALL` for every path with no arm.
fn resolve_mask(path: &str) -> FamilyMask {
    arm_for(path).map_or(FamilyMask::ALL, |arm| arm.type_plane.mask())
}

fn resolve_call_edges(
    path: &str,
    output: &ExtractOutput,
    cx: &ProjectCx,
) -> Vec<ProjectEdge<CallF>> {
    let Some(arm) = arm_for(path) else {
        tracing::warn!(path, "no resolve arm is wired for this path");
        return Vec::new();
    };
    let Some(resolve) = arm.call else {
        return Vec::new();
    };
    let span = tracing::debug_span!("resolve_arm", lang = arm.name, family = "call");
    let _entered = span.enter();
    let leg = crate::trace::phase_span(arm.name, crate::trace::Phase::ResolveLeg);
    let _legging = leg.enter();
    let edges = resolve(output, cx);
    let asked = output.call.as_ref().map_or(0, |bundle| bundle.aux.sites.len());
    crate::trace::record_phase(&leg, 0, edges.len() as u64, asked as u64);
    edges
}

fn resolve_type_edges(
    path: &str,
    output: &ExtractOutput,
    cx: &ProjectCx,
) -> Vec<ProjectEdge<TypeF>> {
    let Some(arm) = arm_for(path) else {
        tracing::warn!(path, "no resolve arm is wired for this path");
        return Vec::new();
    };
    let Some(resolve) = arm.types else {
        return Vec::new();
    };
    let span = tracing::debug_span!("resolve_arm", lang = arm.name, family = "type");
    let _entered = span.enter();
    let leg = crate::trace::phase_span(arm.name, crate::trace::Phase::ResolveLeg);
    let _legging = leg.enter();
    let edges = resolve(output, cx);
    let asked = output.types.as_ref().map_or(0, |bundle| bundle.aux.candidates.len());
    crate::trace::record_phase(&leg, 0, edges.len() as u64, asked as u64);
    edges
}

/// One comparison per edge under an index, one per (edge, input) pair under a
/// scan. Same counter for the node-by-span lookup a resolved edge names.
static RESOLVE_PROBES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn resolve_probes() -> u64 {
    RESOLVE_PROBES.load(std::sync::atomic::Ordering::Relaxed)
}

type SpanNames = std::collections::HashMap<(u32, u32), Option<crate::shape::NameId>>;

/// Every resolved edge names a target file and a span inside it. Both lookups
/// are tables built once over the whole input set, never a walk per edge.
struct TargetIndex<'a> {
    by_blob: std::collections::HashMap<&'a ContentId, &'a ProjectInput>,
    call_names: std::collections::HashMap<&'a ContentId, SpanNames>,
    type_names: std::collections::HashMap<&'a ContentId, SpanNames>,
}

/// First node at a span wins, the order the scan it replaces answered in.
fn span_names<F: crate::family::Family>(bundle: &FamilyBundle<F>) -> SpanNames {
    let mut names = SpanNames::with_capacity(bundle.nodes.len());
    for node in &bundle.nodes {
        names
            .entry((node.span.start, node.span.len))
            .or_insert(node.name);
    }
    names
}

impl<'a> TargetIndex<'a> {
    fn build(inputs: &'a [ProjectInput]) -> TargetIndex<'a> {
        let mut by_blob = std::collections::HashMap::with_capacity(inputs.len());
        let mut call_names = std::collections::HashMap::new();
        let mut type_names = std::collections::HashMap::new();
        for input in inputs {
            // First wins, the answer the scan this replaces gave when two paths
            // carry one blob.
            by_blob.entry(&input.blob).or_insert(input);
            if let Some(bundle) = input.output.call.as_ref() {
                call_names.insert(&input.blob, span_names(bundle));
            }
            if let Some(bundle) = input.output.types.as_ref() {
                type_names.insert(&input.blob, span_names(bundle));
            }
        }
        TargetIndex {
            by_blob,
            call_names,
            type_names,
        }
    }

    fn input(&self, blob: &ContentId) -> Option<&'a ProjectInput> {
        RESOLVE_PROBES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.by_blob.get(blob).copied()
    }
}

/// The declared name at `span` in one file's table, through that file's own
/// interner.
fn name_at(names: Option<&SpanNames>, output: &ExtractOutput, span: Span) -> Option<String> {
    RESOLVE_PROBES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    names?
        .get(&(span.start, span.len))
        .copied()
        .flatten()
        .map(|name| output.strings.lookup(name).to_string())
}

/// The callee's declared name at `span`. A constructor resolves to a class,
/// which is a TypeF def and never a CallF one, so the call table alone answers null.
fn callee_name(targets: &TargetIndex<'_>, target: &ProjectInput, span: Span) -> Option<String> {
    name_at(targets.call_names.get(&target.blob), &target.output, span)
        .or_else(|| name_at(targets.type_names.get(&target.blob), &target.output, span))
}

fn call_facts(
    input: &ProjectInput,
    targets: &TargetIndex<'_>,
    edges: &[ProjectEdge<CallF>],
    trail: &mut LegTrail,
) -> Vec<FlatFact> {
    let Some(call) = input.output.call.as_ref() else {
        return Vec::new();
    };
    edges
        .iter()
        .filter_map(|edge| {
            let target = targets.input(&edge.dst_blob)?;
            trail.push(edge);
            Some(FlatFact::ResolvedEdge {
                fact: None,
                caller_path: input.path.clone(),
                caller_name: caller_name(call, &input.output, edge.src),
                callee_path: target.path.clone(),
                callee_name: callee_name(targets, target, edge.dst_span),
                caller_site_start: edge.call_site.map_or(0, |span| span.start),
                caller_site_end: edge.call_site.map_or(0, |span| span.end()),
                kind: edge.kind.as_str().to_string(),
                resolution_origin: edge.origin.as_str().to_string(),
            })
        })
        .collect()
}

/// Every import binding one input writes. A file belongs to at most one
/// language's plane, so only one of the two closures below yields rows.
fn import_facts(input: &ProjectInput, cx: &ProjectCx) -> Vec<FlatFact> {
    let ts_rows = cx.indexes.ts_modules.get().into_iter().flat_map(|modules| {
        modules
            .bindings(&input.path)
            .into_iter()
            .map(|row| FlatFact::ResolvedImportRow {
                src_path: input.path.clone(),
                name: row.name,
                local: row.local,
                target_path: row.target_path,
                target_name: row.target_name,
                kind: row.kind.as_str().to_string(),
                hops: row.hops,
            })
    });
    let rust_rows = cx
        .indexes
        .rust_modules
        .get()
        .into_iter()
        .flat_map(|modules| {
            modules
                .bindings(&input.path)
                .into_iter()
                .map(|row| FlatFact::ResolvedImportRow {
                    src_path: input.path.clone(),
                    name: row.name,
                    local: row.local,
                    target_path: row.target_path,
                    target_name: row.target_name,
                    kind: row.kind.as_str().to_string(),
                    hops: row.hops,
                })
        });
    let go_rows = cx.indexes.go_modules.get().into_iter().flat_map(|modules| {
        modules
            .bindings(&input.path)
            .into_iter()
            .map(|row| FlatFact::ResolvedImportRow {
                src_path: input.path.clone(),
                name: row.name,
                local: row.local,
                target_path: row.target_path,
                target_name: row.target_name,
                kind: row.kind.as_str().to_string(),
                hops: 0,
            })
    });
    let py_rows = cx.indexes.py_modules.get().into_iter().flat_map(|modules| {
        modules
            .bindings(&input.path)
            .into_iter()
            .map(|row| FlatFact::ResolvedImportRow {
                src_path: input.path.clone(),
                name: row.name,
                local: row.local,
                target_path: row.target_path,
                target_name: row.target_name,
                kind: row.kind.as_str().to_string(),
                hops: row.hops,
            })
    });
    let kt_rows = cx.indexes.kt_modules.get().into_iter().flat_map(|modules| {
        modules
            .bindings(&input.path)
            .into_iter()
            .map(|row| FlatFact::ResolvedImportRow {
                src_path: input.path.clone(),
                name: row.name,
                local: row.local,
                target_path: row.target_path,
                target_name: row.target_name,
                kind: row.kind.as_str().to_string(),
                hops: row.hops,
            })
    });
    ts_rows
        .chain(rust_rows)
        .chain(go_rows)
        .chain(py_rows)
        .chain(kt_rows)
        .collect()
}

/// The `unresolved` rows for one input: the sites its `call` arm dropped. The
/// path rides the row because a resolve run spans files.
fn call_drop_facts(
    input: &ProjectInput,
    cx: &ProjectCx,
    edges: &[ProjectEdge<CallF>],
) -> Vec<FlatFact> {
    let Some(drops) = arm_for(&input.path).and_then(|arm| arm.drops) else {
        return Vec::new();
    };
    drops(&input.output, cx, edges)
        .into_iter()
        .map(|drop| FlatFact::Unresolved {
            family: crate::shape::FamilyTag::Call,
            path: Some(input.path.clone()),
            span: crate::wire::SpanOut::new(drop.span.start, drop.span.end()),
            reason: drop.reason.as_str().to_string(),
            detail: drop.detail,
        })
        .collect()
}

/// One `Resolve<TypeF>` edge's owner span and the name declared there. A
/// doc-plane owner is a heading: it carries its own name, out of the span table.
fn type_owner(
    plane: TypePlane,
    input: &ProjectInput,
    types: &FamilyBundle<TypeF>,
    names: Option<&SpanNames>,
    src: crate::shape::NodeRef,
) -> Option<(Span, Option<String>)> {
    match plane {
        // Past the node vec is an `ImplOwner`, which carries its own name for
        // the same reason a doc node does: it is not in the span table.
        TypePlane::Nodes => match types.nodes.get(src.0 as usize) {
            Some(node) => Some((node.span, name_at(names, &input.output, node.span))),
            None => {
                let owner = types.aux.impl_owners.get(src.0 as usize - types.nodes.len())?;
                let name = input.output.strings.lookup(owner.name).to_string();
                Some((owner.span, Some(name)))
            }
        },
        TypePlane::DocNodes => {
            let node = types.aux.doc_nodes.get(src.0 as usize)?;
            let name = input.output.strings.lookup(node.name).to_string();
            Some((node.span, Some(name)))
        }
    }
}

fn type_facts(
    input: &ProjectInput,
    targets: &TargetIndex<'_>,
    cx: &ProjectCx,
    trail: &mut LegTrail,
) -> Vec<FlatFact> {
    let Some(types) = input.output.types.as_ref() else {
        return Vec::new();
    };
    let plane = arm_for(&input.path).map_or(TypePlane::Nodes, |arm| arm.type_plane);
    resolve_type_edges(&input.path, &input.output, cx)
        .iter()
        .filter_map(|edge| {
            let target = targets.input(&edge.dst_blob)?;
            let names = targets.type_names.get(&input.blob);
            let (owner, owner_name) = type_owner(plane, input, types, names, edge.src)?;
            trail.push(edge);
            Some(FlatFact::ResolvedTypeEdge {
                fact: None,
                owner_path: input.path.clone(),
                owner_name,
                owner_start: owner.start,
                owner_end: owner.end(),
                target_path: target.path.clone(),
                target_name: name_at(
                    targets.type_names.get(&target.blob),
                    &target.output,
                    edge.dst_span,
                ),
                kind: edge.kind.as_str().to_string(),
                resolution_origin: edge.origin.as_str().to_string(),
            })
        })
        .collect()
}

/// A closure def carries no name, and `resolve_at` types caller_name `text`
/// (`v6/dl/fixtures/flagship-flow.dl6:35`): a null drops the whole row.
fn caller_name(
    bundle: &FamilyBundle<crate::types::CallF>,
    output: &ExtractOutput,
    src: crate::shape::NodeRef,
) -> Option<String> {
    let node = bundle.node(src);
    // The python module-as-caller ext kind answers null: the 4-col bench join
    // turns a null into the empty src_name the oracle uses for module rows.
    if node.kind == crate::lang::python::MODULE_CALLER {
        return None;
    }
    Some(match node.name {
        Some(name) => output.strings.lookup(name).to_string(),
        None => format!("closure@{}", node.span.start),
    })
}

/// Test-only filesystem `BlobSource`: project-relative path in, bytes out,
/// rooted at one directory, reading any path with no revision and no content
/// verification.
///
/// NOT a production source: production readers use `SourceTreeBlobSource`
/// (revision-aware, sees the worktree). This type survives only as a
/// plain-directory fixture and as the stable public re-export at `lib.rs`; it
/// has zero production call sites.
pub struct FsBlobSource {
    root: PathBuf,
}

/// Worktree- or revision-pinned source backed by `soopy`: enumeration happens
/// once at construction, and later reads retain that pass's exact source
/// identity for pinned revisions, or read the current worktree for the default
/// worktree mode.
///
/// KEYS. `blob` is addressed by PROJECT-ROOT-relative path (the `ProjectCx`
/// reader / `join_documents` contract), so entries are re-keyed from soopy's
/// repo-root-relative paths at construction by stripping the project root's
/// prefix under the repository root. A project root that is a subdirectory of
/// the Git root (a monorepo package) is the normal SCIP case, and a source that
/// keyed by repo-relative paths would miss every such document.
pub struct SourceTreeBlobSource {
    tree: std::sync::Mutex<soopy::SourceTree>,
    entries: std::collections::BTreeMap<String, soopy::SourceEntry>,
}

impl SourceTreeBlobSource {
    /// The default: a worktree snapshot. `Revision::Worktree` routes to soopy's
    /// fs-glob enumeration (`_4_worktree`), which sees untracked and dirty
    /// files, and later reads hit current disk.
    pub fn open_worktree(
        root: impl AsRef<Path>,
        patterns: &[soopy::Pattern],
    ) -> Result<Self, String> {
        Self::open(root, soopy::Revision::Worktree, patterns)
    }

    /// Revision-pinned: content-verified reads at the named revision. Callers
    /// that pass a revision opt into verification; the default above does not.
    pub fn open(
        root: impl AsRef<Path>,
        revision: soopy::Revision,
        patterns: &[soopy::Pattern],
    ) -> Result<Self, String> {
        let root = root.as_ref();
        let repository = soopy::discover(root).map_err(|error| error.to_string())?;
        let prefix = project_prefix(root, &repository.root)?;
        let mut tree = soopy::SourceTree::open(repository);
        let entries = tree
            .snapshot(&soopy::SourceQuery {
                revision,
                patterns: patterns.to_vec(),
            })
            .map_err(|error| error.to_string())?
            .files
            .into_iter()
            .filter_map(|entry| {
                let repo_path = entry.source.path.0.to_string();
                let key = if prefix.is_empty() {
                    repo_path
                } else {
                    repo_path.strip_prefix(&format!("{prefix}/"))?.to_string()
                };
                Some((key, entry))
            })
            .collect();
        Ok(Self {
            tree: std::sync::Mutex::new(tree),
            entries,
        })
    }

    /// Open a worktree source over exactly the given project-relative files,
    /// read in one batched `read_many`, without enumerating the whole
    /// repository. Used where the caller already knows the paths it needs
    /// (corpus ingest and the SCIP document reader), so it pays for the corpus,
    /// not the repository.
    pub fn open_files(root: impl AsRef<Path>, files: &[&str]) -> Result<Self, String> {
        let root = root.as_ref();
        let repository = soopy::discover(root).map_err(|error| error.to_string())?;
        let prefix = project_prefix(root, &repository.root)?;
        let mut tree = soopy::SourceTree::open(repository.clone());
        let revision = tree
            .resolve_revision(soopy::Revision::Worktree)
            .map_err(|error| error.to_string())?;
        let mut requests = Vec::with_capacity(files.len());
        for file in files {
            let repo_path = if prefix.is_empty() {
                file.to_string()
            } else {
                format!("{prefix}/{file}")
            };
            requests.push(soopy::ReadRequest {
                source: soopy::SourceRef {
                    repository: repository.identity.clone(),
                    revision: revision.clone(),
                    path: soopy::RepoPath(std::sync::Arc::from(repo_path.as_str())),
                },
                expected: None,
            });
        }
        let answers = tree
            .read_many(&requests)
            .map_err(|error| error.to_string())?;
        let entries = files
            .iter()
            .zip(answers)
            .map(|(file, answer)| {
                (
                    file.to_string(),
                    soopy::SourceEntry {
                        source: answer.source.clone(),
                        content: answer.content.clone(),
                        size: answer.bytes.len() as u64,
                    },
                )
            })
            .collect();
        Ok(Self {
            tree: std::sync::Mutex::new(tree),
            entries,
        })
    }

    pub fn entries(&self) -> impl Iterator<Item = &soopy::SourceEntry> {
        self.entries.values()
    }

    /// Read many project-relative paths in one batched `read_many`. Returns one
    /// `Option` per input path, aligned by position; `None` means the path is
    /// not in the enumerated corpus or could not be read.
    pub fn read_many(&self, paths: &[&str]) -> Vec<Option<Vec<u8>>> {
        let mut answers = vec![None; paths.len()];
        let mut tree = match self.tree.lock() {
            Ok(tree) => tree,
            Err(_) => return answers,
        };
        let mut slot: Vec<Option<usize>> = vec![None; paths.len()];
        let mut requests = Vec::new();
        for (index, path) in paths.iter().enumerate() {
            if let Some(entry) = self.entries.get(*path) {
                slot[index] = Some(requests.len());
                requests.push(soopy::ReadRequest {
                    source: entry.source.clone(),
                    expected: expected_for(entry),
                });
            }
        }
        let Ok(read) = tree.read_many(&requests) else {
            return answers;
        };
        for (index, request_index) in slot.iter().enumerate() {
            if let Some(request_index) = request_index {
                answers[index] = Some(read[*request_index].bytes.to_vec());
            }
        }
        answers
    }

    /// The `ProjectCx.reader` shape: a borrowed closure over this source.
    pub fn reader(&self) -> impl Fn(&str) -> Option<Vec<u8>> + '_ {
        |relative: &str| self.blob(relative)
    }
}

/// The project root's `/`-separated prefix under the repository root, `""` when
/// the two coincide. Both sides are canonical (soopy canonicalizes the
/// repository root in `open`), so the strip is symlink-safe.
fn project_prefix(root: &Path, repo_root: &Path) -> Result<String, String> {
    let project_root = std::fs::canonicalize(root).map_err(|error| error.to_string())?;
    Ok(project_root
        .strip_prefix(repo_root)
        .map_err(|_| {
            format!(
                "{} is outside the repository root {}",
                project_root.display(),
                repo_root.display()
            )
        })?
        .to_string_lossy()
        .replace('\\', "/"))
}

/// A pinned commit verifies the entry's content identity; a worktree read hits
/// current disk so a dirty tree (a file edited after the snapshot) reads the
/// same bytes a plain filesystem read would, never a stale-verification `None`.
fn expected_for(entry: &soopy::SourceEntry) -> Option<soopy::ContentId> {
    match &entry.source.revision {
        soopy::RevisionId::Commit(_) => Some(entry.content.clone()),
        soopy::RevisionId::Worktree { .. } => None,
    }
}

impl BlobSource for SourceTreeBlobSource {
    fn blob(&self, path: &str) -> Option<Vec<u8>> {
        // `None` is a corpus miss (the path was not enumerated/read at open). For
        // the Worktree mode that is the only source of `None`; for a pinned
        // Commit, a read that fails content verification also collapses to
        // `None`, because this trait has no error channel to tell the two apart.
        let entry = self.entries.get(path)?;
        let request = soopy::ReadRequest {
            source: entry.source.clone(),
            expected: expected_for(entry),
        };
        let mut tree = self.tree.lock().ok()?;
        let answers = tree.read_many(&[request]).ok()?;
        answers
            .into_iter()
            .next()
            .map(|answer| answer.bytes.to_vec())
    }
}

impl FsBlobSource {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The `ProjectCx.reader` shape: a borrowed closure over this source.
    /// `ProjectCx` takes a bare `Fn` rather than a `&dyn BlobSource`, so this is
    /// the adapter between the two.
    pub fn reader(&self) -> impl Fn(&str) -> Option<Vec<u8>> + '_ {
        |relative: &str| self.blob(relative)
    }
}

impl BlobSource for FsBlobSource {
    fn blob(&self, path: &str) -> Option<Vec<u8>> {
        std::fs::read(self.root.join(path)).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        root: PathBuf,
    }

    impl Fixture {
        fn build() -> Self {
            let root = std::env::temp_dir().join(format!(
                "parallel_dispatch_order_{}_{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).expect("fixture directory");
            Fixture { root }
        }

        fn write(&self, name: &str, body: &str) -> PathBuf {
            let path = self.root.join(name);
            std::fs::write(&path, body).expect("write fixture file");
            path
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    /// SABOTAGE, drop the `saturating_sub(1)` from the cap: 5 passed, 1 failed,
    /// only this test, the 12-core row reading 8 where 7 is held back.
    #[test]
    fn thread_cap_honors_the_request_then_clamps() {
        assert_eq!(thread_cap_from(Some("3"), 12), 3);
        assert_eq!(thread_cap_from(Some("  4 "), 12), 4);
        assert_eq!(thread_cap_from(Some("0"), 12), 7, "zero falls back");
        assert_eq!(
            thread_cap_from(Some("many"), 12),
            7,
            "unparseable falls back"
        );
        assert_eq!(
            thread_cap_from(None, 12),
            7,
            "12 cores clamp to 8, hold one back"
        );
        assert_eq!(
            thread_cap_from(None, 64),
            7,
            "the clamp is 8, not the core count"
        );
        assert_eq!(thread_cap_from(None, 2), 1);
        assert_eq!(
            thread_cap_from(None, 1),
            1,
            "a single core still gets a worker"
        );
    }

    /// SABOTAGE, reverse the flattened results (any collect that loses index
    /// order): 4 passed, 2 failed, this test and the skips one.
    #[test]
    fn read_inputs_preserves_path_order() {
        let fixture = Fixture::build();
        let mut paths = Vec::new();
        for index in 0..32 {
            paths.push(fixture.write(
                &format!("file_{index:02}.rs"),
                &format!("pub fn f{index}() -> i32 {{ {index} }}\n"),
            ));
        }
        let inputs = read_inputs(&paths).expect("read inputs");
        let actual: Vec<String> = inputs.iter().map(|input| input.path.clone()).collect();
        let expected: Vec<String> = paths
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect();
        assert_eq!(
            actual, expected,
            "input order must survive the parallel map"
        );
    }

    /// Unmatched files are dropped without disturbing the survivors' order.
    /// Same sabotage as above, same 4 passed 2 failed.
    #[test]
    fn read_inputs_skips_unmatched_in_order() {
        let fixture = Fixture::build();
        let mut paths = Vec::new();
        for index in 0..10 {
            paths.push(fixture.write(
                &format!("src_{index:02}.rs"),
                &format!("pub fn g{index}() -> i32 {{ {index} }}\n"),
            ));
        }
        for index in 0..4 {
            paths.push(fixture.write(&format!("notes_{index:02}.txt"), "plain text\n"));
        }
        for index in 10..20 {
            paths.push(fixture.write(
                &format!("src_{index:02}.rs"),
                &format!("pub fn g{index}() -> i32 {{ {index} }}\n"),
            ));
        }
        let inputs = read_inputs(&paths).expect("read inputs");
        let actual: Vec<String> = inputs.iter().map(|input| input.path.clone()).collect();
        let expected: Vec<String> = paths
            .iter()
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("rs"))
            .map(|path| path.to_string_lossy().to_string())
            .collect();
        assert_eq!(
            actual, expected,
            "skips must drop unmatched files without reordering"
        );
    }
}
