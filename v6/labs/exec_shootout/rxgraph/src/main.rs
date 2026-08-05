//! exec_shootout rx graph engine binary.
//! Usage: <binary> --input <path>; emits three JSONL events, logs to stderr.

use std::io::{BufRead, BufReader};
use std::time::Instant;

use rxgraph::{build_reachability, Row};

const USAGE: &str = "usage: rxgraph --input <path>";

struct Input {
    edges: Vec<Row>,
    edge_count: usize,
}

fn read_input(path: &str) -> Result<Input, String> {
    let file = std::fs::File::open(path).map_err(|error| format!("open {}: {error}", path))?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();

    let header = lines
        .next()
        .ok_or_else(|| "empty input file".to_string())?
        .map_err(|error| format!("read header: {error}"))?;
    let mut tokens = header.split_whitespace();
    let marker = tokens
        .next()
        .ok_or_else(|| "missing p marker".to_string())?;
    if marker != "p" {
        return Err(format!("expected p marker, saw {marker}"));
    }
    let _node_count: u32 = tokens
        .next()
        .ok_or_else(|| "missing node count".to_string())?
        .parse()
        .map_err(|error| format!("bad node count: {error}"))?;
    let edge_count: usize = tokens
        .next()
        .ok_or_else(|| "missing edge count".to_string())?
        .parse()
        .map_err(|error| format!("bad edge count: {error}"))?;

    let mut edges = Vec::with_capacity(edge_count);
    for line in lines {
        let line = line.map_err(|error| format!("read line: {error}"))?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        let from: u32 = parts
            .next()
            .ok_or_else(|| format!("missing from in line: {trimmed}"))?
            .parse()
            .map_err(|error| format!("bad from: {error}"))?;
        let to: u32 = parts
            .next()
            .ok_or_else(|| format!("missing to in line: {trimmed}"))?
            .parse()
            .map_err(|error| format!("bad to: {error}"))?;
        if parts.next().is_some() {
            return Err(format!("too many tokens in line: {trimmed}"));
        }
        edges.push(Row { from, to });
    }

    if edges.len() != edge_count {
        return Err(format!("declared {edge_count} edges, read {}", edges.len()));
    }

    Ok(Input { edges, edge_count })
}

fn peak_rss_kb() -> i64 {
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    let result = unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
    if result != 0 {
        return -1;
    }
    let maxrss = usage.ru_maxrss as i64;
    if cfg!(target_os = "macos") {
        maxrss / 1024
    } else {
        maxrss
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let input_path = match find_input_arg(&args) {
        Some(path) => path,
        None => {
            eprintln!("{USAGE}");
            std::process::exit(1);
        }
    };

    let load_start = Instant::now();
    let parsed = match read_input(&input_path) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("rxgraph: {message}");
            std::process::exit(1);
        }
    };
    let load_ms = load_start.elapsed().as_millis() as i64;

    println!(
        "{{\"event\":\"loaded\",\"edges\":{},\"ms\":{}}}",
        parsed.edge_count, load_ms
    );

    let fixpoint_start = Instant::now();
    let mut program = build_reachability(&parsed.edges);
    let report = program.run(parsed.edges);
    let fixpoint_ms = fixpoint_start.elapsed().as_millis() as i64;

    println!(
        "{{\"event\":\"fixpoint\",\"derived\":{},\"ms\":{}}}",
        report.derived, fixpoint_ms
    );

    let checksum = format!("{:016x}", report.checksum);
    let peak_rss_kb = peak_rss_kb();
    println!("{{\"event\":\"done\",\"checksum\":\"{checksum}\",\"peak_rss_kb\":{peak_rss_kb}}}");

    eprintln!(
        "rounds={} operator_pushes={}",
        report.max_round, report.operator_pushes
    );
}

fn find_input_arg(args: &[String]) -> Option<String> {
    let mut index = 1;
    while index < args.len() {
        if args[index] == "--input" {
            return args.get(index + 1).cloned();
        }
        index += 1;
    }
    None
}
