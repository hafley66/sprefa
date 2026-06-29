//! End-to-end cross-language flow on REAL compiler output. Generates a real
//! rust-analyzer SCIP index over bench/flow/rust and a real scip-typescript index
//! over bench/flow/ts, merges them into one index (prefixing each document's
//! relative path to the scan root: `rust/…`, `ts/…`), then runs the SAME
//! flow_scip.dl as the hand-built unit test (flow_xlang_scip.rs) and asserts the
//! same disambiguation holds on real monikers: the Rust getPet handler and the TS
//! getPet stub are distinct symbols, so `impl_sym` selects only the Rust handler.
//!
//! Runtime-skips (does not fail) when either toolchain is absent. Override with
//! SPREFA_RUST_ANALYZER / SPREFA_SCIP_TYPESCRIPT.

use protobuf::Message;
use scip::types::Index;
use sprefa_v5::{db, engine::Engine, lex, parse};
use std::path::{Path, PathBuf};
use std::process::Command;

const FLOW: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/bench/flow");

fn find_ra() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("SPREFA_RUST_ANALYZER") {
        let p = PathBuf::from(p);
        if p.is_file() { return Some(p); }
    }
    let home = std::env::var("HOME").ok()?;
    let ext = Path::new(&home).join(".vscode/extensions");
    let mut found = None;
    if let Ok(entries) = std::fs::read_dir(&ext) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with("rust-lang.rust-analyzer-") {
                let bin = e.path().join("server/rust-analyzer");
                if bin.is_file() { found = Some(bin); }
            }
        }
    }
    found
}

/// scip-typescript is a node CLI on PATH; the override points at a binary.
fn find_scip_ts() -> Option<String> {
    let cand = std::env::var("SPREFA_SCIP_TYPESCRIPT").unwrap_or_else(|_| "scip-typescript".into());
    let ok = Command::new(&cand).arg("--version").output().map(|o| o.status.success()).unwrap_or(false);
    ok.then_some(cand)
}

fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for e in std::fs::read_dir(src).unwrap().flatten() {
        let p = e.path();
        let d = dst.join(e.file_name());
        if p.is_dir() { copy_dir(&p, &d); } else { std::fs::copy(&p, &d).unwrap(); }
    }
}

/// Load a SCIP index and prefix every document's relative path so it joins the
/// scan root (the tool ran in a subdir, so its paths are subdir-relative).
fn load_prefixed(path: &Path, prefix: &str) -> Vec<scip::types::Document> {
    let bytes = std::fs::read(path).unwrap();
    let mut index = Index::parse_from_bytes(&bytes).unwrap();
    for doc in &mut index.documents {
        doc.relative_path = format!("{prefix}/{}", doc.relative_path);
    }
    index.documents
}

fn s(v: &serde_json::Value) -> String { v.as_str().unwrap_or_default().to_string() }

#[test]
fn real_merged_index_disambiguates_handler_from_stub() {
    let (Some(ra), Some(scip_ts)) = (find_ra(), find_scip_ts()) else {
        eprintln!("SKIP flow_xlang_scip_real: need rust-analyzer AND scip-typescript \
            (set SPREFA_RUST_ANALYZER / SPREFA_SCIP_TYPESCRIPT)");
        return;
    };

    let root = std::env::temp_dir().join(format!("flow_scip_real_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    copy_dir(&PathBuf::from(FLOW), &root);

    // real indices, each generated in its own subdir.
    let rust_scip = root.join("rust.scip");
    let st = Command::new(&ra)
        .args(["scip", ".", "--output", rust_scip.to_str().unwrap()])
        .current_dir(root.join("rust")).output().expect("run rust-analyzer scip");
    assert!(rust_scip.is_file(), "rust-analyzer produced no index: {}",
        String::from_utf8_lossy(&st.stderr));

    let ts_scip = root.join("ts.scip");
    let st = Command::new(&scip_ts)
        .args(["index", "--output", ts_scip.to_str().unwrap()])
        .current_dir(root.join("ts")).output().expect("run scip-typescript");
    assert!(ts_scip.is_file(), "scip-typescript produced no index: {}",
        String::from_utf8_lossy(&st.stderr));

    // merge: one index, paths prefixed to the scan root.
    let mut docs = load_prefixed(&rust_scip, "rust");
    docs.extend(load_prefixed(&ts_scip, "ts"));
    let merged = Index { documents: docs, ..Default::default() };
    std::fs::write(root.join("index.scip"), merged.write_to_bytes().unwrap()).unwrap();

    // run the same tier-1 page against the real merged index.
    let prog_src = std::fs::read_to_string(root.join("flow_scip.dl")).unwrap();
    let prog = parse::parse(lex::lex(&prog_src).unwrap()).unwrap();
    let conn = db::open(Some(root.join("flow.db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, root);
    eng.tick(&prog, true).unwrap();

    // both getPet defs present as distinct symbols.
    let defs: Vec<(String, String)> = eng
        .query_sql("SELECT \"symbol\",\"file\" FROM rel_scip_def", &[]).unwrap()
        .into_iter().map(|r| (s(&r[0]), s(&r[1]))).collect();
    let getpet_defs: Vec<_> = defs.iter()
        .filter(|(sym, _)| sym.ends_with("getPet().")).collect();
    assert_eq!(getpet_defs.len(), 2, "two distinct getPet symbols expected: {getpet_defs:?}");

    // impl_sym: only the Rust handler (the .rs role filter), keyed by symbol.
    let impls: Vec<(String, String, String)> = eng
        .query_sql("SELECT \"op\",\"sym\",\"file\" FROM rel_impl_sym", &[]).unwrap()
        .into_iter().map(|r| (s(&r[0]), s(&r[1]), s(&r[2]))).collect();
    let getpet_impls: Vec<_> = impls.iter().filter(|(op, _, _)| op == "getPet").collect();
    assert_eq!(getpet_impls.len(), 1, "getPet has exactly one impl (the Rust handler): {impls:?}");
    assert_eq!(getpet_impls[0].2, "rust/src/handler.rs", "impl is the Rust file: {impls:?}");
    assert!(!impls.iter().any(|(_, _, f)| f == "ts/client.ts"),
        "TS stub must not tag as impl on real monikers: {impls:?}");

    // client_sym: the TS getPet call resolves to the TS stub (def_file ts/...),
    // never the Rust handler — the cross-lang tie is the operationId, not an edge.
    let clients: Vec<(String, String, String)> = eng
        .query_sql("SELECT \"op\",\"file\",\"def_file\" FROM rel_client_sym", &[]).unwrap()
        .into_iter().map(|r| (s(&r[0]), s(&r[1]), s(&r[2]))).collect();
    assert!(clients.iter().any(|(op, f, df)|
        op == "getPet" && f == "ts/client.ts" && df == "ts/client.ts"),
        "TS getPet call resolves within TS to the stub: {clients:?}");
}
