//! The checker tier's loader: `cargo metadata` into a salsa db, then
//! rust-analyzer's own resolution over every supplied file. Seam: `rust_checker`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ra_ap_hir::{ModuleDef, PathResolution, Semantics, attach_db};
use ra_ap_ide::{AnalysisHost, NavigationTarget, RootDatabase, TryToNav};
use ra_ap_load_cargo::{LoadCargoConfig, ProcMacroServerChoice, load_workspace_at};
use ra_ap_project_model::CargoConfig;
use ra_ap_syntax::{AstNode, ast};

use super::rust_checker::{CheckerAnswers, CheckerError, CheckerRef, OffsetMap};

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
) -> Result<CheckerAnswers, CheckerError> {
    let load_config = LoadCargoConfig {
        load_out_dirs_from_check: false,
        with_proc_macro_server: ProcMacroServerChoice::None,
        prefill_caches: false,
        num_worker_threads: 4,
        proc_macro_processes: 0,
    };
    let started = Instant::now();
    let (db, vfs, _proc_macro) =
        load_workspace_at(root, &CargoConfig::default(), &load_config, &|_| {})
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
    for (vfs_id, vfs_path) in vfs.iter() {
        let Some(absolute) = vfs_path.as_path() else { continue };
        let key = PathBuf::from(absolute.to_string());
        let key = std::fs::canonicalize(&key).unwrap_or(key);
        if let Some(supplied) = wanted.get(&key) {
            by_file_id.insert(ra_ap_ide::FileId::from_raw(vfs_id.index()), supplied);
        }
    }

    let host = AnalysisHost::with_database(db);
    let db = host.raw_database();
    let sema = Semantics::new(db);
    let walk_started = Instant::now();
    let mut answers = CheckerAnswers { load, ..CheckerAnswers::default() };

    // The next-solver interner reads a thread-attached db; without this every
    // resolve panics in hir_ty's `next_solver/interner.rs`.
    attach_db(db, || {
        let walk_files: Vec<WalkFile> = by_file_id
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
            .collect();
        // Every destination coordinate is read in the SOURCE file's own offset
        // unit, so a nav into any corpus file needs that file's map in hand.
        let destination: HashMap<ra_ap_ide::FileId, &WalkFile> =
            walk_files.iter().map(|file| (file.file_id, file)).collect();
        answers.files_answered = walk_files.len();
        for file in &walk_files {
            let source = sema.parse_guess_edition(file.file_id);
            let mut calls: Vec<CheckerRef> = Vec::new();
            let mut types: Vec<CheckerRef> = Vec::new();
            for node in source.syntax().descendants() {
                if let Some(call) = ast::MethodCallExpr::cast(node.clone()) {
                    if let Some(reference) =
                        method_call_ref(&sema, &destination, file, &call)
                    {
                        calls.push(reference);
                    }
                    continue;
                }
                if let Some(call) = ast::CallExpr::cast(node.clone()) {
                    if let Some(ast::Expr::PathExpr(path_expr)) = call.expr() {
                        if let Some(path) = path_expr.path() {
                            if let Some(reference) =
                                path_call_ref(&sema, &destination, file, &path)
                            {
                                calls.push(reference);
                            }
                        }
                    }
                    continue;
                }
                if let Some(record) = ast::RecordExpr::cast(node.clone()) {
                    if let Some(path) = record.path() {
                        if let Some(reference) = path_call_ref(&sema, &destination, file, &path) {
                            calls.push(reference);
                        }
                    }
                    continue;
                }
                if let Some(path) = ast::Path::cast(node) {
                    if let Some(reference) = type_ref(&sema, &destination, file, &path) {
                        types.push(reference);
                    }
                }
            }
            answers.calls.insert(file.path.clone(), calls);
            answers.types.insert(file.path.clone(), types);
        }
    });
    answers.walk = walk_started.elapsed();
    Ok(answers)
}

/// `recv.m(..)`: the method the compiler dispatches to, receiver type and trait
/// resolution included. The reference range is the method identifier alone.
fn method_call_ref(
    sema: &Semantics<'_, RootDatabase>,
    destination: &HashMap<ra_ap_ide::FileId, &WalkFile>,
    file: &WalkFile,
    call: &ast::MethodCallExpr,
) -> Option<CheckerRef> {
    let name_ref = call.name_ref()?;
    let function = sema.resolve_method_call(call)?;
    let nav = nav_of(sema, ModuleDef::Function(function))?;
    mint(destination, file, name_ref.syntax().text_range(), &nav)
}

/// `a::b::c(..)` and `Foo { .. }`: the item the trailing segment names.
fn path_call_ref(
    sema: &Semantics<'_, RootDatabase>,
    destination: &HashMap<ra_ap_ide::FileId, &WalkFile>,
    file: &WalkFile,
    path: &ast::Path,
) -> Option<CheckerRef> {
    let name_ref = path.segment()?.name_ref()?;
    let PathResolution::Def(def) = sema.resolve_path(path)? else {
        return None;
    };
    if matches!(def, ModuleDef::Module(_) | ModuleDef::BuiltinType(_)) {
        return None;
    }
    let nav = nav_of(sema, def)?;
    mint(destination, file, name_ref.syntax().text_range(), &nav)
}

/// A path naming a type declaration, the shape `Resolve<TypeF>`'s candidates
/// carry. Anything else on the path plane is left to the syntax leg.
fn type_ref(
    sema: &Semantics<'_, RootDatabase>,
    destination: &HashMap<ra_ap_ide::FileId, &WalkFile>,
    file: &WalkFile,
    path: &ast::Path,
) -> Option<CheckerRef> {
    let name_ref = path.segment()?.name_ref()?;
    let PathResolution::Def(def) = sema.resolve_path(path)? else {
        return None;
    };
    if !matches!(
        def,
        ModuleDef::Adt(_) | ModuleDef::Trait(_) | ModuleDef::TypeAlias(_)
    ) {
        return None;
    }
    let nav = nav_of(sema, def)?;
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
