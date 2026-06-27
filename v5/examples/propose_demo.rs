// Propose-demo: run the extract-fn proposer over a Rust file under each
// similarity kernel and print the contrast + top proposals.
//
//   verbatim  — exact line text (Type-1, name-dependent)
//   ast       — normalized CST leaf stream, idents/lits erased (Type-2, name-agnostic)
//
// Usage: cargo run --example propose_demo [path] [N] [kernel]
//   default path = src/engine.rs, N = 3, kernel = ast
use std::env;

fn main() {
    let default = format!("{}/src/engine.rs", env!("CARGO_MANIFEST_DIR"));
    let args: Vec<String> = env::args().collect();
    let path = args.get(1).cloned().unwrap_or(default);
    let n: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(3);
    let kernel = args.get(3).map(|s| s.as_str()).unwrap_or("ast");

    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| { eprintln!("read {path}: {e}"); std::process::exit(1); });
    let base = path.rsplit('/').next().unwrap_or(&path);

    let verbatim = sprefa_v5::propose::extract_proposals(&content);
    let ast = sprefa_v5::propose::ast_shape_proposals(&content);
    println!("== {} ==\n   verbatim (Type-1): {} blocks\n   ast-shape (Type-2): {} blocks\n",
             base, verbatim.len(), ast.len());

    let shown = match kernel {
        "verbatim" => &verbatim,
        _ => &ast,
    };
    println!("== top {} by {} kernel gain ==\n", n.min(shown.len()), kernel);
    for p in shown.iter().take(n) {
        println!("{}", sprefa_v5::propose::render_proposal(p, &content));
        println!();
    }
}
