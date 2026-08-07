// Writes the TEXT-keyed twin of a harness case. Topology comes from the
// committed tuner and generator, so the two inputs describe the same graph.

use exec_shootout_harness::gen::{self, Family};
use exec_shootout_harness::tuner;
use intern_bench::keys::{node_name, node_path, TEXT_HEADER_TAG};
use std::io::{BufWriter, Write};

fn family_from_name(name: &str) -> Family {
    match name {
        "chain" => Family::Chain,
        "layered" => Family::Layered,
        "grid" => Family::Grid,
        other => {
            eprintln!("gen_text: unknown family '{other}'");
            std::process::exit(1);
        }
    }
}

fn write_text_input(path: &str, node_count: u32, edges: &[(u32, u32)]) {
    let file = std::fs::File::create(path)
        .unwrap_or_else(|error| panic!("cannot create {path}: {error}"));
    let mut out = BufWriter::new(file);
    writeln!(out, "p {} {} {}", node_count, edges.len(), TEXT_HEADER_TAG).expect("write header");
    for (from, to) in edges {
        writeln!(
            out,
            "{}\t{}\t{}\t{}",
            node_path(*from),
            node_name(*from),
            node_path(*to),
            node_name(*to)
        )
        .expect("write edge");
    }
}

// Byte-for-byte the shape harness/src/main.rs writes, so a diff against the
// harness .in file proves both files describe the same edge list.
fn write_int_input(path: &str, node_count: u32, edges: &[(u32, u32)]) {
    let file = std::fs::File::create(path)
        .unwrap_or_else(|error| panic!("cannot create {path}: {error}"));
    let mut out = BufWriter::new(file);
    writeln!(out, "p {} {}", node_count, edges.len()).expect("write header");
    for (from, to) in edges {
        writeln!(out, "{} {}", from, to).expect("write edge");
    }
}

fn main() {
    let arguments: Vec<String> = std::env::args().collect();
    let mut family_name: Option<String> = None;
    let mut scale: u32 = 10_000;
    let mut text_out: Option<String> = None;
    let mut int_out: Option<String> = None;

    let mut index = 1;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--family" => {
                index += 1;
                family_name = arguments.get(index).cloned();
            }
            "--scale" => {
                index += 1;
                scale = arguments[index].parse().expect("scale not an int");
            }
            "--out" => {
                index += 1;
                text_out = arguments.get(index).cloned();
            }
            "--also-int" => {
                index += 1;
                int_out = arguments.get(index).cloned();
            }
            "--help" | "-h" => {
                eprintln!("gen_text --family chain|grid|layered --scale N --out PATH [--also-int PATH]");
                std::process::exit(0);
            }
            other => {
                eprintln!("gen_text: unknown argument '{other}'");
                std::process::exit(1);
            }
        }
        index += 1;
    }

    let family = family_from_name(&family_name.unwrap_or_else(|| {
        eprintln!("gen_text: missing --family");
        std::process::exit(1);
    }));
    let text_out = text_out.unwrap_or_else(|| {
        eprintln!("gen_text: missing --out");
        std::process::exit(1);
    });

    // The seed formula is harness/src/main.rs's; changing it would change the
    // layered topology and break the comparison against the .in file.
    let seed = scale as u64 ^ 0x5eed_cafe;
    let tuned = tuner::tune(family, scale);
    let generated = gen::generate(family, tuned.params, scale, seed);

    write_text_input(&text_out, generated.node_count, &generated.edges);
    if let Some(path) = int_out {
        write_int_input(&path, generated.node_count, &generated.edges);
    }

    eprintln!(
        "gen_text: family={} scale={} params={} nodes={} edges={} -> {}",
        gen::family_name(family),
        scale,
        gen::params_label(family, tuned.params),
        generated.node_count,
        generated.edges.len(),
        text_out
    );
}
