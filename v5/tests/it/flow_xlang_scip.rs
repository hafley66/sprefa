//! Tier-1 cross-language flow (bench/flow/flow_scip.dl): the SCIP fidelity
//! tier-up over the tier-0 string-identity page (flow_xlang.rs). With a real
//! symbol index, the Rust getPet handler and the TS getPet stub are DISTINCT
//! symbols, so `impl_sym` (a DEF in a .rs file whose descriptor is the
//! operationId) selects ONLY the Rust handler — the tier-0 conflation, where
//! the TS stub also tagged "impl" because the rule matched a bare name, is gone.
//!
//! The index here is hand-constructed in-process via the `scip` crate (already a
//! dependency), with monikers shaped like scip-typescript / rust-analyzer would
//! emit (`scheme manager pkg version …/name().`). That keeps the test
//! deterministic and free of an external toolchain: swapping this for a
//! tool-generated index.scip is a drop-in, the relations consumed are identical.

use protobuf::Message;
use scip::types::{Document, Index, Occurrence, SymbolRole};
use sprefa_v5::{db, engine::Engine, lex, parse};
use std::path::{Path, PathBuf};

const FLOW: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/bench/flow");

fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for e in std::fs::read_dir(src).unwrap().flatten() {
        let p = e.path();
        let d = dst.join(e.file_name());
        if p.is_dir() { copy_dir(&p, &d); } else { std::fs::copy(&p, &d).unwrap(); }
    }
}

fn occ(line0: i32, col0: i32, end_col: i32, symbol: &str, def: bool) -> Occurrence {
    Occurrence {
        range: vec![line0, col0, end_col],
        symbol: symbol.to_string(),
        symbol_roles: if def { SymbolRole::Definition as i32 } else { 0 },
        ..Default::default()
    }
}

/// A two-document index: the Rust handler crate and the TS client. Each `getPet`
/// is its OWN symbol (different scheme/package/file), plus the internal call
/// occurrences so `scip_ref` resolves each call to its true def.
fn build_index() -> Index {
    // distinct symbols for the same descriptor name across the two languages.
    let rs_get = "rust-analyzer cargo flow_handler 0.1.0 handler/getPet().";
    let rs_create = "rust-analyzer cargo flow_handler 0.1.0 handler/createPet().";
    let ts_get = "scip-typescript npm flow-client 1.0.0 ts/`client.ts`/getPet().";
    let ts_create = "scip-typescript npm flow-client 1.0.0 ts/`client.ts`/createPet().";
    let ts_load = "scip-typescript npm flow-client 1.0.0 ts/`client.ts`/loadPetPage().";

    let rust = Document {
        language: "rust".into(),
        relative_path: "rust/src/handler.rs".into(),
        occurrences: vec![
            occ(13, 7, 13, rs_get, true),        // pub fn getPet (line 14)
            occ(18, 7, 16, rs_create, true),     // pub fn createPet (line 19)
            occ(19, 21, 27, rs_get, false),      // createPet body calls getPet (line 20)
        ],
        ..Default::default()
    };
    let ts = Document {
        language: "typescript".into(),
        relative_path: "ts/client.ts".into(),
        occurrences: vec![
            occ(3, 9, 15, ts_get, true),         // function getPet (line 4)
            occ(7, 9, 18, ts_create, true),      // function createPet (line 8)
            occ(12, 16, 27, ts_load, true),      // function loadPetPage (line 13)
            occ(13, 9, 15, ts_get, false),       // loadPetPage body calls getPet (line 14)
        ],
        ..Default::default()
    };
    Index { documents: vec![rust, ts], ..Default::default() }
}

fn run() -> Engine {
    let root = std::env::temp_dir().join(format!("flow_scip_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    copy_dir(&PathBuf::from(FLOW), &root);
    std::fs::write(root.join("index.scip"), build_index().write_to_bytes().unwrap()).unwrap();

    let prog_src = std::fs::read_to_string(root.join("flow_scip.dl")).unwrap();
    let prog = parse::parse(lex::lex(&prog_src).unwrap()).unwrap();
    let dbp = root.join("flow_scip.db");
    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, root);
    eng.tick(&prog, true).unwrap();
    eng
}

fn s(v: &serde_json::Value) -> String { v.as_str().unwrap_or_default().to_string() }

#[test]
fn scip_disambiguates_handler_from_client_stub() {
    let eng = run();

    // sanity: both getPet symbols are present as distinct scip_def rows.
    let defs: Vec<(String, String)> = eng
        .query_sql("SELECT \"symbol\",\"file\" FROM rel_scip_def", &[]).unwrap()
        .into_iter().map(|r| (s(&r[0]), s(&r[1]))).collect();
    let getpet_defs: Vec<_> = defs.iter().filter(|(sym, _)| sym.contains("getPet")).collect();
    assert_eq!(getpet_defs.len(), 2, "two distinct getPet symbols expected: {getpet_defs:?}");

    // impl_sym: ONLY the Rust handler, keyed by symbol + .rs role filter.
    let impls: Vec<(String, String, String)> = eng
        .query_sql("SELECT \"op\",\"sym\",\"file\" FROM rel_impl_sym", &[]).unwrap()
        .into_iter().map(|r| (s(&r[0]), s(&r[1]), s(&r[2]))).collect();
    let getpet_impls: Vec<_> = impls.iter().filter(|(op, _, _)| op == "getPet").collect();
    assert_eq!(getpet_impls.len(), 1, "getPet must have exactly one impl (the Rust handler): {impls:?}");
    let (_, sym, file) = getpet_impls[0];
    assert_eq!(file, "rust/src/handler.rs", "impl is the Rust file: {impls:?}");
    assert!(sym.starts_with("rust-analyzer"), "impl symbol is the Rust moniker: {sym}");
    // the conflation fix: the TS stub is NOT an impl.
    assert!(!impls.iter().any(|(_, _, f)| f == "ts/client.ts"),
        "TS stub must not tag as impl (tier-0 conflation): {impls:?}");

    // client_sym: SCIP resolves each call to its TRUE def. The TS loadPetPage
    // call to getPet binds the TS stub (def_file == ts/client.ts), NOT the Rust
    // handler — the cross-lang tie is the operationId, never a resolved edge.
    let clients: Vec<(String, String, String)> = eng
        .query_sql("SELECT \"op\",\"file\",\"def_file\" FROM rel_client_sym", &[]).unwrap()
        .into_iter().map(|r| (s(&r[0]), s(&r[1]), s(&r[2]))).collect();
    let ts_call = clients.iter().find(|(op, f, _)| op == "getPet" && f == "ts/client.ts");
    let (_, _, def_file) = ts_call.unwrap_or_else(|| panic!("TS getPet call site missing: {clients:?}"));
    assert_eq!(def_file, "ts/client.ts",
        "TS getPet call resolves to the TS stub, not the Rust handler: {clients:?}");
    // and the Rust internal createPet->getPet call resolves within Rust.
    assert!(clients.iter().any(|(op, f, df)|
        op == "getPet" && f == "rust/src/handler.rs" && df == "rust/src/handler.rs"),
        "Rust internal getPet call resolves within Rust: {clients:?}");
}
