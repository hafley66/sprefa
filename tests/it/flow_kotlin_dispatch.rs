//! Kotlin interface dispatch + multi-op participation (bench/flow/kotlin_flow.dl).
//!
//! The capability question: can a query name the function that sits on the
//! call-graph path of >= 2 OpenAPI ops, when the path runs THROUGH an interface
//! -> concrete-override hop? A static occurrence graph (scip_ref / scip_fn_edge)
//! never has that hop: the call site only names the interface method. SCIP emits
//! the override relationship separately (`SymbolInformation.relationships`,
//! is_implementation), which the loader surfaces as `scip_impl(impl, iface)`.
//!
//! Topology (one generated client interface, one impl, one shared helper):
//!
//!   PetApi#getPet()    --dispatch-->  PetApiImpl#getPet()    --calls-->  HttpClient#exec()
//!   PetApi#createPet() --dispatch-->  PetApiImpl#createPet() --calls-->  HttpClient#exec()
//!
//! getPet and createPet are the two spec ops; their interface methods are the
//! entries; HttpClient#exec is the shared helper reached from BOTH, so it is the
//! sole `multi_op`. The index is hand-built (the `scip` crate) so the test is
//! deterministic and toolchain-free; the monikers are shaped like scip-java /
//! scip-kotlin output (`pkg/Type#method().`). The dispatch hop is the only new
//! primitive being exercised; everything else is the existing flow machinery.

use protobuf::Message;
use scip::types::{Document, Index, Occurrence, Relationship, SymbolInformation, SymbolRole};
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");
const FLOW: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/bench/flow");

fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for e in std::fs::read_dir(src).unwrap().flatten() {
        let p = e.path();
        let d = dst.join(e.file_name());
        if p.is_dir() {
            copy_dir(&p, &d);
        } else {
            std::fs::copy(&p, &d).unwrap();
        }
    }
}

fn occ(line0: i32, col0: i32, end_col: i32, symbol: &str, def: bool) -> Occurrence {
    Occurrence {
        range: vec![line0, col0, end_col],
        symbol: symbol.to_string(),
        symbol_roles: if def {
            SymbolRole::Definition as i32
        } else {
            0
        },
        ..Default::default()
    }
}

/// A SymbolInformation carrying one is_implementation relationship: `impl`
/// overrides `iface`. This is the row that becomes `scip_impl(impl, iface)`.
fn impl_si(impl_sym: &str, iface_sym: &str) -> SymbolInformation {
    let mut rel = Relationship::new();
    rel.symbol = iface_sym.to_string();
    rel.is_implementation = true;
    SymbolInformation {
        symbol: impl_sym.to_string(),
        relationships: vec![rel],
        ..Default::default()
    }
}

fn build_index() -> Index {
    let iface_get = "scip-java maven . . api/PetApi#getPet().";
    let iface_create = "scip-java maven . . api/PetApi#createPet().";
    let impl_get = "scip-java maven . . api/PetApiImpl#getPet().";
    let impl_create = "scip-java maven . . api/PetApiImpl#createPet().";
    let helper = "scip-java maven . . api/HttpClient#exec().";

    // The generated client interface: one method per op.
    let api = Document {
        language: "kotlin".into(),
        relative_path: "src/PetApi.kt".into(),
        occurrences: vec![
            occ(1, 4, 10, iface_get, true),
            occ(2, 4, 13, iface_create, true),
        ],
        ..Default::default()
    };
    // The concrete impl: each override calls the shared helper in its body. The
    // is_implementation edges live on the impl methods' SymbolInformation.
    let imp = Document {
        language: "kotlin".into(),
        relative_path: "src/PetApiImpl.kt".into(),
        occurrences: vec![
            occ(1, 8, 14, impl_get, true),    // override fun getPet
            occ(2, 8, 12, helper, false),     //   exec(...)  -> fn_edge impl_get->helper
            occ(5, 8, 17, impl_create, true), // override fun createPet
            occ(6, 8, 12, helper, false),     //   exec(...)  -> fn_edge impl_create->helper
        ],
        symbols: vec![
            impl_si(impl_get, iface_get),
            impl_si(impl_create, iface_create),
        ],
        ..Default::default()
    };
    // The shared helper's home.
    let http = Document {
        language: "kotlin".into(),
        relative_path: "src/HttpClient.kt".into(),
        occurrences: vec![occ(1, 8, 12, helper, true)],
        ..Default::default()
    };
    Index {
        documents: vec![api, imp, http],
        ..Default::default()
    }
}

/// rows of a `? rel` block from dl stdout (tab-split values).
fn query_block<'a>(stdout: &'a str, header: &str) -> Vec<Vec<&'a str>> {
    let mut rows = Vec::new();
    let mut in_q = false;
    for line in stdout.lines() {
        if line.starts_with(header) {
            in_q = true;
            continue;
        }
        if line.starts_with('?') {
            in_q = false;
            continue;
        }
        if in_q {
            let t = line.trim();
            if t.is_empty() || t.starts_with('(') {
                continue;
            }
            rows.push(t.split('\t').collect());
        }
    }
    rows
}

#[test]
fn interface_dispatch_finds_function_on_two_op_paths() {
    let root = std::env::temp_dir().join(format!("flow_kt_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    copy_dir(&PathBuf::from(FLOW), &root);
    std::fs::write(
        root.join("index.scip"),
        build_index().write_to_bytes().unwrap(),
    )
    .unwrap();

    let out = Command::new(DL)
        .arg(root.join("dispatch_flow.dl"))
        .current_dir(&root)
        .args(["--db", root.join("kt.db").to_str().unwrap()])
        .output()
        .expect("run dl");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "dl failed: {}\n{stdout}",
        String::from_utf8_lossy(&out.stderr)
    );

    // both ops surface from the spec.
    let ops = query_block(&stdout, "? spec_op");
    assert!(
        ops.iter().any(|r| r == &["getPet"]),
        "spec_op getPet: {ops:?}\n{stdout}"
    );
    assert!(
        ops.iter().any(|r| r == &["createPet"]),
        "spec_op createPet: {ops:?}"
    );

    // each op's entry is the INTERFACE method (the iface target of scip_impl).
    let entries = query_block(&stdout, "? op_entry");
    assert!(
        entries
            .iter()
            .any(|r| r == &["getPet", "scip-java maven . . api/PetApi#getPet()."]),
        "getPet entry is the interface method: {entries:?}"
    );

    // the shared helper is reached from BOTH op entries via the dispatch hop, so
    // it is the multi_op hit. Without scip_impl there is no edge from either
    // interface method into the impl body, and this set would be empty.
    let multi = query_block(&stdout, "? multi_op");
    assert!(
        multi
            .iter()
            .any(|r| r == &["scip-java maven . . api/HttpClient#exec()."]),
        "HttpClient#exec is on both op paths: {multi:?}\n{stdout}"
    );
    // the per-op impl override is on exactly one path, so it is NOT multi_op.
    assert!(
        !multi
            .iter()
            .any(|r| r.first() == Some(&"scip-java maven . . api/PetApiImpl#getPet().")),
        "an impl override belongs to a single op, not multi_op: {multi:?}"
    );
}
