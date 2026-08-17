use std::collections::HashSet;
use std::env;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct Edge {
    source: u32,
    target: u32,
}

struct Config {
    engine: PathBuf,
    input: PathBuf,
    ticks: usize,
    warmups: usize,
    passes: usize,
    seed: u64,
    work: PathBuf,
    label: String,
    output: PathBuf,
}

struct Graph {
    nodes: u32,
    edges: Vec<Edge>,
}

struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }
}

struct TickResult {
    wall_us: u128,
    derived: u64,
    checksum: String,
}

fn main() {
    let config = parse_args();
    let base = read_graph(&config.input);
    fs::create_dir_all(&config.work).expect("create work directory");
    let mut output = BufWriter::new(File::create(&config.output).expect("create output"));
    writeln!(
        output,
        "label\tphase\tpass\ttick\twall_us\tderived\tchecksum"
    )
    .unwrap();

    let total_passes = config.warmups + config.passes;
    let mut expected: Option<Vec<(u64, String)>> = None;
    let mut measured = Vec::<u128>::new();

    for pass_index in 0..total_passes {
        let phase = if pass_index < config.warmups {
            "warmup"
        } else {
            "measured"
        };
        let shown_pass = if phase == "warmup" {
            pass_index + 1
        } else {
            pass_index - config.warmups + 1
        };
        let results = run_pass(&config, &base);

        if phase == "measured" {
            let answers: Vec<(u64, String)> = results
                .iter()
                .map(|result| (result.derived, result.checksum.clone()))
                .collect();
            match &expected {
                Some(first) if first != &answers => {
                    panic!("derived/checksum sequence changed between measured passes")
                }
                None => expected = Some(answers),
                _ => {}
            }
            measured.extend(results.iter().map(|result| result.wall_us));
        }

        for (tick_index, result) in results.iter().enumerate() {
            writeln!(
                output,
                "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                config.label,
                phase,
                shown_pass,
                tick_index + 1,
                result.wall_us,
                result.derived,
                result.checksum
            )
            .unwrap();
        }
        output.flush().unwrap();

        let pass_times: Vec<u128> = results.iter().map(|result| result.wall_us).collect();
        print_summary(&config.label, phase, shown_pass, &pass_times);
    }

    print_summary(&config.label, "aggregate", config.passes, &measured);
}

fn parse_args() -> Config {
    let arguments: Vec<String> = env::args().collect();
    let mut engine = None;
    let mut input = None;
    let mut ticks = 100usize;
    let mut warmups = 1usize;
    let mut passes = 3usize;
    let mut seed = 0x6d65_7263_7572_7901u64;
    let mut work = None;
    let mut label = None;
    let mut output = None;
    let mut index = 1;
    while index < arguments.len() {
        let name = &arguments[index];
        index += 1;
        let value = arguments
            .get(index)
            .unwrap_or_else(|| panic!("{name} needs a value"));
        match name.as_str() {
            "--engine" => engine = Some(PathBuf::from(value)),
            "--input" => input = Some(PathBuf::from(value)),
            "--ticks" => ticks = value.parse().expect("ticks is an integer"),
            "--warmups" => warmups = value.parse().expect("warmups is an integer"),
            "--passes" => passes = value.parse().expect("passes is an integer"),
            "--seed" => seed = parse_seed(value),
            "--work" => work = Some(PathBuf::from(value)),
            "--label" => label = Some(value.clone()),
            "--output" => output = Some(PathBuf::from(value)),
            _ => panic!("unknown argument: {name}"),
        }
        index += 1;
    }
    Config {
        engine: engine.expect("--engine is required"),
        input: input.expect("--input is required"),
        ticks,
        warmups,
        passes,
        seed,
        work: work.expect("--work is required"),
        label: label.expect("--label is required"),
        output: output.expect("--output is required"),
    }
}

fn parse_seed(value: &str) -> u64 {
    if let Some(hex) = value.strip_prefix("0x") {
        u64::from_str_radix(hex, 16).expect("seed is hexadecimal")
    } else {
        value.parse().expect("seed is an integer")
    }
}

fn read_graph(path: &Path) -> Graph {
    let input = fs::read_to_string(path).expect("read input graph");
    let mut tokens = input.split_whitespace();
    assert_eq!(tokens.next(), Some("p"));
    let nodes = tokens.next().unwrap().parse().expect("node count");
    let declared_edges: usize = tokens.next().unwrap().parse().expect("edge count");
    let mut edges = Vec::with_capacity(declared_edges);
    while let Some(source) = tokens.next() {
        let target = tokens.next().expect("edge target");
        edges.push(Edge {
            source: source.parse().expect("edge source"),
            target: target.parse().expect("edge target"),
        });
    }
    assert_eq!(edges.len(), declared_edges);
    Graph { nodes, edges }
}

fn run_pass(config: &Config, base: &Graph) -> Vec<TickResult> {
    let mut rng = SplitMix64 { state: config.seed };
    let mut edges = base.edges.clone();
    let mut edge_set: HashSet<Edge> = edges.iter().copied().collect();
    assert_eq!(
        edge_set.len(),
        edges.len(),
        "input contains duplicate edges"
    );
    let tick_input = config.work.join("tick.in");
    let mut results = Vec::with_capacity(config.ticks);

    for _ in 0..config.ticks {
        let remove_index = rng.below(edges.len());
        let removed = edges.swap_remove(remove_index);
        assert!(edge_set.remove(&removed));
        let added = next_forward_edge(base.nodes, removed, &edge_set, &mut rng);
        assert!(edge_set.insert(added));
        edges.push(added);
        write_graph(&tick_input, base.nodes, &edges);
        results.push(run_engine(&config.engine, &tick_input));
    }
    results
}

fn next_forward_edge(
    nodes: u32,
    removed: Edge,
    edge_set: &HashSet<Edge>,
    rng: &mut SplitMix64,
) -> Edge {
    loop {
        let first = rng.below(nodes as usize) as u32;
        let second = rng.below(nodes as usize) as u32;
        if first == second {
            continue;
        }
        let candidate = Edge {
            source: first.min(second),
            target: first.max(second),
        };
        if candidate != removed && !edge_set.contains(&candidate) {
            return candidate;
        }
    }
}

fn write_graph(path: &Path, nodes: u32, edges: &[Edge]) {
    let mut output = BufWriter::new(File::create(path).expect("create tick input"));
    writeln!(output, "p {} {}", nodes, edges.len()).unwrap();
    for edge in edges {
        writeln!(output, "{} {}", edge.source, edge.target).unwrap();
    }
}

fn run_engine(engine: &Path, input: &Path) -> TickResult {
    let started = Instant::now();
    let output = Command::new(engine)
        .arg("--input")
        .arg(input)
        .output()
        .expect("run engine");
    let wall_us = started.elapsed().as_micros();
    if !output.status.success() {
        panic!(
            "engine failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let stdout = String::from_utf8(output.stdout).expect("engine stdout is UTF-8");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 3, "engine must emit three JSONL events");
    assert!(lines[0].contains("\"event\":\"loaded\""));
    assert!(lines[1].contains("\"event\":\"fixpoint\""));
    assert!(lines[2].contains("\"event\":\"done\""));
    TickResult {
        wall_us,
        derived: parse_u64_field(lines[1], "\"derived\":"),
        checksum: parse_string_field(lines[2], "\"checksum\":\""),
    }
}

fn parse_u64_field(line: &str, marker: &str) -> u64 {
    let tail = line.split_once(marker).expect("missing numeric field").1;
    let end = tail
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(tail.len());
    tail[..end].parse().expect("numeric field")
}

fn parse_string_field(line: &str, marker: &str) -> String {
    let tail = line.split_once(marker).expect("missing string field").1;
    tail[..tail.find('"').expect("unterminated string field")].to_string()
}

fn print_summary(label: &str, phase: &str, pass: usize, times: &[u128]) {
    let mut sorted = times.to_vec();
    sorted.sort_unstable();
    let median_us = if sorted.len() % 2 == 0 {
        (sorted[sorted.len() / 2 - 1] + sorted[sorted.len() / 2]) / 2
    } else {
        sorted[sorted.len() / 2]
    };
    let p95_index = (sorted.len() * 95).div_ceil(100) - 1;
    let total_us: u128 = sorted.iter().sum();
    println!(
        "summary\t{label}\t{phase}\t{pass}\tticks={}\tmedian_us={median_us}\tp95_us={}\ttotal_us={total_us}",
        sorted.len(), sorted[p95_index]
    );
}
