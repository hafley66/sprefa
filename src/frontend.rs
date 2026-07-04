//! Module surface: `use` inclusion + canonical-path dedup. The smallest viable
//! module system. The lexer needs no new keyword: `use "path".` lexes as
//! `Ident("use") Str(p) Dot`, which `parse::parse_surface` recognizes and
//! produces `SurfaceItem::Use(Import { path })`. `expand` is the only place
//! sugar disappears: it recursively loads each `Use`, splices the Core items
//! in file order, and dedups rel decls by name + cols. The resulting
//! `ast::Program` flows into the unchanged engine.
//!
//! Include roots (first existing match wins). Each root is a *container* — a
//! directory that modules resolve *against*, so `use "std/foo.dl"` joins to
//! `<root>/std/foo.dl`. The root set:
//!   1. The program file's directory (`use "lib.dl"` for a sibling, and
//!      `use "std/foo.dl"` when the program lives next to a `std/` dir).
//!   2. `$SPREFA_STD` (explicit override; the install / CI hand-lever).
//!   3. `<crate>` (dev path: the repo root ships a `std/` so `use "std/foo.dl"`
//!      resolves from any test fixture without an install step).
//!   4. `<exe>/..` (installed layout: `std/` sits next to the binary).
//!
//! Diamond imports load once: `loaded` is keyed by canonical path, so a `use`
//! reached a second time is a cache hit. A rel decl with the same name and the
//! same cols dedups across the merge; conflicting cols hard-error naming both
//! source paths so a typo never silently shadows a library rel.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::ast::{self, BodyItem, Item, SurfaceItem, Term};
use crate::{lex, parse};

/// Resolve, load, and dedup the module surface for one program file. Returns
/// the flat list of core items ready for `typecheck::check_and_normalize`. The
/// entry path's own surface is the starting set; transitive `use`s splice in;
/// `def` templates are collected into a table and inlined at every call site
/// in the surrounding rules.
pub fn load_program(entry: &Path) -> Result<Vec<Item>> {
    let mut loader = ModuleLoader::new(entry);
    let surface = loader.load_entry(entry)?;
    let origin = entry.canonicalize().unwrap_or_else(|_| entry.to_path_buf());
    let mut items = expand_program(surface, &mut loader, origin)?;
    resolve_named_args(&mut items)?;
    desugar_effects(&mut items);
    dedup_rels(&mut items)?;
    Ok(items)
}

/// D-3: lower the head-response `@async`/`@stream` form to the canonical
/// body-effect form so the runtime has ONE model. A rule already carrying an
/// explicit `BodyItem::Effect` is left untouched (it is already canonical). For
/// the head-response form the synthesized effect's `args` are ALL positive-body
/// bound vars (the template hole map, keyed by var name — the legacy
/// `effect_cmd`/`sh` convention), and `outs` are the head vars NOT bound by the
/// body (the response columns, in head order). Head literals are neither arg nor
/// out; they reconstruct directly at drain time. Runs after `use`/`def` expansion
/// so the body is final (a `def` could contribute the binding atoms).
fn desugar_effects(items: &mut [Item]) {
    for item in items.iter_mut() {
        let Item::Rule(r) = item else { continue };
        if !(r.is_async() || r.is_stream()) { continue; }
        if r.effect().is_some() { continue; }
        let bound = pos_bound_vars(&r.body);
        let args: Vec<Term> = bound.iter().cloned().map(Term::Var).collect();
        let outs: Vec<Term> = r.head.terms.iter().filter_map(|t| match t {
            Term::Var(v) if !bound.contains(v) => Some(Term::Var(v.clone())),
            _ => None,
        }).collect();
        let name = r.head.rel.clone();
        r.body.push(BodyItem::Effect { name, args, outs });
    }
}

/// Distinct variables bound by the positive (`Pos`) body atoms, first-appearance
/// order. Mirrors `engine::async_bound_vars` — the head-response request args.
fn pos_bound_vars(body: &[BodyItem]) -> Vec<String> {
    let mut seen = Vec::new();
    for b in body {
        if let BodyItem::Pos(a) = b {
            for t in &a.terms {
                if let Term::Var(v) = t {
                    if !seen.contains(v) { seen.push(v.clone()); }
                }
            }
        }
    }
    seen
}

/// Resolve and expand the multi-file discovery shape (a `<dir>/*.dl` set). Each
/// file's surface is loaded with a shared loader so a `use` cached by file A is
/// a hit from file B. The merged items splice in file (lexicographic) order.
pub fn load_program_set(entry_paths: &[PathBuf]) -> Result<Vec<Item>> {
    if entry_paths.is_empty() { bail!("no program files to load"); }
    let mut loader = ModuleLoader::new(&entry_paths[0]);
    let mut items = Vec::new();
    let mut templates: HashMap<String, ast::RuleTemplate> = HashMap::new();
    for p in entry_paths {
        let surface = loader.load_entry(p)?;
        let origin = p.canonicalize().unwrap_or_else(|_| p.clone());
        items.extend(expand_with(surface, &mut loader, &mut templates, origin)?);
    }
    cycle_check(&templates)?;
    let mut counter = 0u32;
    for item in &mut items {
        if let Item::Rule(r) = item {
            inline_template_calls(&mut r.body, &templates, &mut counter)?;
        }
    }
    resolve_named_args(&mut items)?;
    desugar_effects(&mut items);
    dedup_rels(&mut items)?;
    Ok(items)
}

/// Fold `col: term` named args in body and `?` atoms into positional slots using
/// each relation's declared column names. Positional args fill left-to-right;
/// named args fill by column name; any column mentioned by neither becomes
/// `Term::Wild` (a don't-care) — that padding is the win over counting `_`. A
/// fully-positional atom (no named args) is left exactly as written, so existing
/// programs and the arity check downstream are unaffected. Column names come from
/// the user's `rel` decls (which override) plus the built-in schemas, so the
/// forward-reference case (`hit(l: 5)` before `rel hit`) still resolves. Runs
/// before `desugar_effects`, which reads the now-positional body terms.
fn resolve_named_args(items: &mut [Item]) -> Result<()> {
    let mut schema: HashMap<String, Vec<String>> = HashMap::new();
    for d in crate::engine::all_builtin_decls() {
        schema.insert(d.name, d.cols.into_iter().map(|c| c.name).collect());
    }
    for it in items.iter() {
        if let Item::Rel(d) = it {
            schema.insert(d.name.clone(), d.cols.iter().map(|c| c.name.clone()).collect());
        }
    }
    for it in items.iter_mut() {
        match it {
            Item::Rule(r) => {
                // The HEAD too: `diag(path: p, line: l, msg: m) <- ...` names only
                // the columns it writes, the rest pad to `Term::Wild` (which the
                // head projection lowers to NULL). A fully-positional head has no
                // `named` and is left exactly as written.
                resolve_atom(&mut r.head, &schema)?;
                for b in &mut r.body {
                    if let BodyItem::Pos(a) | BodyItem::Neg(a) = b {
                        resolve_atom(a, &schema)?;
                    }
                }
            }
            Item::Query(q) => resolve_atom(&mut q.head, &schema)?,
            _ => {}
        }
    }
    Ok(())
}

/// Fold one atom's named args into positional slots. Once ANY `col:` appears the
/// atom is in named mode. Binding then follows one rule: **a term that carries a
/// name binds by name; a nameless term binds by position.**
///   - `col: term` binds `col`.
///   - a bare Var `x` puns to `x: x` (binds the column of its own name — the JS
///     `{a, b}` / Rust `Foo { c }` shorthand), in any order, interleaved freely
///     with `col:` args (unlike Python, which forbids positional-after-keyword).
///   - a bare LITERAL (or any non-Var: interp / `_` / arith / call) has no name,
///     so it fills the next column left open by the named + pun args, in
///     declaration order, matched to the literals in written order. This is the
///     Python-style positional prefix: `diag("synth.rs", 1, severity: "error")`
///     puts "synth.rs" in `path`, 1 in `line`.
/// A column claimed by nothing becomes a don't-care (`Term::Wild` -> NULL).
///
/// A FULLY-bare atom (no `col:` at all) needs no anchor to shorthand: if it has
/// fewer terms than the rel has columns AND every term is a Var naming a column,
/// it resolves as all-puns — `diag(path, line, msg)` == `diag(path: path, line:
/// line, msg: msg)`, the rest Wild. That only fires when the atom would OTHERWISE
/// be an arity error (count != arity), so a genuinely positional atom (count ==
/// arity) is never reinterpreted and existing programs are untouched.
fn resolve_atom(a: &mut ast::Atom, schema: &HashMap<String, Vec<String>>) -> Result<()> {
    if a.named.is_empty() {
        // Positional atom. Leave it exactly positional unless it is a bare
        // shorthand that would otherwise fail the arity check.
        let is_shorthand = match schema.get(&a.rel) {
            Some(cols) => a.terms.len() != cols.len()
                && !a.terms.is_empty()
                && a.terms.iter().all(|t| matches!(t, Term::Var(v) if cols.contains(v))),
            None => false,
        };
        if !is_shorthand { return Ok(()); }
    }
    let cols = schema.get(&a.rel).ok_or_else(|| anyhow::anyhow!(
        "named args on `{0}` need a `rel {0}(...)` declaration (or a built-in schema) \
         to map column names to positions", a.rel))?;
    let pos = std::mem::take(&mut a.terms);
    let named = std::mem::take(&mut a.named);
    let mut slots: Vec<Option<Term>> = vec![None; cols.len()];
    // Named + pun args bind by name FIRST (so positional literals fill only the
    // holes they leave). A bare Var is a pun; a nameless term is deferred to the
    // positional pass.
    let mut positional: Vec<Term> = Vec::new();
    {
        let mut set = |name: &str, t: Term| -> Result<()> {
            let i = cols.iter().position(|c| c == name).ok_or_else(|| anyhow::anyhow!(
                "unknown column `{name}` on `{}` (columns: {})", a.rel, cols.join(", ")))?;
            if slots[i].is_some() {
                bail!("column `{name}` on `{}` is set twice", a.rel);
            }
            slots[i] = Some(t);
            Ok(())
        };
        for t in pos {
            match t {
                Term::Var(v) => set(&v.clone(), Term::Var(v))?,
                other => positional.push(other),
            }
        }
        for (name, t) in named { set(&name, t)?; }
    }
    // Positional literals fill the columns left open, in declaration order.
    let mut next = 0usize;
    for t in positional {
        while next < slots.len() && slots[next].is_some() { next += 1; }
        if next >= slots.len() {
            bail!("too many positional args on `{}`: all {} columns are already \
                   claimed by name", a.rel, cols.len());
        }
        slots[next] = Some(t);
        next += 1;
    }
    a.terms = slots.into_iter().map(|s| s.unwrap_or(Term::Wild)).collect();
    Ok(())
}

/// Top-level expand: same as `expand_with` but owns the templates table and
/// runs the cycle check + inline pass once everything (including transitive
/// `use`s) is collected.
fn expand_program(
    surface: Vec<SurfaceItem>,
    loader: &mut ModuleLoader,
    origin: PathBuf,
) -> Result<Vec<Item>> {
    let mut templates: HashMap<String, ast::RuleTemplate> = HashMap::new();
    let mut items = expand_with(surface, loader, &mut templates, origin)?;
    cycle_check(&templates)?;
    let mut counter = 0u32;
    for item in &mut items {
        if let Item::Rule(r) = item {
            inline_template_calls(&mut r.body, &templates, &mut counter)?;
        }
    }
    Ok(items)
}

/// Walk one surface, recursively expanding `Use`s and collecting `Def`s into
/// the shared `templates` table. The table is shared across the whole program
/// load so a `def` from an imported file is callable from the importer.
fn expand_with(
    items: Vec<SurfaceItem>,
    loader: &mut ModuleLoader,
    templates: &mut HashMap<String, ast::RuleTemplate>,
    origin: PathBuf,
) -> Result<Vec<Item>> {
    let mut out: Vec<Item> = Vec::with_capacity(items.len());
    for it in items {
        match it {
            SurfaceItem::Core(mut core) => {
                // Stamp the rule's source file so a self-form scan can resolve
                // to this file's `.git` ancestor (loaded-script portability).
                if let Item::Rule(r) = &mut core { r.origin = Some(origin.clone()); }
                out.push(core);
            }
            SurfaceItem::Use(imp) => {
                match loader.resolve(&imp.path) {
                    Ok(path) => {
                        if loader.loaded.contains_key(&path) {
                            continue; // diamond import: canonical-path dedup
                        }
                        let surface = loader.load_entry(&path)?;
                        // The included items were authored in `path`, not the entry.
                        let expanded = expand_with(surface, loader, templates, path)?;
                        out.extend(expanded);
                    }
                    Err(disk_err) => {
                        // No file on any disk root: fall back to the std lib baked
                        // into the binary (`use "std/callgraph.dl".` with no source
                        // tree). Disk always wins; this is the last resort.
                        let Some(src) = crate::corpus::std_lib(&imp.path) else {
                            return Err(disk_err);
                        };
                        if !loader.loaded_embedded.insert(imp.path.clone()) {
                            continue; // already spliced this embedded lib
                        }
                        let surface = lex::lex(src)
                            .and_then(parse::parse_surface)
                            .with_context(|| format!("in embedded {}", imp.path))?;
                        let key = PathBuf::from(format!("<embedded>/{}", imp.path));
                        let expanded = expand_with(surface, loader, templates, key)?;
                        out.extend(expanded);
                    }
                }
            }
            SurfaceItem::Def(tpl) => {
                let name = tpl.name.clone();
                if templates.insert(name.clone(), tpl).is_some() {
                    bail!("def {name} declared twice in the merge");
                }
            }
        }
    }
    Ok(out)
}

/// A template that transitively calls itself would inline forever. Reject up
/// front; the inline-only contract keeps the model simple (no recursion, no
/// fixed-point).
fn cycle_check(templates: &HashMap<String, ast::RuleTemplate>) -> Result<()> {
    for name in templates.keys() {
        let mut stack = vec![name.clone()];
        let mut seen = std::collections::HashSet::new();
        while let Some(cur) = stack.pop() {
            if !seen.insert(cur.clone()) {
                bail!("def cycle through {name}: recursion is not supported (inline-only)");
            }
            let Some(t) = templates.get(&cur) else { continue };
            for b in &t.body {
                if let BodyItem::Pos(a) = b {
                    if templates.contains_key(&a.rel) {
                        stack.push(a.rel.clone());
                    }
                }
            }
        }
    }
    Ok(())
}

/// Recursively inline template calls in a body. `counter` is a global per-call
/// counter across the whole program so two instantiations of the same template
/// (even siblings in the same body) get disjoint alpha-renamed vars. After
/// inlining, no body atom's rel matches a template name.
fn inline_template_calls(
    body: &mut Vec<BodyItem>,
    templates: &HashMap<String, ast::RuleTemplate>,
    counter: &mut u32,
) -> Result<()> {
    let mut i = 0;
    while i < body.len() {
        // Only a Pos atom can be a template call. Negation, closure, and the
        // source ops (`scan`, `match`, ...) are not call sites.
        let BodyItem::Pos(atom) = &body[i] else { i += 1; continue };
        let Some(tpl) = templates.get(&atom.rel) else { i += 1; continue };
        if atom.terms.len() != tpl.params.len() {
            bail!(
                "def {} expects {} args, got {}",
                tpl.name, tpl.params.len(), atom.terms.len(),
            );
        }
        // Build the substitution: param name -> arg term (cloned, so the
        // template body is left untouched for the next call site).
        let mut sub: HashMap<String, Term> = HashMap::new();
        for (p, a) in tpl.params.iter().zip(atom.terms.iter()) {
            sub.insert(p.clone(), a.clone());
        }
        // Clone + rewrite the body. Alpha-rename every non-param var so two
        // instantiations of the same template never share a binding. The
        // namespace is `__<tplname>_<counter>_` so the rewritten body stays
        // debuggable in error messages and SQLite traces; each call site
        // bumps the counter so siblings stay disjoint.
        let stamp = format!("__{}_{}_", tpl.name, *counter);
        *counter += 1;
        let mut inlined: Vec<BodyItem> = tpl.body.iter().map(|b| {
            let mut cloned = b.clone();
            rewrite_terms(&mut cloned, &sub, &tpl.params, &stamp);
            cloned
        }).collect();
        // Recurse: the inlined body may itself contain template calls.
        inline_template_calls(&mut inlined, templates, counter)?;
        // Splice the inlined items where the call site was. The recursive call
        // already expanded nested templates, so advance past the whole block.
        let n = inlined.len();
        body.splice(i..=i, inlined);
        i += n;
    }
    Ok(())
}

/// Apply the substitution + alpha-rename to every Term in a BodyItem. A var
/// that names a template param is replaced by its arg term; any other var is
/// renamed with the stamp prefix so two instantiations stay disjoint.
fn rewrite_terms(b: &mut BodyItem, sub: &HashMap<String, Term>, params: &[String], stamp: &str) {
    let rewrite_term = |t: &mut Term| match t {
        Term::Var(v) if params.iter().any(|p| p == v) => {
            if let Some(arg) = sub.get(v) { *t = arg.clone(); }
        }
        Term::Var(v) => {
            v.insert_str(0, stamp);
        }
        _ => {}
    };
    let rewrite_opt = |t: &mut Option<Term>| { if let Some(t) = t { rewrite_term(t); } };
    match b {
        BodyItem::Pos(a) | BodyItem::Neg(a) => {
            for t in &mut a.terms { rewrite_term(t); }
        }
        BodyItem::Scan { repo, rev, glob, path, rev_out } => {
            for t in [repo, rev, glob, path, rev_out] { rewrite_term(t); }
        }
        BodyItem::Match { path, rev, line, id, col, end_col, .. } => {
            for t in [path, rev, line] { rewrite_term(t); }
            for t in [id, col, end_col].into_iter().flatten() { rewrite_term(t); }
        }
        BodyItem::Ast { path, rev, line, end, id, .. } => {
            for t in [path, rev, line] { rewrite_term(t); }
            rewrite_opt(end);
            rewrite_opt(id);
        }
        BodyItem::Sg { src, rev, line, col, end_line, end_col, id, .. } => {
            for t in [src, line, col, end_line, end_col] { rewrite_term(t); }
            if let Some(r) = rev { rewrite_term(r); }
            rewrite_opt(id);
        }
        BodyItem::AstYaml { path, rev, line, col, end_line, end_col, .. } => {
            for t in [path, rev, line, col, end_line, end_col] { rewrite_term(t); }
        }
        BodyItem::JsonP { src, rev, out, id, .. } => {
            for t in [src, out] { rewrite_term(t); }
            if let Some(r) = rev { rewrite_term(r); }
            rewrite_opt(id);
        }
        BodyItem::Json { src, rev, .. } => {
            rewrite_term(src);
            if let Some(r) = rev { rewrite_term(r); }
        }
        BodyItem::Cmd { path, rev, line, out, .. } => {
            for t in [path, rev, line, out] { rewrite_term(t); }
        }
        BodyItem::Comment { path, rev, l0, l1, label, .. } => {
            for t in [path, rev, l0, l1, label] { rewrite_term(t); }
        }
        BodyItem::Cmp(c) => {
            rewrite_term(&mut c.lhs);
            rewrite_term(&mut c.rhs);
        }
        BodyItem::Effect { args, outs, .. } => {
            for t in args.iter_mut().chain(outs.iter_mut()) { rewrite_term(t); }
        }
        BodyItem::Closure { .. } | BodyItem::Scc { .. } | BodyItem::Node2vec { .. } => {}
    }
}

/// The module cache and root resolver. Lives for one compile (`load_program` ->
/// `expand` -> `Program`), then dropped. Pure: no shared state across compiles.
pub struct ModuleLoader {
    roots: Vec<PathBuf>,
    /// Canonical path -> that file's parsed surface. A second `use` of the same
    /// canonical path is a cache hit (the dedup invariant for diamond imports).
    loaded: HashMap<PathBuf, Vec<SurfaceItem>>,
    /// Embedded-std `use` paths already spliced (dedup for the no-disk fallback).
    loaded_embedded: std::collections::HashSet<String>,
}

impl ModuleLoader {
    /// Roots derived from an entry path: see the module doc for the order.
    pub fn new(entry: &Path) -> Self {
        ModuleLoader {
            roots: include_roots(entry),
            loaded: HashMap::new(),
            loaded_embedded: std::collections::HashSet::new(),
        }
    }

    /// Resolve a `use` path against the configured roots. The first root whose
    /// join exists wins. An absolute path bypasses the roots. Errors name the
    /// path and every root tried so a missing module is debuggable.
    fn resolve(&self, path: &str) -> Result<PathBuf> {
        let p = Path::new(path);
        if p.is_absolute() && p.exists() {
            return p.canonicalize().with_context(|| format!("canonicalize {path}"));
        }
        for root in &self.roots {
            let cand = root.join(path);
            if cand.exists() {
                return cand.canonicalize().with_context(|| format!("canonicalize {}", cand.display()));
            }
        }
        let tried = self.roots.iter()
            .map(|r| r.join(path).display().to_string())
            .collect::<Vec<_>>().join(", ");
        bail!("module {path:?} not found (tried: {tried})");
    }

    /// Read, lex, parse_surface, and cache one file. Caches by canonical path
    /// so a second `use` is a no-op at the call site (see `expand`). Per-file
    /// parse errors carry the file path via context.
    fn load_entry(&mut self, path: &Path) -> Result<Vec<SurfaceItem>> {
        let canon = path.canonicalize().with_context(|| format!("canonicalize {}", path.display()))?;
        if let Some(s) = self.loaded.get(&canon) {
            return Ok(s.clone());
        }
        let src = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let surface = lex::lex(&src)
            .and_then(parse::parse_surface)
            .with_context(|| format!("in {}", path.display()))?;
        self.loaded.insert(canon, surface.clone());
        Ok(surface)
    }
}

/// The include-root search list. Each root is a *container* directory that
/// `use` paths resolve against (so `use "std/foo.dl"` joins to
/// `<root>/std/foo.dl`). The program dir is first (closest intent), then
/// `$SPREFA_STD`, then the dev-time crate root, then the exe's parent.
fn include_roots(entry: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(dir) = entry.parent() {
        roots.push(dir.to_path_buf());
    }
    if let Some(p) = std::env::var_os("SPREFA_STD") {
        roots.push(PathBuf::from(p));
    }
    // Dev path: the repo root (crate root), which ships a `std/` subdir. Lets
    // `use "std/callgraph.dl".` resolve from any test fixture or example
    // without an install step. Compiled out under release builds where the
    // source tree is gone and only the exe-adjacent layout remains.
    roots.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            roots.push(dir.to_path_buf());
        }
    }
    roots
}

/// Same-name same-cols dedups; conflicting cols hard-error naming both col
/// vectors. Rules and queries splice verbatim. A typo'd col name or type
/// silently shadowing a library rel would be the import story's worst failure
/// mode, so the conflict case is loud.
///
/// The engine's `declare_all` runs the same check a second time as a defense
/// (with the older "declared twice" message); the frontend catches it first
/// because it sees the merge before the engine does.
fn dedup_rels(items: &mut Vec<Item>) -> Result<()> {
    let mut seen: HashMap<String, Vec<ast::Col>> = HashMap::new();
    let mut drop: Vec<usize> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        if let Item::Rel(decl) = item {
            if let Some(prior) = seen.get(&decl.name) {
                if prior == &decl.cols {
                    drop.push(i);
                    continue;
                }
                bail!(
                    "rel {} declared twice with different columns:\n  prior:    {:?}\n  conflict: {:?}",
                    decl.name, prior, decl.cols,
                );
            }
            seen.insert(decl.name.clone(), decl.cols.clone());
        }
    }
    for i in drop.iter().rev() {
        items.remove(*i);
    }
    Ok(())
}
