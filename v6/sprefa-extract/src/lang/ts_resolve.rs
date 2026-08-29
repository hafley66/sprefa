//! TS module specifier -> file path, on the real filesystem, bought.
//! @comment-ok: module header, the seam list every lang file opens with
//!
//! `oxc_resolver` is the ESM/CJS + tsconfig algorithm: extensionless names,
//! `index`, the `.js` written for a `.ts`, package.json `exports`/`main`,
//! tsconfig `paths`/`baseUrl`/`extends`/`references`. It re-uses the oxc family
//! this crate already links.
//!
//! WHY NOT `deps::resolve_specifier`. That one answers a different question and
//! keeps answering it: it resolves against a SUPPLIED file universe with no
//! syscall per specifier (`deps.rs:43-47`), which is what a corpus-wide dep fold
//! needs and what its madge grading measures. A move needs on-disk truth for a
//! handful of specifiers: package `exports`, a tsconfig `extends` chain, and a
//! monorepo's sibling packages are all outside the supplied universe. The two
//! stay separate; neither replaces the other.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use oxc_allocator::Allocator;
use oxc_ast::ast as ts;
use oxc_resolver::{ResolveOptions, Resolver, TsconfigDiscovery};

use crate::seams::DefIndex;
use crate::shape::{ContentId, FamilyTag, Span};

/// The candidate extensions, TS sources before their JS twins and `.d.ts` after
/// `.ts` (a declaration file loses to an implementation): `deps.rs:58-60`.
const EXTENSIONS: [&str; 9] = [
    ".ts", ".tsx", ".d.ts", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs",
];

/// `./x.js` names what the compiler emits from `x.ts` (`deps.rs:65-70`). A list
/// REPLACES the written extension, so each ends with its own: a real `.js`.
const EXTENSION_ALIAS: [(&str, &[&str]); 4] = [
    (".js", &[".ts", ".tsx", ".d.ts", ".js"]),
    (".mjs", &[".mts", ".mjs"]),
    (".cjs", &[".cts", ".cjs"]),
    (".jsx", &[".tsx", ".jsx"]),
];

/// One resolver for a whole run, holding the options and the library's own
/// filesystem cache. `Resolver` is `Send + Sync`, so a rayon pool shares one.
pub struct TsResolver {
    inner: Resolver,
    root: PathBuf,
}

impl TsResolver {
    /// `root` bounds what counts as a move target; it is canonicalized so the
    /// resolver's own canonical paths compare against it.
    pub fn new(root: &Path) -> Result<Self, String> {
        let root = root
            .canonicalize()
            .map_err(|error| format!("canonicalize root {}: {error}", root.display()))?;
        Ok(Self {
            inner: Resolver::new(options()),
            root,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The file `module` names when written by `from`. `None` when it resolves
    /// to nothing: a missing file, an uninstalled package, a builtin.
    pub fn resolve(&self, from: &Path, module: &str) -> Option<PathBuf> {
        // `resolve_file` takes the IMPORTING FILE, not its directory: the
        // TsconfigDiscovery::Auto branch only runs there (oxc_resolver lib.rs:250).
        let resolution = self.inner.resolve_file(from, module).ok()?;
        Some(resolution.path().to_path_buf())
    }

    /// The same answer, kept only when it lands inside the root: a package in
    /// `node_modules` is no move target, a tsconfig alias into the tree is.
    pub fn resolve_in_root(&self, from: &Path, module: &str) -> Option<PathBuf> {
        let path = self.resolve(from, module)?;
        path.starts_with(&self.root).then_some(path)
    }
}

/// The ESM-style TS options. Every value is a stated policy; the defaults this
/// leaves alone are `symlinks` (true) and `exports_fields` (`[["exports"]]`).
fn options() -> ResolveOptions {
    ResolveOptions {
        extensions: EXTENSIONS.iter().map(|ext| (*ext).to_string()).collect(),
        extension_alias: EXTENSION_ALIAS
            .iter()
            .map(|(written, sources)| {
                (
                    (*written).to_string(),
                    sources.iter().map(|ext| (*ext).to_string()).collect(),
                )
            })
            .collect(),
        main_files: vec!["index".to_string()],
        main_fields: vec!["module".to_string(), "main".to_string()],
        condition_names: vec!["node".to_string(), "import".to_string()],
        tsconfig: Some(TsconfigDiscovery::Auto),
        ..ResolveOptions::default()
    }
}

/// The replacement for a relative specifier now aiming at `relative`, keeping
/// `original`'s extension style and quote. `./` leads, else TS reads a package.
pub fn respell(relative: &str, original: &str, quote: char) -> String {
    let text = match written_extension(original) {
        // The spec named the emitted twin of a source file; keep that spelling.
        Some(written) if backs(written, extension_of(relative)) => {
            format!("{}{written}", strip_extension(relative))
        }
        Some(_) => relative.to_string(),
        None => directory_form(strip_extension(relative), original),
    };
    let text = if text.starts_with("..") {
        text
    } else {
        format!("./{}", text.trim_start_matches("./"))
    };
    format!("{quote}{text}{quote}")
}

/// An extensionless spec that resolved through `index` keeps naming the
/// directory, unless the spec itself spelled `index`.
fn directory_form(stripped: &str, original: &str) -> String {
    let named_index = original
        .rsplit('/')
        .next()
        .is_some_and(|last| last == "index");
    match stripped.strip_suffix("/index") {
        Some(directory) if !named_index => directory.to_string(),
        _ => stripped.to_string(),
    }
}

/// The extension `original` wrote, when it is one this resolver knows. A spec
/// ending in an unknown suffix (`./v1.2`) wrote no extension.
fn written_extension(original: &str) -> Option<&'static str> {
    EXTENSIONS
        .iter()
        .chain(EXTENSION_ALIAS.iter().map(|(written, _)| written))
        .find(|ext| original.ends_with(**ext))
        .copied()
}

/// Whether a file with extension `source` is what `written` names on disk.
fn backs(written: &str, source: Option<&str>) -> bool {
    let Some(source) = source else {
        return false;
    };
    EXTENSION_ALIAS
        .iter()
        .find(|(candidate, _)| *candidate == written)
        .is_some_and(|(_, sources)| sources.contains(&source))
}

fn extension_of(path: &str) -> Option<&str> {
    EXTENSIONS.iter().find(|ext| path.ends_with(**ext)).copied()
}

fn strip_extension(path: &str) -> &str {
    match extension_of(path) {
        Some(ext) => &path[..path.len() - ext.len()],
        None => path,
    }
}

// ════════════════════════════════════════════════════════════════════════════
// THE MODULE PLANE: ECMAScript ResolveExport over the whole corpus
// ════════════════════════════════════════════════════════════════════════════
// @comment-ok: section header, the algorithm's spec citation and its three inputs
//
// ECMA-262 16.2.1.6.3 ResolveExport, run once per file set, so a resolve arm
// binds an imported name the way the module system binds it; name-matching
// across files (`TsSource::call_name_match`) is what a FREE name falls to.
// Inputs: `oxc_parser`'s `ModuleRecord` (the spec's [[ImportEntries]] /
// [[LocalExportEntries]] / [[IndirectExportEntries]] / [[StarExportEntries]]),
// `TsResolver` above (specifier -> a file on disk), and the corpus `DefIndex`
// (a declaration's identifier span -> the def node containing it).

/// What one import statement binds a local name to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportBinding {
    /// The module specifier as written (`./x.js`, `rxjs`, `node:fs`).
    pub module: String,
    /// The name the SOURCE module is asked for.
    pub imported: ImportedName,
}

/// The name an import asks its source module for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportedName {
    Named(String),
    Default,
    /// `import * as ns from 'm'` / `import ns = require('m')`: the binding IS
    /// the module object, so a member access on it is an export lookup.
    Namespace,
}

/// One file's module facts, OWNED: the oxc arena that produced them dies with
/// the parse, and phase 2 reads them long after.
#[derive(Clone, Debug, Default)]
pub struct ModuleFacts {
    /// local binding name -> what it imports.
    pub imports: BTreeMap<String, ImportBinding>,
    /// exported name -> (the local name this file declares, its identifier span).
    pub local_exports: BTreeMap<String, (String, Span)>,
    /// exported name -> (module as written, the name that module is asked for).
    pub indirect_exports: BTreeMap<String, (String, ImportedName)>,
    /// `export * from 'm'` module specifiers, source order.
    pub star_exports: Vec<String>,
}

impl ModuleFacts {
    /// Every module specifier this file names, deduped, for the resolver pass.
    fn specifiers(&self) -> BTreeSet<&str> {
        self.imports
            .values()
            .map(|binding| binding.module.as_str())
            .chain(
                self.indirect_exports
                    .values()
                    .map(|(module, _)| module.as_str()),
            )
            .chain(self.star_exports.iter().map(String::as_str))
            .collect()
    }
}

/// One file's module facts off its own parse, a SECOND one: phase 1's arena
/// dies with dispatch and the `Parser` seam returns a `Program` alone.
pub fn module_facts(path: &str, content: &[u8]) -> Option<ModuleFacts> {
    let source_type = crate::lang::ts::source_type_for(path)?;
    let source = std::str::from_utf8(content).ok()?;
    let allocator = Allocator::default();
    let parsed = oxc_parser::Parser::new(&allocator, source, source_type).parse();
    if parsed.panicked {
        return None;
    }
    let mut facts = ModuleFacts::default();
    for entry in &parsed.module_record.import_entries {
        let imported = match &entry.import_name {
            oxc_syntax::module_record::ImportImportName::Name(name) => {
                ImportedName::Named(name.name.to_string())
            }
            oxc_syntax::module_record::ImportImportName::Default(_) => ImportedName::Default,
            oxc_syntax::module_record::ImportImportName::NamespaceObject => ImportedName::Namespace,
        };
        facts.imports.insert(
            entry.local_name.name.to_string(),
            ImportBinding {
                module: entry.module_request.name.to_string(),
                imported,
            },
        );
    }
    for entry in &parsed.module_record.local_export_entries {
        let Some(exported) = export_name(&entry.export_name) else {
            continue;
        };
        let span = match &entry.local_name {
            oxc_syntax::module_record::ExportLocalName::Name(name)
            | oxc_syntax::module_record::ExportLocalName::Default(name) => to_local_span(name.span),
            // `export default <expression>`: no local binding, so the entry's
            // own span is the seat, and it contains the declaration.
            oxc_syntax::module_record::ExportLocalName::Null => to_local_span(entry.span),
        };
        let local = entry
            .local_name
            .name()
            .map_or_else(|| exported.clone(), |name| name.to_string());
        facts.local_exports.insert(exported, (local, span));
    }
    for entry in &parsed.module_record.indirect_export_entries {
        let Some(module) = entry.module_request.as_ref() else {
            continue;
        };
        let Some(exported) = export_name(&entry.export_name) else {
            continue;
        };
        let imported = match &entry.import_name {
            oxc_syntax::module_record::ExportImportName::Name(name) => {
                ImportedName::Named(name.name.to_string())
            }
            // `export * as ns from 'm'`: the export IS the module object.
            oxc_syntax::module_record::ExportImportName::All
            | oxc_syntax::module_record::ExportImportName::AllButDefault => ImportedName::Namespace,
            oxc_syntax::module_record::ExportImportName::Null => continue,
        };
        facts
            .indirect_exports
            .insert(exported, (module.name.to_string(), imported));
    }
    for entry in &parsed.module_record.star_export_entries {
        if let Some(module) = entry.module_request.as_ref() {
            facts.star_exports.push(module.name.to_string());
        }
    }
    typescript_module_forms(&parsed.program, &mut facts);
    Some(facts)
}

/// TypeScript's own module forms, absent from the ECMAScript `ModuleRecord`.
/// `import x = require('m')` binds a module object; `export = X` reads as
/// that module's `default`.
/// @comment-ok: names the two AST forms this walk exists for
fn typescript_module_forms(program: &ts::Program<'_>, facts: &mut ModuleFacts) {
    for statement in &program.body {
        match statement {
            ts::Statement::TSImportEqualsDeclaration(declaration) => {
                let ts::TSModuleReference::ExternalModuleReference(reference) =
                    &declaration.module_reference
                else {
                    continue;
                };
                facts.imports.insert(
                    declaration.id.name.to_string(),
                    ImportBinding {
                        module: reference.expression.value.to_string(),
                        imported: ImportedName::Namespace,
                    },
                );
            }
            ts::Statement::TSExportAssignment(assignment) => {
                if let ts::Expression::Identifier(identifier) = &assignment.expression {
                    facts.local_exports.insert(
                        "default".to_string(),
                        (identifier.name.to_string(), to_local_span(identifier.span)),
                    );
                }
            }
            _ => {}
        }
    }
}

fn export_name(name: &oxc_syntax::module_record::ExportExportName<'_>) -> Option<String> {
    match name {
        oxc_syntax::module_record::ExportExportName::Name(name) => Some(name.name.to_string()),
        oxc_syntax::module_record::ExportExportName::Default(_) => Some("default".to_string()),
        oxc_syntax::module_record::ExportExportName::Null => None,
    }
}

fn to_local_span(span: oxc_span::Span) -> Span {
    Span {
        start: span.start,
        len: span.end - span.start,
    }
}

/// How an import binding reached the declaration it names. The record's `kind`
/// column, and a closed vocabulary every language's module plane can reuse.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ResolvedImportKind {
    /// The source module declares the name itself.
    Local,
    /// At least one `export { x } from 'm'` hop.
    Indirect,
    /// At least one `export * from 'm'` hop.
    Star,
    /// The binding is a module namespace object, not one declaration.
    Namespace,
    /// The name asked of the source module was `default`.
    Default,
}

impl ResolvedImportKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            ResolvedImportKind::Local => "local",
            ResolvedImportKind::Indirect => "indirect",
            ResolvedImportKind::Star => "star",
            ResolvedImportKind::Namespace => "namespace",
            ResolvedImportKind::Default => "default",
        }
    }

    /// The chain's kind after one more hop. Namespace > star > indirect >
    /// default > local; the import form sets namespace and default.
    fn promote(self, other: ResolvedImportKind) -> ResolvedImportKind {
        match (self, other) {
            (ResolvedImportKind::Namespace, _) | (_, ResolvedImportKind::Namespace) => {
                ResolvedImportKind::Namespace
            }
            (ResolvedImportKind::Star, _) | (_, ResolvedImportKind::Star) => {
                ResolvedImportKind::Star
            }
            (ResolvedImportKind::Indirect, _) | (_, ResolvedImportKind::Indirect) => {
                ResolvedImportKind::Indirect
            }
            (ResolvedImportKind::Default, _) | (_, ResolvedImportKind::Default) => {
                ResolvedImportKind::Default
            }
            _ => ResolvedImportKind::Local,
        }
    }
}

/// ResolveExport's four outcomes (ECMA-262 16.2.1.6.3). `path` is shared, not
/// owned: a barrel's table repeats one target thousands of times.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportResolution {
    /// The declaration `path` makes, at the identifier span `span`.
    Binding {
        path: std::sync::Arc<str>,
        span: Span,
        kind: ResolvedImportKind,
        hops: u32,
    },
    /// The whole module object of `path` (`export * as ns from`).
    Namespace {
        path: std::sync::Arc<str>,
        hops: u32,
    },
    /// Two `export *` targets offer different bindings for one name. The spec
    /// makes this a link-time error; here it is a row on the drops channel.
    Ambiguous,
    /// No corpus file exports the name.
    None,
}

impl ExportResolution {
    /// The same answer one module further out: the hop's arm folded into the
    /// kind, and one more module counted.
    fn walked(self, arm: ResolvedImportKind) -> ExportResolution {
        match self {
            ExportResolution::Binding {
                path,
                span,
                kind,
                hops,
            } => ExportResolution::Binding {
                path,
                span,
                kind: kind.promote(arm),
                hops: hops + 1,
            },
            ExportResolution::Namespace { path, hops } => ExportResolution::Namespace {
                path,
                hops: hops + 1,
            },
            other => other,
        }
    }

    /// The coordinate two star arms have to agree on to not be ambiguous.
    fn seat(&self) -> Option<(&str, u32, u32)> {
        match self {
            ExportResolution::Binding { path, span, .. } => Some((path, span.start, span.len)),
            ExportResolution::Namespace { path, .. } => Some((path, u32::MAX, u32::MAX)),
            _ => None,
        }
    }
}

/// One module's WHOLE export set. The spec is written per name, and a per-name
/// walk over a 73-star barrel measured wall(400)/wall(200) = 2.66, over budget.
type ExportTable = HashMap<String, ExportResolution>;

/// The `resolved_import` record's row: one import binding as the wire states
/// it, with the blob and span the arms need already dropped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportRow {
    pub local: String,
    pub name: String,
    pub target_path: String,
    pub target_name: Option<String>,
    pub kind: ResolvedImportKind,
    pub hops: u32,
}

/// One import binding, resolved to a corpus declaration. What
/// `Resolve<CallF>` and `Resolve<TypeF>` bind through.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedImport {
    /// The name as bound in the importing file.
    pub local: String,
    /// The name asked of the source module.
    pub name: String,
    /// The file the declaration lives in.
    pub target_path: String,
    pub target_blob: ContentId,
    /// The def node's span in the target file (never the identifier's).
    pub target_span: Span,
    /// The name the target file declares it under.
    pub target_name: Option<String>,
    pub kind: ResolvedImportKind,
    /// Modules walked past the importing file: 1 = the module it names.
    pub hops: u32,
}

/// THE corpus module plane, built ONCE per refresh in `resolve_project`: every
/// ts/js file's facts, every specifier resolved, def sites span-sorted.
#[derive(Default)]
pub struct TsModuleIndex {
    facts: HashMap<String, ModuleFacts>,
    /// (importing file, specifier as written) -> the corpus file it names.
    targets: HashMap<(String, String), String>,
    blobs: HashMap<String, ContentId>,
    /// blob -> its def sites sorted by (start, end), CallF facet first at a
    /// shared span so a call edge lands on the callable.
    defs: HashMap<ContentId, Vec<(Span, String, FamilyTag)>>,
    /// One shared handle per corpus path, so a table entry costs no string.
    shared: HashMap<String, std::sync::Arc<str>>,
    /// Each module's export table, built on first ask. A table reached through
    /// a re-export CYCLE is incomplete by construction and is never cached.
    tables: std::sync::Mutex<HashMap<String, std::sync::Arc<ExportTable>>>,
}

impl TsModuleIndex {
    /// `corpus` is EVERY input's (path, blob), whatever its language: a ts
    /// specifier may name a `.js` file no ts arm produced facts for.
    pub fn build(
        files: Vec<(String, ModuleFacts)>,
        corpus: &[(String, ContentId)],
        def_index: &DefIndex,
    ) -> TsModuleIndex {
        let mut index = TsModuleIndex::default();
        let mut by_real_path: HashMap<PathBuf, String> = HashMap::new();
        for (path, blob) in corpus {
            index.blobs.insert(path.clone(), blob.clone());
            index
                .shared
                .insert(path.clone(), std::sync::Arc::from(path.as_str()));
            if let Ok(real) = std::fs::canonicalize(path) {
                by_real_path.entry(real).or_insert_with(|| path.clone());
            }
        }
        for (name, sites) in &def_index.map {
            for site in sites {
                index.defs.entry(site.blob.clone()).or_default().push((
                    site.span,
                    name.clone(),
                    site.family,
                ));
            }
        }
        for spans in index.defs.values_mut() {
            spans.sort_by_key(|(span, name, family)| {
                (
                    span.start,
                    span.len,
                    *family != FamilyTag::Call,
                    name.clone(),
                )
            });
        }

        let resolver = Resolver::new(options());
        // One filesystem answer per (directory, specifier): a package's files
        // import the same modules, and the syscalls are the cost here.
        let mut answers: HashMap<(PathBuf, String), Option<String>> = HashMap::new();
        for (path, facts) in &files {
            let directory = Path::new(path)
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_default();
            for specifier in facts.specifiers() {
                let key = (directory.clone(), specifier.to_string());
                let answer = answers.entry(key).or_insert_with(|| {
                    let resolved = resolver.resolve_file(Path::new(path), specifier).ok()?;
                    let real = resolved.path();
                    by_real_path.get(real).cloned().or_else(|| {
                        by_real_path
                            .get(&std::fs::canonicalize(real).ok()?)
                            .cloned()
                    })
                });
                if let Some(target) = answer {
                    index
                        .targets
                        .insert((path.clone(), specifier.to_string()), target.clone());
                }
            }
        }
        index.facts = files.into_iter().collect();
        index
    }

    /// Whether this file has module facts at all (a ts/js input of the run).
    pub fn knows(&self, path: &str) -> bool {
        self.facts.contains_key(path)
    }

    /// What `local` is bound to by an import statement in `path`.
    pub fn import(&self, path: &str, local: &str) -> Option<&ImportBinding> {
        self.facts.get(path)?.imports.get(local)
    }

    /// The corpus blob of one corpus path.
    pub fn blob_of(&self, path: &str) -> Option<&ContentId> {
        self.blobs.get(path)
    }

    /// Where one name EXPORTED by `path` is written, as (file, identifier
    /// span). `bind` needs a def node; a `namespace` or typed `const` has none.
    pub fn export_seat(&self, path: &str, name: &str) -> Option<(String, Span)> {
        match self.resolve_export(path, name) {
            ExportResolution::Binding { path, span, .. } => Some((path.to_string(), span)),
            _ => None,
        }
    }

    /// The corpus file a specifier written in `path` names.
    pub fn target(&self, path: &str, specifier: &str) -> Option<&str> {
        self.targets
            .get(&(path.to_string(), specifier.to_string()))
            .map(String::as_str)
    }

    /// Every import binding `path` writes, resolved. One row per binding, in
    /// local-name order; an ambiguous or corpus-external binding has no row.
    pub fn bindings(&self, path: &str) -> Vec<ImportRow> {
        let Some(facts) = self.facts.get(path) else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        for (local, binding) in &facts.imports {
            let Some(target) = self.target(path, &binding.module) else {
                continue;
            };
            match &binding.imported {
                // A namespace binding names a MODULE; its members resolve one
                // by one, so the row's target is the module and nothing else.
                ImportedName::Namespace => rows.push(ImportRow {
                    local: local.clone(),
                    name: "*".to_string(),
                    target_path: target.to_string(),
                    target_name: None,
                    kind: ResolvedImportKind::Namespace,
                    hops: 1,
                }),
                _ => {
                    if let Ok(Some(found)) = self.bind(path, local) {
                        rows.push(ImportRow {
                            local: found.local,
                            name: found.name,
                            target_path: found.target_path,
                            target_name: found.target_name,
                            kind: found.kind,
                            hops: found.hops,
                        });
                    }
                }
            }
        }
        rows
    }

    /// ECMA-262 16.2.1.6.3 ResolveExport(`name`) on the module at `path`.
    pub fn resolve_export(&self, path: &str, name: &str) -> ExportResolution {
        self.export_table(path, &mut Vec::new())
            .0
            .get(name)
            .cloned()
            .unwrap_or(ExportResolution::None)
    }

    /// ResolveExport applied to every name at once. `stack` is the spec's
    /// resolveSet; the bool is false inside a re-export cycle (partial table).
    fn export_table(
        &self,
        path: &str,
        stack: &mut Vec<String>,
    ) -> (std::sync::Arc<ExportTable>, bool) {
        if let Some(hit) = self.tables.lock().expect("module tables").get(path) {
            return (hit.clone(), true);
        }
        if stack.iter().any(|open| open == path) {
            return (std::sync::Arc::new(ExportTable::new()), false);
        }
        let Some(facts) = self.facts.get(path) else {
            return (std::sync::Arc::new(ExportTable::new()), true);
        };
        let Some(shared) = self.shared.get(path) else {
            return (std::sync::Arc::new(ExportTable::new()), true);
        };
        stack.push(path.to_string());
        let mut table = ExportTable::new();
        // Step 3: local entries, and they win the name outright.
        for (exported, (_, span)) in &facts.local_exports {
            table.insert(
                exported.clone(),
                ExportResolution::Binding {
                    path: shared.clone(),
                    span: *span,
                    kind: ResolvedImportKind::Local,
                    hops: 0,
                },
            );
        }
        let mut complete = true;
        // Step 4: an indirect entry asks its target for the imported name.
        for (exported, (module, imported)) in &facts.indirect_exports {
            if table.contains_key(exported) {
                continue;
            }
            let Some(target) = self.target(path, module).map(str::to_string) else {
                continue;
            };
            let found = match imported {
                ImportedName::Namespace => match self.shared.get(&target) {
                    Some(target) => ExportResolution::Namespace {
                        path: target.clone(),
                        hops: 0,
                    },
                    None => continue,
                },
                ImportedName::Default | ImportedName::Named(_) => {
                    let asked = match imported {
                        ImportedName::Named(name) => name.as_str(),
                        _ => "default",
                    };
                    let (sub, sub_complete) = self.export_table(&target, stack);
                    complete &= sub_complete;
                    match sub.get(asked) {
                        Some(found) => found.clone(),
                        None => continue,
                    }
                }
            };
            table.insert(exported.clone(), found.walked(ResolvedImportKind::Indirect));
        }
        // Steps 5-6: every star arm, `default` never among them, and two arms
        // that disagree make the name AMBIGUOUS rather than picking one.
        let mut starred = ExportTable::new();
        for module in &facts.star_exports {
            let Some(target) = self.target(path, module).map(str::to_string) else {
                continue;
            };
            let (sub, sub_complete) = self.export_table(&target, stack);
            complete &= sub_complete;
            for (exported, found) in sub.iter() {
                if exported == "default" {
                    continue;
                }
                let found = found.clone().walked(ResolvedImportKind::Star);
                match starred.get(exported) {
                    None => {
                        starred.insert(exported.clone(), found);
                    }
                    Some(incumbent)
                        if *incumbent == ExportResolution::Ambiguous
                            || found == ExportResolution::Ambiguous
                            || incumbent.seat() != found.seat() =>
                    {
                        starred.insert(exported.clone(), ExportResolution::Ambiguous);
                    }
                    Some(_) => {}
                }
            }
        }
        for (exported, found) in starred {
            table.entry(exported).or_insert(found);
        }
        stack.pop();
        let table = std::sync::Arc::new(table);
        if complete {
            self.tables
                .lock()
                .expect("module tables")
                .insert(path.to_string(), table.clone());
        }
        (table, complete)
    }

    /// The local name `local` in `path`, bound to ONE corpus declaration.
    /// `Err(())` is AMBIGUOUS, a fact the drops channel carries.
    pub fn bind(&self, path: &str, local: &str) -> Result<Option<ResolvedImport>, ()> {
        let binding = match self.import(path, local) {
            Some(binding) => binding,
            None => return Ok(None),
        };
        let Some(target) = self.target(path, &binding.module) else {
            return Ok(None);
        };
        let (asked, form) = match &binding.imported {
            ImportedName::Named(name) => (name.clone(), None),
            ImportedName::Default => ("default".to_string(), Some(ResolvedImportKind::Default)),
            // A namespace binding names a module, not a declaration; its
            // members resolve through `member`.
            ImportedName::Namespace => return Ok(None),
        };
        self.finish(local, &asked, form, self.resolve_export(target, &asked))
    }

    /// `ns.member` where `ns` is a namespace import binding in `path`: the
    /// member IS an export of the module `ns` names.
    pub fn member(
        &self,
        path: &str,
        receiver: &str,
        member: &str,
    ) -> Result<Option<ResolvedImport>, ()> {
        let Some(binding) = self.import(path, receiver) else {
            return Ok(None);
        };
        if binding.imported != ImportedName::Namespace {
            return Ok(None);
        }
        let Some(target) = self.target(path, &binding.module) else {
            return Ok(None);
        };
        self.finish(
            receiver,
            member,
            Some(ResolvedImportKind::Namespace),
            self.resolve_export(target, member),
        )
    }

    /// One `ExportResolution` turned into the row the arms and the record read:
    /// the identifier span joined to the def node that contains it.
    fn finish(
        &self,
        local: &str,
        asked: &str,
        form: Option<ResolvedImportKind>,
        resolution: ExportResolution,
    ) -> Result<Option<ResolvedImport>, ()> {
        match resolution {
            ExportResolution::Ambiguous => Err(()),
            ExportResolution::None | ExportResolution::Namespace { .. } => Ok(None),
            // `target_name` is the DEF NODE's name, never the export entry's
            // local name, so this row joins `resolved_edge.callee_name`.
            ExportResolution::Binding {
                path,
                span,
                kind,
                hops,
            } => {
                let Some(blob) = self.blobs.get(&*path) else {
                    return Ok(None);
                };
                let Some((def_span, def_name)) = self.def_at(blob, span) else {
                    return Ok(None);
                };
                Ok(Some(ResolvedImport {
                    local: local.to_string(),
                    name: asked.to_string(),
                    target_path: path.to_string(),
                    target_blob: blob.clone(),
                    target_span: def_span,
                    target_name: Some(def_name).filter(|name| !name.is_empty()),
                    kind: form.map_or(kind, |form| kind.promote(form)),
                    hops: hops + 1,
                }))
            }
        }
    }

    /// The def node in `blob` CONTAINING an identifier span: innermost, CallF
    /// first, never `<module>` (it spans the file and swallows every export).
    fn def_at(&self, blob: &ContentId, identifier: Span) -> Option<(Span, String)> {
        let spans = self.defs.get(blob)?;
        let cut = spans.partition_point(|(span, _, _)| span.start <= identifier.start);
        let mut best: Option<(Span, &str, FamilyTag)> = None;
        for (span, name, family) in &spans[..cut] {
            if name == crate::lang::ts::MODULE_DEF_NAME || identifier.end() > span.end() {
                continue;
            }
            let better = match best {
                None => true,
                Some((incumbent, _, incumbent_family)) => {
                    let call_bias = (
                        *family == FamilyTag::Call,
                        incumbent_family == FamilyTag::Call,
                    );
                    call_bias.0 && !call_bias.1
                        || (call_bias.0 == call_bias.1 && span.len < incumbent.len)
                }
            };
            if better {
                best = Some((*span, name.as_str(), *family));
            }
        }
        best.map(|(span, name, _)| (span, name.to_string()))
    }
}
