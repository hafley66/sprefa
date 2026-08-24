//! DIET MODULE RESOLUTION: TypeScript module specifiers -> file paths, with no
//! type checker, no indexer subprocess, and no new dependency.
//!
//! WHY IT EXISTS BESIDE `--scip-deps`. The SCIP fold resolves through the
//! TypeScript program, which is correct and costs an indexer run plus a 13MB
//! index per corpus. The raw material for a syntactic answer was already in
//! the crate: oxc parses every file anyway and `lang/ts.rs` already collects
//! import / export-from rows. The only missing step was specifier text -> file
//! path, which is what this module is. It is explicitly BEST EFFORT and is
//! allowed to lose to the indexer; `tools/1_madge_oracle.sh diet` is where the
//! loss is measured rather than asserted.
//!
//! GRADED, on the same corpus and against the same outside oracle as
//! `--scip-deps` (`tools/1_madge_oracle.sh both` over v6/tsv2, 761 madge edges):
//!
//! ```text
//!          edges  agree  madge-only  own-only  recall  precision
//! scip       764    755           6         9   0.992      0.988
//! diet       761    761           0         0   1.000      1.000
//! ```
//!
//! READ THAT SECOND ROW CAREFULLY. A perfect score against madge is agreement
//! between two syntactic import scanners, not correctness. The 9 edges scip has
//! and both syntactic tools lack are REAL: they are references that reach a
//! declaration through an inferred type with no import statement naming it, and
//! diet misses every one of them. That divergence is STRUCTURAL and no
//! syntactic resolver closes it without a type checker. The 6 edges diet has and
//! scip lacks are the reverse and are NOT a resolution difference at all: they
//! are files the corpus tsconfig's `include` omits, so the indexer never saw
//! them, and diet's universe is the file list it was handed. That one is
//! fixable on the scip side by widening the program definition.
//!
//! WHAT DIET CANNOT SEE AT ALL, stated rather than discovered later: dynamic
//! `import()` with a computed specifier, `require(...)`, and
//! `import x = require(...)`. The first is in this corpus five times and madge
//! misses it too, so it is invisible to the grading above rather than absent
//! from it.
//!
//! EVERY RULE IS A STATED POLICY, never a silent heuristic. `Policy` names them
//! and `resolve_specifier` returns the one that fired, so an unresolved
//! specifier says WHY it did not resolve instead of vanishing.
//!
//! NO PER-SPECIFIER SYSCALL. The universe of files is indexed once into a set
//! of project-relative paths and every candidate is a set lookup. Probing the
//! filesystem per specifier would be the N+1 shape at 755 specifiers per
//! corpus, and it would also make the answer depend on files outside the
//! resolution universe.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::project::{read_inputs, ProjectError, ResolveRequest};
use crate::types::{FlatFact, SpecifierKind};

/// The extensions a bare or extensionless specifier may name, in the order
/// TypeScript's own resolver tries them: TS sources before their JS twins, and
/// `.d.ts` after `.ts` because a declaration file loses to an implementation.
const CANDIDATE_EXTS: &[&str] = &[
    ".ts", ".tsx", ".d.ts", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs",
];

/// The ESM output-name rewrites: a NodeNext specifier names the file the
/// compiler EMITS, so `./x.js` on disk is `./x.ts`. Each entry maps the
/// written extension to the source extensions that can back it.
const EMITTED_REWRITES: &[(&str, &[&str])] = &[
    (".js", &[".ts", ".tsx", ".d.ts"]),
    (".mjs", &[".mts"]),
    (".cjs", &[".cts"]),
    (".jsx", &[".tsx"]),
];

/// Why one specifier resolved, or why it did not. Every arm is a rule this
/// module applies deliberately; there is no "other".
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Policy {
    /// A relative specifier that named an existing file exactly.
    RelativeExact,
    /// A relative specifier whose emitted extension was rewritten to a source
    /// extension (`./x.js` -> `./x.ts`).
    RelativeEmittedRewrite,
    /// A relative specifier with no extension, resolved by appending one.
    RelativeExtensionInferred,
    /// A relative specifier that named a directory, resolved to its index file.
    RelativeIndexFile,
    /// A bare specifier rewritten through a tsconfig `paths` pattern.
    TsconfigPaths,
    /// A bare specifier resolved against tsconfig `baseUrl`.
    TsconfigBaseUrl,
    /// A NAMED STOP, not a failure: a bare specifier no tsconfig rule claims is
    /// a package, and packages live outside the corpus by definition.
    NodeModulesBoundary,
    /// A NAMED STOP: a filesystem-absolute specifier. TypeScript permits them
    /// and no corpus-relative answer exists, so this resolver refuses rather
    /// than guessing a root.
    AbsolutePath,
    /// A relative specifier that resolved to nothing in the universe. Either
    /// the file is outside the supplied path list or the specifier is broken;
    /// this resolver cannot tell those apart and does not pretend to.
    RelativeUnresolved,
}

impl Policy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RelativeExact => "relative_exact",
            Self::RelativeEmittedRewrite => "relative_emitted_rewrite",
            Self::RelativeExtensionInferred => "relative_extension_inferred",
            Self::RelativeIndexFile => "relative_index_file",
            Self::TsconfigPaths => "tsconfig_paths",
            Self::TsconfigBaseUrl => "tsconfig_base_url",
            Self::NodeModulesBoundary => "node_modules_boundary",
            Self::AbsolutePath => "absolute_path",
            Self::RelativeUnresolved => "relative_unresolved",
        }
    }
}

/// The `paths` / `baseUrl` half of a tsconfig, the only two options that change
/// where a bare specifier resolves.
///
/// PARSED BEST EFFORT WITH serde_json, ALREADY A DEPENDENCY. A tsconfig is
/// JSON-with-comments and may carry trailing commas, so the text is stripped of
/// both before parsing; a file that still does not parse yields an EMPTY config
/// rather than an error, which downgrades every bare specifier to the
/// node_modules boundary. That is the honest degradation for a best-effort
/// resolver: it under-resolves, it never mis-resolves.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct TsconfigPaths {
    /// `baseUrl`, project-relative and normalized ("" = the project root).
    pub base_url: Option<String>,
    /// `paths`, pattern -> replacements, both as written.
    pub paths: BTreeMap<String, Vec<String>>,
}

impl TsconfigPaths {
    /// Read `<root>/tsconfig.json`. A missing or unparseable file is an empty
    /// config, never an error.
    pub fn read(root: &Path) -> Self {
        std::fs::read_to_string(root.join("tsconfig.json"))
            .ok()
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    pub fn parse(text: &str) -> Self {
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&strip_json_extras(text)) else {
            return Self::default();
        };
        let options = &json["compilerOptions"];
        let base_url = options["baseUrl"]
            .as_str()
            .map(|raw| normalize(&join_relative("", raw)));
        let mut paths = BTreeMap::new();
        if let Some(table) = options["paths"].as_object() {
            for (pattern, replacements) in table {
                let list: Vec<String> = replacements
                    .as_array()
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|item| item.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                paths.insert(pattern.clone(), list);
            }
        }
        Self { base_url, paths }
    }

    /// The candidate module texts a bare specifier maps to, base-relative.
    /// Empty means no tsconfig rule claims it.
    fn rewrite(&self, specifier: &str) -> Vec<(String, Policy)> {
        let base = self.base_url.clone().unwrap_or_default();
        let mut out = Vec::new();
        for (pattern, replacements) in &self.paths {
            let Some(captured) = match_pattern(pattern, specifier) else {
                continue;
            };
            for replacement in replacements {
                out.push((
                    join_relative(&base, &replacement.replacen('*', &captured, 1)),
                    Policy::TsconfigPaths,
                ));
            }
        }
        // baseUrl alone makes every bare specifier a candidate directory under
        // the base. It is tried only after `paths`, which is TypeScript's own
        // precedence.
        if out.is_empty() && self.base_url.is_some() {
            out.push((join_relative(&base, specifier), Policy::TsconfigBaseUrl));
        }
        out
    }
}

/// One `paths` pattern match: `"@app/*"` against `"@app/x/y"` captures `"x/y"`.
/// A pattern with no `*` matches only exactly, capturing the empty string.
fn match_pattern(pattern: &str, specifier: &str) -> Option<String> {
    match pattern.split_once('*') {
        None => (pattern == specifier).then(String::new),
        Some((prefix, suffix)) => {
            let rest = specifier.strip_prefix(prefix)?;
            let captured = rest.strip_suffix(suffix)?;
            Some(captured.to_string())
        }
    }
}

/// Strip `//` and `/* */` comments and trailing commas from JSON-with-comments,
/// respecting string literals and their escapes. Enough for a tsconfig; not a
/// general JSON5 reader, and it does not claim to be.
fn strip_json_extras(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    let mut in_string = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if in_string {
            out.push(byte as char);
            if byte == b'\\' && index + 1 < bytes.len() {
                out.push(bytes[index + 1] as char);
                index += 2;
                continue;
            }
            if byte == b'"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        match (byte, bytes.get(index + 1)) {
            (b'"', _) => {
                in_string = true;
                out.push('"');
                index += 1;
            }
            (b'/', Some(b'/')) => {
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            (b'/', Some(b'*')) => {
                index += 2;
                while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/')
                {
                    index += 1;
                }
                index = (index + 2).min(bytes.len());
            }
            _ => {
                out.push(byte as char);
                index += 1;
            }
        }
    }
    drop_trailing_commas(&out)
}

/// Remove a comma that is followed only by whitespace and a closing bracket.
fn drop_trailing_commas(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut in_string = false;
    for (index, &byte) in bytes.iter().enumerate() {
        if byte == b'"' && (index == 0 || bytes[index - 1] != b'\\') {
            in_string = !in_string;
        }
        if !in_string && byte == b',' {
            let next = bytes[index + 1..]
                .iter()
                .find(|candidate| !candidate.is_ascii_whitespace());
            if matches!(next, Some(b'}') | Some(b']')) {
                continue;
            }
        }
        out.push(byte as char);
    }
    out
}

/// Resolve one specifier written in `from_path` against `universe`, returning
/// the resolved project-relative path (when there is one) and the policy that
/// decided. `universe` holds every project-relative path the resolution may
/// reach; a specifier resolving outside it is unresolved by construction, which
/// is the same rule `resolve_project` states for names.
pub fn resolve_specifier(
    from_path: &str,
    specifier: &str,
    universe: &BTreeSet<String>,
    tsconfig: &TsconfigPaths,
) -> (Option<String>, Policy) {
    if specifier.starts_with('/') {
        return (None, Policy::AbsolutePath);
    }
    if !specifier.starts_with('.') {
        for (candidate, policy) in tsconfig.rewrite(specifier) {
            if let Some((hit, _)) = probe(&candidate, universe) {
                return (Some(hit), policy);
            }
        }
        // `node:fs`, `rxjs`, `@scope/pkg`: a package, and packages are outside
        // the corpus. This is the boundary, stated, not a failure to resolve.
        return (None, Policy::NodeModulesBoundary);
    }
    let directory = from_path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
    let joined = join_relative(directory, specifier);
    match probe(&joined, universe) {
        Some((hit, policy)) => (Some(hit), policy),
        None => (None, Policy::RelativeUnresolved),
    }
}

/// The candidate ladder for one already-joined path, in TypeScript's order:
/// the path as written, then the emitted-name rewrite, then extension
/// inference, then the directory's index file. Pure set lookups.
fn probe(joined: &str, universe: &BTreeSet<String>) -> Option<(String, Policy)> {
    if universe.contains(joined) {
        return Some((joined.to_string(), Policy::RelativeExact));
    }
    for (emitted, sources) in EMITTED_REWRITES {
        let Some(stem) = joined.strip_suffix(emitted) else {
            continue;
        };
        for source in *sources {
            let candidate = format!("{stem}{source}");
            if universe.contains(&candidate) {
                return Some((candidate, Policy::RelativeEmittedRewrite));
            }
        }
    }
    for ext in CANDIDATE_EXTS {
        let candidate = format!("{joined}{ext}");
        if universe.contains(&candidate) {
            return Some((candidate, Policy::RelativeExtensionInferred));
        }
    }
    for ext in CANDIDATE_EXTS {
        let candidate = format!("{joined}/index{ext}");
        if universe.contains(&candidate) {
            return Some((candidate, Policy::RelativeIndexFile));
        }
    }
    None
}

/// Join a specifier onto a directory and normalize `.` / `..` LEXICALLY. Never
/// touches the filesystem, so it never follows a symlink into a path the
/// universe does not name. A `..` that climbs past the root is clamped, which
/// makes such a specifier unresolvable rather than an escape.
pub fn join_relative(directory: &str, specifier: &str) -> String {
    let combined = if directory.is_empty() {
        specifier.to_string()
    } else {
        format!("{directory}/{specifier}")
    };
    normalize(&combined)
}

fn normalize(path: &str) -> String {
    let mut segments: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }
    segments.join("/")
}

/// One file's module specifiers, as the fold reads them.
pub struct SpecifierRow<'a> {
    /// The importing file, project-relative.
    pub from_path: &'a str,
    /// The source module text as written.
    pub module: &'a str,
    /// The bound local name, which is what `symbols` counts.
    pub name: &'a str,
    /// How the name entered scope; the edge's `kind` column.
    pub kind: SpecifierKind,
}

/// Fold module specifiers into file-to-file dependency edges: `src_path` writes
/// an import whose module resolves to `dst_path`.
///
/// The output record is `file_edge`, the SAME record `--scip-deps` produces, so
/// the dl layer sees ONE module-graph relation regardless of which resolver
/// filled it. `symbols` is the count of distinct bound names crossing the edge,
/// which is the same quantity the SCIP fold counts (distinct symbols) reached
/// syntactically: `import {a, b} from './m'` is two.
///
/// THE EDGE KEY IS (src, dst, kind). One pair carries one row per import form,
/// because `import x from './m'` and `export {y} from './m'` are different
/// facts about the same pair and a single row would have to drop one of them.
/// The per-pair total is the sum over its kinds, which a consumer can still ask
/// for; the reverse (recovering the forms from one summed row) is impossible.
///
/// A self-edge is dropped, matching the SCIP fold.
pub fn fold_edges(
    rows: &[SpecifierRow],
    universe: &BTreeSet<String>,
    tsconfig: &TsconfigPaths,
) -> Vec<FlatFact> {
    let mut crossings: BTreeMap<(&str, String, &'static str), BTreeSet<&str>> = BTreeMap::new();
    for row in rows {
        let (Some(target), _) = resolve_specifier(row.from_path, row.module, universe, tsconfig)
        else {
            continue;
        };
        if target == row.from_path {
            continue;
        }
        crossings
            .entry((row.from_path, target, row.kind.as_str()))
            .or_default()
            .insert(row.name);
    }
    crossings
        .into_iter()
        .map(|((src, dst, kind), names)| FlatFact::FileEdgeRow {
            src_path: src.to_string(),
            dst_path: dst,
            kind: kind.to_string(),
            symbols: names.len() as u32,
        })
        .collect()
}

/// Fold the specifiers that produced NO edge into `file_unresolved` rows, keyed
/// on (file, module, policy).
///
/// v5's `module_unresolved`. Every named stop is here, including the two that
/// are deliberate (`node_modules_boundary`, `absolute_path`): a consumer asking
/// which imports left the corpus has to be able to tell those from a broken
/// relative path, and a dropped row answers neither question.
pub fn fold_unresolved(
    rows: &[SpecifierRow],
    universe: &BTreeSet<String>,
    tsconfig: &TsconfigPaths,
) -> Vec<FlatFact> {
    let mut stops: BTreeSet<(&str, &str, &'static str)> = BTreeSet::new();
    for row in rows {
        let (target, policy) = resolve_specifier(row.from_path, row.module, universe, tsconfig);
        if target.is_some() {
            continue;
        }
        stops.insert((row.from_path, row.module, policy.as_str()));
    }
    for (src, module, reason) in &stops {
        tracing::warn!(
            src,
            module,
            reason,
            "module specifier resolved to no corpus file"
        );
    }
    stops
        .into_iter()
        .map(|(src, module, reason)| FlatFact::FileUnresolvedRow {
            src_path: src.to_string(),
            module: module.to_string(),
            reason: reason.to_string(),
        })
        .collect()
}

/// Resolve the supplied files' module specifiers syntactically and fold them to
/// file-to-file dependency edges: the DIET twin of `scip_file_edges`, with no
/// indexer subprocess and no type checker.
///
/// The supplied paths are the resolution universe AND the corpus. A specifier
/// resolving outside them produces no edge, which is the same rule
/// `resolve_project` states for names, and it is what keeps the answer
/// independent of whatever else happens to sit on disk.
///
/// Reads `<root>/tsconfig.json` once for `paths` / `baseUrl`. Every rule the
/// resolver applies is a named `deps::Policy`; see that module.
pub fn diet_file_edges(request: &ResolveRequest) -> Result<Vec<FlatFact>, ProjectError> {
    let Some(root) = request.project_root else {
        return Err(ProjectError::DepsNeedRoot);
    };
    let span = tracing::info_span!(
        "deps",
        files = tracing::field::Empty,
        specifiers = tracing::field::Empty
    );
    let _entered = span.enter();
    let inputs = read_inputs(request.paths)?;
    span.record("files", inputs.len() as u64);
    let root_absolute =
        std::fs::canonicalize(root).map_err(|err| ProjectError::Read(root.to_path_buf(), err))?;
    let relative: Vec<String> = inputs
        .iter()
        .map(|input| project_relative(&input.path, &root_absolute))
        .collect::<Result<_, _>>()?;
    let universe: BTreeSet<String> = relative.iter().cloned().collect();
    let tsconfig = TsconfigPaths::read(root);

    let rows: Vec<SpecifierRow> = inputs
        .iter()
        .zip(&relative)
        .filter_map(|(input, from_path)| Some((input, from_path, input.output.call.as_ref()?)))
        .flat_map(|(input, from_path, call)| {
            call.aux.specifiers.iter().filter_map(move |specifier| {
                Some(SpecifierRow {
                    from_path,
                    module: input.output.strings.lookup(specifier.module?),
                    name: input.output.strings.lookup(specifier.name),
                    kind: specifier.kind,
                })
            })
        })
        .collect();
    span.record("specifiers", rows.len() as u64);
    let mut facts = fold_edges(&rows, &universe, &tsconfig);
    facts.extend(fold_unresolved(&rows, &universe, &tsconfig));
    Ok(facts)
}

/// Serialize diet file edges to sorted JSONL lines.
pub fn diet_file_edges_jsonl(request: &ResolveRequest) -> Result<Vec<String>, ProjectError> {
    Ok(crate::project::sorted_lines(diet_file_edges(request)?))
}

/// One supplied path as a project-relative slash path. Canonicalized on both
/// sides so a relative argument and an absolute root still meet.
pub(crate) fn project_relative(path: &str, root_absolute: &Path) -> Result<String, ProjectError> {
    let absolute =
        std::fs::canonicalize(path).map_err(|err| ProjectError::Read(PathBuf::from(path), err))?;
    absolute
        .strip_prefix(root_absolute)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .map_err(|_| ProjectError::DepsPathOutsideRoot(PathBuf::from(path)))
}
