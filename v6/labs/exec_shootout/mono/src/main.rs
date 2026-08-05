use std::fs::File;
use std::io::{BufRead, BufReader};
use std::time::Instant;

use fxhash::{FxHashMap, FxHashSet};

// The relation is seen-set sharded per source node: one FxHashSet of
// targets per source. A membership test touches one bucket, never the
// whole derived set.
type Reachable = Vec<FxHashSet<u32>>;
type EdgeIndex = FxHashMap<u32, Vec<u32>>;

struct State {
    edges: usize,
    index: EdgeIndex,
    seen: Reachable,
    delta: Vec<(u32, u32)>,
}

fn main() {
    let input = find_input_arg();
    let trace = std::env::var("MONO_TRACE").is_ok();
    let start_at = Instant::now();

    let mut state = load(&input);

    let loaded_ms = start_at.elapsed().as_millis() as i64;
    println!("{{\"event\":\"loaded\",\"edges\":{},\"ms\":{}}}", state.edges, loaded_ms);

    let fix_start = Instant::now();
    derive(&mut state, trace);
    let fixpoint_ms = fix_start.elapsed().as_millis() as i64;
    println!("{{\"event\":\"fixpoint\",\"derived\":{},\"ms\":{}}}", derived_count(&state.seen), fixpoint_ms);

    let checksum = checksum(&state.seen);
    let peak_rss_kb = peak_rss_kb();
    println!("{{\"event\":\"done\",\"checksum\":\"{checksum:016x}\",\"peak_rss_kb\":{peak_rss_kb}}}");
}

fn find_input_arg() -> String {
    let args: Vec<String> = std::env::args().collect();
    let mut input = None;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--input" => {
                index += 1;
                input = Some(args.get(index).expect("--input needs a path").clone());
            }
            "--threads" => {
                index += 1;
            }
            other => panic!("unknown argument: {other}"),
        }
        index += 1;
    }
    input.expect("--input <path> is required")
}

fn insert_pair(seen: &mut Reachable, source: u32, target: u32) -> bool {
    let bucket = source as usize;
    if seen.len() <= bucket {
        seen.resize(bucket + 1, FxHashSet::default());
    }
    seen[bucket].insert(target)
}

fn load(path: &str) -> State {
    let file = File::open(path).unwrap_or_else(|why| panic!("open {path}: {why}"));
    let reader = BufReader::new(file);
    let mut index: EdgeIndex = FxHashMap::default();
    let mut seen: Reachable = Vec::new();
    let mut delta = Vec::<(u32, u32)>::new();
    let mut edges = 0usize;

    for (line_no, line) in reader.lines().enumerate() {
        let line = line.unwrap();
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let first = parts.next().expect("line has a token");
        if first == "p" {
            // Header: p <nodes> <edges>. Node count is not used by either rule.
            continue;
        }
        if first.chars().all(|ch| ch.is_ascii_digit()) {
            let tail: u32 = first.parse().expect("edge tail is a u32");
            let head: u32 = parts.next().expect("edge head").parse().expect("edge head is a u32");
            index.entry(tail).or_default().push(head);
            if insert_pair(&mut seen, tail, head) {
                edges += 1;
                delta.push((tail, head));
            }
        } else {
            panic!("unexpected token {first} on line {}", line_no + 1);
        }
    }
    State { edges, index, seen, delta }
}

fn derive(state: &mut State, trace: bool) {
    let mut round = 0u32;
    loop {
        let mut next = Vec::<(u32, u32)>::new();
        round += 1;
        for &(source, mid) in &state.delta {
            if let Some(outgoing) = state.index.get(&mid) {
                for &target in outgoing {
                    if insert_pair(&mut state.seen, source, target) {
                        next.push((source, target));
                    }
                }
            }
        }
            if trace {
                eprintln!("mono: round={round} delta={}", next.len());
            }
        if next.is_empty() {
            break;
        }
        state.delta = next;
    }
    state.delta = Vec::new();
}

fn derived_count(seen: &Reachable) -> i64 {
    seen.iter().fold(0i64, |count, bucket| count + bucket.len() as i64)
}

// fnv1a64 over the 8 little-endian bytes of (source, target), source first.
fn fnv1a64(source: u32, target: u32) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in source.to_le_bytes().iter().chain(target.to_le_bytes().iter()) {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn checksum(seen: &Reachable) -> u64 {
    let mut xor = 0u64;
    for (source, bucket) in seen.iter().enumerate() {
        for &target in bucket {
            xor ^= fnv1a64(source as u32, target);
        }
    }
    xor
}

fn peak_rss_kb() -> i64 {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    let ok = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    if ok != 0 {
        return -1;
    }
    let usage = unsafe { usage.assume_init() };
    // macOS reports ru_maxrss in bytes; the contract says /1024 for KB.
    (usage.ru_maxrss / 1024) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_three_seeds_and_joins() {
        let path = "/tmp/mono_chain.txt";
        std::fs::write(path, "p 3 2\n1 2\n2 3\n").unwrap();
        let mut state = load(path);
        derive(&mut state, false);
        assert_eq!(derived_count(&state.seen), 3);
        assert_eq!(checksum(&state.seen), 0xa6b23e50ed0dd5c5);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn stops_on_empty_delta() {
        let path = "/tmp/mono_stop.txt";
        std::fs::write(path, "p 2 1\n1 2\n").unwrap();
        let mut state = load(path);
        derive(&mut state, false);
        assert_eq!(derived_count(&state.seen), 1);
        assert!(state.delta.is_empty());
        std::fs::remove_file(path).unwrap();
    }
}

