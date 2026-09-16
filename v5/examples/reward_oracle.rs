// Reward oracle: measure each kernel's recall against the 3 validated
// refactor moves in research/refactor-reward/refactor_log.md.
//
// Two of the three validated moves are consolidation class (verbatim dup
// removal) — the clone-detection kernels' target. The third (iter 2) is a
// god-fn split (single oversized arm, no dup) which clone kernels don't
// address; it's listed for completeness but not scored.
//
// Usage: cargo run --example reward_oracle
//
// Output: hit/miss matrix + per-kernel recall %.

fn main() {
    let patterns = validated_patterns();
    let kernels = all_kernels();

    println!(
        "== reward oracle: {} validated consolidation patterns ==\n",
        patterns.len()
    );
    for (name, desc, _) in &patterns {
        println!("  {name}: {desc}");
    }
    println!();

    print!("{:<12}", "kernel");
    for (name, _, _) in &patterns {
        print!("{:>12}", name);
    }
    println!("{:>10}", "recall");

    let mut best_recall = 0.0;
    let mut best_kernel = "";
    for (kname, kfn) in &kernels {
        let mut hits = 0usize;
        print!("{:<12}", kname);
        for (_, _, src) in &patterns {
            let proposals = kfn(src);
            let caught = !proposals.is_empty();
            if caught {
                hits += 1;
            }
            print!(
                "{:>12}",
                format!(
                    "{} ({})",
                    if caught { "HIT" } else { "miss" },
                    proposals.len()
                )
            );
        }
        let recall = hits as f64 / patterns.len() as f64 * 100.0;
        println!("{:>9.0}%", recall);
        if recall > best_recall {
            best_recall = recall;
            best_kernel = kname;
        }
    }

    println!("\nbest: {} ({:.0}% recall)", best_kernel, best_recall);
}

type KernelFn = fn(&str) -> Vec<sprefa_v5::propose::Proposal>;

fn all_kernels() -> Vec<(&'static str, KernelFn)> {
    vec![
        ("verbatim", |s| sprefa_v5::propose::extract_proposals(s)),
        ("ast", |s| sprefa_v5::propose::ast_shape_proposals(s)),
        ("tree", |s| sprefa_v5::propose::tree_shape_proposals(s)),
        ("cfg", |s| sprefa_v5::propose::cfg_shape_proposals(s)),
        ("ddg", |s| sprefa_v5::propose::ddg_shape_proposals(s)),
        ("cgraph", |s| {
            sprefa_v5::propose::callgraph_shape_proposals(s)
        }),
        ("ngram", |s| sprefa_v5::propose::ngram_stat_proposals(s)),
    ]
}

/// Synthetic reconstructions of the two consolidation-class validated moves
/// from refactor_log.md. Each fixture contains the pre-refactor duplication
/// pattern; a kernel that catches it would have recommended the validated move.
fn validated_patterns() -> Vec<(&'static str, &'static str, String)> {
    vec![
        // Iter 1: bind_whole_match_span — 2-site verbatim block dup (~18 lines).
        // Two arms of parse_file had identical span-id + capture-binding logic.
        (
            "iter1_span",
            "2-site verbatim block dup (bind_whole_match_span pattern)",
            r#"fn parse_file() {
    if kind == "ast" {
        let lo = caps.iter().map(|c| c.1).min().unwrap();
        let hi = caps.iter().map(|c| c.2).max().unwrap();
        let span_id = where_bytes.insert(lo, hi, file);
        ext.push((span_id, lo, hi));
        for (n, t, cl, ch) in caps {
            let sid = where_bytes.insert(cl, ch, file);
            ext.push((sid, cl, ch));
        }
        let next_id = idv.clone();
        results.push((next_id, ext.clone()));
    }
    if kind == "sg" {
        let lo = caps.iter().map(|c| c.1).min().unwrap();
        let hi = caps.iter().map(|c| c.2).max().unwrap();
        let span_id = where_bytes.insert(lo, hi, file);
        ext.push((span_id, lo, hi));
        for (n, t, cl, ch) in caps {
            let sid = where_bytes.insert(cl, ch, file);
            ext.push((sid, cl, ch));
        }
        let next_id = idv.clone();
        results.push((next_id, ext.clone()));
    }
}
"#
            .to_string(),
        ),
        // Iter 3: bind_captures — 3-site verbatim short-block triplication.
        // Three arms had identical 4-line per-capture binding loops.
        (
            "iter3_caps",
            "3-site verbatim block triplication (bind_captures pattern)",
            r#"fn parse_file() {
    if arm == "ast" {
        let mut ext: Vec<Bind> = Vec::new();
        for (n, t, lo, hi) in caps {
            ext.push((n, t, lo, hi));
        }
        process(ext);
    }
    if arm == "sg" {
        let mut ext: Vec<Bind> = Vec::new();
        for (n, t, lo, hi) in caps {
            ext.push((n, t, lo, hi));
        }
        process(ext);
    }
    if arm == "yaml" {
        let mut ext: Vec<Bind> = Vec::new();
        for (n, t, lo, hi) in caps {
            ext.push((n, t, lo, hi));
        }
        process(ext);
    }
}
"#
            .to_string(),
        ),
    ]
}
