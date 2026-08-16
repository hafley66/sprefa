//! CLI door for the CPG protobuf importer: one `CpgStruct`-encoded file in,
//! decoded node/edge kind counts out. `cargo run --example cpg_import -- FILE`.

use std::path::PathBuf;

use sprefa_extract::decode_cpg_struct;

fn run() -> Result<(), String> {
    let mut argv = std::env::args().skip(1);
    let path = argv
        .next()
        .map(PathBuf::from)
        .ok_or("usage: cpg_import <CpgStruct.bin>")?;
    let bytes = std::fs::read(&path).map_err(|err| format!("read {}: {err}", path.display()))?;
    let import = decode_cpg_struct(&bytes).map_err(|err| err.to_string())?;

    println!("nodes={} edges={}", import.nodes.len(), import.edges.len());
    for node in &import.nodes {
        println!("node key={} kind={:?}", node.key, node.kind);
    }
    for edge in &import.edges {
        println!("edge {} -> {} kind={:?}", edge.src, edge.dst, edge.kind);
    }
    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("cpg_import: {err}"); // @eprintln-ok: example CLI usage line
        std::process::exit(2);
    }
}
