//! `impl Rename for KotlinSource`: every question `extract rename` asks a
//! language, answered for Kotlin over the tree-sitter-kotlin-sg parse
//! `lang/kotlin.rs` already carries. tree-sitter nodes give raw byte offsets, so
//! every seat is identifier-exact with no bridge and `RenameStop::Inexact` never
//! fires here.
//! @comment-ok: module header, the seam list every lang file opens with
//!
//! Kotlin's own name law, restated for a `RenameCx` the way `kotlin_rehome.rs`
//! restates it for a `MoveCx`: the `package` declaration is truth and the
//! directory is advisory, so a file reaches the anchor's declaration through its
//! own `package`, through an explicit `import <package>.<name>`, or through a
//! fully-qualified `<package>.<name>` written in place.
//!
//! Three seats are reported and never rewritten, because rewriting one guesses:
//! a `import <package>.*` that puts the bare name in a scope no clause names,
//! the arguments of an annotation, and any spelling inside a string literal or a
//! KDoc block. The first is a `Dynamic` stop; the other two reach `--text-refs`
//! through `text_spellings`.
//!
//! Two limits this arm states rather than hides. A local or member binding that
//! reuses the name is respelled too: only TOP-LEVEL declarations are read as
//! shadows, since a Kotlin block scope is not walked. And a backtick-quoted
//! declaration spells its own delimiters, so `` `old` `` matches no request and
//! reads as `NotFound` rather than as a half-renamed tree.

use std::collections::BTreeMap;

use super::kotlin::{kt_child_kind, kt_first_child, kt_parse, kt_text};
use super::KotlinSource;
use crate::rename_cx::{RenameCx, RenameRequest};
use crate::types::{RefRole, Rename, RenameStop, Respell, Span, SymbolRef, SymbolSeat};

impl Rename for KotlinSource {
    fn symbol_refs(
        &self,
        cx: &RenameCx,
        request: &RenameRequest,
    ) -> Result<Vec<SymbolRef>, RenameStop> {
        let corpus = Corpus::open(cx, &request.old);
        let anchor_scan = corpus
            .scans
            .get(&request.anchor)
            .ok_or_else(|| not_found(request))?;
        let declaration = match anchor_scan.decls.as_slice() {
            [] => return Err(not_found(request)),
            [one] => *one,
            many => {
                select_by_at(many, request.at).ok_or_else(|| ambiguous(request, many.to_vec()))?
            }
        };
        let package = anchor_scan.package.clone();
        let qualified = match package.is_empty() {
            true => request.old.clone(),
            false => format!("{package}.{}", request.old),
        };

        let mut refs = vec![seat(
            &request.anchor,
            declaration,
            RefRole::Definition,
            &request.old,
        )];
        let anchor = Anchor {
            rel: &request.anchor,
            package: &package,
            qualified: &qualified,
        };
        let mut stops: Vec<SymbolSeat> = Vec::new();
        for (rel, scan) in &corpus.scans {
            harvest(rel, scan, request, &anchor, &mut refs, &mut stops);
        }
        if !stops.is_empty() {
            return Err(RenameStop::Dynamic(stops));
        }
        Ok(settle(refs))
    }

    fn respell_symbol(
        &self,
        _cx: &RenameCx,
        request: &RenameRequest,
        reference: &SymbolRef,
    ) -> Option<Respell> {
        Some(Respell {
            file: reference.file.clone(),
            span: reference.span,
            text: request.new.clone(),
            receipt: None,
        })
    }

    fn text_spellings(&self, _cx: &RenameCx, request: &RenameRequest) -> Vec<(String, String)> {
        vec![(request.old.clone(), request.new.clone())]
    }
}

fn not_found(request: &RenameRequest) -> RenameStop {
    RenameStop::NotFound {
        anchor: request.anchor.clone(),
        old: request.old.clone(),
    }
}

fn ambiguous(request: &RenameRequest, sites: Vec<Span>) -> RenameStop {
    RenameStop::Ambiguous {
        anchor: request.anchor.clone(),
        old: request.old.clone(),
        sites,
    }
}

/// `--at` picks the declaration the offset lands in, else the nearest one
/// opening at or before it, the law `ts_rename.rs:111` states.
fn select_by_at(candidates: &[Span], at: Option<u32>) -> Option<Span> {
    let at = at?;
    let inside: Vec<Span> = candidates
        .iter()
        .copied()
        .filter(|span| span.start <= at && at < span.end())
        .collect();
    match inside.as_slice() {
        [one] => Some(*one),
        [] => candidates
            .iter()
            .copied()
            .filter(|span| span.start <= at)
            .max_by_key(|span| span.start),
        _ => None,
    }
}

/// One seat per `(file, offset)`, in plan order: an import clause's trailing
/// segment and a fully-qualified use can name the same token.
fn settle(mut refs: Vec<SymbolRef>) -> Vec<SymbolRef> {
    refs.sort_by(|left, right| {
        left.file
            .cmp(&right.file)
            .then(left.span.start.cmp(&right.span.start))
    });
    refs.dedup_by(|left, right| left.file == right.file && left.span.start == right.span.start);
    refs
}

fn seat(rel: &str, span: Span, role: RefRole, old: &str) -> SymbolRef {
    SymbolRef {
        file: rel.to_string(),
        span,
        role,
        text: old.to_string(),
    }
}

// ── the corpus view ─────────────────────────────────────────────────────────

/// The declaration every file in the corpus is measured against.
struct Anchor<'a> {
    rel: &'a str,
    /// The anchor file's own `package`, empty for the default package.
    package: &'a str,
    /// `package.old`, the spelling an import clause carries.
    qualified: &'a str,
}

/// Every Kotlin file that spells the old name, scanned once.
struct Corpus {
    scans: BTreeMap<String, FileScan>,
}

impl Corpus {
    fn open(cx: &RenameCx, old: &str) -> Self {
        let mut scans = BTreeMap::new();
        for rel in cx.files_of(&KotlinSource) {
            let Some(text) = cx.text(rel) else {
                continue;
            };
            // A file that never spells the name seats it nowhere, and every hop
            // that reaches it writes the name, so the filter drops no seat.
            if !text.contains(old) {
                continue;
            }
            let Some(scan) = scan_file(&text, old) else {
                continue;
            };
            scans.insert(rel.to_string(), scan);
        }
        Corpus { scans }
    }
}

/// One file's seats. A wildcard importer that writes the bare name is a stop; a
/// wildcard whose scope never writes it survives the rename untouched.
fn harvest(
    rel: &str,
    scan: &FileScan,
    request: &RenameRequest,
    anchor: &Anchor,
    refs: &mut Vec<SymbolRef>,
    stops: &mut Vec<SymbolSeat>,
) {
    let anchored = rel == anchor.rel;
    let shadowed = !anchored && !scan.decls.is_empty();
    let mut imported = false;
    let mut imported_bare = false;
    let mut wildcard = None;
    for row in &scan.imports {
        if row.wildcard {
            if !anchor.package.is_empty() && row.path == anchor.package {
                wildcard = Some(row.item);
            }
            continue;
        }
        if row.path != anchor.qualified {
            continue;
        }
        imported = true;
        imported_bare |= !row.aliased;
        refs.push(seat(rel, row.tail, RefRole::Import, &request.old));
    }

    let binds = !shadowed && (anchored || scan.package == anchor.package || imported_bare);
    let writes_bare = scan.idents.iter().any(|row| row.prefix.is_empty());
    match wildcard {
        Some(item) if !binds && !shadowed && !imported && writes_bare => stops.push(SymbolSeat {
            file: rel.to_string(),
            span: item,
            form: "wildcard import",
        }),
        _ => {}
    }
    for row in &scan.idents {
        let mine = match row.prefix.is_empty() {
            true => binds,
            false => !anchor.package.is_empty() && row.prefix == anchor.package,
        };
        if mine {
            refs.push(seat(rel, row.span, row.role, &request.old));
        }
    }
}

// ── the tree-sitter scan ────────────────────────────────────────────────────

/// One Kotlin file's old-name seats, off ONE `kt_parse`.
struct FileScan {
    /// The `package a.b` name, empty when the file declares none.
    package: String,
    imports: Vec<ImportRow>,
    /// The identifier span of every TOP-LEVEL declaration of the name.
    decls: Vec<Span>,
    idents: Vec<IdentSeat>,
}

/// One `import` header naming a path this rename may care about.
struct ImportRow {
    /// The dotted path as written, without the alias and without the `.*`.
    path: String,
    /// The trailing segment's own span; a wildcard's is its package's last one.
    tail: Span,
    /// The whole `import ...` header, which is what a wildcard stop reports.
    item: Span,
    wildcard: bool,
    aliased: bool,
}

/// One identifier token spelling the name, and the dotted path written before it.
struct IdentSeat {
    span: Span,
    /// Empty when the name is written bare.
    prefix: String,
    role: RefRole,
}

fn scan_file(text: &str, old: &str) -> Option<FileScan> {
    let tree = kt_parse(text)?;
    let root = tree.root_node();
    let src = text.as_bytes();
    let mut imports = Vec::new();
    walk_imports(root, src, &mut imports);
    let mut idents = Vec::new();
    walk_idents(root, src, old, &mut idents);
    Some(FileScan {
        package: package_name(root, src),
        imports,
        decls: top_level_decls(root, src, old),
        idents,
    })
}

fn span_of(node: tree_sitter::Node) -> Span {
    Span {
        start: node.start_byte() as u32,
        len: (node.end_byte() - node.start_byte()) as u32,
    }
}

fn children<'a>(node: tree_sitter::Node<'a>) -> Vec<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor).collect()
}

fn package_name(root: tree_sitter::Node, src: &[u8]) -> String {
    kt_child_kind(root, "package_header")
        .and_then(|header| kt_child_kind(header, "identifier"))
        .map(|identifier| kt_text(identifier, src).to_string())
        .unwrap_or_default()
}

fn walk_imports(node: tree_sitter::Node, src: &[u8], out: &mut Vec<ImportRow>) {
    if node.kind() == "import_header" {
        if let Some(row) = import_row(node, src) {
            out.push(row);
        }
        return;
    }
    for child in children(node) {
        walk_imports(child, src, out);
    }
}

fn import_row(header: tree_sitter::Node, src: &[u8]) -> Option<ImportRow> {
    let identifier = kt_child_kind(header, "identifier")?;
    let tail = children(identifier)
        .into_iter()
        .rfind(|child| child.kind() == "simple_identifier")?;
    Some(ImportRow {
        path: kt_text(identifier, src).to_string(),
        tail: span_of(tail),
        item: span_of(header),
        wildcard: kt_child_kind(header, "wildcard_import").is_some(),
        aliased: kt_child_kind(header, "import_alias").is_some(),
    })
}

/// Every name an importer can spell after the package: the DIRECT children of
/// the file, since a nested declaration is reached through its owner's name.
fn top_level_decls(root: tree_sitter::Node, src: &[u8], old: &str) -> Vec<Span> {
    children(root)
        .into_iter()
        .filter_map(decl_ident)
        .filter(|ident| kt_text(*ident, src) == old)
        .map(span_of)
        .collect()
}

fn decl_ident(node: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    match node.kind() {
        "class_declaration" | "object_declaration" | "type_alias" => {
            kt_first_child(node, "type_identifier")
        }
        "function_declaration" => kt_first_child(node, "simple_identifier"),
        "property_declaration" => kt_first_child(
            kt_first_child(node, "variable_declaration")?,
            "simple_identifier",
        ),
        _ => None,
    }
}

fn walk_idents(node: tree_sitter::Node, src: &[u8], old: &str, out: &mut Vec<IdentSeat>) {
    match node.kind() {
        // The package and import headers own their own spans, and an annotation's
        // arguments are metadata keyed by name rather than a bound reference.
        "package_header" | "import_header" | "annotation" => return,
        "simple_identifier" | "type_identifier" => {
            if kt_text(node, src) == old {
                if let Some(row) = ident_seat(node, src) {
                    out.push(row);
                }
            }
            return;
        }
        _ => {}
    }
    for child in children(node) {
        walk_idents(child, src, old, out);
    }
}

fn ident_seat(node: tree_sitter::Node, src: &[u8]) -> Option<IdentSeat> {
    let parent = node.parent()?;
    if binding_name(parent.kind()) {
        return None;
    }
    let role = match node.kind() {
        "type_identifier" => RefRole::TypeRef,
        _ => RefRole::Read,
    };
    let span = span_of(node);
    match parent.kind() {
        "user_type" => {
            let segments: Vec<tree_sitter::Node> = children(parent)
                .into_iter()
                .filter(|child| child.kind() == "type_identifier")
                .collect();
            let index = segments.iter().position(|child| child.id() == node.id())?;
            // Only the LAST segment names the symbol; the ones before it spell a
            // package or an outer type.
            if index + 1 != segments.len() {
                return None;
            }
            Some(IdentSeat {
                span,
                prefix: dotted(&segments[..index], src),
                role,
            })
        }
        "navigation_suffix" => {
            let navigation = parent.parent()?;
            let prefix = dotted_receiver(navigation.child(0)?, src)?;
            Some(IdentSeat { span, prefix, role })
        }
        _ => Some(IdentSeat {
            span,
            prefix: String::new(),
            role,
        }),
    }
}

/// The declaration kinds whose direct identifier child NAMES that declaration.
/// Such a token binds its own scope; it never references the anchor.
fn binding_name(kind: &str) -> bool {
    matches!(
        kind,
        "class_declaration"
            | "object_declaration"
            | "companion_object"
            | "type_alias"
            | "function_declaration"
            | "variable_declaration"
            | "parameter"
            | "class_parameter"
            | "enum_entry"
            | "type_parameter"
            | "import_alias"
    )
}

fn dotted(segments: &[tree_sitter::Node], src: &[u8]) -> String {
    segments
        .iter()
        .map(|segment| kt_text(*segment, src))
        .collect::<Vec<&str>>()
        .join(".")
}

/// The receiver of a navigation as a dotted path, or None when it is an
/// expression rather than a package prefix.
fn dotted_receiver(node: tree_sitter::Node, src: &[u8]) -> Option<String> {
    match node.kind() {
        "simple_identifier" => Some(kt_text(node, src).to_string()),
        "navigation_expression" => {
            let head = dotted_receiver(node.child(0)?, src)?;
            let suffix = kt_child_kind(node, "navigation_suffix")?;
            let leaf = kt_child_kind(suffix, "simple_identifier")?;
            Some(format!("{head}.{}", kt_text(leaf, src)))
        }
        _ => None,
    }
}
