// Propose-demo: run the extract-fn proposer over a Rust file and print the top
// recommendations ranked by predicted dup-removal gain, each rendered as the
// Rust fn sprefa would extract. sprefa ranking its own refactor opportunities.
//
// Usage: cargo run --example propose_demo [path] [N]
//   default path = this crate's src/engine.rs, N = 3
use std::env;

fn main() {
    let default = format!("{}/src/engine.rs", env!("CARGO_MANIFEST_DIR"));
    let args: Vec<String> = env::args().collect();
    let path = args.get(1).cloned().unwrap_or(default);
    let n: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(3);

    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| { eprintln!("read {path}: {e}"); std::process::exit(1); });
    let props = sprefa_v5::propose::extract_proposals(&content);
    let base = path.rsplit('/').next().unwrap_or(&path);
    println!("== {} proposals in {}, top {} by gain ==\n",
             props.len(), base, n.min(props.len()));
    for p in props.iter().take(n) {
        println!("{}", sprefa_v5::propose::render_proposal(p, &content));
        println!();
    }
}
