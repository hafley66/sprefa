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

use crate::lang::{source_for, GoSource, KotlinSource, PrologSource, RustSource, TsSource};
use crate::rows::FamilyBundle;
use crate::scip::{ScipGo, ScipRust, ScipTypescript};
use crate::scip_rows::ScipRecords;
use crate::seams::{
    build_def_index, BlobSource, FileSet, IndexBag, ManifestMap, ProjectCx, ProjectDigest,
};
use crate::shape::{BlobHash, Span};
use crate::source::{ExtractOutput, FamilyMask, Resolve, Source};
use crate::types::{CallF, ProjectEdge, ScipError, ScipIndex, ScipSource, TypeF};
use crate::wire::FlatFact;

/// Which phase-2 arms to run. Both default off at the type level so a caller
/// states its intent; the CLI defaults `call` on for backward compatibility.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct ResolveArms {
    /// `Resolve<CallF>`: resolved caller-to-callee edges. Implemented for every
    /// source in the roster except the ast-grep CST fallback.
    pub call: bool,
    /// `Resolve<TypeF>`: resolved type reference edges. Implemented for TS, Go
    /// and Rust only; Kotlin and Prolog have no arm and are skipped rather than
    /// dispatched, because the trait's default body is `todo!()`.
    pub types: bool,
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
        }
    }
}

impl std::error::Error for ProjectError {}

/// One supplied file, extracted once, kept for the whole resolve.
struct ProjectInput {
    path: String,
    blob: BlobHash,
    output: ExtractOutput,
}

/// Run the requested arms over the whole supplied file set and return the flat
/// facts, sorted by their serialized form so callers get a byte-stable stream.
pub fn resolve_project(request: &ResolveRequest) -> Result<Vec<FlatFact>, ProjectError> {
    let inputs = read_inputs(request.paths)?;
    let scip_index = load_scip(request, &inputs)?;

    let pairs: Vec<(BlobHash, &ExtractOutput)> = inputs
        .iter()
        .map(|input| (input.blob, &input.output))
        .collect();
    let files = FileSet;
    let manifests = ManifestMap;
    // The reader is what makes a SCIP index usable: `join_documents` maps every
    // project-relative document path to its bytes, and the arms find their own
    // document by content hash off that join.
    let blobs = request.project_root.map(FsBlobSource::new);
    let reader = move |relative: &str| -> Option<Vec<u8>> { blobs.as_ref()?.blob(relative) };
    let cx = ProjectCx {
        files: &files,
        manifests: &manifests,
        reader: scip_index.is_some().then_some(&reader),
        digest: ProjectDigest::default(),
        indexes: IndexBag::default(),
    };
    cx.indexes
        .def_index
        .set(build_def_index(&pairs))
        .expect("fresh project definition index");
    if let Some(index) = scip_index {
        cx.indexes
            .scip_index
            .set(index)
            .expect("fresh project scip index");
    }

    let mut facts = Vec::new();
    for input in &inputs {
        if request.arms.call {
            facts.extend(call_facts(input, &inputs, &cx));
        }
        if request.arms.types {
            facts.extend(type_facts(input, &inputs, &cx));
        }
    }
    Ok(facts)
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
    let blobs = FsBlobSource::new(root);
    let reader = blobs.reader();
    Ok(crate::scip_rows::flatten_scip_records(
        &index,
        &reader,
        &request.scip_records,
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
    Ok(sorted_lines(resolve_project(request)?))
}

fn sorted_lines(facts: Vec<FlatFact>) -> Vec<String> {
    let mut lines: Vec<String> = facts
        .iter()
        .map(|fact| serde_json::to_string(fact).expect("flat fact is serializable"))
        .collect();
    lines.sort();
    lines
}

fn read_inputs(paths: &[PathBuf]) -> Result<Vec<ProjectInput>, ProjectError> {
    let mut inputs = Vec::with_capacity(paths.len());
    for path in paths {
        let content = std::fs::read(path).map_err(|err| ProjectError::Read(path.clone(), err))?;
        let path = path.to_string_lossy().to_string();
        if let Some(output) = crate::dispatch(&path, &content, FamilyMask::ALL) {
            inputs.push(ProjectInput {
                blob: BlobHash::of(&content),
                path,
                output,
            });
        }
    }
    Ok(inputs)
}

fn load_scip(
    request: &ResolveRequest,
    inputs: &[ProjectInput],
) -> Result<Option<ScipIndex>, ProjectError> {
    let source = match request.scip {
        ScipMode::Off => return Ok(None),
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
    let index_path = source.build(root).map_err(ProjectError::Scip)?;
    source
        .load(&index_path)
        .map(Some)
        .map_err(ProjectError::Scip)
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

/// Dispatch the `Resolve<CallF>` arm by source name. Explicit rather than
/// blanket: the trait's default body is `todo!()`, so a source without an arm
/// must never be dispatched.
fn resolve_call_edges(
    path: &str,
    output: &ExtractOutput,
    cx: &ProjectCx,
) -> Vec<ProjectEdge<CallF>> {
    match source_for(path).map(Source::name) {
        Some("ts") => Resolve::<CallF>::resolve(&TsSource, output, cx),
        Some("rust") => Resolve::<CallF>::resolve(&RustSource, output, cx),
        Some("go") => Resolve::<CallF>::resolve(&GoSource, output, cx),
        Some("kotlin") => Resolve::<CallF>::resolve(&KotlinSource, output, cx),
        Some("prolog") => Resolve::<CallF>::resolve(&PrologSource, output, cx),
        _ => Vec::new(),
    }
}

/// Dispatch the `Resolve<TypeF>` arm by source name. Only TS, Go and Rust have
/// one; Kotlin's is deferred to the traits arc and Prolog has no type plane, so
/// both are skipped here rather than hitting the trait's `todo!()`.
fn resolve_type_edges(
    path: &str,
    output: &ExtractOutput,
    cx: &ProjectCx,
) -> Vec<ProjectEdge<TypeF>> {
    match source_for(path).map(Source::name) {
        Some("ts") => Resolve::<TypeF>::resolve(&TsSource, output, cx),
        Some("rust") => Resolve::<TypeF>::resolve(&RustSource, output, cx),
        Some("go") => Resolve::<TypeF>::resolve(&GoSource, output, cx),
        _ => Vec::new(),
    }
}

fn call_facts(input: &ProjectInput, inputs: &[ProjectInput], cx: &ProjectCx) -> Vec<FlatFact> {
    let Some(call) = input.output.call.as_ref() else {
        return Vec::new();
    };
    resolve_call_edges(&input.path, &input.output, cx)
        .iter()
        .filter_map(|edge| {
            let target = inputs.iter().find(|other| other.blob == edge.dst_blob)?;
            Some(FlatFact::ResolvedEdge {
                caller_path: input.path.clone(),
                caller_name: node_name(call, &input.output, call.node(edge.src).span),
                callee_path: target.path.clone(),
                callee_name: target
                    .output
                    .call
                    .as_ref()
                    .and_then(|bundle| node_name(bundle, &target.output, edge.dst_span)),
                caller_site_start: edge.call_site.map_or(0, |span| span.start),
                caller_site_end: edge.call_site.map_or(0, |span| span.end()),
                kind: edge.kind.as_str().to_string(),
            })
        })
        .collect()
}

fn type_facts(input: &ProjectInput, inputs: &[ProjectInput], cx: &ProjectCx) -> Vec<FlatFact> {
    let Some(types) = input.output.types.as_ref() else {
        return Vec::new();
    };
    resolve_type_edges(&input.path, &input.output, cx)
        .iter()
        .filter_map(|edge| {
            let target = inputs.iter().find(|other| other.blob == edge.dst_blob)?;
            let owner = types.node(edge.src).span;
            Some(FlatFact::ResolvedTypeEdge {
                owner_path: input.path.clone(),
                owner_name: node_name(types, &input.output, owner),
                owner_start: owner.start,
                owner_end: owner.end(),
                target_path: target.path.clone(),
                target_name: target
                    .output
                    .types
                    .as_ref()
                    .and_then(|bundle| node_name(bundle, &target.output, edge.dst_span)),
                kind: edge.kind.as_str().to_string(),
            })
        })
        .collect()
}

/// The declared name of the node at `span` in one bundle, through that file's
/// own interner. A span with no node, or a node with no name, is `None` rather
/// than a fabricated string.
fn node_name<F: crate::family::Family>(
    bundle: &FamilyBundle<F>,
    output: &ExtractOutput,
    span: Span,
) -> Option<String> {
    bundle
        .nodes
        .iter()
        .find(|node| node.span == span)
        .and_then(|node| node.name)
        .map(|name| output.strings.lookup(name).to_string())
}

/// The filesystem `BlobSource`: project-relative path in, bytes out, rooted at
/// one directory.
///
/// `BlobSource` shipped as a trait with no implementation anywhere in the crate,
/// which is why every caller that needed a rev-correct reader wrote the same
/// closure by hand (this module did it twice before this type existed, and
/// golden_parity still does it per test). Naming it once makes the root explicit
/// and gives the phase-2 cache something to own later.
///
/// SOURCE-AGNOSTIC BY DESIGN, per the trait's own contract: this is the plain
/// directory implementation. A git worktree or an object-store implementation is
/// another type behind the same trait, and nothing above the trait changes.
///
/// A path that does not resolve, or cannot be read, is `None`. That is the
/// honest answer for a document the corpus does not contain, which is exactly
/// how `join_documents` tells a corpus file from an external one.
pub struct FsBlobSource {
    root: PathBuf,
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
