// Kernel-compare: run all 5 similarity kernels on a file, compute per-kernel
// quality metrics, pairwise overlap, and multi-kernel consensus (proposals
// found by ≥K kernels = high-confidence refactor targets).
//
// Usage: cargo run --example kernel_compare [path]
//   default path = src/engine.rs
//
// Metrics per kernel:
//   blocks       — raw proposal count
//   ranges       — distinct line ranges (deduped overlapping windows)
//   mean/max gain, params — quality distribution
//   0-param      — pure duplication (highest signal, no inputs to thread)
//   >10-param    — extraction-infeasible (noise ceiling)
//
// Overlap matrix: proposals sharing ≥1 line with another kernel's proposal.
// Consensus: line ranges found by N kernels, ranked by agreement × gain.
use std::collections::HashMap;

fn main() {
    let default = format!("{}/src/engine.rs", env!("CARGO_MANIFEST_DIR"));
    let path = std::env::args().nth(1).unwrap_or(default);
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| { eprintln!("read {path}: {e}"); std::process::exit(1); });
    let base = path.rsplit('/').next().unwrap_or(&path);

    let verbatim = sprefa_v5::propose::extract_proposals(&content);
    let ast = sprefa_v5::propose::ast_shape_proposals(&content);
    let tree = sprefa_v5::propose::tree_shape_proposals(&content);
    let cfg = sprefa_v5::propose::cfg_shape_proposals(&content);
    let ddg = sprefa_v5::propose::ddg_shape_proposals(&content);
    let callgraph = sprefa_v5::propose::callgraph_shape_proposals(&content);
    let ngram = sprefa_v5::propose::ngram_stat_proposals(&content);

    let repo_root = format!("{}/..", env!("CARGO_MANIFEST_DIR"));
    let idx = std::path::PathBuf::from(std::env::var("SPREFA_SCIP_INDEX")
        .unwrap_or_else(|_| format!("{repo_root}/index.scip")));
    let (sym, call) = match sprefa_v5::scip_import::load(&idx) {
        Ok(rows) => {
            let rel = path.strip_prefix(&format!("{}/", env!("CARGO_MANIFEST_DIR")))
                .unwrap_or(&path).to_string();
            let suffix = std::path::Path::new(&rel)
                .components().rev().take(4)
                .collect::<Vec<_>>()
                .into_iter().rev()
                .collect::<std::path::PathBuf>()
                .to_string_lossy().to_string();
            let spans: Vec<(i32, i32, &str)> = rows.occ_spans.iter()
                .filter(|(f, _, _, _)| f == &rel || f.ends_with(&suffix))
                .map(|(_, l, c, s)| (*l, *c, s.as_str())).collect();
            eprintln!("[scip] {} occurrences matched (suffix={})", spans.len(), suffix);
            let s = sprefa_v5::propose::symbol_shape_proposals(&content, &spans);
            let c = sprefa_v5::propose::call_seq_proposals(&content, &spans);
            (s, c)
        }
        Err(e) => {
            eprintln!("[scip] {}: {e}; skipping symbol + call-seq", idx.display());
            (Vec::new(), Vec::new())
        }
    };

    let kernels: Vec<(&str, &[sprefa_v5::propose::Proposal])> = vec![
        ("verbatim", &verbatim),
        ("ast", &ast),
        ("tree", &tree),
        ("cfg", &cfg),
        ("ddg", &ddg),
        ("cgraph", &callgraph),
        ("ngram", &ngram),
        ("symbol", &sym),
        ("call", &call),
    ];

    println!("== {base} ==\n");

    // Per-kernel metrics.
    println!("{:<10} {:>7} {:>7} {:>9} {:>9} {:>9} {:>9} {:>9} {:>10}",
        "kernel", "blocks", "ranges", "mean_gn", "max_gn", "mean_pm", "max_pm", "0-param", ">10pm");
    let mut all_ranges: Vec<(usize, usize, usize, &str)> = Vec::new();
    for (name, props) in &kernels {
        let count = props.len();
        let ranges = dedup_ranges(props);
        let gains: Vec<usize> = props.iter().map(|p| p.gain).collect();
        let params: Vec<usize> = props.iter().map(|p| p.params.len()).collect();
        let zero_p = props.iter().filter(|p| p.params.is_empty()).count();
        let big_p = props.iter().filter(|p| p.params.len() > 10).count();
        let mean_g = if gains.is_empty() { 0.0 } else { gains.iter().sum::<usize>() as f64 / gains.len() as f64 };
        let max_g = gains.iter().copied().max().unwrap_or(0);
        let mean_p = if params.is_empty() { 0.0 } else { params.iter().sum::<usize>() as f64 / params.len() as f64 };
        let max_p = params.iter().copied().max().unwrap_or(0);
        println!("{:<10} {:>7} {:>7} {:>9.1} {:>9} {:>9.1} {:>9} {:>9} {:>10}",
            name, count, ranges.len(), mean_g, max_g, mean_p, max_p, zero_p, big_p);
        for p in &ranges {
            all_ranges.push((p.0, p.1, p.2, name));
        }
    }

    // Effect of feasibility filter (min_lines=5 + max_params=10) + weighted-gain.
    println!("\n== feasibility filter (≥5 lines, ≤10 params) + weighted-gain impact ==");
    println!("{:<10} {:>8} {:>8} {:>12} {:>12}",
        "kernel", "raw_blk", "feasible", "raw_top_gain", "wgt_top_gain");
    for (name, props) in &kernels {
        let filtered = sprefa_v5::propose::feasibility_filter(props.to_vec());
        let raw_top = props.iter().map(|p| p.gain).max().unwrap_or(0);
        let wgt_top = filtered.iter().map(|p| sprefa_v5::propose::weighted_gain(p)).max().unwrap_or(0);
        println!("{:<10} {:>8} {:>8} {:>12} {:>12}",
            name, props.len(), filtered.len(), raw_top, wgt_top);
    }

    // Pairwise overlap matrix.
    println!("\n== pairwise overlap (ranges shared) ==");
    print!("{:<10}", "");
    for (n, _) in &kernels {
        print!("{:>8}", n);
    }
    println!();
    for (na, pa) in &kernels {
        let ra = dedup_ranges(pa);
        print!("{:<10}", na);
        for (_, pb) in &kernels {
            let rb = dedup_ranges(pb);
            let overlap = ra.iter()
                .filter(|a| rb.iter().any(|b| ranges_overlap(a, b)))
                .count();
            print!("{:>8}", overlap);
        }
        println!();
    }

    // Consensus: ranges found by ≥2 kernels.
    let consensus = consensus_ranges(&kernels);
    println!("\n== multi-kernel consensus (range found by ≥2 kernels) ==");
    println!("   {} consensus ranges", consensus.len());
    let mut sorted = consensus.clone();
    sorted.sort_by(|a, b| (b.3, b.2).cmp(&(a.3, a.2)));
    for (lo, hi, gain, count, names) in sorted.iter().take(10) {
        println!("   {} kernels  L{}-{}  gain={}  [{}]",
            count, lo, hi, gain, names.join(","));
    }
}

type Range = (usize, usize, usize);

fn dedup_ranges(props: &[sprefa_v5::propose::Proposal]) -> Vec<Range> {
    let mut seen: Vec<Range> = Vec::new();
    for p in props {
        let r = (p.lo, p.hi, p.gain);
        if !seen.iter().any(|s| ranges_overlap(s, &r)) {
            seen.push(r);
        }
    }
    seen
}

fn ranges_overlap(a: &Range, b: &Range) -> bool {
    a.0 <= b.1 && b.0 <= a.1
}

fn consensus_ranges(kernels: &[(&str, &[sprefa_v5::propose::Proposal])]) -> Vec<(usize, usize, usize, usize, Vec<String>)> {
    let mut triples: Vec<(usize, usize, usize, &str)> = Vec::new();
    for (name, props) in kernels {
        for r in dedup_ranges(props) {
            triples.push((r.0, r.1, r.2, name));
        }
    }
    let n = triples.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut Vec<usize>, i: usize) -> usize {
        if parent[i] != i {
            parent[i] = find(parent, parent[i]);
        }
        parent[i]
    }
    for i in 0..n {
        for j in (i + 1)..n {
            if triples[i].0 <= triples[j].1 && triples[j].0 <= triples[i].1 {
                let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                if ri != rj {
                    parent[ri] = rj;
                }
            }
        }
    }
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        groups.entry(find(&mut parent, i)).or_default().push(i);
    }
    let mut out: Vec<(usize, usize, usize, usize, Vec<String>)> = Vec::new();
    for (_, members) in &groups {
        let mut names: Vec<String> = Vec::new();
        for &i in members {
            let k = triples[i].3.to_string();
            if !names.contains(&k) {
                names.push(k);
            }
        }
        if names.len() < 2 {
            continue;
        }
        let max_gain = members.iter().map(|&i| triples[i].2).max().unwrap_or(0);
        let lo = members.iter().map(|&i| triples[i].0).min().unwrap_or(0);
        let hi = members.iter().map(|&i| triples[i].1).max().unwrap_or(0);
        names.sort();
        out.push((lo, hi, max_gain, names.len(), names));
    }
    out.sort_by(|a, b| (b.3, b.2).cmp(&(a.3, a.2)));
    out
}
