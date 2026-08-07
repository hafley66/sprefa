// Loads a TEXT-keyed .tin, interns every column to u32, runs the shootout
// closure on the interned ids, then materializes the derived rows back to TEXT.

use intern_bench::engine_flat;
use intern_bench::engine2;
use intern_bench::intern::{Interner, NodeTable};
use intern_bench::keys::node_from_columns;
use intern_bench::textinput::{parse_edge, parse_header};
use intern_bench::{pair_checksum, peak_rss_kb};
use smallvec::SmallVec;
use std::time::Instant;

type Tuple = SmallVec<[u32; 4]>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Pair,
    PairFlat,
    Col4,
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Pair => "pair",
        Mode::PairFlat => "pair-flat",
        Mode::Col4 => "col4",
    }
}

struct Interned {
    edges: Vec<Tuple>,
    interner: Interner,
    nodes: NodeTable,
}

// One pass over the file: four string lookups per row, plus two pair lookups
// when the mode collapses each (path, name) endpoint into a node id.
fn intern_edges(contents: &str, mode: Mode) -> Result<Interned, String> {
    let mut lines = contents.lines();
    let header = lines.next().ok_or_else(|| "input file is empty".to_string())?;
    let declared = parse_header(header)?;

    let mut interner = Interner::default();
    let mut nodes = NodeTable::default();
    let mut edges: Vec<Tuple> = Vec::with_capacity(declared.edge_count as usize);

    for line in lines {
        if line.is_empty() {
            continue;
        }
        let edge = parse_edge(line)?;
        let from_path = interner.intern(edge.from_path);
        let from_name = interner.intern(edge.from_name);
        let to_path = interner.intern(edge.to_path);
        let to_name = interner.intern(edge.to_name);
        match mode {
            Mode::Col4 => {
                edges.push(Tuple::from_slice(&[from_path, from_name, to_path, to_name]))
            }
            Mode::Pair | Mode::PairFlat => {
                let from_node = nodes.node_for(from_path, from_name);
                let to_node = nodes.node_for(to_path, to_name);
                edges.push(Tuple::from_slice(&[from_node, to_node]));
            }
        }
    }

    Ok(Interned {
        edges,
        interner,
        nodes,
    })
}

struct Materialized {
    checksum: u64,
    bytes: usize,
    materialize_us: u128,
    checksum_us: u128,
}

// Materialize is the cost the seam pays handing rows back as TEXT; the checksum
// pass parses the ids back out and is a gate cost the seam would never pay.
fn materialize_and_check(
    rows: &[Tuple],
    key_width: usize,
    interner: &Interner,
    nodes: &NodeTable,
) -> Materialized {
    let columns_of = |value: u32| -> (u32, u32) {
        if key_width == 1 {
            nodes.columns(value)
        } else {
            panic!("columns_of is only for the pair modes");
        }
    };

    let mut buffer = String::with_capacity(256);
    let mut bytes = 0usize;
    let materialize_clock = Instant::now();
    for tuple in rows {
        buffer.clear();
        if key_width == 1 {
            let (from_path, from_name) = columns_of(tuple[0]);
            let (to_path, to_name) = columns_of(tuple[1]);
            buffer.push_str(interner.text(from_path));
            buffer.push('\t');
            buffer.push_str(interner.text(from_name));
            buffer.push('\t');
            buffer.push_str(interner.text(to_path));
            buffer.push('\t');
            buffer.push_str(interner.text(to_name));
        } else {
            buffer.push_str(interner.text(tuple[0]));
            buffer.push('\t');
            buffer.push_str(interner.text(tuple[1]));
            buffer.push('\t');
            buffer.push_str(interner.text(tuple[2]));
            buffer.push('\t');
            buffer.push_str(interner.text(tuple[3]));
        }
        bytes += buffer.len();
    }
    let materialize_us = materialize_clock.elapsed().as_micros();

    let checksum_clock = Instant::now();
    let mut checksum = 0u64;
    for tuple in rows {
        let (from_path, from_name, to_path, to_name) = if key_width == 1 {
            let (from_path, from_name) = columns_of(tuple[0]);
            let (to_path, to_name) = columns_of(tuple[1]);
            (from_path, from_name, to_path, to_name)
        } else {
            (tuple[0], tuple[1], tuple[2], tuple[3])
        };
        let left = node_from_columns(interner.text(from_path), interner.text(from_name))
            .expect("from columns decode to a node id");
        let right = node_from_columns(interner.text(to_path), interner.text(to_name))
            .expect("to columns decode to a node id");
        checksum ^= pair_checksum(left, right);
    }
    let checksum_us = checksum_clock.elapsed().as_micros();

    Materialized {
        checksum,
        bytes,
        materialize_us,
        checksum_us,
    }
}

fn parse_mode(token: &str) -> Mode {
    match token {
        "pair" => Mode::Pair,
        "pair-flat" => Mode::PairFlat,
        "col4" => Mode::Col4,
        other => {
            eprintln!("bench: unknown mode '{other}'");
            std::process::exit(1);
        }
    }
}

fn main() {
    let arguments: Vec<String> = std::env::args().collect();
    let mut input_path: Option<String> = None;
    let mut mode = Mode::Pair;

    let mut index = 1;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--input" => {
                index += 1;
                input_path = arguments.get(index).cloned();
            }
            "--mode" => {
                index += 1;
                mode = parse_mode(&arguments[index]);
            }
            "--help" | "-h" => {
                eprintln!("bench --input <path.tin> [--mode pair|pair-flat|col4]");
                std::process::exit(0);
            }
            other => {
                eprintln!("bench: unknown argument '{other}'");
                std::process::exit(1);
            }
        }
        index += 1;
    }
    let input_path = input_path.unwrap_or_else(|| {
        eprintln!("usage: bench --input <path.tin> [--mode pair|pair-flat|col4]");
        std::process::exit(1);
    });

    let read_clock = Instant::now();
    let contents = std::fs::read_to_string(&input_path).unwrap_or_else(|error| {
        eprintln!("cannot read input {input_path}: {error}");
        std::process::exit(1);
    });
    let read_us = read_clock.elapsed().as_micros();

    let intern_clock = Instant::now();
    let interned = intern_edges(&contents, mode).unwrap_or_else(|message| {
        eprintln!("input error: {message}");
        std::process::exit(1);
    });
    let intern_us = intern_clock.elapsed().as_micros();

    let edge_count = interned.edges.len();
    let key_width = if mode == Mode::Col4 { 2 } else { 1 };

    // interp charges the edge insert to its load phase, so this one does too or
    // the two load numbers are not the same measurement.
    let (derived, seed_us, fixpoint_us, materialized) = match mode {
        Mode::Pair => {
            let seed_clock = Instant::now();
            let mut program = engine2::build_program();
            for tuple in &interned.edges {
                program.relations[0].insert(tuple.clone());
            }
            let seed_us = seed_clock.elapsed().as_micros();
            let fixpoint_clock = Instant::now();
            let derived = engine2::semi_naive(&mut program);
            let fixpoint_us = fixpoint_clock.elapsed().as_micros();
            let materialized = materialize_and_check(
                &program.relations[1].rows,
                key_width,
                &interned.interner,
                &interned.nodes,
            );
            (derived, seed_us, fixpoint_us, materialized)
        }
        Mode::PairFlat | Mode::Col4 => {
            let seed_clock = Instant::now();
            let mut program = engine_flat::build_program(key_width);
            for tuple in &interned.edges {
                program.relations[0].insert(tuple.clone());
            }
            let seed_us = seed_clock.elapsed().as_micros();
            let fixpoint_clock = Instant::now();
            let derived = engine_flat::semi_naive(&mut program);
            let fixpoint_us = fixpoint_clock.elapsed().as_micros();
            let materialized = materialize_and_check(
                &program.relations[1].rows,
                key_width,
                &interned.interner,
                &interned.nodes,
            );
            (derived, seed_us, fixpoint_us, materialized)
        }
    };

    let load_us = read_us + intern_us + seed_us;
    let peak_rss_kb = unsafe { peak_rss_kb() };
    let mode_label = mode_name(mode);

    println!(
        "{{\"event\":\"loaded\",\"edges\":{edge_count},\"ms\":{},\"us\":{load_us},\"read_us\":{read_us},\"intern_us\":{intern_us},\"seed_us\":{seed_us},\"strings\":{},\"nodes\":{}}}",
        load_us / 1000,
        interned.interner.len(),
        interned.nodes.len()
    );
    println!(
        "{{\"event\":\"fixpoint\",\"derived\":{derived},\"ms\":{},\"us\":{fixpoint_us}}}",
        fixpoint_us / 1000
    );
    println!(
        "{{\"event\":\"materialize\",\"rows\":{derived},\"bytes\":{},\"ms\":{},\"us\":{}}}",
        materialized.bytes,
        materialized.materialize_us / 1000,
        materialized.materialize_us
    );
    println!(
        "{{\"event\":\"done\",\"mode\":\"{mode_label}\",\"checksum\":\"{:016x}\",\"checksum_us\":{},\"peak_rss_kb\":{peak_rss_kb}}}",
        materialized.checksum, materialized.checksum_us
    );
}
