//! `impl Rehome for KotlinSource`: every question `extract move` asks a
//! language, answered for Kotlin. Import headers come off the same
//! tree-sitter-kotlin walk `lang/kotlin.rs` already carries
//! (`kt_walk_import_headers`), and the `package` declaration off the same parse.
//! @comment-ok: module header, the seam list every lang file opens with
//!
//! A Kotlin import is `package.Decl` and the `package` declaration is truth
//! (the directory is advisory), so a move changes imports only when the new
//! directory implies a new package under the SAME source root the old file sat
//! in. The source root is derived from the old path plus the declared package;
//! a layout that disagrees with the package is a named stop, never a guess
//! (v5 `src/ktpath.rs:24-50`).
//!
//! Four trait methods stay at their defaults, by decision rather than omission:
//! `manifests`/`manifest_refs` (a Gradle or Maven build file names source ROOTS
//! and never one `.kt` file, so no move can invalidate a target row),
//! `shim` (`--shim` answers `kotlin has no shim form` at `0_move.rs:129`; a
//! `typealias` left at the old path only forwards types, not functions or
//! properties, so it is NOT a shim) and `text_spellings` (a `.class` under
//! `build/` is a build's spelling, not one the corpus carries).
//!
//! The `warn` and `error` lines this arm prints lead the plan table rather than
//! following it: they are read off during `Plan::build`, which runs before
//! `0_move.rs:64` prints `root`.

use std::collections::{BTreeMap, BTreeSet};

use rayon::prelude::*;

use super::kotlin::{kt_child_kind, kt_first_child, kt_parse, kt_text, kt_walk_import_headers};
use super::KotlinSource;
use crate::family::SpecifierKind;
use crate::move_cx::{dirname, owned_by, MoveCx};
use crate::project::extract_pool;
use crate::shape::Strings;
use crate::types::{ImportRef, ImportRefKind, LangKind, Rehome, Respell, Span};

/// The moved file's own `package a.b` declaration, a kind only Kotlin constructs.
pub const PACKAGE_DECL: ImportRefKind = ImportRefKind::Ext(LangKind {
    lang: "kotlin",
    tag: "package_decl",
});

impl Rehome for KotlinSource {
    fn import_refs(&self, cx: &MoveCx) -> Vec<ImportRef> {
        let (plans, stops) = plans(cx);
        for stop in &stops {
            println!("error {stop}");
        }
        if plans.is_empty() {
            return Vec::new();
        }
        let corpus = cx.files_of(self);
        let packages: Vec<&str> = plans.iter().map(|plan| plan.old_package.as_str()).collect();

        // Read and parse fan out; the merge below stays sequential over `corpus`
        // in path order, so the ref order is rel order.
        let scans: Vec<Option<FileScan>> = extract_pool().install(|| {
            corpus
                .par_iter()
                .map(|rel| {
                    let bytes = cx.read(rel)?;
                    if !carries_package(&bytes, &packages) {
                        return None;
                    }
                    scan_file(String::from_utf8(bytes).ok()?)
                })
                .collect()
        });

        let mut refs = Vec::new();
        for plan in &plans {
            refs.push(ImportRef {
                importer: plan.old_rel.clone(),
                literal: plan.package_span,
                text: plan.old_package.clone(),
                target: plan.old_rel.clone(),
                kind: PACKAGE_DECL,
            });
        }
        let mut wildcards: BTreeMap<&str, usize> = BTreeMap::new();
        let mut bare: BTreeMap<&str, usize> = BTreeMap::new();
        for (rel, scan) in corpus.iter().zip(&scans) {
            let Some(scan) = scan else { continue };
            for plan in &plans {
                if *rel != plan.old_rel && scan.package() == Some(plan.old_package.as_str()) {
                    let count = bare_uses(&scan.text, &plan.decls);
                    if count > 0 {
                        *bare.entry(plan.old_rel.as_str()).or_default() += count;
                    }
                }
                for row in &scan.imports {
                    // A wildcard may still cover the moved decls and the package
                    // may hold other files: counted, never rewritten.
                    if row.wildcard {
                        if row.path == plan.old_package {
                            *wildcards.entry(plan.old_rel.as_str()).or_default() += 1;
                        }
                        continue;
                    }
                    if rewrite(plan, &row.path).is_none() {
                        continue;
                    }
                    refs.push(ImportRef {
                        importer: rel.to_string(),
                        literal: Span {
                            start: row.start,
                            len: row.path.len() as u32,
                        },
                        text: row.path.clone(),
                        target: plan.old_rel.clone(),
                        kind: ImportRefKind::Import,
                    });
                }
            }
        }
        for plan in &plans {
            if let Some(count) = wildcards.get(plan.old_rel.as_str()) {
                println!(
                    "warn {}: {count} wildcard import(s) of {} left alone; the moved decls may need explicit imports of {}",
                    plan.old_rel, plan.old_package, plan.new_package
                );
            }
            if let Some(count) = bare.get(plan.old_rel.as_str()) {
                println!(
                    "warn {}: {count} same-package bare use(s) of a moved decl left alone; the file leaves {} for {}",
                    plan.old_rel, plan.old_package, plan.new_package
                );
            }
        }
        tracing::debug!(corpus = corpus.len(), refs = refs.len(), "move kotlin refs");
        refs
    }

    fn respell(&self, cx: &MoveCx, reference: &ImportRef) -> Option<Respell> {
        // ONE plan, re-derived off the same read of the same (still unwritten)
        // file `import_refs` read, so the two calls share no state.
        let plan = plan_move(cx, &reference.target, cx.destination(&reference.target)?).ok()?;
        let text = match reference.kind {
            PACKAGE_DECL => plan.new_package.clone(),
            ImportRefKind::Import => rewrite(&plan, &reference.text)?,
            _ => return None,
        };
        (text != reference.text).then(|| Respell {
            file: reference.importer.clone(),
            span: reference.literal,
            text,
            receipt: None,
        })
    }
}

// ── the move plan ───────────────────────────────────────────────────────────

/// One moved Kotlin file resolved to the import rewrite it implies.
struct KotlinMove {
    old_rel: String,
    old_package: String,
    new_package: String,
    /// The span of the package NAME in the moved file's own declaration.
    package_span: Span,
    /// Top-level decl names the moved file exports: the only names whose
    /// imports respell, since an import names a decl and not a file.
    decls: BTreeSet<String>,
}

/// Every Kotlin move this run makes, and the named stop for each one whose
/// package cannot be derived. A stop rewrites nothing; the file still moves.
fn plans(cx: &MoveCx) -> (Vec<KotlinMove>, Vec<String>) {
    let mut plans = Vec::new();
    let mut stops = Vec::new();
    for (old, new) in cx.moved() {
        if !owned_by(old, &KotlinSource) {
            continue;
        }
        match plan_move(cx, old, new) {
            Ok(plan) => plans.push(plan),
            Err(stop) => stops.push(stop),
        }
    }
    (plans, stops)
}

fn plan_move(cx: &MoveCx, old: &str, new: &str) -> Result<KotlinMove, String> {
    let text = cx
        .text(old)
        .ok_or_else(|| format!("{old}: not readable as UTF-8 kotlin"))?;
    let scan = scan_file(text).ok_or_else(|| format!("{old}: does not parse as kotlin"))?;
    let (package_span, old_package) = scan
        .package
        .ok_or_else(|| format!("{old}: no package declaration, so its decls are not importable"))?;
    let root = source_root(old, &old_package).ok_or_else(|| {
        format!(
            "{old}: its directory {} does not match its declared package {old_package}, \
             so extract move will not guess the package {new} lands in",
            shown(dirname(old))
        )
    })?;
    let new_package = package_for(new, &root).ok_or_else(|| {
        format!(
            "{new}: outside the source root {} that {old} sits under",
            shown(&root)
        )
    })?;
    if new_package.is_empty() {
        return Err(format!(
            "{new}: sits at the source root {}, so it lands in the default package \
             and its decls stop being importable",
            shown(&root)
        ));
    }
    Ok(KotlinMove {
        old_rel: old.to_string(),
        old_package,
        new_package,
        package_span,
        decls: scan.decls,
    })
}

fn shown(dir: &str) -> &str {
    match dir.is_empty() {
        true => "the corpus root",
        false => dir,
    }
}

/// The source root `root` such that `dirname(rel) == root/<package as dirs>`,
/// `""` being the corpus root. None when the layout disagrees with the package.
fn source_root(rel: &str, package: &str) -> Option<String> {
    let dir = dirname(rel);
    if package.is_empty() {
        return Some(dir.to_string());
    }
    let suffix = package.replace('.', "/");
    if dir == suffix {
        return Some(String::new());
    }
    dir.strip_suffix(&format!("/{suffix}")).map(str::to_string)
}

/// The package `rel` answers to under `root`, or None when it sits outside it.
fn package_for(rel: &str, root: &str) -> Option<String> {
    let dir = dirname(rel);
    let within = if root.is_empty() {
        dir
    } else if dir == root {
        ""
    } else {
        dir.strip_prefix(&format!("{root}/"))?
    };
    Some(within.replace('/', "."))
}

/// `old_package.Decl[.Nested]` re-aimed at the new package, or None when the
/// path names another file's decl, another package, or nothing that moved.
fn rewrite(plan: &KotlinMove, path: &str) -> Option<String> {
    let rest = path.strip_prefix(&plan.old_package)?.strip_prefix('.')?;
    let head = rest.split('.').next().unwrap_or(rest);
    plan.decls
        .contains(head)
        .then(|| format!("{}.{rest}", plan.new_package))
}

// ── the tree-sitter scan ────────────────────────────────────────────────────

struct FileScan {
    /// The package NAME's span and text, when the file declares one.
    package: Option<(Span, String)>,
    imports: Vec<ImportRow>,
    decls: BTreeSet<String>,
    text: String,
}

impl FileScan {
    fn package(&self) -> Option<&str> {
        self.package.as_ref().map(|(_, name)| name.as_str())
    }
}

/// `start` + `path.len()` spans the dotted path ALONE; `import_header` itself
/// runs on through an alias, a `.*` and the line terminator.
struct ImportRow {
    start: u32,
    path: String,
    wildcard: bool,
}

fn scan_file(text: String) -> Option<FileScan> {
    let tree = kt_parse(&text)?;
    let root = tree.root_node();
    let source = text.as_bytes();
    let mut strings = Strings::new();
    let mut rows = Vec::new();
    kt_walk_import_headers(root, source, &mut strings, &mut rows);
    let imports = rows
        .into_iter()
        .filter_map(|row| {
            let path = strings.lookup(row.module?).to_string();
            (!path.is_empty()).then(|| ImportRow {
                start: row.span.start,
                path,
                wildcard: matches!(row.kind, SpecifierKind::Namespace),
            })
        })
        .collect();
    Some(FileScan {
        package: package_decl(root, source),
        imports,
        decls: top_level_decls(root, source),
        text,
    })
}

fn package_decl(root: tree_sitter::Node, source: &[u8]) -> Option<(Span, String)> {
    let header = find_kind(root, "package_header")?;
    let identifier = kt_child_kind(header, "identifier")?;
    let start = identifier.start_byte() as u32;
    Some((
        Span {
            start,
            len: identifier.end_byte() as u32 - start,
        },
        kt_text(identifier, source).to_string(),
    ))
}

fn find_kind<'a>(node: tree_sitter::Node<'a>, kind: &str) -> Option<tree_sitter::Node<'a>> {
    if node.kind() == kind {
        return Some(node);
    }
    let mut cursor = node.walk();
    let children: Vec<tree_sitter::Node<'a>> = node.named_children(&mut cursor).collect();
    children
        .into_iter()
        .find_map(|child| find_kind(child, kind))
}

/// Every name an importer can spell after the package: the DIRECT children of
/// the file, since a nested decl is reached through its owner's name.
fn top_level_decls(root: tree_sitter::Node, source: &[u8]) -> BTreeSet<String> {
    let mut cursor = root.walk();
    let children: Vec<tree_sitter::Node<'_>> = root.named_children(&mut cursor).collect();
    children
        .into_iter()
        .filter_map(|child| decl_name(child, source))
        .collect()
}

fn decl_name(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    let identifier = match node.kind() {
        "class_declaration" | "object_declaration" | "type_alias" => {
            kt_first_child(node, "type_identifier")?
        }
        "function_declaration" => kt_first_child(node, "simple_identifier")?,
        "property_declaration" => {
            let variable = kt_first_child(node, "variable_declaration")?;
            kt_first_child(variable, "simple_identifier")?
        }
        _ => return None,
    };
    Some(kt_text(identifier, source).trim_matches('`').to_string())
}

// ── the corpus filter and the bare-use count ────────────────────────────────

/// Whether a file can name the batch at all. A superset filter: an importer
/// writes the old package, and so does a peer declaring itself in it.
fn carries_package(bytes: &[u8], packages: &[&str]) -> bool {
    packages
        .iter()
        .any(|package| memchr::memmem::find(bytes, package.as_bytes()).is_some())
}

/// How many of `decls` a same-package peer spells bare. A bare use breaks when
/// the file it names leaves the package, and it is not an import to rewrite.
fn bare_uses(text: &str, decls: &BTreeSet<String>) -> usize {
    decls.iter().filter(|decl| mentions(text, decl)).count()
}

fn mentions(text: &str, name: &str) -> bool {
    let bytes = text.as_bytes();
    let needle = name.as_bytes();
    if needle.is_empty() {
        return false;
    }
    let mut at = 0;
    while let Some(hit) = memchr::memmem::find(&bytes[at..], needle) {
        let start = at + hit;
        let end = start + needle.len();
        let before = start == 0 || !is_word_byte(bytes[start - 1]);
        let after = end == bytes.len() || !is_word_byte(bytes[end]);
        if before && after {
            return true;
        }
        at = start + 1;
    }
    false
}

fn is_word_byte(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphanumeric()
}
