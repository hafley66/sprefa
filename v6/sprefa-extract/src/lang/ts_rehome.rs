//! `impl Rehome for TsSource`: every question `extract move` asks a language,
//! answered for the TS family. Specifiers come off the oxc parse
//! (`lang/ts.rs`), resolution off `oxc_resolver` (`lang/ts_resolve.rs`), path
//! constants off `lang/ts_paths.rs`, and `package.json` targets off the
//! tree-sitter-json parse this crate already links.
//! @comment-ok: module header, the seam list every lang file opens with

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use rayon::prelude::*;

use crate::lang::ts::{ts_specifiers, TsSource, TsSpecifier};
use crate::lang::ts_paths::{ts_path_literals, TsPathLiteral};
use crate::lang::ts_resolve::{respell, TsResolver};
use crate::move_cx::{dirname, join_rel, relative_between, MoveCx};
use crate::project::extract_pool;
use crate::types::{
    ImportRef, ImportRefKind, Rehome, RehomeManifests, RehomeTextSpellings, Respell, Span,
};

/// Emitted output whose specifiers mirror the source tree's. The corpus walk
/// keeps it (a `dist/package.json` is still a manifest); the parse does not.
const EMITTED_DIR: &str = "dist";

/// The `package.json` fields that can name a file this run moves.
const CANDIDATE_FIELDS: [&str; 6] = ["main", "module", "types", "browser", "bin", "exports"];

impl Rehome for TsSource {
    fn directory_stem(&self) -> Option<&'static str> {
        Some("index")
    }

    fn import_refs(&self, cx: &MoveCx) -> Vec<ImportRef> {
        let Ok(resolver) = resolver(cx.root()) else {
            return Vec::new();
        };
        let names = self.moved_names(cx);
        let corpus: Vec<&str> = cx
            .files_of(self)
            .into_iter()
            .filter(|rel| !emitted(rel))
            .collect();
        // An indexed rayon collect keeps corpus order, and corpus order is rel order.
        let per_file: Vec<Vec<ImportRef>> = extract_pool().install(|| {
            corpus
                .par_iter()
                .map(|rel| file_refs(cx, resolver, &names, rel))
                .collect()
        });
        let refs: Vec<ImportRef> = per_file.into_iter().flatten().collect();
        tracing::debug!(corpus = corpus.len(), refs = refs.len(), "move ts refs");
        refs
    }

    fn respell(&self, cx: &MoveCx, reference: &ImportRef) -> Option<Respell> {
        let text = match reference.kind {
            ImportRefKind::Import => import_respell(cx, reference)?,
            ImportRefKind::PathLiteral => literal_respell(cx, reference)?,
            ImportRefKind::ManifestTarget => manifest_respell(cx, reference)?,
            _ => return None,
        };
        if text == reference.text {
            return None;
        }
        let receipt = (reference.kind == ImportRefKind::ManifestTarget).then(|| {
            format!(
                "manifest {}: {} {} -> {}",
                reference.importer,
                reference.target,
                bare(&reference.text),
                bare(&text)
            )
        });
        Some(Respell {
            file: reference.importer.clone(),
            span: reference.literal,
            text,
            receipt,
        })
    }
}

impl RehomeManifests for TsSource {
    fn manifests(&self, cx: &MoveCx) -> Vec<String> {
        cx.files()
            .iter()
            .filter(|rel| rel.as_str() == "package.json" || rel.ends_with("/package.json"))
            .cloned()
            .collect()
    }

    fn manifest_refs(&self, cx: &MoveCx) -> Vec<ImportRef> {
        let manifests = self.manifests(cx);
        let package_dirs: Vec<&str> = manifests.iter().map(|rel| dirname(rel)).collect();
        let mut refs = Vec::new();
        for manifest in &manifests {
            let package_dir = dirname(manifest);
            // A package naming no moved file is never parsed, so it is never
            // opened for writing either.
            if !owns_a_move(cx, &package_dirs, package_dir) {
                continue;
            }
            let Some(text) = cx.text(manifest) else {
                continue;
            };
            for leaf in manifest_leaves(&text) {
                refs.push(ImportRef {
                    importer: manifest.clone(),
                    literal: leaf.span,
                    text: leaf.literal,
                    target: leaf.field_path,
                    kind: ImportRefKind::ManifestTarget,
                });
            }
        }
        refs
    }
}

impl RehomeTextSpellings for TsSource {
    fn text_spellings(&self, cx: &MoveCx, old: &str, new: &str) -> Vec<(String, String)> {
        let manifests = self.manifests(cx);
        let package_dirs: Vec<&str> = manifests.iter().map(|rel| dirname(rel)).collect();
        let Some(package_dir) = owning_package(&package_dirs, old) else {
            return Vec::new();
        };
        let build = build_paths(&cx.abs(package_dir));
        let mut out = Vec::new();
        for (old_image, new_image) in compiled_spellings(package_dir, &build, old, new) {
            out.push((format!("../{old_image}"), format!("../{new_image}")));
            out.push((old_image, new_image));
        }
        out
    }
}

// ── the specifier and path-constant scan ────────────────────────────────────

/// One TS file's references: its specifiers, plus the relative path constants a
/// file this run moves writes (a file staying put re-aims none of them).
fn file_refs(
    cx: &MoveCx,
    resolver: &TsResolver,
    names: &BTreeSet<String>,
    rel: &str,
) -> Vec<ImportRef> {
    let Some(text) = cx.text(rel) else {
        return Vec::new();
    };
    let file = cx.abs(rel);
    let is_moved = cx.destination(rel).is_some();
    let mut refs = Vec::new();
    if let Ok(rows) = ts_specifiers(&file.to_string_lossy(), &text) {
        for row in &rows {
            if let Some(reference) =
                specifier_ref(cx, resolver, names, rel, &text, &file, is_moved, row)
            {
                refs.push(reference);
            }
        }
    }
    if is_moved {
        for literal in ts_path_literals(&file.to_string_lossy(), &text).unwrap_or_default() {
            refs.push(literal_ref(rel, &text, &literal));
        }
    }
    refs
}

/// One specifier row as an `ImportRef`. A relative spec spells its target's own
/// name; only the resolver says what a bare or alias spec reaches.
#[allow(clippy::too_many_arguments)]
fn specifier_ref(
    cx: &MoveCx,
    resolver: &TsResolver,
    names: &BTreeSet<String>,
    rel: &str,
    text: &str,
    file: &Path,
    is_moved: bool,
    row: &TsSpecifier,
) -> Option<ImportRef> {
    let relative_spec = row.module.starts_with('.');
    if !is_moved && relative_spec && !spec_may_name(&row.module, names) {
        return None;
    }
    let target = resolver.resolve(file, &row.module)?;
    if target == *file {
        return None;
    }
    let target = cx.rel(&target)?;
    let start = row.module_span.start as usize;
    let end = start + row.module_span.len as usize;
    Some(ImportRef {
        importer: rel.to_string(),
        literal: row.module_span,
        text: text.get(start..end)?.to_string(),
        target,
        kind: ImportRefKind::Import,
    })
}

fn literal_ref(rel: &str, text: &str, literal: &TsPathLiteral) -> ImportRef {
    let start = literal.span.start as usize;
    let end = start + literal.span.len as usize;
    ImportRef {
        importer: rel.to_string(),
        literal: literal.span,
        text: text
            .get(start..end)
            .map(str::to_string)
            .unwrap_or_else(|| format!("{}{}{}", literal.quote, literal.text, literal.quote)),
        target: literal.text.clone(),
        kind: ImportRefKind::PathLiteral,
    }
}

/// The replacement for one specifier, or None when it names nothing that moved.
fn import_respell(cx: &MoveCx, reference: &ImportRef) -> Option<String> {
    let module = bare(&reference.text);
    let relative_spec = module.starts_with('.');
    let is_moved = cx.destination(&reference.importer).is_some();
    let aimed = match cx.destination(&reference.target) {
        Some(new) => new.to_string(),
        // A file that stays put is re-aimed only for a relative spec in a moving
        // importer: a tsconfig path and a package name anchor to the root.
        None if is_moved && relative_spec => reference.target.clone(),
        None => return None,
    };
    let quote = quote_of(&reference.text);
    let from_dir = dirname(cx.after(&reference.importer));
    if !relative_spec {
        if let Some(alias) = alias_respell(cx, reference, module, &aimed, quote) {
            return Some(alias);
        }
    }
    Some(respell(&relative_between(from_dir, &aimed), module, quote))
}

/// The replacement for one relative path constant a moved file writes, or None
/// when the text it already carries still names the same file.
fn literal_respell(cx: &MoveCx, reference: &ImportRef) -> Option<String> {
    let written = &reference.target;
    let old_dir = dirname(&reference.importer);
    let new_dir = dirname(cx.after(&reference.importer));
    let target = join_rel(old_dir, written);
    let aimed = cx
        .destination(&target)
        .map(str::to_string)
        .or_else(|| moved_directory(cx, &target))
        .unwrap_or(target);
    let mut relative = relative_between(new_dir, &aimed);
    if relative.is_empty() {
        relative = ".".to_string();
    } else if !relative.starts_with("..") {
        relative = format!("./{relative}");
    }
    if written.ends_with('/') && !relative.ends_with('/') {
        relative.push('/');
    }
    let quote = quote_of(&reference.text);
    Some(format!("{quote}{relative}{quote}"))
}

/// Where a directory lands, when every move under it keeps its relative suffix
/// and they all agree on one destination. Disagreement means it did not move.
fn moved_directory(cx: &MoveCx, target: &str) -> Option<String> {
    let prefix = format!("{target}/");
    let mut mapped: Option<String> = None;
    for (old, new) in cx.moved() {
        let Some(suffix) = old.strip_prefix(&prefix) else {
            continue;
        };
        let directory = new.strip_suffix(suffix)?.trim_end_matches('/').to_string();
        match &mapped {
            Some(seen) if *seen != directory => return None,
            Some(_) => {}
            None => mapped = Some(directory),
        }
    }
    mapped.filter(|directory| directory != target)
}

/// An alias keeps its alias when the prefix it resolved through still covers the
/// destination, re-probed against a file already there. Else: a relative path.
fn alias_respell(
    cx: &MoveCx,
    reference: &ImportRef,
    module: &str,
    aimed: &str,
    quote: char,
) -> Option<String> {
    let (prefix, mapped) = alias_prefix(module, &reference.target)?;
    let covered = if mapped.is_empty() {
        true
    } else {
        aimed.starts_with(&format!("{mapped}/"))
    };
    if !covered {
        return None;
    }
    let witness = alias_witness(cx, &mapped, aimed, &reference.target)?;
    let witness_rel = relative_between(&mapped, &witness);
    let stripped = witness_rel
        .rsplit_once('.')
        .map(|(head, _)| head)
        .unwrap_or(&witness_rel);
    let resolver = resolver(cx.root()).ok()?;
    let from = cx.abs(&reference.importer);
    let probe = resolver.resolve(&from, &format!("{prefix}/{stripped}"))?;
    if cx.rel(&probe).as_deref() != Some(witness.as_str()) {
        return None;
    }
    // `respell` writes a `./`-led relative path with the original's extension
    // style; the alias prefix replaces that lead.
    let spelled = respell(&relative_between(&mapped, aimed), module, quote);
    let tail = spelled.trim_matches(quote).strip_prefix("./")?.to_string();
    Some(format!("{quote}{prefix}/{tail}{quote}"))
}

/// The alias prefix and the directory it maps to, read off one resolution: a
/// `paths` entry splices text, so the spec's tail is the resolved path's tail.
fn alias_prefix(original: &str, target_rel: &str) -> Option<(String, String)> {
    let spec: Vec<&str> = original
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    let mut path: Vec<&str> = target_rel
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    let spec_last = segment_stem(spec.last()?);
    if segment_stem(path.last()?) == "index" && spec_last != "index" {
        path.pop();
    }
    let mut shared = 0;
    while shared + 1 < spec.len() && shared < path.len() {
        let left = spec[spec.len() - 1 - shared];
        let right = path[path.len() - 1 - shared];
        let same = if shared == 0 {
            segment_stem(left) == segment_stem(right)
        } else {
            left == right
        };
        if !same {
            break;
        }
        shared += 1;
    }
    if shared == 0 {
        return None;
    }
    Some((
        spec[..spec.len() - shared].join("/"),
        path[..path.len() - shared].join("/"),
    ))
}

fn segment_stem(segment: &str) -> &str {
    segment.split('.').next().unwrap_or(segment)
}

/// The deepest corpus file under both the mapped directory and the
/// destination's ancestry. The moved file is the last resort: it proves least.
fn alias_witness(cx: &MoveCx, mapped: &str, aimed: &str, old_target: &str) -> Option<String> {
    let mut probe = dirname(aimed).to_string();
    loop {
        if within(mapped, &probe) {
            let mut fallback: Option<String> = None;
            for rel in cx.files() {
                if dirname(rel) != probe || !is_ts_file(rel) {
                    continue;
                }
                if rel == old_target {
                    fallback = fallback.or_else(|| Some(rel.clone()));
                    continue;
                }
                return Some(rel.clone());
            }
            if let Some(rel) = fallback {
                return Some(rel);
            }
        }
        if probe == mapped {
            return None;
        }
        let parent = dirname(&probe).to_string();
        if parent == probe {
            return None;
        }
        probe = parent;
    }
}

/// Whether the root-relative directory `inner` sits at or under `outer`.
fn within(outer: &str, inner: &str) -> bool {
    outer.is_empty() || inner == outer || inner.starts_with(&format!("{outer}/"))
}

/// Whether a relative spec's last segment can name one of the moved files. A
/// spec with no readable last segment is never gated out.
fn spec_may_name(module: &str, names: &BTreeSet<String>) -> bool {
    let last = module.rsplit('/').next().unwrap_or(module);
    let stem = last.split('.').next().unwrap_or(last);
    stem.is_empty() || names.contains(stem)
}

fn is_ts_file(rel: &str) -> bool {
    crate::move_cx::owned_by(rel, &TsSource)
}

/// Whether a corpus path is build output rather than source.
fn emitted(rel: &str) -> bool {
    rel.split('/').any(|part| part == EMITTED_DIR)
}

fn bare(literal: &str) -> &str {
    let bytes = literal.as_bytes();
    let quoted = bytes.len() >= 2
        && matches!(bytes[0], b'\'' | b'"' | b'`')
        && bytes[bytes.len() - 1] == bytes[0];
    if quoted {
        &literal[1..literal.len() - 1]
    } else {
        literal
    }
}

fn quote_of(literal: &str) -> char {
    match literal.as_bytes().first() {
        Some(b'\'') => '\'',
        Some(b'`') => '`',
        _ => '"',
    }
}

/// ONE resolver per root per process: `oxc_resolver` holds its own filesystem
/// cache, so rebuilding it per specifier would re-pay every stat the run made.
fn resolver(root: &Path) -> Result<&'static TsResolver, String> {
    static CACHE: OnceLock<Mutex<BTreeMap<PathBuf, &'static TsResolver>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut held = cache.lock().map_err(|error| error.to_string())?;
    if let Some(existing) = held.get(root) {
        return Ok(existing);
    }
    let leaked: &'static TsResolver = Box::leak(Box::new(TsResolver::new(root)?));
    held.insert(root.to_path_buf(), leaked);
    Ok(leaked)
}

// ── package.json targets ────────────────────────────────────────────────────

/// One string leaf under a candidate field: where it sits, what it spells, and
/// the `field["./browser"].types` display a receipt line prints.
struct ManifestLeaf {
    span: Span,
    literal: String,
    field_path: String,
}

/// Whether any move this run makes belongs to `package_dir`.
fn owns_a_move(cx: &MoveCx, package_dirs: &[&str], package_dir: &str) -> bool {
    cx.moved()
        .keys()
        .any(|old| owning_package(package_dirs, old) == Some(package_dir))
}

/// The deepest `package.json` directory containing `old_rel`; `""` (root) wins
/// only when nothing more specific does.
fn owning_package<'a>(package_dirs: &[&'a str], old_rel: &str) -> Option<&'a str> {
    package_dirs
        .iter()
        .copied()
        .filter(|dir| {
            dir.is_empty()
                || (old_rel.starts_with(*dir) && old_rel.as_bytes().get(dir.len()) == Some(&b'/'))
        })
        .max_by_key(|dir| dir.len())
}

/// The replacement for one manifest target, or None when it names nothing moved.
fn manifest_respell(cx: &MoveCx, reference: &ImportRef) -> Option<String> {
    let manifests = TsSource.manifests(cx);
    let package_dirs: Vec<&str> = manifests.iter().map(|rel| dirname(rel)).collect();
    let package_dir = dirname(&reference.importer);
    let build = build_paths(&cx.abs(package_dir));
    let raw = json_text(&reference.text)?;
    let (prefix, bare_path) = split_prefix(&raw);
    let owned: Vec<(&String, &String)> = cx
        .moved()
        .iter()
        .filter(|(old, _)| owning_package(&package_dirs, old) == Some(package_dir))
        .collect();
    for (old, new) in &owned {
        if strip_dir(package_dir, old) == Some(bare_path) {
            let within_package = strip_dir(package_dir, new)?;
            return Some(json_literal(&format!("{prefix}{within_package}")));
        }
    }
    for (old, new) in &owned {
        for (old_image, new_image) in compiled_spellings(package_dir, &build, old, new) {
            if bare_path == old_image {
                return Some(json_literal(&format!("{prefix}{new_image}")));
            }
        }
    }
    None
}

fn split_prefix(raw: &str) -> (&str, &str) {
    match raw.strip_prefix("./") {
        Some(rest) => ("./", rest),
        None => ("", raw),
    }
}

fn strip_dir<'a>(dir: &str, rel: &'a str) -> Option<&'a str> {
    if dir.is_empty() {
        return Some(rel);
    }
    rel.strip_prefix(dir)?.strip_prefix('/')
}

fn json_text(literal: &str) -> Option<String> {
    serde_json::from_str::<String>(literal).ok()
}

fn json_literal(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| format!("\"{text}\""))
}

/// Every string leaf under a candidate field, in document order. Spans include
/// the quotes, so every byte outside a rewritten literal survives untouched.
fn manifest_leaves(text: &str) -> Vec<ManifestLeaf> {
    let mut parser = tree_sitter::Parser::new();
    if parser
        .set_language(&tree_sitter_json::LANGUAGE.into())
        .is_err()
    {
        return Vec::new();
    }
    let Some(tree) = parser.parse(text, None) else {
        return Vec::new();
    };
    let source = text.as_bytes();
    let mut out = Vec::new();
    let root = tree.root_node();
    let Some(object) = first_object(root) else {
        return out;
    };
    let mut cursor = object.walk();
    for pair in object.named_children(&mut cursor) {
        if pair.kind() != "pair" {
            continue;
        }
        let (Some(key), Some(value)) = (
            pair.child_by_field_name("key"),
            pair.child_by_field_name("value"),
        ) else {
            continue;
        };
        let Some(field) = node_string(key, source) else {
            continue;
        };
        if !CANDIDATE_FIELDS.contains(&field.as_str()) {
            continue;
        }
        collect_leaves(value, source, &mut vec![field], &mut out);
    }
    out
}

fn first_object(node: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    if node.kind() == "object" {
        return Some(node);
    }
    let mut cursor = node.walk();
    let children: Vec<tree_sitter::Node<'_>> = node.named_children(&mut cursor).collect();
    children.into_iter().find_map(first_object)
}

fn collect_leaves(
    node: tree_sitter::Node<'_>,
    source: &[u8],
    path: &mut Vec<String>,
    out: &mut Vec<ManifestLeaf>,
) {
    match node.kind() {
        "string" => {
            let start = node.start_byte() as u32;
            out.push(ManifestLeaf {
                span: Span {
                    start,
                    len: node.end_byte() as u32 - start,
                },
                literal: String::from_utf8_lossy(&source[node.start_byte()..node.end_byte()])
                    .to_string(),
                field_path: field_path_display(path),
            });
        }
        "object" => {
            let mut cursor = node.walk();
            for pair in node.named_children(&mut cursor) {
                if pair.kind() != "pair" {
                    continue;
                }
                let (Some(key), Some(value)) = (
                    pair.child_by_field_name("key"),
                    pair.child_by_field_name("value"),
                ) else {
                    continue;
                };
                let Some(name) = node_string(key, source) else {
                    continue;
                };
                path.push(name);
                collect_leaves(value, source, path, out);
                path.pop();
            }
        }
        "array" => {
            let mut cursor = node.walk();
            for (index, item) in node.named_children(&mut cursor).enumerate() {
                path.push(index.to_string());
                collect_leaves(item, source, path, out);
                path.pop();
            }
        }
        _ => {}
    }
}

fn node_string(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    let literal = std::str::from_utf8(&source[node.start_byte()..node.end_byte()]).ok()?;
    json_text(literal)
}

/// `field["./browser"].types` style: a plain identifier segment reads as
/// `.name`, an all-digit segment (array index) as `[n]`, anything else quoted.
fn field_path_display(path: &[String]) -> String {
    let mut out = String::new();
    for (index, segment) in path.iter().enumerate() {
        if index == 0 {
            out.push_str(segment);
        } else if segment.chars().all(|c| c.is_ascii_digit()) {
            out.push('[');
            out.push_str(segment);
            out.push(']');
        } else if segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            out.push('.');
            out.push_str(segment);
        } else {
            out.push_str("[\"");
            out.push_str(segment);
            out.push_str("\"]");
        }
    }
    out
}

// ── tsconfig outDir/rootDir, package-relative ───────────────────────────────

pub struct BuildPaths {
    pub root_dir: String,
    pub out_dir: String,
}

const BUILD_CONFIGS: [&str; 2] = ["tsconfig.build.json", "tsconfig.json"];

/// Read `rootDir`/`outDir` off the package's tsconfig chain, defaulting to
/// `src`/`dist` per field when no config states it.
pub fn build_paths(package_dir_abs: &Path) -> BuildPaths {
    let mut root_dir = None;
    let mut out_dir = None;
    for name in BUILD_CONFIGS {
        if root_dir.is_some() && out_dir.is_some() {
            break;
        }
        collect_build_paths(package_dir_abs, name, &mut root_dir, &mut out_dir, 0);
    }
    BuildPaths {
        root_dir: root_dir.unwrap_or_else(|| "src".to_string()),
        out_dir: out_dir.unwrap_or_else(|| "dist".to_string()),
    }
}

fn collect_build_paths(
    dir: &Path,
    file_name: &str,
    root_dir: &mut Option<String>,
    out_dir: &mut Option<String>,
    depth: u8,
) {
    if depth > 5 {
        return;
    }
    let Ok(text) = std::fs::read_to_string(dir.join(file_name)) else {
        return;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return;
    };
    if let Some(options) = value.get("compilerOptions") {
        if root_dir.is_none() {
            *root_dir = options
                .get("rootDir")
                .and_then(serde_json::Value::as_str)
                .map(strip_rel);
        }
        if out_dir.is_none() {
            *out_dir = options
                .get("outDir")
                .and_then(serde_json::Value::as_str)
                .map(strip_rel);
        }
    }
    if root_dir.is_none() || out_dir.is_none() {
        if let Some(extends) = value.get("extends").and_then(serde_json::Value::as_str) {
            let target = dir.join(extends);
            if let (Some(parent), Some(name)) = (target.parent(), target.file_name()) {
                collect_build_paths(
                    parent,
                    &name.to_string_lossy(),
                    root_dir,
                    out_dir,
                    depth + 1,
                );
            }
        }
    }
}

fn strip_rel(raw: &str) -> String {
    let trimmed = raw.trim_start_matches("./");
    if trimmed.is_empty() || trimmed == "." {
        String::new()
    } else {
        trimmed.trim_end_matches('/').to_string()
    }
}

fn join_dir(base: &str, rel: &str) -> String {
    if base.is_empty() {
        rel.to_string()
    } else if rel.is_empty() {
        base.to_string()
    } else {
        format!("{base}/{rel}")
    }
}

fn source_to_within_root(package_dir: &str, root_dir: &str, rel: &str) -> Option<String> {
    let full_root = join_dir(package_dir, root_dir);
    strip_dir(&full_root, rel).map(str::to_string)
}

const SOURCE_EXTS: [&str; 4] = [".ts", ".tsx", ".mts", ".cts"];

/// Emitted extension -> the source extensions it can come from. `.d.ts` loses
/// to nothing here; a declaration and its implementation share one stem.
const EMITTED_EXTS: [(&str, &[&str]); 7] = [
    (".d.ts", &[".ts", ".tsx"]),
    (".js.map", &[".ts", ".tsx"]),
    (".mjs.map", &[".mts"]),
    (".cjs.map", &[".cts"]),
    (".js", &[".ts", ".tsx"]),
    (".mjs", &[".mts"]),
    (".cjs", &[".cts"]),
];

/// `<outDir>/<within-rootDir path><emitted ext>` pairs, one per emitted ext
/// whose source set covers the move's own extension; empty outside `rootDir`.
pub fn compiled_spellings(
    package_dir: &str,
    build: &BuildPaths,
    old_rel: &str,
    new_rel: &str,
) -> Vec<(String, String)> {
    let Some(old_ext) = SOURCE_EXTS
        .iter()
        .copied()
        .find(|ext| old_rel.ends_with(ext))
    else {
        return Vec::new();
    };
    let Some(new_ext) = SOURCE_EXTS
        .iter()
        .copied()
        .find(|ext| new_rel.ends_with(ext))
    else {
        return Vec::new();
    };
    let Some(old_within) = source_to_within_root(package_dir, &build.root_dir, old_rel) else {
        return Vec::new();
    };
    let Some(new_within) = source_to_within_root(package_dir, &build.root_dir, new_rel) else {
        return Vec::new();
    };
    let Some(old_stem) = old_within.strip_suffix(old_ext) else {
        return Vec::new();
    };
    let Some(new_stem) = new_within.strip_suffix(new_ext) else {
        return Vec::new();
    };
    EMITTED_EXTS
        .iter()
        .filter(|(_, sources)| sources.contains(&old_ext))
        .map(|(emitted, _)| {
            (
                format!("{}/{old_stem}{emitted}", build.out_dir),
                format!("{}/{new_stem}{emitted}", build.out_dir),
            )
        })
        .collect()
}
