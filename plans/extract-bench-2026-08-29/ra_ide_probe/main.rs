use ra_ap_hir::{ModuleDef, PathResolution, Semantics, attach_db};
use ra_ap_ide::{
    AnalysisHost, CallHierarchyConfig, FilePosition, FileStructureConfig, NavigationTarget,
    RootDatabase, StructureNodeKind, SymbolKind, TryToNav,
};
use ra_ap_ide_db::ra_fixture::RaFixtureConfig;
use ra_ap_load_cargo::{LoadCargoConfig, ProcMacroServerChoice, load_workspace_at};
use ra_ap_project_model::CargoConfig;
use ra_ap_syntax::{AstNode, SyntaxNode, ast, ast::HasName, match_ast};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

fn rel(root: &str, p: &str) -> String {
    p.strip_prefix(root).map(|s| s.trim_start_matches('/').to_string()).unwrap_or_else(|| p.to_string())
}

fn main() {
    let root = std::env::args().nth(1).expect("corpus root arg");
    let budget_s: f64 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(240.0);
    let out_dir = std::env::args().nth(3).unwrap_or_else(|| "/tmp/ra_ide_probe".to_string());
    let family = std::env::args().nth(4).unwrap_or_else(|| "all".to_string());
    let cargo_config = CargoConfig::default();
    let load_config = LoadCargoConfig {
        load_out_dirs_from_check: false,
        with_proc_macro_server: ProcMacroServerChoice::None,
        prefill_caches: false,
        num_worker_threads: 4,
        proc_macro_processes: 0,
    };

    let t0 = Instant::now();
    let (db, vfs, _proc_macro) =
        load_workspace_at(Path::new(&root), &cargo_config, &load_config, &|_| {}).expect("load failed");
    eprintln!("workspace loaded in {:.2}s", t0.elapsed().as_secs_f64());

    let host = AnalysisHost::with_database(db);
    let analysis = host.analysis();

    let mut rs_files: Vec<(ra_ap_ide::FileId, String)> = Vec::new();
    for (vfs_id, path) in vfs.iter() {
        if let Some(p) = path.as_path() {
            let p = p.to_string();
            if p.starts_with(&root) && p.ends_with(".rs") {
                rs_files.push((ra_ap_ide::FileId::from_raw(vfs_id.index()), p));
            }
        }
    }
    eprintln!("rs files in vfs under root: {}", rs_files.len());

    if family == "type" {
        write_type_edges(&host, &vfs, &root, &out_dir, &rs_files);
        return;
    }
    write_type_edges(&host, &vfs, &root, &out_dir, &rs_files);

    let structure_config = FileStructureConfig { exclude_locals: true };
    let call_config = CallHierarchyConfig { exclude_tests: false, ra_fixture: RaFixtureConfig::default() };

    let mut defs_seen = 0usize;
    let mut edges = 0usize;
    let mut rows: Vec<String> = Vec::new();
    let t1 = Instant::now();
    let mut files_visited = 0usize;
    'outer: for (file_id, path) in &rs_files {
        files_visited += 1;
        let nodes = match analysis.file_structure(&structure_config, *file_id) {
            Ok(n) => n,
            Err(_) => continue,
        };
        for node in nodes {
            if !matches!(
                node.kind,
                StructureNodeKind::SymbolKind(SymbolKind::Function | SymbolKind::Method)
            ) {
                continue;
            }
            defs_seen += 1;
            let pos = FilePosition { file_id: *file_id, offset: node.navigation_range.start() };
            let Ok(Some(calls)) = analysis.outgoing_calls(&call_config, pos) else { continue };
            for call in calls {
                let dst_path = vfs
                    .file_path(ra_ap_vfs::FileId::from_raw(call.target.file_id.index()))
                    .to_string();
                rows.push(format!(
                    "{}\t{}\t{}\t{}",
                    rel(&root, path),
                    node.label,
                    rel(&root, &dst_path),
                    call.target.name.as_str(),
                ));
                edges += 1;
            }
            if t1.elapsed().as_secs_f64() > budget_s {
                eprintln!("budget exceeded at file {files_visited}/{}", rs_files.len());
                break 'outer;
            }
        }
    }
    eprintln!(
        "files_visited={files_visited}/{} defs_seen={defs_seen} edges={edges} wall={:.2}s",
        rs_files.len(),
        t1.elapsed().as_secs_f64()
    );
    rows.sort();
    rows.dedup();
    std::fs::write(format!("{out_dir}/rust.oracle.callhier.tsv"), rows.join("\n") + "\n").unwrap();
}

fn is_type_def(def: &ModuleDef) -> bool {
    matches!(def, ModuleDef::Adt(_) | ModuleDef::Trait(_) | ModuleDef::TypeAlias(_))
}

fn nav_of(sema: &Semantics<'_, RootDatabase>, def: ModuleDef) -> Option<NavigationTarget> {
    def.try_to_nav(sema).map(|nav| nav.call_site)
}

fn impl_self_name(imp: &ast::Impl) -> Option<String> {
    let self_ty = imp.self_ty()?;
    self_ty
        .syntax()
        .descendants()
        .find_map(ast::PathSegment::cast)
        .and_then(|seg| seg.name_ref())
        .map(|name| name.text().to_string())
}

/// The nearest named ancestor declaration, and whether it declares a type
/// (as opposed to a fn, const or static).
fn owner_of(node: &SyntaxNode) -> Option<(String, bool)> {
    node.ancestors().find_map(|ancestor| {
        match_ast! {
            match ancestor {
                ast::Struct(it) => it.name().map(|n| (n.text().to_string(), true)),
                ast::Enum(it) => it.name().map(|n| (n.text().to_string(), true)),
                ast::Union(it) => it.name().map(|n| (n.text().to_string(), true)),
                ast::Trait(it) => it.name().map(|n| (n.text().to_string(), true)),
                ast::TypeAlias(it) => it.name().map(|n| (n.text().to_string(), true)),
                ast::Impl(it) => impl_self_name(&it).map(|n| (n, true)),
                ast::Fn(it) => it.name().map(|n| (n.text().to_string(), false)),
                ast::Const(it) => it.name().map(|n| (n.text().to_string(), false)),
                ast::Static(it) => it.name().map(|n| (n.text().to_string(), false)),
                _ => None,
            }
        }
    })
}

fn write_type_edges(
    host: &AnalysisHost,
    vfs: &ra_ap_vfs::Vfs,
    root: &str,
    out_dir: &str,
    rs_files: &[(ra_ap_ide::FileId, String)],
) {
    let db = host.raw_database();
    let sema = Semantics::new(db);
    // 5-col row -> owner is a type declaration.
    let mut rows: BTreeMap<String, bool> = BTreeMap::new();
    let mut refs = 0usize;
    let mut implements = 0usize;
    let t = Instant::now();

    let dst_of = |nav: &NavigationTarget| -> Option<(String, String)> {
        let path = vfs.file_path(ra_ap_vfs::FileId::from_raw(nav.file_id.index())).to_string();
        if !path.starts_with(root) {
            return None;
        }
        Some((rel(root, &path), nav.name.as_str().to_string()))
    };

    // The next-solver interner reads a thread-attached db; without this every
    // resolve_path panics at hir_ty next_solver/interner.rs:2487.
    attach_db(db, || {
    for (file_id, src_abs) in rs_files {
        let src_path = rel(root, src_abs);
        let source_file = sema.parse_guess_edition(*file_id);
        for node in source_file.syntax().descendants() {
            if let Some(path) = ast::Path::cast(node.clone()) {
                let Some(PathResolution::Def(def)) = sema.resolve_path(&path) else { continue };
                if !is_type_def(&def) {
                    continue;
                }
                let Some(nav) = nav_of(&sema, def) else { continue };
                let Some((dst_path, dst_name)) = dst_of(&nav) else { continue };
                let Some((src_name, type_decl_owner)) = owner_of(path.syntax()) else { continue };
                let row = format!("{src_path}\t{src_name}\t{dst_path}\t{dst_name}\tref");
                let entry = rows.entry(row).or_insert(false);
                *entry = *entry || type_decl_owner;
                refs += 1;
                continue;
            }
            let Some(imp) = ast::Impl::cast(node) else { continue };
            let Some(hir_impl) = sema.to_impl_def(&imp) else { continue };
            let Some(trait_) = hir_impl.trait_(db) else { continue };
            let Some(adt) = hir_impl.self_ty(db).as_adt() else { continue };
            let Some(src_nav) = nav_of(&sema, ModuleDef::Adt(adt)) else { continue };
            let Some(dst_nav) = nav_of(&sema, ModuleDef::Trait(trait_)) else { continue };
            let Some((adt_path, adt_name)) = dst_of(&src_nav) else { continue };
            let Some((trait_path, trait_name)) = dst_of(&dst_nav) else { continue };
            rows.insert(
                format!("{adt_path}\t{adt_name}\t{trait_path}\t{trait_name}\timplements"),
                true,
            );
            implements += 1;
        }
    }
    });

    let mut kinded: Vec<String> = Vec::with_capacity(rows.len());
    let mut bare: std::collections::BTreeSet<String> = Default::default();
    let mut type_decl: std::collections::BTreeSet<String> = Default::default();
    for (row, owned_by_type_decl) in &rows {
        kinded.push(row.clone());
        let stripped = row[..row.rfind('\t').unwrap()].to_string();
        if *owned_by_type_decl {
            type_decl.insert(stripped.clone());
        }
        bare.insert(stripped);
    }
    eprintln!(
        "type edges: kinded={} bare={} typedecl={} ref_hits={refs} implements={implements} wall={:.2}s",
        kinded.len(),
        bare.len(),
        type_decl.len(),
        t.elapsed().as_secs_f64()
    );
    write_lines(&format!("{out_dir}/rust.oracle.type.tsv"), bare.into_iter());
    write_lines(&format!("{out_dir}/rust.oracle.type.typedecl.tsv"), type_decl.into_iter());
    write_lines(&format!("{out_dir}/rust.oracle.type.kinds.tsv"), kinded.into_iter());
}

fn write_lines(path: &str, lines: impl Iterator<Item = String>) {
    let body: Vec<String> = lines.collect();
    std::fs::write(path, body.join("\n") + "\n").unwrap();
}
