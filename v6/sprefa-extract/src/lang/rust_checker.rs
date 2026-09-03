//! The rust CHECKER tier: rust-analyzer answers the DESTINATION of a reference
//! this crate's parse found; caller, site spans and drops stay ours.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::shape::FamilyTag;
use crate::types::{ContentId, DefIndex, DefSite, Span};

/// One resolved reference. Offsets are the parse plane's unit (a line's start
/// byte plus the CHARACTER column), converted by `OffsetMap`, never raw bytes.
#[derive(Clone, Debug)]
pub struct CheckerRef {
    pub start: u32,
    pub end: u32,
    pub name: String,
    /// Empty when the checker resolved the reference OUTSIDE the resolve
    /// universe: std, a dependency, a file this run was not handed.
    pub dst_path: String,
    pub dst_name: String,
    /// The declaration identifier's offset: several defs in one file share a name.
    pub dst_offset: u32,
}

/// What the checker knows about one reference. `External` is knowledge, not
/// absence: no corpus edge exists, so no name-match leg may invent one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckerAnswer {
    Corpus(ContentId, Span),
    External,
}

/// The loader's return: resolved references per referring file, plus the two
/// costs the tier is judged on separately.
#[derive(Default)]
pub struct CheckerAnswers {
    pub calls: HashMap<String, Vec<CheckerRef>>,
    pub types: HashMap<String, Vec<CheckerRef>>,
    /// The item walk's own rows, ids run-local across the whole workspace. Empty
    /// unless the caller asked for them: the walk is not free.
    pub tsi: Vec<crate::tsi::FactOut>,
    /// A claim about the whole run, never a file.
    pub coverage: Vec<crate::tsi::CoverageClaim>,
    /// `cargo metadata` plus the salsa workspace load.
    pub load: Duration,
    /// The per-file resolve walk over the loaded workspace.
    pub walk: Duration,
    pub files_answered: usize,
    /// Every `MethodCallExpr` the walk visited, and the count rust-analyzer
    /// declined to name a function for: the tier's own answer-coverage gap.
    pub method_sites: usize,
    pub method_unresolved: usize,
}

/// Why the tier could not run. Every one falls back to the syntax leg.
#[derive(Debug)]
pub enum CheckerError {
    NotBuilt,
    NoWorkspace(String),
    Budget(Duration),
}

impl std::fmt::Display for CheckerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotBuilt => write!(
                f,
                "the rust checker tier needs --features rust-checker; falling back to the syntax leg"
            ),
            Self::NoWorkspace(detail) => write!(f, "no cargo workspace: {detail}"),
            Self::Budget(budget) => {
                write!(f, "workspace load exceeded {:.0}s", budget.as_secs_f64())
            }
        }
    }
}

/// One resolved reference, already joined to a corpus definition coordinate.
#[derive(Clone, Debug)]
struct Bound {
    start: u32,
    end: u32,
    name: String,
    answer: CheckerAnswer,
}

/// Every answer joined ONCE to a `(blob, def span)` at build time; per-file
/// lists sorted by start, so a site lookup is a range scan, not a corpus walk.
#[derive(Default)]
pub struct RustCheckerIndex {
    calls: HashMap<String, Vec<Bound>>,
    /// A TypeF candidate carries no reference span, so the type plane keys on
    /// (file, name); a name one file resolves two ways binds nothing.
    types: HashMap<String, HashMap<String, Option<CheckerAnswer>>>,
    /// The walk's rows, span digests already substituted for the supplied paths
    /// the walk wrote.
    tsi: Vec<crate::tsi::FactOut>,
    coverage: Vec<crate::tsi::CoverageClaim>,
    /// Answers naming a corpus file whose parse minted no def there; they fall
    /// back to the syntax leg, so this is the tier's own miss count.
    pub unjoined: usize,
    /// References the checker resolved outside the corpus.
    pub external: usize,
    /// Type names one file resolved two ways: an answer the tier HAS and the
    /// (file, name) key cannot carry.
    pub type_ambiguous: usize,
    pub load: Duration,
    pub walk: Duration,
    pub files_answered: usize,
    pub method_sites: usize,
    pub method_unresolved: usize,
}

impl RustCheckerIndex {
    /// An answer naming a file outside the resolve universe, or a def
    /// coordinate the parse never minted, is dropped: the syntax leg answers.
    pub fn build(
        answers: CheckerAnswers,
        corpus: &[(String, ContentId)],
        defs: &DefIndex,
    ) -> RustCheckerIndex {
        let blob_of: HashMap<&str, &ContentId> = corpus
            .iter()
            .map(|(path, blob)| (path.as_str(), blob))
            .collect();
        let mut index = RustCheckerIndex {
            load: answers.load,
            walk: answers.walk,
            files_answered: answers.files_answered,
            method_sites: answers.method_sites,
            method_unresolved: answers.method_unresolved,
            tsi: stamp_digests(answers.tsi, corpus),
            coverage: answers.coverage,
            ..RustCheckerIndex::default()
        };
        for (path, refs) in answers.calls {
            let mut bounds: Vec<Bound> = Vec::with_capacity(refs.len());
            for reference in refs {
                match answer_of(&reference, CALL_FACETS, &blob_of, defs) {
                    Some(answer) => {
                        index.external += (answer == CheckerAnswer::External) as usize;
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
            let mut by_name: HashMap<String, Option<CheckerAnswer>> = HashMap::new();
            for reference in refs {
                let Some(answer) = answer_of(&reference, TYPE_FACETS, &blob_of, defs) else {
                    index.unjoined += 1;
                    continue;
                };
                index.external += (answer == CheckerAnswer::External) as usize;
                match by_name.entry(reference.name) {
                    std::collections::hash_map::Entry::Vacant(slot) => {
                        slot.insert(Some(answer));
                    }
                    std::collections::hash_map::Entry::Occupied(mut slot) => {
                        if slot.get().as_ref() != Some(&answer) {
                            index.type_ambiguous += slot.get().is_some() as usize;
                            slot.insert(None);
                        }
                    }
                }
            }
            index.types.insert(path, by_name);
        }
        index
    }

    /// A site span covers the whole callee path (`a::b::c`) or a bare method
    /// ident, so the answer is the RIGHTMOST inside it carrying the callee name.
    pub fn call_at(&self, path: &str, site: Span, callee: &str) -> Option<CheckerAnswer> {
        let bounds = self.calls.get(path)?;
        let end = site.end();
        bounds
            .iter()
            .filter(|bound| bound.start >= site.start && bound.end <= end && bound.name == callee)
            .next_back()
            .map(|bound| bound.answer.clone())
    }

    pub fn type_at(&self, path: &str, name: &str) -> Option<CheckerAnswer> {
        self.types.get(path)?.get(name)?.clone()
    }

    pub fn semantic_rows(&self) -> &[crate::tsi::FactOut] {
        &self.tsi
    }

    pub fn coverage(&self) -> &[crate::tsi::CoverageClaim] {
        &self.coverage
    }
}

impl crate::tsi::SemanticRows for RustCheckerIndex {
    fn facts(&self) -> &[crate::tsi::FactOut] {
        self.semantic_rows()
    }

    fn coverage(&self) -> &[crate::tsi::CoverageClaim] {
        RustCheckerIndex::coverage(self)
    }
}

/// The walk wrote each corpus span's SUPPLIED path; that becomes the file's
/// content digest, and any other path stays as it is, naming a file off-corpus.
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

/// A call answer prefers the call facet and settles for the type facet: a tuple
/// struct or variant constructor is a call whose only def is a type entity.
const CALL_FACETS: &[FamilyTag] = &[FamilyTag::Call, FamilyTag::Type];
const TYPE_FACETS: &[FamilyTag] = &[FamilyTag::Type];

/// The declaration identifier's offset picks between several defs of one name in
/// one file; a lone def of the name binds without it, which mbe expansion needs.
fn answer_of(
    reference: &CheckerRef,
    facets: &[FamilyTag],
    blob_of: &HashMap<&str, &ContentId>,
    defs: &DefIndex,
) -> Option<CheckerAnswer> {
    if reference.dst_path.is_empty() {
        return Some(CheckerAnswer::External);
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
        Some(CheckerAnswer::Corpus(chosen.blob.clone(), chosen.span))
    })
}

/// Run the checker over `root` and answer every reference in `files`
/// (supplied path, absolute path).
#[cfg(not(feature = "rust-checker"))]
pub fn answer(
    _root: &Path,
    _files: &[(String, PathBuf)],
    _budget: Duration,
    _tsi: bool,
) -> Result<CheckerAnswers, CheckerError> {
    Err(CheckerError::NotBuilt)
}

#[cfg(feature = "rust-checker")]
pub fn answer(
    root: &Path,
    files: &[(String, PathBuf)],
    budget: Duration,
    tsi: bool,
) -> Result<CheckerAnswers, CheckerError> {
    super::rust_checker_ra::answer(root, files, budget, tsi)
}

/// A file's byte offset -> the parse plane's offset for the same position: a
/// line's start byte plus its CHARACTER column, the unit `syn_span` writes.
pub struct OffsetMap {
    line_starts: Vec<u32>,
    /// Per line, the byte offsets of its non-ASCII bytes. An all-ASCII line
    /// carries an empty slice and converts by identity.
    wide: Vec<Vec<u32>>,
}

impl OffsetMap {
    pub fn new(text: &str) -> OffsetMap {
        let mut line_starts = vec![0u32];
        let mut wide: Vec<Vec<u32>> = vec![Vec::new()];
        for (offset, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(offset as u32 + 1);
                wide.push(Vec::new());
            } else if byte >= 0x80 {
                wide.last_mut().expect("a line entry per line start").push(offset as u32);
            }
        }
        OffsetMap { line_starts, wide }
    }

    /// Every continuation byte of a multi-byte character is one byte the
    /// character column does not count.
    pub fn to_span_offset(&self, byte: u32) -> u32 {
        let line = match self.line_starts.binary_search(&byte) {
            Ok(exact) => exact,
            Err(next) => next.saturating_sub(1),
        };
        let start = self.line_starts[line];
        let continuations = self.wide[line]
            .iter()
            .take_while(|offset| **offset < byte)
            .count() as u32;
        start + (byte - start) - continuations
    }
}
