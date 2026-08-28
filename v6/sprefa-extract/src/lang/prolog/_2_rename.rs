//! `impl Rename for PrologSource`: every question `extract rename` asks a
//! language, answered for Prolog over the DataGrout tree-sitter grammar
//! `_0_source.rs` already parses with. No new crate.
//! @comment-ok: module header, the seam list every lang file opens with
//!
//! A Prolog symbol is a predicate NAME AT A FIXED ARITY, `helper/2`, distinct
//! from `helper/3` and from the DCG non-terminal `helper//2`. The anchor's own
//! declarations pick the arity; two arities in one anchor need `--at`. A
//! variable is clause-local and heads no clause, so a request naming one lands
//! on `NotFound`.
//!
//! Spans are the functor atom's own node, not the whole term: `_0_source.rs:182`
//! spans a clause and `:665` a goal, and a rename needs the identifier token.
//! The module-file law (a spec resolves against the loading file's directory and
//! takes `.pl` when bare) is restated here rather than shared: the same law sits
//! in `_1_rehome.rs:368` behind a `MoveCx`, and a rename carries a `RenameCx`.
//!
//! Four forms are reported and never rewritten, because rewriting one guesses:
//! `Term =.. List`, a `call`/`apply`/`maplist` whose closure is a variable or a
//! partial application, an atom built by `atom_concat`, and the name written in
//! a DATA position, where `assertz(helper(A, B))` and a plain term of the same
//! name are indistinguishable. A quoted spelling (`'helper'(A, B)`) is an
//! `Inexact` span: its bytes carry quotes the identifier does not.
//!
//! Two limits this arm states rather than hides. An `include(File)` splices text
//! into the including module and its goals are scanned as that file's own, which
//! can only drop a seat. A goal whose module qualifier is written but whose
//! module is not the anchor's is not a seat, so a qualifier bound at load time
//! by `module_transparent` is missed.

use std::collections::{BTreeMap, BTreeSet};

use super::PrologSource;
use crate::move_cx::{dirname, join_rel, stem};
use crate::rename_cx::{RenameCx, RenameRequest};
use crate::types::{RefRole, Rename, RenameStop, Respell, Span, SymbolRef, SymbolSeat};

impl Rename for PrologSource {
    fn symbol_refs(
        &self,
        cx: &RenameCx,
        request: &RenameRequest,
    ) -> Result<Vec<SymbolRef>, RenameStop> {
        let corpus = Corpus::open(cx, request);
        let anchor = corpus
            .scans
            .get(&request.anchor)
            .ok_or_else(|| not_found(request))?;
        let key = pick_key(anchor, request)?;
        let qualifier = anchor
            .module
            .clone()
            .unwrap_or_else(|| stem(&request.anchor));
        let exporting = corpus.exporting(&request.anchor, key);
        let seeing = corpus.seeing(&request.anchor, &exporting, key);

        let mut refs: Vec<SymbolRef> = Vec::new();
        let mut seats: Vec<SymbolSeat> = Vec::new();
        for (rel, scan) in &corpus.scans {
            let anchored = rel == &request.anchor;
            let visible = anchored || seeing.contains(rel);
            corpus.harvest(
                rel, scan, key, anchored, visible, &qualifier, &exporting, request, &mut refs,
                &mut seats,
            );
        }
        if let Some(stop) = corpus.inexact(&request.anchor, &refs) {
            return Err(stop);
        }
        if !seats.is_empty() {
            seats.sort_by(|left, right| {
                left.file
                    .cmp(&right.file)
                    .then(left.span.start.cmp(&right.span.start))
            });
            seats.dedup_by(|left, right| left.file == right.file && left.span == right.span);
            return Err(RenameStop::Dynamic(seats));
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

    /// The name inside a comment, a string, or a `format/2` template is text the
    /// scope plane never bound, so it rides the report and not the plan.
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

/// The one symbol the anchor names: its single declared arity, or the one `--at`
/// picks out of several. Two arities with no offset name two symbols.
fn pick_key(anchor: &FileScan, request: &RenameRequest) -> Result<PredKey, RenameStop> {
    let mut keys: Vec<PredKey> = anchor.decls.iter().map(|decl| decl.key).collect();
    keys.sort();
    keys.dedup();
    match keys.as_slice() {
        [] => Err(not_found(request)),
        [one] => Ok(*one),
        _ => match select_by_at(&anchor.decls, request.at) {
            Some(decl) => Ok(decl.key),
            None => {
                let mut seen: BTreeSet<PredKey> = BTreeSet::new();
                let mut sites: Vec<Span> = anchor
                    .decls
                    .iter()
                    .filter(|decl| seen.insert(decl.key))
                    .map(|decl| decl.span)
                    .collect();
                sites.sort_by_key(|site| site.start);
                Err(RenameStop::Ambiguous {
                    anchor: request.anchor.clone(),
                    old: request.old.clone(),
                    sites,
                })
            }
        },
    }
}

/// `--at` picks the declaration its offset lands in, else the nearest one
/// opening at or before it (`ts_rename.rs:111`); a head's span is its clause.
fn select_by_at(candidates: &[Decl], at: Option<u32>) -> Option<&Decl> {
    let at = at?;
    let inside: Vec<&Decl> = candidates
        .iter()
        .filter(|decl| decl.outer.start <= at && at < decl.outer.end())
        .collect();
    let keys: BTreeSet<PredKey> = inside.iter().map(|decl| decl.key).collect();
    match keys.len() {
        1 => inside.first().copied(),
        0 => candidates
            .iter()
            .filter(|decl| decl.outer.start <= at)
            .max_by_key(|decl| decl.outer.start),
        _ => None,
    }
}

/// One seat per `(file, offset)`, in plan order: a head's own functor and a
/// declaration directive can name the same token in a one-clause file.
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

/// Every Prolog file that can reach the symbol, scanned once: the files
/// spelling the name or the anchor's stem, plus what their loads name.
struct Corpus {
    scans: BTreeMap<String, FileScan>,
    files: BTreeSet<String>,
}

impl Corpus {
    fn open(cx: &RenameCx, request: &RenameRequest) -> Self {
        let files: BTreeSet<String> = cx
            .files_of(&PrologSource)
            .into_iter()
            .map(str::to_string)
            .collect();
        let anchor_stem = stem(&request.anchor);
        let mut pending: Vec<String> = files
            .iter()
            .filter(|rel| *rel == &request.anchor || spells(cx, rel, &request.old, &anchor_stem))
            .cloned()
            .collect();
        let mut scans: BTreeMap<String, FileScan> = BTreeMap::new();
        while let Some(rel) = pending.pop() {
            if scans.contains_key(&rel) {
                continue;
            }
            let Some(text) = cx.text(&rel) else {
                continue;
            };
            let Some(tree) = parse(&text) else {
                continue;
            };
            let scan = scan_file(tree.root_node(), text.as_bytes(), &request.old);
            for load in &scan.loads {
                if let Some(target) = resolve_spec(&files, dirname(&rel), &load.spec) {
                    if !scans.contains_key(&target) {
                        pending.push(target);
                    }
                }
            }
            scans.insert(rel, scan);
        }
        Corpus { scans, files }
    }

    fn resolve(&self, rel: &str, spec: &str) -> Option<String> {
        resolve_spec(&self.files, dirname(rel), spec)
    }

    /// Every file whose module interface carries the symbol: the anchor when it
    /// exports it, plus a hop per `reexport` that carries it on, to a fixpoint.
    fn exporting(&self, anchor: &str, key: PredKey) -> BTreeSet<String> {
        let mut set = BTreeSet::new();
        if self.scans.get(anchor).is_some_and(|scan| scan.exports(key)) {
            set.insert(anchor.to_string());
        }
        loop {
            let mut grew = false;
            for (rel, scan) in &self.scans {
                if set.contains(rel) {
                    continue;
                }
                for load in &scan.loads {
                    if !load.reexports {
                        continue;
                    }
                    let Some(target) = self.resolve(rel, &load.spec) else {
                        continue;
                    };
                    if set.contains(&target) && load.names(key) {
                        grew |= set.insert(rel.clone());
                    }
                }
            }
            if !grew {
                return set;
            }
        }
    }

    /// Every file whose bare goals can name the symbol: the anchor, plus each
    /// file loading a carrying interface under an import list that admits it.
    fn seeing(&self, anchor: &str, exporting: &BTreeSet<String>, key: PredKey) -> BTreeSet<String> {
        let mut set = BTreeSet::new();
        set.insert(anchor.to_string());
        for (rel, scan) in &self.scans {
            for load in &scan.loads {
                let Some(target) = self.resolve(rel, &load.spec) else {
                    continue;
                };
                if exporting.contains(&target) && load.names(key) {
                    set.insert(rel.clone());
                }
            }
        }
        set
    }

    /// One file's seats. `anchored` admits declarations, `visible` bare goals;
    /// a qualified goal needs neither, only the anchor's own module name.
    #[allow(clippy::too_many_arguments)]
    fn harvest(
        &self,
        rel: &str,
        scan: &FileScan,
        key: PredKey,
        anchored: bool,
        visible: bool,
        qualifier: &str,
        exporting: &BTreeSet<String>,
        request: &RenameRequest,
        refs: &mut Vec<SymbolRef>,
        seats: &mut Vec<SymbolSeat>,
    ) {
        if anchored {
            for decl in &scan.decls {
                if decl.key == key {
                    refs.push(seat(rel, decl.span, decl.role, &request.old));
                }
            }
        }
        for load in &scan.loads {
            let Some(target) = self.resolve(rel, &load.spec) else {
                continue;
            };
            if !exporting.contains(&target) {
                continue;
            }
            for indicator in load.list.iter().flatten() {
                if indicator.key == key {
                    refs.push(seat(rel, indicator.span, RefRole::Import, &request.old));
                }
            }
        }
        for goal in &scan.goals {
            if goal.key != key {
                continue;
            }
            let admitted = match &goal.qualifier {
                Some(name) => name == qualifier,
                None => visible,
            };
            if admitted {
                refs.push(seat(rel, goal.span, RefRole::Read, &request.old));
            }
        }
        if visible {
            for (span, form) in &scan.seats {
                seats.push(SymbolSeat {
                    file: rel.to_string(),
                    span: *span,
                    form,
                });
            }
        }
    }

    /// The first quoted spelling in a touched file, the anchor always counted:
    /// one such span makes every span in that file suspect, so the run stops.
    fn inexact(&self, anchor: &str, refs: &[SymbolRef]) -> Option<RenameStop> {
        let mut touched: BTreeSet<&str> = refs
            .iter()
            .map(|reference| reference.file.as_str())
            .collect();
        touched.insert(anchor);
        touched
            .into_iter()
            .find_map(|rel| Some((rel, *self.scans.get(rel)?.inexact.first()?)))
            .map(|(rel, span)| RenameStop::Inexact {
                file: rel.to_string(),
                span,
                why: "a quoted atom spells the name with bytes the identifier does not carry",
            })
    }
}

/// Whether a file can hold a seat: it writes the name, or the anchor's stem,
/// which every spec naming the anchor carries verbatim (`_1_rehome.rs:218`).
fn spells(cx: &RenameCx, rel: &str, old: &str, anchor_stem: &str) -> bool {
    cx.read(rel).is_some_and(|bytes| {
        memchr::memmem::find(&bytes, old.as_bytes()).is_some()
            || memchr::memmem::find(&bytes, anchor_stem.as_bytes()).is_some()
    })
}

/// A file spec resolves against the loading file's directory and takes `.pl`
/// when bare; `library(...)` and every other alias term names no corpus file.
fn resolve_spec(files: &BTreeSet<String>, dir: &str, raw: &str) -> Option<String> {
    let bare = unquote(raw);
    if bare.is_empty() || bare.contains('(') || bare.starts_with('/') {
        return None;
    }
    let joined = join_rel(dir, bare);
    if files.contains(&joined) {
        return Some(joined);
    }
    let with_extension = format!("{joined}.pl");
    files.contains(&with_extension).then_some(with_extension)
}

fn unquote(raw: &str) -> &str {
    let bytes = raw.as_bytes();
    let quoted = bytes.len() >= 2
        && (bytes[0] == b'\'' || bytes[0] == b'"')
        && bytes[bytes.len() - 1] == bytes[0];
    match quoted {
        true => &raw[1..raw.len() - 1],
        false => raw,
    }
}

// ── one file's seats ────────────────────────────────────────────────────────

/// A predicate identity. `helper/2`, `helper/3` and the DCG non-terminal
/// `helper//2` are three symbols wearing one name.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct PredKey {
    arity: usize,
    dcg: bool,
}

/// One construct declaring the name: a clause head, an export-list entry, or a
/// `dynamic`/`discontiguous`/`multifile`/`table`/`meta_predicate` directive.
struct Decl {
    key: PredKey,
    /// The functor atom's own span, which is what a respell replaces.
    span: Span,
    /// The clause or directive holding it, which is what `--at` lands in.
    outer: Span,
    role: RefRole,
}

/// One `Name/Arity` indicator naming the old name, and its own atom's span.
struct Indicator {
    key: PredKey,
    span: Span,
}

/// One load directive. `reexports` separates `reexport`, which carries the
/// interface on, from `use_module`/`ensure_loaded`/`consult`, which do not.
struct Load {
    spec: String,
    reexports: bool,
    /// `None` = the whole interface. `Some` = the indicators naming the old name
    /// in an import list that was written out.
    list: Option<Vec<Indicator>>,
}

impl Load {
    fn names(&self, key: PredKey) -> bool {
        match &self.list {
            None => true,
            Some(list) => list.iter().any(|indicator| indicator.key == key),
        }
    }
}

/// One goal seat: the functor's span, the identity it calls, and the module it
/// was qualified with, if any.
struct Goal {
    key: PredKey,
    span: Span,
    qualifier: Option<String>,
}

#[derive(Default)]
struct FileScan {
    /// `:- module(Name, _)`, the name a qualified goal writes.
    module: Option<String>,
    decls: Vec<Decl>,
    loads: Vec<Load>,
    goals: Vec<Goal>,
    /// Runtime and data forms reported and never rewritten.
    seats: Vec<(Span, &'static str)>,
    inexact: Vec<Span>,
}

impl FileScan {
    /// Whether this file's interface carries the symbol. A file with no module
    /// directive declares no interface, so everything in it is visible.
    fn exports(&self, key: PredKey) -> bool {
        self.module.is_none()
            || self
                .decls
                .iter()
                .any(|decl| decl.role == RefRole::Export && decl.key == key)
    }
}

// ── the tree-sitter scan ────────────────────────────────────────────────────

fn parse(content: &str) -> Option<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    let language = tree_sitter::Language::new(tree_sitter_prolog::LANGUAGE);
    parser.set_language(&language).ok()?;
    parser.parse(content, None)
}

fn span(node: tree_sitter::Node) -> Span {
    Span {
        start: node.start_byte() as u32,
        len: (node.end_byte() - node.start_byte()) as u32,
    }
}

fn text<'a>(node: tree_sitter::Node, src: &'a [u8]) -> &'a str {
    node.utf8_text(src).unwrap_or("")
}

fn field<'a>(node: tree_sitter::Node<'a>, name: &str) -> Option<tree_sitter::Node<'a>> {
    node.child_by_field_name(name)
}

fn operator<'a>(node: tree_sitter::Node, src: &'a [u8]) -> &'a str {
    field(node, "operator")
        .map(|op| text(op, src))
        .unwrap_or("")
}

fn named_children(node: tree_sitter::Node) -> Vec<tree_sitter::Node> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

fn arguments(node: tree_sitter::Node) -> Vec<tree_sitter::Node> {
    let mut cursor = node.walk();
    node.children_by_field_name("argument", &mut cursor)
        .collect()
}

/// The atom as Prolog reads it: a quoted atom's quotes are syntax, and `''` is
/// one quote (`_0_source.rs:83`).
fn atom_text(node: tree_sitter::Node, src: &[u8]) -> String {
    let raw = text(node, src);
    match raw.len() >= 2 && raw.starts_with('\'') && raw.ends_with('\'') {
        true => raw[1..raw.len() - 1].replace("''", "'"),
        false => raw.to_string(),
    }
}

fn clause_term(clause: tree_sitter::Node) -> Option<tree_sitter::Node> {
    field(clause, "term").or_else(|| clause.named_child(0))
}

fn strip_annotation<'a>(mut node: tree_sitter::Node<'a>, src: &[u8]) -> tree_sitter::Node<'a> {
    while node.kind() == "binary_operation" && operator(node, src) == "::" {
        node = field(node, "right").unwrap_or(node);
    }
    node
}

/// The head, the body, and whether the clause is a DCG rule (`_0_source.rs:100`).
/// `None` is a directive, which carries no head.
fn head_body<'a>(
    clause: tree_sitter::Node<'a>,
    src: &[u8],
) -> Option<(tree_sitter::Node<'a>, Option<tree_sitter::Node<'a>>, bool)> {
    let term = clause_term(clause)?;
    if term.kind() == "unary_operation" {
        return None;
    }
    if term.kind() == "binary_operation" {
        match operator(term, src) {
            ":-" => return Some((field(term, "left")?, field(term, "right"), false)),
            "-->" => return Some((field(term, "left")?, field(term, "right"), true)),
            _ => {}
        }
    }
    Some((strip_annotation(term, src), None, false))
}

const ATOM_KINDS: [&str; 4] = ["atom", "unquoted_atom", "quoted_atom", "operator_atom"];

/// The directives that declare a predicate without giving it a clause.
const DECLARATION_DIRECTIVES: [&str; 5] = [
    "dynamic",
    "discontiguous",
    "multifile",
    "table",
    "meta_predicate",
];

fn scan_file(root: tree_sitter::Node, src: &[u8], old: &str) -> FileScan {
    let mut scan = Scan {
        old,
        src,
        spelled: false,
        pending: Vec::new(),
        out: FileScan::default(),
    };
    for clause in named_children(root) {
        if clause.kind() != "clause" {
            continue;
        }
        scan.spelled = false;
        scan.pending.clear();
        match head_body(clause, src) {
            Some((head, body, dcg)) => {
                scan.clause_head(head, span(clause), dcg);
                if let Some(body) = body {
                    scan.walk_goals(body, dcg, None);
                }
            }
            None => scan.directive(clause),
        }
        // A metacall only reaches THIS symbol when its clause writes the name;
        // every clause in the corpus would stop the run otherwise.
        if scan.spelled {
            let pending = std::mem::take(&mut scan.pending);
            scan.out.seats.extend(pending);
        }
    }
    scan.out
}

struct Scan<'a> {
    old: &'a str,
    src: &'a [u8],
    /// Whether the clause under the walk writes the name anywhere.
    spelled: bool,
    /// The clause's metacall seats, kept only when it writes the name.
    pending: Vec<(Span, &'static str)>,
    out: FileScan,
}

impl Scan<'_> {
    /// A span that reads back as the old name, else a recorded `inexact`: a
    /// quoted atom spells the name with bytes a bare identifier does not carry.
    fn exact(&mut self, node: tree_sitter::Node) -> Option<Span> {
        let span = span(node);
        let start = span.start as usize;
        match std::str::from_utf8(self.src.get(start..start + span.len as usize)?) {
            Ok(text) if text == self.old => Some(span),
            _ => {
                self.out.inexact.push(span);
                None
            }
        }
    }

    /// The name and arity a callable term writes, and the node spelling the name
    /// (`_0_source.rs:66`).
    fn callable<'t>(
        &self,
        node: tree_sitter::Node<'t>,
    ) -> Option<(tree_sitter::Node<'t>, String, usize)> {
        let node = strip_annotation(node, self.src);
        match node.kind() {
            "compound_term" => {
                let functor = field(node, "functor")?;
                Some((functor, atom_text(functor, self.src), arguments(node).len()))
            }
            kind if ATOM_KINDS.contains(&kind) => Some((node, atom_text(node, self.src), 0)),
            _ => None,
        }
    }

    fn clause_head(&mut self, head: tree_sitter::Node, outer: Span, dcg: bool) {
        let Some((functor, name, arity)) = self.callable(head) else {
            return;
        };
        if name == self.old {
            self.spelled = true;
            if let Some(span) = self.exact(functor) {
                self.out.decls.push(Decl {
                    key: PredKey { arity, dcg },
                    span,
                    outer,
                    role: RefRole::Definition,
                });
            }
        }
        for argument in arguments(head) {
            self.walk_data(argument);
        }
    }

    // ── directives ──────────────────────────────────────────────────────────

    fn directive(&mut self, clause: tree_sitter::Node) {
        let Some(term) = clause_term(clause) else {
            return;
        };
        if term.kind() != "unary_operation" || operator(term, self.src) != ":-" {
            return;
        }
        let Some(operand) = field(term, "operand") else {
            return;
        };
        let outer = span(clause);
        // `:- dynamic f/2.` is a prefix operator and `:- dynamic(f/2).` a
        // compound; both spell the same declaration.
        if operand.kind() == "unary_operation" {
            let name = operator(operand, self.src).to_string();
            if DECLARATION_DIRECTIVES.contains(&name.as_str()) {
                if let Some(inner) = field(operand, "operand") {
                    self.declare(inner, outer);
                }
                return;
            }
            self.walk_goals(operand, false, None);
            return;
        }
        let Some((_, name, _)) = self.callable(operand) else {
            self.walk_goals(operand, false, None);
            return;
        };
        let args = arguments(operand);
        match name.as_str() {
            "module" => self.module_declaration(&args, outer),
            "use_module" | "ensure_loaded" | "consult" => self.load(&args, false),
            "reexport" => self.load(&args, true),
            name if DECLARATION_DIRECTIVES.contains(&name) => {
                for argument in args {
                    self.declare(argument, outer);
                }
            }
            // `include(File)` splices text; the included file is scanned as
            // itself, so nothing is claimed here.
            "include" => {}
            _ => self.walk_goals(operand, false, None),
        }
    }

    /// A declaration directive's operand: `Name/Arity` indicators, plus the
    /// `meta_predicate` compound form, which spells the arity by its arguments.
    fn declare(&mut self, node: tree_sitter::Node, outer: Span) {
        if node.kind() == "compound_term" {
            if let Some((functor, name, arity)) = self.callable(node) {
                if name == self.old {
                    self.spelled = true;
                    if let Some(span) = self.exact(functor) {
                        self.out.decls.push(Decl {
                            key: PredKey { arity, dcg: false },
                            span,
                            outer,
                            role: RefRole::Definition,
                        });
                    }
                    return;
                }
            }
        }
        for indicator in self.indicators(node) {
            self.out.decls.push(Decl {
                key: indicator.key,
                span: indicator.span,
                outer,
                role: RefRole::Definition,
            });
        }
    }

    fn module_declaration(&mut self, args: &[tree_sitter::Node], outer: Span) {
        let [name, exports] = args else {
            return;
        };
        self.out.module = Some(atom_text(*name, self.src));
        for indicator in self.indicators(*exports) {
            self.out.decls.push(Decl {
                key: indicator.key,
                span: indicator.span,
                outer,
                role: RefRole::Export,
            });
        }
    }

    fn load(&mut self, args: &[tree_sitter::Node], reexports: bool) {
        let Some(source) = args.first() else {
            return;
        };
        let list = args.get(1).map(|list| self.indicators(*list));
        self.out.loads.push(Load {
            spec: text(*source, self.src).to_string(),
            reexports,
            list,
        });
    }

    /// Every `Name/Arity` and `Name//Arity` indicator naming the old name
    /// (`_0_source.rs:533`); `op(P, T, N)` declares none.
    fn indicators(&mut self, node: tree_sitter::Node) -> Vec<Indicator> {
        let mut out = Vec::new();
        self.collect_indicators(node, &mut out);
        out
    }

    fn collect_indicators(&mut self, node: tree_sitter::Node, out: &mut Vec<Indicator>) {
        if node.kind() == "binary_operation" {
            let symbol = operator(node, self.src);
            if symbol == "/" || symbol == "//" {
                let (Some(name), Some(arity)) = (field(node, "left"), field(node, "right")) else {
                    return;
                };
                let Ok(arity) = text(arity, self.src).trim().parse::<usize>() else {
                    return;
                };
                if atom_text(name, self.src) != self.old {
                    return;
                }
                self.spelled = true;
                if let Some(span) = self.exact(name) {
                    out.push(Indicator {
                        key: PredKey {
                            arity,
                            dcg: symbol == "//",
                        },
                        span,
                    });
                }
                return;
            }
        }
        for child in named_children(node) {
            self.collect_indicators(child, out);
        }
    }

    // ── goals ───────────────────────────────────────────────────────────────

    fn walk_goals(&mut self, node: tree_sitter::Node, dcg: bool, qualifier: Option<&str>) {
        match node.kind() {
            "parenthesized" | "curly_block" => {
                for child in named_children(node) {
                    self.walk_goals(child, dcg, qualifier);
                }
            }
            "unary_operation" => {
                let symbol = operator(node, self.src).to_string();
                let Some(operand) = field(node, "operand") else {
                    return;
                };
                match symbol.as_str() {
                    "\\+" => self.walk_goals(operand, dcg, qualifier),
                    name if DECLARATION_DIRECTIVES.contains(&name) => {
                        self.declare(operand, span(node))
                    }
                    _ => self.walk_data(operand),
                }
            }
            "binary_operation" => self.binary_goal(node, dcg, qualifier),
            "compound_term" => self.compound_goal(node, dcg, qualifier),
            kind if ATOM_KINDS.contains(&kind) => {
                if atom_text(node, self.src) == self.old {
                    self.goal_seat(node, PredKey { arity: 0, dcg }, qualifier);
                }
            }
            "cut" => {}
            _ => self.walk_data(node),
        }
    }

    fn binary_goal(&mut self, node: tree_sitter::Node, dcg: bool, qualifier: Option<&str>) {
        let symbol = operator(node, self.src).to_string();
        match symbol.as_str() {
            "," | ";" | "|" | "->" | "*->" => {
                for side in ["left", "right"] {
                    if let Some(child) = field(node, side) {
                        self.walk_goals(child, dcg, qualifier);
                    }
                }
            }
            ":" => {
                let module = field(node, "left").map(|left| atom_text(left, self.src));
                if let Some(right) = field(node, "right") {
                    self.walk_goals(right, dcg, module.as_deref());
                }
            }
            ":-" | "-->" | "::" => {}
            "=.." => {
                self.pending.push((span(node), "=.. builds the goal"));
                for child in named_children(node) {
                    self.walk_data(child);
                }
            }
            _ => {
                for child in named_children(node) {
                    self.walk_data(child);
                }
            }
        }
    }

    fn compound_goal(&mut self, node: tree_sitter::Node, dcg: bool, qualifier: Option<&str>) {
        let Some((functor, name, arity)) = self.callable(node) else {
            return;
        };
        if name == self.old {
            self.goal_seat(functor, PredKey { arity, dcg }, qualifier);
        }
        let args = arguments(node);
        let closure = matches!(name.as_str(), "call" | "apply" | "maplist") && arity >= 1;
        if closure {
            self.closure(node, &args, arity);
        }
        // The goal an atom_concat builds is spelled at runtime, not here.
        if matches!(name.as_str(), "atom_concat" | "atomic_list_concat") {
            self.pending.push((span(node), "a built atom"));
        }
        let rest = match closure {
            true => &args[1..],
            false => &args[..],
        };
        for argument in rest {
            self.walk_data(*argument);
        }
    }

    /// A closure argument: a bare atom names the predicate at the arity the
    /// remaining arguments give it, anything else is spelled at runtime.
    fn closure(&mut self, node: tree_sitter::Node, args: &[tree_sitter::Node], arity: usize) {
        let Some(first) = args.first() else {
            return;
        };
        if first.kind() == "variable" {
            self.pending.push((span(node), "a variable goal"));
            return;
        }
        let Some((functor, name, carried)) = self.callable(*first) else {
            return;
        };
        if name != self.old {
            return;
        }
        if carried > 0 {
            self.spelled = true;
            self.pending.push((span(node), "a partial application"));
            return;
        }
        self.goal_seat(
            functor,
            PredKey {
                arity: arity - 1,
                dcg: false,
            },
            None,
        );
    }

    fn goal_seat(&mut self, functor: tree_sitter::Node, key: PredKey, qualifier: Option<&str>) {
        self.spelled = true;
        let Some(span) = self.exact(functor) else {
            return;
        };
        self.out.goals.push(Goal {
            key,
            span,
            qualifier: qualifier.map(str::to_string),
        });
    }

    // ── data ────────────────────────────────────────────────────────────────

    /// A term in an argument, not a goal: `assertz(helper(A, B))` and a plain
    /// term of that name read alike here, so the name is reported, never moved.
    fn walk_data(&mut self, node: tree_sitter::Node) {
        match node.kind() {
            "compound_term" => {
                if let Some((functor, name, _)) = self.callable(node) {
                    if name == self.old {
                        self.spelled = true;
                        self.out.seats.push((span(functor), "a term argument"));
                    }
                }
                for argument in arguments(node) {
                    self.walk_data(argument);
                }
            }
            kind if ATOM_KINDS.contains(&kind) => {
                if atom_text(node, self.src) == self.old {
                    self.spelled = true;
                    self.out.seats.push((span(node), "a term argument"));
                }
            }
            "variable" | "number" | "string" | "back_quoted_string" => {}
            _ => {
                for child in named_children(node) {
                    self.walk_data(child);
                }
            }
        }
    }
}
