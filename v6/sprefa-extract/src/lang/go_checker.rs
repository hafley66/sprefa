//! The go CHECKER tier: `go/types`, driven by a sidecar this crate builds,
//! answers the DESTINATION of a reference this crate's parse found; caller,
//! site spans and drops stay ours.
//!
//! A per-lang copy of `ts_checker`, the way the resolve arms are. The join runs
//! as a project post-pass rather than inside the go arm, so one tier reaches
//! both go families without the arm carrying a second resolution order.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::shape::{FamilyTag, NodeRef};
use crate::types::{
    CallEdgeKind, CallF, ContentId, DefIndex, DefSite, ExtractOutput, ProjectEdge, ResolutionOrigin,
    Span, TypeF,
};

/// One resolved reference. Offsets are the UTF-8 byte offset `to_span` writes,
/// which is also what `go/token.Position.Offset` counts.
#[derive(Clone, Debug)]
pub struct GoCheckerRef {
    pub start: u32,
    pub end: u32,
    pub name: String,
    /// Empty when the checker resolved the reference OUTSIDE the resolve
    /// universe: the standard library, a dependency, a file this run was not
    /// handed.
    pub dst_path: String,
    pub dst_name: String,
    /// The declaration identifier's offset: several defs in one file share a name.
    pub dst_offset: u32,
}

/// What the checker knows about one reference. `External` is knowledge, not
/// absence: no corpus edge exists, so no name-match leg may invent one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GoCheckerAnswer {
    Corpus(ContentId, Span),
    External,
}

/// The driver's return: resolved references per referring file, plus the two
/// costs the tier is judged on separately.
#[derive(Default)]
pub struct GoCheckerAnswers {
    pub calls: HashMap<String, Vec<GoCheckerRef>>,
    pub types: HashMap<String, Vec<GoCheckerRef>>,
    /// The checker walk's own rows, ids run-local across the whole program.
    /// Empty unless the caller asked for them: the walk is not free.
    pub tsi: Vec<crate::tsi::FactOut>,
    /// (relation, complete, diagnostic). A claim about the whole run, never a file.
    pub coverage: Vec<(String, bool, Option<String>)>,
    /// `packages.Load` over the root: parse, type-check, module resolution.
    pub load: Duration,
    /// The per-file resolve walk over the loaded packages.
    pub walk: Duration,
    pub files_answered: usize,
}

/// Why the tier could not run. Every one falls back to the syntax leg.
#[derive(Debug)]
pub enum GoCheckerError {
    NotBuilt,
    /// `go` is not on PATH, or the sidecar could not be staged or compiled.
    NoDriver(String),
    /// The sidecar ran and failed; the string is its last stderr line.
    Failed(String),
    Budget(u64),
}

impl std::fmt::Display for GoCheckerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotBuilt => write!(
                f,
                "the go checker tier needs --features go-checker; falling back to the syntax leg"
            ),
            Self::NoDriver(detail) => write!(f, "no go driver: {detail}"),
            Self::Failed(detail) => write!(f, "the driver failed: {detail}"),
            Self::Budget(secs) => write!(f, "the driver exceeded {secs}s"),
        }
    }
}

/// One resolved reference, already joined to a corpus definition coordinate.
#[derive(Clone, Debug)]
struct Bound {
    start: u32,
    end: u32,
    name: String,
    answer: GoCheckerAnswer,
}

/// Every answer joined ONCE to a `(blob, def span)` at build time; per-file
/// lists sorted by start, so a site lookup is a range scan, not a corpus walk.
#[derive(Default)]
pub struct GoCheckerIndex {
    calls: HashMap<String, Vec<Bound>>,
    /// A TypeF candidate carries no reference span, so the type plane keys on
    /// (file, name AS WRITTEN); a name one file resolves two ways binds nothing.
    types: HashMap<String, HashMap<String, Option<GoCheckerAnswer>>>,
    /// The name behind each corpus def coordinate. The type post-pass reads a
    /// syntax edge's target name off this instead of re-walking the def index.
    def_names: HashMap<(ContentId, u32, u32), String>,
    tsi: Vec<crate::tsi::FactOut>,
    coverage: Vec<crate::tsi::CoverageClaim>,
    /// Answers naming a corpus file whose parse minted no def there; they fall
    /// back to the syntax leg, so this is the tier's own miss count.
    pub unjoined: usize,
    /// References the checker resolved outside the corpus.
    pub external: usize,
    pub load: Duration,
    pub walk: Duration,
    pub files_answered: usize,
}

impl GoCheckerIndex {
    /// An answer naming a file outside the resolve universe, or a def
    /// coordinate the parse never minted, is dropped: the syntax leg answers.
    pub fn build(
        answers: GoCheckerAnswers,
        corpus: &[(String, ContentId)],
        defs: &DefIndex,
    ) -> GoCheckerIndex {
        let blob_of: HashMap<&str, &ContentId> = corpus
            .iter()
            .map(|(path, blob)| (path.as_str(), blob))
            .collect();
        let mut def_names: HashMap<(ContentId, u32, u32), String> = HashMap::new();
        for (name, sites) in &defs.map {
            for site in sites {
                def_names
                    .entry((site.blob.clone(), site.span.start, site.span.end()))
                    .or_insert_with(|| name.clone());
            }
        }
        let mut index = GoCheckerIndex {
            load: answers.load,
            walk: answers.walk,
            files_answered: answers.files_answered,
            tsi: stamp_digests(answers.tsi, corpus),
            def_names,
            coverage: answers
                .coverage
                .into_iter()
                .map(|(relation, complete, diagnostic)| crate::tsi::CoverageClaim {
                    relation,
                    complete,
                    diagnostic,
                })
                .collect(),
            ..GoCheckerIndex::default()
        };
        for (path, refs) in answers.calls {
            let mut bounds: Vec<Bound> = Vec::with_capacity(refs.len());
            for reference in refs {
                match answer_of(&reference, CALL_FACETS, &blob_of, defs) {
                    Some(answer) => {
                        index.external += (answer == GoCheckerAnswer::External) as usize;
                        bounds.push(Bound {
                            start: reference.start,
                            end: reference.end,
                            name: reference.name,
                            answer,
                        });
                    }
                    None => index.unjoined += 1,
                }
            }
            bounds.sort_by_key(|bound| (bound.start, bound.end));
            index.calls.insert(path, bounds);
        }
        for (path, refs) in answers.types {
            let mut by_name: HashMap<String, Option<GoCheckerAnswer>> = HashMap::new();
            for reference in refs {
                let Some(answer) = answer_of(&reference, TYPE_FACETS, &blob_of, defs) else {
                    index.unjoined += 1;
                    continue;
                };
                index.external += (answer == GoCheckerAnswer::External) as usize;
                match by_name.entry(reference.name) {
                    std::collections::hash_map::Entry::Vacant(slot) => {
                        slot.insert(Some(answer));
                    }
                    std::collections::hash_map::Entry::Occupied(mut slot) => {
                        if slot.get().as_ref() != Some(&answer) {
                            slot.insert(None);
                        }
                    }
                }
            }
            index.types.insert(path, by_name);
        }
        index
    }

    /// Whether the tier answered this file at all. A file it never saw keeps
    /// every syntax edge untouched.
    pub fn knows(&self, path: &str) -> bool {
        self.calls.contains_key(path) || self.types.contains_key(path)
    }

    /// A site span covers the whole callee expression (`a.b.c(x)`): the answer
    /// is the RIGHTMOST inside it carrying the name.
    pub fn call_at(&self, path: &str, site: Span, callee: &str) -> Option<GoCheckerAnswer> {
        let bounds = self.calls.get(path)?;
        let end = site.end();
        bounds
            .iter()
            .filter(|bound| bound.start >= site.start && bound.end <= end && bound.name == callee)
            .next_back()
            .map(|bound| bound.answer.clone())
    }

    /// `name` is the candidate's `to` AS WRITTEN, which is the text the sidecar
    /// keys on too.
    pub fn type_at(&self, path: &str, name: &str) -> Option<GoCheckerAnswer> {
        self.types.get(path)?.get(name)?.clone()
    }

    pub fn semantic_rows(&self) -> &[crate::tsi::FactOut] {
        &self.tsi
    }

    pub fn coverage(&self) -> &[crate::tsi::CoverageClaim] {
        &self.coverage
    }
}

impl crate::tsi::SemanticRows for GoCheckerIndex {
    fn facts(&self) -> &[crate::tsi::FactOut] {
        self.semantic_rows()
    }

    fn coverage(&self) -> &[crate::tsi::CoverageClaim] {
        GoCheckerIndex::coverage(self)
    }
}

/// The tier's answers folded into one file's already-resolved call edges. A
/// site the checker names in the corpus takes the checker's coordinate and
/// leg; a site it names OUTSIDE loses its syntax edge, since the checker
/// knowing the target is off-corpus is knowledge the name match cannot beat.
pub fn apply_calls(
    index: &GoCheckerIndex,
    path: &str,
    output: &ExtractOutput,
    edges: &mut Vec<ProjectEdge<CallF>>,
) {
    if !index.knows(path) {
        return;
    }
    let Some(call) = output.call.as_ref() else {
        return;
    };
    let mut answered: HashSet<(u32, u32)> = HashSet::new();
    edges.retain_mut(|edge| {
        let Some(site) = edge.call_site else {
            return true;
        };
        let Some(callee) = site_callee(call, output, site) else {
            return true;
        };
        match index.call_at(path, site, callee) {
            Some(GoCheckerAnswer::Corpus(blob, span)) => {
                answered.insert((site.start, site.end()));
                edge.dst_blob = blob;
                edge.dst_span = span;
                edge.kind = CallEdgeKind::CheckerResolve;
                edge.origin = ResolutionOrigin::Checker;
                if !edge.witnesses.is_empty() {
                    edge.witnesses.push(ResolutionOrigin::Checker);
                }
                true
            }
            Some(GoCheckerAnswer::External) => false,
            None => true,
        }
    });
    // A site the syntax leg dropped and the checker answered is the tier's
    // whole recall gain, so it mints an edge of its own.
    for site in &call.aux.sites {
        if answered.contains(&(site.span.start, site.span.end())) {
            continue;
        }
        let callee = output.strings.lookup(site.callee);
        let Some(GoCheckerAnswer::Corpus(blob, span)) = index.call_at(path, site.span, callee)
        else {
            continue;
        };
        let Some(src) = crate::types::covering_def(call, site.span) else {
            continue;
        };
        edges.push(
            ProjectEdge::new(
                src,
                blob,
                span,
                CallEdgeKind::CheckerResolve,
                ResolutionOrigin::Checker,
            )
            .with_call_site(site.span),
        );
    }
}

/// The type twin. The type plane is name-keyed on both sides, so a syntax edge
/// whose target carries a name the checker also answered for this file is the
/// one the checker's own edge replaces.
pub fn apply_types(
    index: &GoCheckerIndex,
    path: &str,
    output: &ExtractOutput,
    edges: &mut Vec<ProjectEdge<TypeF>>,
) {
    if !index.knows(path) {
        return;
    }
    let Some(types) = output.types.as_ref() else {
        return;
    };
    let mut minted: Vec<ProjectEdge<TypeF>> = Vec::new();
    let mut replaced: HashSet<String> = HashSet::new();
    for candidate in &types.aux.candidates {
        let name = output.strings.lookup(candidate.to);
        match index.type_at(path, name) {
            Some(GoCheckerAnswer::Corpus(blob, span)) => {
                replaced.insert(name.to_string());
                let Some(src) = types
                    .nodes
                    .iter()
                    .position(|node| node.span == candidate.owner)
                else {
                    continue;
                };
                minted.push(ProjectEdge::new(
                    NodeRef(src as u32),
                    blob,
                    span,
                    candidate.kind,
                    ResolutionOrigin::Checker,
                ));
            }
            Some(GoCheckerAnswer::External) => {
                replaced.insert(name.to_string());
            }
            None => {}
        }
    }
    if replaced.is_empty() {
        return;
    }
    edges.retain(|edge| {
        match index
            .def_names
            .get(&(edge.dst_blob.clone(), edge.dst_span.start, edge.dst_span.end()))
        {
            Some(name) => !replaced.contains(name),
            None => true,
        }
    });
    // One (src, kind, target) per row: the same name written twice in one
    // signature is one edge, the way the syntax leg's own candidates fold.
    let mut seen: HashSet<(u32, String, u32, u32, &'static str)> = HashSet::new();
    for edge in minted {
        let key = (
            edge.src.0,
            edge.dst_blob.to_string(),
            edge.dst_span.start,
            edge.dst_span.end(),
            edge.kind.as_str(),
        );
        if seen.insert(key) {
            edges.push(edge);
        }
    }
}

/// The callee spelling at one call site, which is the name the sidecar wrote
/// beside its answer.
fn site_callee<'a>(
    call: &crate::types::FamilyBundle<CallF>,
    output: &'a ExtractOutput,
    site: Span,
) -> Option<&'a str> {
    call.aux
        .sites
        .iter()
        .find(|candidate| candidate.span == site)
        .map(|candidate| output.strings.lookup(candidate.callee))
}

/// The driver wrote each span's SUPPLIED path; a corpus path becomes the file's
/// content digest and any other path stays as it is, naming a file off-corpus.
fn stamp_digests(
    rows: Vec<crate::tsi::FactOut>,
    corpus: &[(String, ContentId)],
) -> Vec<crate::tsi::FactOut> {
    let digest_of: HashMap<&str, String> = corpus
        .iter()
        .map(|(path, blob)| (path.as_str(), blob.to_string()))
        .collect();
    rows.into_iter()
        .map(|mut row| {
            for arg in &mut row.args {
                if let crate::tsi::Arg::Span(key, _, _) = arg {
                    if let Some(digest) = digest_of.get(key.as_str()) {
                        *key = digest.clone();
                    }
                }
            }
            row
        })
        .collect()
}

/// A call answer prefers the call facet and settles for the type facet: a
/// conversion-shaped constructor's only def may be a type entity.
const CALL_FACETS: &[FamilyTag] = &[FamilyTag::Call, FamilyTag::Type];
/// Type prefers type and settles for call, the way the ts tier does: go's own
/// type resolve joins through facet-agnostic corpus defs, so a type-only
/// fallback would answer less than the leg it displaces.
const TYPE_FACETS: &[FamilyTag] = &[FamilyTag::Type, FamilyTag::Call];

/// The declaration identifier's offset picks between several defs of one name
/// in one file; a lone def of the name binds without it.
fn answer_of(
    reference: &GoCheckerRef,
    facets: &[FamilyTag],
    blob_of: &HashMap<&str, &ContentId>,
    defs: &DefIndex,
) -> Option<GoCheckerAnswer> {
    if reference.dst_path.is_empty() {
        return Some(GoCheckerAnswer::External);
    }
    let blob = *blob_of.get(reference.dst_path.as_str())?;
    let sites = defs.map.get(reference.dst_name.as_str())?;
    facets.iter().find_map(|facet| {
        let in_file: Vec<&DefSite> = sites
            .iter()
            .filter(|site| &site.blob == blob && site.family == *facet)
            .collect();
        let covering = in_file.iter().find(|site| {
            site.span.start <= reference.dst_offset && reference.dst_offset < site.span.end()
        });
        let chosen = match covering {
            Some(site) => *site,
            None if in_file.len() == 1 => in_file[0],
            None => return None,
        };
        Some(GoCheckerAnswer::Corpus(chosen.blob.clone(), chosen.span))
    })
}

/// Run the checker over `root` and answer every reference in `files`
/// (supplied path, absolute path).
#[cfg(not(feature = "go-checker"))]
pub fn answer(
    _root: &Path,
    _files: &[(String, PathBuf)],
    _tsi: bool,
) -> Result<GoCheckerAnswers, GoCheckerError> {
    Err(GoCheckerError::NotBuilt)
}

/// The sidecar, embedded rather than installed: a tier that needs a separate
/// `go install` to answer is a tier that silently does not run.
#[cfg(feature = "go-checker")]
const DRIVER_MAIN: &str = include_str!("../../tools/go_checker/main.go");
#[cfg(feature = "go-checker")]
const DRIVER_MOD: &str = include_str!("../../tools/go_checker/go.mod");
#[cfg(feature = "go-checker")]
const DRIVER_SUM: &str = include_str!("../../tools/go_checker/go.sum");

#[cfg(feature = "go-checker")]
#[derive(serde::Serialize)]
struct DriverRequest<'a> {
    root: &'a Path,
    files: &'a [(String, PathBuf)],
    /// The checker walk is the tier's expensive half and answers no resolve
    /// site, so it runs only for a stream that carries the TSI envelope.
    tsi: bool,
}

/// One `[start, end, name, dst_path, dst_name, dst_offset]` wire row.
#[cfg(feature = "go-checker")]
type WireRow = (u32, u32, String, String, String, u32);

#[cfg(feature = "go-checker")]
#[derive(serde::Deserialize)]
struct WireFile {
    path: String,
    calls: Vec<WireRow>,
    types: Vec<WireRow>,
    /// `[relation, arg, ...]` per row; the ordinal is the wire's, minted here.
    #[serde(default)]
    tsi: Vec<Vec<serde_json::Value>>,
}

#[cfg(feature = "go-checker")]
#[derive(serde::Deserialize)]
struct WireStats {
    stats: WireCosts,
    #[serde(default)]
    coverage: Vec<(String, bool, Option<String>)>,
}

#[cfg(feature = "go-checker")]
#[derive(serde::Deserialize)]
struct WireCosts {
    #[serde(rename = "loadMs")]
    load_ms: u64,
    #[serde(rename = "walkMs")]
    walk_ms: u64,
    files: usize,
}

#[cfg(feature = "go-checker")]
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum WireLine {
    File(WireFile),
    Stats(WireStats),
}

#[cfg(feature = "go-checker")]
fn into_refs(rows: Vec<WireRow>) -> Vec<GoCheckerRef> {
    rows.into_iter()
        .map(|(start, end, name, dst_path, dst_name, dst_offset)| GoCheckerRef {
            start,
            end,
            name,
            dst_path,
            dst_name,
            dst_offset,
        })
        .collect()
}

/// One driver row `[relation, arg, ...]` into a fact. A row the registry does
/// not know, or an argument it cannot decode, stops the tier.
#[cfg(feature = "go-checker")]
fn into_fact(row: Vec<serde_json::Value>) -> Result<crate::tsi::FactOut, GoCheckerError> {
    let mut parts = row.into_iter();
    let relation = parts
        .next()
        .and_then(|head| head.as_str().map(str::to_string))
        .ok_or_else(|| GoCheckerError::Failed("a tsi row opens with its relation".to_string()))?;
    let args: Vec<crate::tsi::Arg> = parts
        .map(serde_json::from_value)
        .collect::<Result<_, _>>()
        .map_err(|err| GoCheckerError::Failed(format!("{relation}: {err}")))?;
    crate::tsi::registry::check(&relation, &args)
        .map_err(|detail| GoCheckerError::Failed(format!("{relation}: {detail}")))?;
    Ok(crate::tsi::FactOut {
        fact: 0,
        relation,
        args,
    })
}

/// The sidecar compiled ONCE per distinct source, keyed by its own digest, so
/// a run pays `go build` only the first time it sees this driver. `go run`
/// re-links per invocation, which is the cost this cache exists to skip.
#[cfg(feature = "go-checker")]
fn staged_binary() -> Result<PathBuf, GoCheckerError> {
    use crate::scip_ensure::{run_capped, Capped};

    let stage = GoCheckerError::NoDriver;
    let digest = ContentId::blake3(
        format!("{DRIVER_MAIN}\u{0}{DRIVER_MOD}\u{0}{DRIVER_SUM}").as_bytes(),
    )
    .to_string();
    let short: String = digest.chars().rev().take(16).collect();
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = home.join(".cache/sprefa/go_checker").join(short);
    let binary = dir.join("go_checker");
    if binary.is_file() {
        return Ok(binary);
    }
    std::fs::create_dir_all(&dir).map_err(|err| stage(err.to_string()))?;
    std::fs::write(dir.join("main.go"), DRIVER_MAIN).map_err(|err| stage(err.to_string()))?;
    std::fs::write(dir.join("go.mod"), DRIVER_MOD).map_err(|err| stage(err.to_string()))?;
    std::fs::write(dir.join("go.sum"), DRIVER_SUM).map_err(|err| stage(err.to_string()))?;
    let Some(out) = binary.to_str() else {
        return Err(stage("the cache path is not utf-8".to_string()));
    };
    match run_capped(&["go", "build", "-o", out, "."], &dir, &dir) {
        Capped::Exited { success: true, .. } => Ok(binary),
        Capped::Exited { stderr_tail, .. } => Err(stage(format!("go build: {stderr_tail}"))),
        Capped::Killed { secs } => Err(GoCheckerError::Budget(secs)),
        Capped::NotLaunched => Err(stage("go is not on PATH".to_string())),
    }
}

/// The wall cap, the process group and the file-backed stdout all come from
/// `run_capped`: the same discipline every scip indexer spawn runs under.
#[cfg(feature = "go-checker")]
pub fn answer(
    root: &Path,
    files: &[(String, PathBuf)],
    tsi: bool,
) -> Result<GoCheckerAnswers, GoCheckerError> {
    use crate::scip_ensure::{run_capped, Capped};

    let binary = staged_binary()?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.subsec_nanos())
        .unwrap_or_default();
    let dir = std::env::temp_dir().join(format!("sprefa-go-checker-{}-{nanos}", std::process::id()));
    let stage = GoCheckerError::NoDriver;
    std::fs::create_dir_all(&dir).map_err(|err| stage(err.to_string()))?;
    let request = dir.join("request.json");
    let body =
        serde_json::to_vec(&DriverRequest { root, files, tsi }).map_err(|err| stage(err.to_string()))?;
    std::fs::write(&request, body).map_err(|err| stage(err.to_string()))?;

    let (Some(binary), Some(request)) = (binary.to_str(), request.to_str()) else {
        return Err(stage("the temp path is not utf-8".to_string()));
    };
    match run_capped(&[binary, request], root, &dir) {
        Capped::Exited { success: true, .. } => {}
        Capped::Exited { stderr_tail, .. } => return Err(GoCheckerError::Failed(stderr_tail)),
        Capped::Killed { secs } => return Err(GoCheckerError::Budget(secs)),
        Capped::NotLaunched => return Err(stage("the staged sidecar did not launch".to_string())),
    }

    let stdout = std::fs::read_to_string(dir.join("indexer.stdout.log"))
        .map_err(|err| GoCheckerError::Failed(err.to_string()))?;
    let mut answers = GoCheckerAnswers::default();
    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        match serde_json::from_str::<WireLine>(line) {
            Ok(WireLine::File(file)) => {
                for row in file.tsi {
                    answers.tsi.push(into_fact(row)?);
                }
                answers
                    .calls
                    .insert(file.path.clone(), into_refs(file.calls));
                answers.types.insert(file.path, into_refs(file.types));
            }
            Ok(WireLine::Stats(WireStats { stats, coverage })) => {
                answers.load = Duration::from_millis(stats.load_ms);
                answers.walk = Duration::from_millis(stats.walk_ms);
                answers.files_answered = stats.files;
                answers.coverage = coverage;
            }
            Err(err) => return Err(GoCheckerError::Failed(err.to_string())),
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    Ok(answers)
}
