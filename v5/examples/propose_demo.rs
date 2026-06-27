// Propose-demo: run the extract-fn proposer over a Rust file under each
// similarity kernel and print the contrast + top proposals.
//
//   verbatim      — exact line text (Type-1, name-dependent)
//   ast           — normalized CST leaf stream, idents/lits erased (Type-2)
//   symbol        — CST leaf stream ⨝ resolved SCIP symbol per ident (Type-2 + semantic);
//                   needs an index.scip (rust-analyzer scip) next to the repo root
//
// Usage: cargo run --example propose_demo [path] [N] [kernel]
//   default path = src/engine.rs, N = 3, kernel = symbol
use std::env;

fn main() {
    let default = format!("{}/src/engine.rs", env!("CARGO_MANIFEST_DIR"));
    let args: Vec<String> = env::args().collect();
    let path = args.get(1).cloned().unwrap_or(default);
    let n: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(3);
    let kernel = args.get(3).map(|s| s.as_str()).unwrap_or("symbol");

    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| { eprintln!("read {path}: {e}"); std::process::exit(1); });
    let base = path.rsplit('/').next().unwrap_or(&path);

    let verbatim = sprefa_v5::propose::extract_proposals(&content);
    let ast = sprefa_v5::propose::ast_shape_proposals(&content);
    let tree = sprefa_v5::propose::tree_shape_proposals(&content);
    let cfg = sprefa_v5::propose::cfg_shape_proposals(&content);

    // symbol-shape + call-seq need the SCIP index; resolve it the same way the engine does.
    let repo_root = format!("{}/..", env!("CARGO_MANIFEST_DIR"));
    let idx = std::path::PathBuf::from(env::var("SPREFA_SCIP_INDEX")
        .unwrap_or_else(|_| format!("{repo_root}/index.scip")));
    let (sym_count, sym, call_count, call) = match sprefa_v5::scip_import::load(&idx) {
        Ok(rows) => {
            let rel = path.strip_prefix(&format!("{}/", env!("CARGO_MANIFEST_DIR")))
                .unwrap_or(&path).to_string();
            let spans: Vec<(i32, i32, &str)> = rows.occ_spans.iter()
                .filter(|(f, _, _, _)| f == &rel)
                .map(|(_, l, c, s)| (*l, *c, s.as_str())).collect();
            let s = sprefa_v5::propose::symbol_shape_proposals(&content, &spans);
            let c = sprefa_v5::propose::call_seq_proposals(&content, &spans);
            (s.len(), s, c.len(), c)
        }
        Err(e) => {
            eprintln!("[scip] no index at {}: {e}; skipping symbol + call-seq kernels", idx.display());
            (0, Vec::new(), 0, Vec::new())
        }
    };

    println!("== {} ==\n   verbatim (Type-1):        {} blocks\n   ast-shape (Type-2):       {} blocks\n   tree-iso (graph-iso):     {} blocks\n   cfg-shape (ctrl-flow):    {} blocks\n   symbol-shape (Type-2+sem): {} blocks\n   call-seq (dataflow):      {} blocks\n",
             base, verbatim.len(), ast.len(), tree.len(), cfg.len(), sym_count, call_count);

    let shown = match kernel {
        "verbatim" => &verbatim,
        "ast" => &ast,
        "tree" => &tree,
        "cfg" => &cfg,
        "call" => &call,
        _ => &sym,
    };
    if shown.is_empty() {
        eprintln!("(no {kernel} proposals to show)");
        return;
    }
    println!("== top {} by {kernel} kernel gain ==\n", n.min(shown.len()));
    for p in shown.iter().take(n) {
        println!("{}", sprefa_v5::propose::render_proposal(p, &content));
        println!();
    }
}
