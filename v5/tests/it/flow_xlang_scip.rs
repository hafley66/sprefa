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
/// is its OWN symbol; the consumer's callsites reference the GENERATED lib symbol
/// (cross-file), so `scip_ref` resolves each call back to the lib def.
fn build_index() -> Index {
    // backend handler (handwritten Rust), the generated TS lib symbols, and the
    // consuming app — the app's refs carry the LIB's symbols (resolved imports).
    let rs_get = "rust-analyzer cargo flow_handler 0.1.0 handler/getPet().";
    let rs_create = "rust-analyzer cargo flow_handler 0.1.0 handler/createPet().";
    let lib_get = "scip-typescript npm . . lib/`petclient.ts`/getPet().";
    let lib_create = "scip-typescript npm . . lib/`petclient.ts`/createPet().";
    let app_load = "scip-typescript npm . . app/`pages.ts`/loadPetPage().";

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
    let lib = Document {
        language: "typescript".into(),
        relative_path: "ts/lib/petclient.ts".into(),
        occurrences: vec![
            occ(4, 16, 22, lib_get, true),       // export function getPet
            occ(8, 16, 25, lib_create, true),    // export function createPet
        ],
        ..Default::default()
    };
    let app = Document {
        language: "typescript".into(),
        relative_path: "ts/app/pages.ts".into(),
        occurrences: vec![
            occ(5, 9, 15, lib_get, false),       // import { getPet, createPet }
            occ(5, 17, 26, lib_create, false),
            occ(7, 16, 27, app_load, true),      // export function loadPetPage
            occ(8, 9, 15, lib_get, false),       // loadPetPage calls getPet
            occ(11, 9, 15, lib_get, false),      // refreshPet calls getPet (2nd)
            occ(15, 9, 18, lib_create, false),   // onCreateClicked calls createPet
        ],
        ..Default::default()
    };
    Index { documents: vec![rust, lib, app], ..Default::default() }
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
fn scip_resolves_generated_symbol_and_its_consumers() {
    let eng = run();

    // gen_def: the generated lib symbol for getPet, keyed by symbol + banner.
    let gens: Vec<(String, String, String)> = eng
        .query_sql("SELECT \"op\",\"sym\",\"file\" FROM rel_gen_def", &[]).unwrap()
        .into_iter().map(|r| (s(&r[0]), s(&r[1]), s(&r[2]))).collect();
    let getpet_gen: Vec<_> = gens.iter().filter(|(op, _, _)| op == "getPet").collect();
    assert_eq!(getpet_gen.len(), 1, "getPet has exactly one generated symbol: {gens:?}");
    assert_eq!(getpet_gen[0].2, "ts/lib/petclient.ts", "generated symbol is in the lib: {gens:?}");

    // impl_sym: the handwritten Rust backend handler (distinct symbol, .rs role).
    let impls: Vec<(String, String, String)> = eng
        .query_sql("SELECT \"op\",\"sym\",\"file\" FROM rel_impl_sym", &[]).unwrap()
        .into_iter().map(|r| (s(&r[0]), s(&r[1]), s(&r[2]))).collect();
    let getpet_impls: Vec<_> = impls.iter().filter(|(op, _, _)| op == "getPet").collect();
    assert_eq!(getpet_impls.len(), 1, "getPet has exactly one impl (the Rust handler): {impls:?}");
    assert_eq!(getpet_impls[0].2, "rust/src/handler.rs", "impl is the Rust file: {impls:?}");
    // the generated lib symbol is NOT a backend impl (it's a .ts file).
    assert!(!impls.iter().any(|(_, _, f)| f.ends_with(".ts")),
        "generated TS lib must not tag as backend impl: {impls:?}");

    // consumer: the blast radius — every resolved ref to the generated symbol.
    // The app's import + two callsites all reference the LIB's getPet symbol, so
    // the consuming file lights up (deduped to one file-level row by the loader).
    let consumers: Vec<(String, String)> = eng
        .query_sql("SELECT \"op\",\"file\" FROM rel_consumer", &[]).unwrap()
        .into_iter().map(|r| (s(&r[0]), s(&r[1]))).collect();
    assert!(consumers.iter().any(|(op, f)| op == "getPet" && f == "ts/app/pages.ts"),
        "getPet blast radius includes the consuming app: {consumers:?}");
    // the Rust handler's internal getPet call references the HANDLER symbol, not
    // the generated lib symbol, so it is not a consumer of the operation.
    assert!(!consumers.iter().any(|(op, f)| op == "getPet" && f == "rust/src/handler.rs"),
        "non-generated internal calls are not consumers: {consumers:?}");
}
