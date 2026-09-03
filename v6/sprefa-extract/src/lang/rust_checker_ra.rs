//! The checker tier's loader: `cargo metadata` into a salsa db, then
//! rust-analyzer's own resolution over every supplied file. Seam: `rust_checker`.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ra_ap_hir::{
    Adt, AssocItem, Crate, Field, Function, GenericDef, HirDisplay, Impl, ModuleDef, PathResolution,
    Semantics, Trait, Type, attach_db,
};
use ra_ap_ide::{AnalysisHost, NavigationTarget, RootDatabase, TryToNav};
use ra_ap_load_cargo::{LoadCargoConfig, ProcMacroServerChoice, load_workspace_at};
use ra_ap_project_model::{CargoConfig, CargoFeatures, RustLibSource};
use ra_ap_syntax::{AstNode, ast};
use tracing::Span;

use super::rust_checker::{CheckerAnswers, CheckerError, CheckerRef, OffsetMap};
use crate::trace::{phase_span, record_phase, Phase};
use crate::tsi::{Arg, CoverageClaim, FactOut};

/// One corpus file the walk visits: its supplied path, its ra file id, its
/// text and the byte -> parse-plane offset map over that text.
struct WalkFile {
    path: String,
    file_id: ra_ap_ide::FileId,
    text: String,
    offsets: OffsetMap,
}

pub fn answer(
    root: &Path,
    files: &[(String, PathBuf)],
    budget: Duration,
    tsi: bool,
) -> Result<CheckerAnswers, CheckerError> {
    let load_config = LoadCargoConfig {
        load_out_dirs_from_check: false,
        with_proc_macro_server: ProcMacroServerChoice::None,
        prefill_caches: false,
        num_worker_threads: 4,
        proc_macro_processes: 0,
    };
    // A crate graph with no sysroot declines every method whose receiver type
    // flows through std; `set_test` puts `#[cfg(test)]` bodies in the tree.
    let cargo_config = CargoConfig {
        sysroot: Some(RustLibSource::Discover),
        set_test: true,
        // The default selects no feature, so a `cfg`-gated module stays out of
        // the crate graph and every file it declares owns no module there.
        features: CargoFeatures::All,
        ..CargoConfig::default()
    };
    let started = Instant::now();
    let (db, vfs, _proc_macro) = load_workspace_at(root, &cargo_config, &load_config, &|_| {})
        .map_err(|err| CheckerError::NoWorkspace(err.to_string()))?;
    let load = started.elapsed();
    if load > budget {
        return Err(CheckerError::Budget(budget));
    }

    let wanted: HashMap<PathBuf, &str> = files
        .iter()
        .map(|(supplied, absolute)| {
            let key = std::fs::canonicalize(absolute).unwrap_or_else(|_| absolute.clone());
            (key, supplied.as_str())
        })
        .collect();
    let mut by_file_id: HashMap<ra_ap_ide::FileId, &str> = HashMap::new();
    // One string per workspace file, so it is built only for a run whose
    // envelope reads it: a leaf type's origin names the file declaring it.
    let mut path_of: HashMap<ra_ap_ide::FileId, String> = HashMap::new();
    for (vfs_id, vfs_path) in vfs.iter() {
        let Some(absolute) = vfs_path.as_path() else { continue };
        let text = absolute.to_string();
        let key = std::fs::canonicalize(&text).unwrap_or_else(|_| PathBuf::from(&text));
        let file_id = ra_ap_ide::FileId::from_raw(vfs_id.index());
        if let Some(supplied) = wanted.get(&key) {
            by_file_id.insert(file_id, supplied);
        }
        if tsi {
            path_of.insert(file_id, text);
        }
    }

    let host = AnalysisHost::with_database(db);
    let db = host.raw_database();
    let walk_started = Instant::now();
    let mut answers = CheckerAnswers { load, ..CheckerAnswers::default() };

    // The next-solver interner reads a THREAD-attached db; without this every
    // resolve panics in hir_ty's `next_solver/interner.rs`.
    let walk_files: Vec<WalkFile> = attach_db(db, || {
        let sema = Semantics::new(db);
        by_file_id
            .iter()
            .map(|(file_id, path)| {
                let text = sema.parse_guess_edition(*file_id).syntax().text().to_string();
                WalkFile {
                    path: (*path).to_string(),
                    file_id: *file_id,
                    offsets: OffsetMap::new(&text),
                    text,
                }
            })
            .collect()
    });
    // Every destination coordinate is read in the SOURCE file's own offset
    // unit, so a nav into any corpus file needs that file's map in hand.
    let destination: HashMap<ra_ap_ide::FileId, &WalkFile> =
        walk_files.iter().map(|file| (file.file_id, file)).collect();
    answers.files_answered = walk_files.len();

    // A salsa handle shares the storage and carries a thread-local query stack,
    // so it is Send and NOT Sync: each chunk owns a moved clone, never a borrow.
    let pool = crate::project::extract_pool();
    let chunk_size = walk_files.len().div_ceil(pool.current_num_threads().max(1)).max(1);
    let chunks: Vec<(RootDatabase, &[WalkFile])> = walk_files
        .chunks(chunk_size)
        .map(|chunk| (db.clone(), chunk))
        .collect();
    let per_file: Vec<FileAnswers> = pool.install(|| {
        use rayon::prelude::*;
        chunks
            .into_par_iter()
            .flat_map_iter(|(db, chunk)| {
                attach_db(&db, || {
                    let sema = Semantics::new(&db);
                    chunk
                        .iter()
                        .map(|file| walk_file(&sema, &destination, file))
                        .collect::<Vec<_>>()
                })
            })
            .collect()
    });

    for answered in per_file {
        answers.method_sites += answered.method_sites;
        answers.method_unresolved += answered.method_unresolved;
        answers.calls.insert(answered.path.clone(), answered.calls);
        answers.types.insert(answered.path, answered.types);
    }
    if tsi {
        // Ids are run-local across the whole workspace, so the item walk owns
        // one counter and runs after the per-file resolve rather than beside it.
        let (facts, coverage, unmodulated) = attach_db(db, || {
            let sema = Semantics::new(db);
            TsiWalk::new(db, &sema, &destination, &path_of).run(&walk_files)
        });
        answers.tsi = facts;
        answers.coverage = coverage;
        answers.unmodulated = unmodulated;
    }
    answers.walk = walk_started.elapsed();
    Ok(answers)
}

/// One file's share of the walk, kept per-worker so the fold is the only
/// contended write.
#[derive(Default)]
struct FileAnswers {
    path: String,
    calls: Vec<CheckerRef>,
    types: Vec<CheckerRef>,
    method_sites: usize,
    method_unresolved: usize,
}

/// One resolution kind over one file: the phase row it folds into, how many
/// rust-analyzer calls it made and how many of those answered.
struct SiteSpan {
    span: Span,
    calls: Cell<u64>,
    answered: Cell<u64>,
}

impl SiteSpan {
    fn new(phase: Phase) -> SiteSpan {
        SiteSpan {
            span: phase_span("rust", phase),
            calls: Cell::new(0),
            answered: Cell::new(0),
        }
    }

    /// Times exactly ONE rust-analyzer call. No guard here wraps another, so a
    /// span's micros are never a sum containing a sibling's.
    fn call<T>(&self, resolve: impl FnOnce() -> Option<T>) -> Option<T> {
        self.calls.set(self.calls.get() + 1);
        let answer = {
            let _entered = self.span.enter();
            resolve()
        };
        if answer.is_some() {
            self.answered.set(self.answered.get() + 1);
        }
        answer
    }

    fn record(&self) {
        record_phase(&self.span, 0, self.answered.get(), self.calls.get());
    }
}

/// The four rust-analyzer calls one file's walk pays for, priced apart.
struct SiteSpans {
    method: SiteSpan,
    call_path: SiteSpan,
    type_path: SiteSpan,
    nav: SiteSpan,
}

impl SiteSpans {
    fn new() -> SiteSpans {
        SiteSpans {
            method: SiteSpan::new(Phase::CheckerMethod),
            call_path: SiteSpan::new(Phase::CheckerCallPath),
            type_path: SiteSpan::new(Phase::CheckerTypePath),
            nav: SiteSpan::new(Phase::CheckerNav),
        }
    }

    fn record(&self) {
        self.method.record();
        self.call_path.record();
        self.type_path.record();
        self.nav.record();
    }
}

fn walk_file(
    sema: &Semantics<'_, RootDatabase>,
    destination: &HashMap<ra_ap_ide::FileId, &WalkFile>,
    file: &WalkFile,
) -> FileAnswers {
    let source = sema.parse_guess_edition(file.file_id);
    let spans = SiteSpans::new();
    let mut out = FileAnswers { path: file.path.clone(), ..FileAnswers::default() };
    for node in source.syntax().descendants() {
        if let Some(call) = ast::MethodCallExpr::cast(node.clone()) {
            out.method_sites += 1;
            match method_call_ref(sema, destination, file, &spans, &call) {
                Some(reference) => out.calls.push(reference),
                None => out.method_unresolved += 1,
            }
            continue;
        }
        if let Some(call) = ast::CallExpr::cast(node.clone()) {
            if let Some(ast::Expr::PathExpr(path_expr)) = call.expr() {
                if let Some(path) = path_expr.path() {
                    if let Some(reference) = path_call_ref(sema, destination, file, &spans, &path) {
                        out.calls.push(reference);
                    }
                }
            }
            continue;
        }
        if let Some(record) = ast::RecordExpr::cast(node.clone()) {
            if let Some(path) = record.path() {
                if let Some(reference) = path_call_ref(sema, destination, file, &spans, &path) {
                    out.calls.push(reference);
                }
            }
            continue;
        }
        if let Some(path) = ast::Path::cast(node) {
            if let Some(reference) = type_ref(sema, destination, file, &spans, &path) {
                out.types.push(reference);
            }
        }
    }
    spans.record();
    out
}

/// `recv.m(..)`: the method the compiler dispatches to, receiver type and trait
/// resolution included. The reference range is the method identifier alone.
fn method_call_ref(
    sema: &Semantics<'_, RootDatabase>,
    destination: &HashMap<ra_ap_ide::FileId, &WalkFile>,
    file: &WalkFile,
    spans: &SiteSpans,
    call: &ast::MethodCallExpr,
) -> Option<CheckerRef> {
    let name_ref = call.name_ref()?;
    let function = spans.method.call(|| sema.resolve_method_call(call))?;
    let nav = spans.nav.call(|| nav_of(sema, ModuleDef::Function(function)))?;
    mint(destination, file, name_ref.syntax().text_range(), &nav)
}

/// `a::b::c(..)` and `Foo { .. }`: the item the trailing segment names.
fn path_call_ref(
    sema: &Semantics<'_, RootDatabase>,
    destination: &HashMap<ra_ap_ide::FileId, &WalkFile>,
    file: &WalkFile,
    spans: &SiteSpans,
    path: &ast::Path,
) -> Option<CheckerRef> {
    let name_ref = path.segment()?.name_ref()?;
    let PathResolution::Def(def) = spans.call_path.call(|| sema.resolve_path(path))? else {
        return None;
    };
    if matches!(def, ModuleDef::Module(_) | ModuleDef::BuiltinType(_)) {
        return None;
    }
    let nav = spans.nav.call(|| nav_of(sema, def))?;
    mint(destination, file, name_ref.syntax().text_range(), &nav)
}

/// A path naming a type declaration, the shape `Resolve<TypeF>`'s candidates
/// carry. Anything else on the path plane is left to the syntax leg.
fn type_ref(
    sema: &Semantics<'_, RootDatabase>,
    destination: &HashMap<ra_ap_ide::FileId, &WalkFile>,
    file: &WalkFile,
    spans: &SiteSpans,
    path: &ast::Path,
) -> Option<CheckerRef> {
    let name_ref = path.segment()?.name_ref()?;
    let PathResolution::Def(def) = spans.type_path.call(|| sema.resolve_path(path))? else {
        return None;
    };
    if !matches!(
        def,
        ModuleDef::Adt(_) | ModuleDef::Trait(_) | ModuleDef::TypeAlias(_)
    ) {
        return None;
    }
    let nav = spans.nav.call(|| nav_of(sema, def))?;
    mint(destination, file, name_ref.syntax().text_range(), &nav)
}

fn nav_of(sema: &Semantics<'_, RootDatabase>, def: ModuleDef) -> Option<NavigationTarget> {
    def.try_to_nav(sema).map(|nav| nav.call_site)
}

/// One nav plus one reference range -> a seam row, with both offsets converted
/// out of rust-analyzer's byte space into the parse plane's.
fn mint(
    destination: &HashMap<ra_ap_ide::FileId, &WalkFile>,
    file: &WalkFile,
    reference: ra_ap_syntax::TextRange,
    nav: &NavigationTarget,
) -> Option<CheckerRef> {
    let start = u32::from(reference.start());
    let end = u32::from(reference.end());
    // A destination outside the resolve universe is an ANSWER: the empty path
    // says "resolved, and no corpus definition is it".
    let (dst_path, dst_offset) = match destination.get(&nav.file_id) {
        Some(target) => {
            let declaration = nav.focus_range.unwrap_or(nav.full_range).start();
            (
                target.path.clone(),
                target.offsets.to_span_offset(u32::from(declaration)),
            )
        }
        None => (String::new(), 0),
    };
    Some(CheckerRef {
        start: file.offsets.to_span_offset(start),
        end: file.offsets.to_span_offset(end),
        name: file.text.get(start as usize..end as usize)?.to_string(),
        dst_path,
        dst_name: nav.name.as_str().to_string(),
        dst_offset,
    })
}

/// Every relation the item walk enumerates to exhaustion. A claim is emitted
/// only where the walk produced a row: `complete` over nothing says too much.
const ENUMERATED: &[&str] = &[
    "tsi.type",
    "tsi.denotes",
    "tsi.origin",
    "tsi.product",
    "tsi.sum",
    "tsi.callable",
    "tsi.primitive",
    "tsi.parameter",
    "tsi.called",
    "tsi.argument",
    "tsi.input",
    "tsi.output",
    "rust.trait",
    "rust.impl",
    "rust.assoc",
    "rust.lifetime",
    "rust.ownership",
];

/// Every relation the walk samples rather than enumerates, with the sentence a
/// partial claim carries beside it.
const SAMPLED: &[(&str, &str)] = &[
    (
        "tsi.edge",
        "enumerated for owners declared in the supplied files",
    ),
    (
        "tsi.conforms",
        "declared impls of supplied types and traits; blanket and auto traits not enumerated",
    ),
    ("tsi.has_type", "occurrences not walked in this arc"),
    ("tsi.subtype", "not enumerated"),
    ("tsi.assignable", "not enumerated"),
    ("tsi.equivalent", "not enumerated"),
];

/// Run-local identity over one workspace walk. Rule 1 has two halves: a
/// declaration is its `ModuleDef`, a structure is its rendering inside a crate.
struct TsiWalk<'db, 'a> {
    db: &'db RootDatabase,
    sema: &'a Semantics<'db, RootDatabase>,
    destination: &'a HashMap<ra_ap_ide::FileId, &'a WalkFile>,
    path_of: &'a HashMap<ra_ap_ide::FileId, String>,
    next: u32,
    nominal: HashMap<ModuleDef, u32>,
    structural: HashMap<(Crate, String), u32>,
    described: HashSet<u32>,
    facts: Vec<FactOut>,
}

impl<'db, 'a> TsiWalk<'db, 'a> {
    fn new(
        db: &'db RootDatabase,
        sema: &'a Semantics<'db, RootDatabase>,
        destination: &'a HashMap<ra_ap_ide::FileId, &'a WalkFile>,
        path_of: &'a HashMap<ra_ap_ide::FileId, String>,
    ) -> Self {
        TsiWalk {
            db,
            sema,
            destination,
            path_of,
            next: 0,
            nominal: HashMap::new(),
            structural: HashMap::new(),
            described: HashSet::new(),
            facts: Vec::new(),
        }
    }

    /// The declarations of every module a supplied file owns, then the impls of
    /// those declarations alone: a crate's whole impl set prices the walk by it.
    fn run(mut self, files: &[WalkFile]) -> (Vec<FactOut>, Vec<CoverageClaim>, Vec<String>) {
        let mut modules: Vec<ra_ap_hir::Module> = Vec::new();
        let mut unmodulated: Vec<String> = Vec::new();
        for file in files {
            let owned: Vec<ra_ap_hir::Module> =
                self.sema.file_to_module_defs(file.file_id).collect();
            if owned.is_empty() {
                unmodulated.push(file.path.clone());
            }
            for module in owned {
                if !modules.contains(&module) {
                    modules.push(module);
                }
            }
        }
        let mut adts: Vec<Adt> = Vec::new();
        let mut traits: Vec<Trait> = Vec::new();
        for module in modules {
            let krate = module.krate(self.db);
            for def in module.declarations(self.db) {
                match def {
                    ModuleDef::Adt(item) => adts.push(item),
                    ModuleDef::Trait(item) => traits.push(item),
                    _ => {}
                }
                self.declaration(def, krate);
            }
        }
        let mut seen: HashSet<Impl> = HashSet::new();
        let mut impls: Vec<Impl> = Vec::new();
        for adt in adts {
            for item in Impl::all_for_type(self.db, adt.ty(self.db)) {
                if seen.insert(item) {
                    impls.push(item);
                }
            }
        }
        for contract in traits {
            for item in Impl::all_for_trait(self.db, contract) {
                if seen.insert(item) {
                    impls.push(item);
                }
            }
        }
        for item in impls {
            let krate = item.module(self.db).krate(self.db);
            self.implementation(item, krate);
        }
        let claims = claims(&self.facts);
        (self.facts, claims, unmodulated)
    }

    fn row(&mut self, relation: &str, args: Vec<Arg>) {
        debug_assert!(
            crate::tsi::registry::check(relation, &args).is_ok(),
            "{relation}: {:?}",
            crate::tsi::registry::check(relation, &args)
        );
        self.facts.push(FactOut {
            fact: 0,
            relation: relation.to_string(),
            args,
        });
    }

    fn fresh(&mut self) -> u32 {
        let id = self.next;
        self.next += 1;
        id
    }

    /// True the first time an id is handed out for description, so a type
    /// reached twice carries one shape.
    fn first_visit(&mut self, id: u32) -> bool {
        self.described.insert(id)
    }

    /// Rule 1's nominal half. The `tsi.type` and `tsi.origin` rows are minted
    /// with the id, so every id an argument names is declared by construction.
    fn nominal(&mut self, def: ModuleDef) -> u32 {
        if let Some(id) = self.nominal.get(&def) {
            return *id;
        }
        let id = self.fresh();
        self.nominal.insert(def, id);
        self.row("tsi.type", vec![Arg::Id(id)]);
        let krate = def.module(self.db).map(|module| module.krate(self.db));
        let origin = self.origin_at(nav_of(self.sema, def), krate);
        self.row(
            "tsi.origin",
            vec![Arg::Id(id), Arg::Atom("rust".to_string()), origin],
        );
        id
    }

    /// Rule 1's structural half: two types that render alike inside one crate
    /// are one type, and the rendering is the only string the id costs.
    fn rendered(&mut self, ty: &Type<'db>, krate: Crate) -> (u32, bool) {
        let target = krate.to_display_target(self.db);
        let key = (krate, ty.display(self.db, target).to_string());
        if let Some(id) = self.structural.get(&key) {
            return (*id, false);
        }
        let id = self.fresh();
        self.structural.insert(key, id);
        self.row("tsi.type", vec![Arg::Id(id)]);
        (id, true)
    }

    /// A declaration is a symbol and a type at once, and `tsi.denotes` is the
    /// join a consumer follows from one to the other.
    fn declared(&mut self, def: ModuleDef) -> (u32, bool) {
        let id = self.nominal(def);
        let fresh = self.first_visit(id);
        if fresh {
            let symbol = self.fresh();
            self.row("tsi.symbol", vec![Arg::Id(symbol)]);
            self.row("tsi.denotes", vec![Arg::Id(symbol), Arg::Id(id)]);
        }
        (id, fresh)
    }

    /// A corpus declaration origins at its own name in the parse plane's offset
    /// unit; one outside the supplied files keeps its file's byte range.
    fn origin_at(&mut self, nav: Option<NavigationTarget>, krate: Option<Crate>) -> Arg {
        let fallback = || {
            let name = krate
                .and_then(|krate| krate.display_name(self.db))
                .map(|name| name.to_string())
                .unwrap_or_else(|| "rust".to_string());
            Arg::Span(name, 0, 0)
        };
        let Some(nav) = nav else { return fallback() };
        let range = nav.focus_range.unwrap_or(nav.full_range);
        let start = u32::from(range.start());
        let end = u32::from(range.end());
        if let Some(target) = self.destination.get(&nav.file_id) {
            return Arg::Span(
                target.path.clone(),
                target.offsets.to_span_offset(start),
                target.offsets.to_span_offset(end),
            );
        }
        match self.path_of.get(&nav.file_id) {
            Some(path) => Arg::Span(path.clone(), start, end),
            None => fallback(),
        }
    }

    fn declaration(&mut self, def: ModuleDef, krate: Crate) {
        match def {
            ModuleDef::Adt(Adt::Struct(item)) => {
                let (id, fresh) = self.declared(def);
                if !fresh {
                    return;
                }
                self.generics(id, GenericDef::from(item), krate);
                self.row("tsi.product", vec![Arg::Id(id)]);
                self.fields(id, item.fields(self.db), krate);
            }
            ModuleDef::Adt(Adt::Union(item)) => {
                let (id, fresh) = self.declared(def);
                if !fresh {
                    return;
                }
                self.generics(id, GenericDef::from(item), krate);
                self.row("tsi.product", vec![Arg::Id(id)]);
                self.fields(id, item.fields(self.db), krate);
            }
            ModuleDef::Adt(Adt::Enum(item)) => {
                let (id, fresh) = self.declared(def);
                if !fresh {
                    return;
                }
                self.generics(id, GenericDef::from(item), krate);
                self.row("tsi.sum", vec![Arg::Id(id)]);
                for (position, variant) in item.variants(self.db).into_iter().enumerate() {
                    let owned = self.nominal(ModuleDef::EnumVariant(variant));
                    if self.first_visit(owned) {
                        self.row("tsi.product", vec![Arg::Id(owned)]);
                        self.fields(owned, variant.fields(self.db), krate);
                    }
                    let edge = self.fresh();
                    let label = variant.name(self.db).as_str().to_string();
                    self.row(
                        "tsi.edge",
                        vec![
                            Arg::Id(edge),
                            Arg::Id(id),
                            Arg::Text(label),
                            Arg::Id(owned),
                            Arg::Int(position as i64),
                        ],
                    );
                }
            }
            ModuleDef::Trait(item) => {
                let (id, fresh) = self.declared(def);
                if !fresh {
                    return;
                }
                self.generics(id, GenericDef::from(item), krate);
                self.row("rust.trait", vec![Arg::Id(id)]);
                for assoc in item.items(self.db) {
                    self.assoc_item(id, assoc, krate);
                }
            }
            ModuleDef::Function(item) => self.callable(item, krate),
            _ => {}
        }
    }

    /// A trait's own associated type has no right-hand side, so its target is
    /// the alias declaration itself: an opaque id that still origins at a name.
    fn assoc_item(&mut self, owner: u32, assoc: AssocItem, krate: Crate) {
        match assoc {
            AssocItem::TypeAlias(alias) => {
                let target = if alias.has_type(self.db) {
                    self.type_id(&alias.ty(self.db), krate)
                } else {
                    self.nominal(ModuleDef::TypeAlias(alias))
                };
                let name = alias.name(self.db).as_str().to_string();
                self.row(
                    "rust.assoc",
                    vec![Arg::Id(owner), Arg::Text(name), Arg::Id(target)],
                );
            }
            AssocItem::Function(item) => self.callable(item, krate),
            AssocItem::Const(_) => {}
        }
    }

    fn implementation(&mut self, item: Impl, krate: Crate) {
        let Some(contract) = item.trait_(self.db) else {
            return;
        };
        let Some(adt) = item.self_ty(self.db).as_adt() else {
            return;
        };
        let owner = self.nominal(ModuleDef::Adt(adt));
        let contract = self.nominal(ModuleDef::Trait(contract));
        let id = self.fresh();
        self.row(
            "rust.impl",
            vec![Arg::Id(id), Arg::Id(owner), Arg::Id(contract)],
        );
        self.row(
            "tsi.conforms",
            vec![
                Arg::Id(owner),
                Arg::Id(contract),
                Arg::Atom("declared".to_string()),
            ],
        );
        for assoc in item.items(self.db) {
            self.assoc_item(owner, assoc, krate);
        }
    }

    fn callable(&mut self, item: Function, krate: Crate) {
        let (id, fresh) = self.declared(ModuleDef::Function(item));
        if !fresh {
            return;
        }
        self.generics(id, GenericDef::from(item), krate);
        self.row("tsi.callable", vec![Arg::Id(id)]);
        for (position, param) in item.params_without_self(self.db).into_iter().enumerate() {
            let param = self.type_id(param.ty(), krate);
            self.row(
                "tsi.input",
                vec![Arg::Id(id), Arg::Int(position as i64), Arg::Id(param)],
            );
        }
        let produced = self.type_id(&item.ret_type(self.db), krate);
        self.row(
            "tsi.output",
            vec![Arg::Id(id), Arg::Int(0), Arg::Id(produced)],
        );
    }

    /// Each field is one edge plus the word its type spells about who owns the
    /// bytes behind it; a borrow and a smart pointer both target their pointee.
    fn fields(&mut self, owner: u32, fields: Vec<Field>, krate: Crate) {
        for field in fields {
            let declared = field.ty(self.db);
            let (ownership, target) = ownership_of(self.db, &declared);
            let target = self.type_id(&target, krate);
            let edge = self.fresh();
            let label = field.name(self.db).as_str().to_string();
            self.row(
                "tsi.edge",
                vec![
                    Arg::Id(edge),
                    Arg::Id(owner),
                    Arg::Text(label),
                    Arg::Id(target),
                    Arg::Int(field.index() as i64),
                ],
            );
            self.row(
                "rust.ownership",
                vec![Arg::Id(edge), Arg::Atom(ownership.to_string())],
            );
        }
    }

    /// rust-analyzer exposes no variance for a type parameter, so the position
    /// carries `unspecified` rather than a word the compiler never said.
    fn generics(&mut self, owner: u32, def: GenericDef, krate: Crate) {
        let declared: Vec<ra_ap_hir::TypeParam> = def
            .type_or_const_params(self.db)
            .into_iter()
            .filter_map(|param| param.as_type_param(self.db))
            .filter(|param| !param.is_implicit(self.db))
            .collect();
        for (position, param) in declared.into_iter().enumerate() {
            let (id, fresh) = self.rendered(&param.ty(self.db), krate);
            // Two owners writing the same parameter name render alike and are one
            // id, so the bounds ride the id rather than the owner that reached it.
            if fresh {
                let nav = param.try_to_nav(self.sema).map(|nav| nav.call_site);
                let origin = self.origin_at(nav, Some(krate));
                self.row(
                    "tsi.origin",
                    vec![Arg::Id(id), Arg::Atom("rust".to_string()), origin],
                );
                for (rank, bound) in param.trait_bounds(self.db).into_iter().enumerate() {
                    let bound = self.nominal(ModuleDef::Trait(bound));
                    let edge = self.fresh();
                    self.row(
                        "tsi.edge",
                        vec![
                            Arg::Id(edge),
                            Arg::Id(id),
                            Arg::Text("bound".to_string()),
                            Arg::Id(bound),
                            Arg::Int(rank as i64),
                        ],
                    );
                }
            }
            self.row(
                "tsi.parameter",
                vec![
                    Arg::Id(id),
                    Arg::Id(owner),
                    Arg::Int(position as i64),
                    Arg::Atom("unspecified".to_string()),
                ],
            );
        }
        for param in def.lifetime_params(self.db) {
            let name = param.name(self.db);
            let name = name.as_str().trim_start_matches('\'').to_string();
            self.row("rust.lifetime", vec![Arg::Id(owner), Arg::Atom(name)]);
        }
    }

    /// The id for one type. A bare declaration takes rule 1's nominal id; an
    /// application takes its rendering's, and declares its parts beside it.
    fn type_id(&mut self, ty: &Type<'db>, krate: Crate) -> u32 {
        if let Some(builtin) = ty.as_builtin() {
            let id = self.nominal(ModuleDef::BuiltinType(builtin));
            if self.first_visit(id) {
                let name = builtin.name().as_str().to_string();
                self.row("tsi.primitive", vec![Arg::Id(id), Arg::Atom(name)]);
            }
            return id;
        }
        let arguments: Vec<Type<'db>> = ty.type_arguments().collect();
        if let Some(adt) = ty.as_adt() {
            if arguments.is_empty() {
                return self.nominal(ModuleDef::Adt(adt));
            }
            let (id, fresh) = self.rendered(ty, krate);
            if fresh {
                let nav = nav_of(self.sema, ModuleDef::Adt(adt));
                let origin = self.origin_at(nav, Some(krate));
                self.row(
                    "tsi.origin",
                    vec![Arg::Id(id), Arg::Atom("rust".to_string()), origin],
                );
                let constructor = self.nominal(ModuleDef::Adt(adt));
                let list = self.fresh();
                self.row(
                    "tsi.called",
                    vec![Arg::Id(id), Arg::Id(constructor), Arg::Id(list)],
                );
                for (position, argument) in arguments.iter().enumerate() {
                    let argument = self.type_id(argument, krate);
                    self.row(
                        "tsi.argument",
                        vec![Arg::Id(list), Arg::Int(position as i64), Arg::Id(argument)],
                    );
                }
            }
            return id;
        }
        let (id, fresh) = self.rendered(ty, krate);
        if !fresh {
            return id;
        }
        let origin = self.origin_at(None, Some(krate));
        self.row(
            "tsi.origin",
            vec![Arg::Id(id), Arg::Atom("rust".to_string()), origin],
        );
        if let Some(callable) = ty.as_callable(self.db) {
            self.row("tsi.callable", vec![Arg::Id(id)]);
            for (position, param) in callable.params().into_iter().enumerate() {
                let param = self.type_id(param.ty(), krate);
                self.row(
                    "tsi.input",
                    vec![Arg::Id(id), Arg::Int(position as i64), Arg::Id(param)],
                );
            }
            let produced = self.type_id(&callable.return_type(), krate);
            self.row(
                "tsi.output",
                vec![Arg::Id(id), Arg::Int(0), Arg::Id(produced)],
            );
        }
        id
    }
}

/// The word a field's type spells about who owns the bytes behind it, and the
/// type the edge then targets: a wrapper is the word, never a node of its own.
fn ownership_of<'db>(db: &'db RootDatabase, ty: &Type<'db>) -> (&'static str, Type<'db>) {
    if ty.is_reference() {
        let word = if ty.is_mutable_reference() {
            "exclusive"
        } else {
            "shared"
        };
        return (word, ty.strip_reference());
    }
    if let Some(adt) = ty.as_adt() {
        let word = match adt.name(db).as_str() {
            "Box" => "owned",
            "Rc" | "Arc" => "shared",
            _ => return ("owned", ty.clone()),
        };
        if let Some(inner) = ty.type_arguments().next() {
            return (word, inner);
        }
    }
    ("owned", ty.clone())
}

fn claims(facts: &[FactOut]) -> Vec<CoverageClaim> {
    let emitted: HashSet<&str> = facts.iter().map(|fact| fact.relation.as_str()).collect();
    ENUMERATED
        .iter()
        .filter(|relation| emitted.contains(*relation))
        .map(|relation| CoverageClaim {
            relation: (*relation).to_string(),
            complete: true,
            diagnostic: None,
        })
        .chain(SAMPLED.iter().map(|(relation, detail)| CoverageClaim {
            relation: (*relation).to_string(),
            complete: false,
            diagnostic: Some((*detail).to_string()),
        }))
        .collect()
}
