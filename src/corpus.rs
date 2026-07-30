//! The embedded `.dl` corpus: examples + std libs baked into the binary by
//! `build.rs`, so `dl examples` (list / semantic-search / show) and the
//! embedded `use` fallback work from a prebuilt download with no source tree.
//!
//!   dl examples                 list every embedded example (name + summary)
//!   dl examples <query…>         semantic search (cosine over the stub embedder)
//!   dl examples --show <name>    print one example/std lib to stdout (read/load)
//!   dl examples --std            list the embedded std libs (`use "std/…"`)
//!   dl examples -n K <query…>     top-K (default 8)
//!
//! "Loadable from the CLI without being on disk": `dl examples --show NAME`
//! prints the body, so `dl <(dl examples --show openapi-lsp)` runs an
//! embedded example directly. `use "std/foo.dl"` resolves a real std lib first,
//! then an EMBEDDED EXAMPLE of the same name (`use "std/gh-checkout.dl"` →
//! examples/gh-checkout.dl) — examples fill in the std/ library namespace. The
//! `std/` prefix is required for examples; bare names reject. Demo `?` queries
//! are stripped on splice so an example loads as a library, not a demo (see
//! frontend.rs).

use anyhow::Result;

include!(concat!(env!("OUT_DIR"), "/embedded_corpus.rs"));

pub fn examples() -> &'static [(&'static str, &'static str)] {
    EMBEDDED_EXAMPLES
}
pub fn std_libs() -> &'static [(&'static str, &'static str)] {
    EMBEDDED_STD
}

/// Embedded std lib body for a `use` path like `"std/callgraph.dl"`. Real std
/// libs win on name clash; otherwise an example of the same name fills in under
/// the `std/` namespace, so `use "std/gh-checkout.dl"` resolves to the embedded
/// `examples/gh-checkout.dl` from a prebuilt binary with no source tree. The
/// `std/` prefix is required for examples (the path is the library namespace —
/// a bare `use "gh-checkout"` does NOT resolve).
pub fn std_lib(path: &str) -> Option<&'static str> {
    if let Some((_, b)) = EMBEDDED_STD.iter().find(|(n, _)| *n == path) {
        return Some(b);
    }
    // Examples fill in the std/ namespace. Strip the prefix + optional .dl so
    // `std/foo.dl`, `std/foo` all match `examples/foo.dl`.
    let stripped = path.strip_prefix("std/")?;
    let want = stripped.trim_end_matches(".dl");
    EMBEDDED_EXAMPLES
        .iter()
        .find(|(n, _)| n.trim_end_matches(".dl") == want)
        .map(|(_, b)| *b)
}

/// Example body by name, with or without the `.dl` suffix.
pub fn example(name: &str) -> Option<&'static str> {
    let want = name.trim_end_matches(".dl");
    EMBEDDED_EXAMPLES
        .iter()
        .find(|(n, _)| *n == name || n.trim_end_matches(".dl") == want)
        .map(|(_, b)| *b)
}

/// First comment line of a `.dl` body (the corpus summary convention).
pub fn summary(body: &str) -> String {
    body.lines()
        .next()
        .map(|l| l.trim_start_matches('#').trim().to_string())
        .unwrap_or_default()
}

/// Cosine-rank examples against the query using the active embedder (the `stub`
/// floor always exists: deterministic, offline, token-overlap — enough to "yolo"
/// a search by intent). Doc text = filename + first-line summary.
pub fn search(query: &str, k: usize) -> Result<Vec<(&'static str, f32, String)>> {
    let emb = crate::embed::make(None)?;
    let docs: Vec<String> = examples()
        .iter()
        .map(|(n, b)| format!("{n} {}", summary(b)))
        .collect();
    let mut inputs: Vec<&str> = docs.iter().map(String::as_str).collect();
    inputs.push(query);
    let mut vecs = emb.encode(&inputs)?;
    for v in &mut vecs {
        crate::embed::l2_normalize(v);
    }
    let qv = vecs.pop().unwrap();
    let mut scored: Vec<(&'static str, f32, String)> = examples()
        .iter()
        .zip(vecs.iter())
        .map(|((n, b), v)| (*n, crate::embed::cosine(&qv, v), summary(b)))
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(k);
    Ok(scored)
}

/// `dl examples …`. Returns the process exit code.
pub fn run(args: &[String]) -> Result<i32> {
    let mut show: Option<String> = None;
    let mut list_std = false;
    let mut k = 8usize;
    let mut query: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--show" | "-s" => {
                i += 1;
                show = args.get(i).cloned();
            }
            "--std" => list_std = true,
            "-n" => {
                i += 1;
                k = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(8);
            }
            "-h" | "--help" => {
                print_help();
                return Ok(0);
            }
            other => query.push(other.to_string()),
        }
        i += 1;
    }

    if let Some(name) = show {
        if let Some(body) = example(&name).or_else(|| std_lib(&name)) {
            print!("{body}");
            return Ok(0);
        }
        eprintln!("dl examples: no example or std lib named {name:?}"); // @eprintln-ok: final user-facing error print at CLI top level
        return Ok(1);
    }
    if list_std {
        println!(
            "embedded std libs ({}), `use \"<name>\".`:",
            std_libs().len()
        );
        for (n, b) in std_libs() {
            println!("  {n}\n      {}", summary(b));
        }
        return Ok(0);
    }
    if query.is_empty() {
        println!("embedded examples ({}):", examples().len());
        for (n, b) in examples() {
            println!("  {n}\n      {}", summary(b));
        }
        println!("\nsearch:  dl examples <query>     show:  dl examples --show <name>");
        return Ok(0);
    }

    let q = query.join(" ");
    println!(
        "examples for {q:?} (top {k}, cosine over {} via {}):",
        examples().len(),
        crate::embed::make(None).map(|e| e.name()).unwrap_or("stub")
    );
    for (name, score, sum) in search(&q, k)? {
        println!("  {score:.3}  {name}\n         {sum}");
    }
    println!("\nread one:  dl examples --show <name>");
    Ok(0)
}

fn print_help() {
    eprintln!("usage: dl examples [<query>…] [--show NAME] [--std] [-n K]"); // @eprintln-ok: usage/help text
}
