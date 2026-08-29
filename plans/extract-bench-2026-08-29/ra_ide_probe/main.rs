use ra_ap_ide::{
    AnalysisHost, CallHierarchyConfig, FilePosition, FileStructureConfig,
    StructureNodeKind, SymbolKind,
};
use ra_ap_ide_db::ra_fixture::RaFixtureConfig;
use ra_ap_load_cargo::{LoadCargoConfig, ProcMacroServerChoice, load_workspace_at};
use ra_ap_project_model::CargoConfig;
use std::path::Path;
use std::time::Instant;

fn rel(root: &str, p: &str) -> String {
    p.strip_prefix(root).map(|s| s.trim_start_matches('/').to_string()).unwrap_or_else(|| p.to_string())
}

fn main() {
    let root = std::env::args().nth(1).expect("corpus root arg");
    let budget_s: f64 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(240.0);
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
    std::fs::write("/tmp/ra_ide_probe/rust.oracle.callhier.tsv", rows.join("\n") + "\n").unwrap();
}
