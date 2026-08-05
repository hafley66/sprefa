use fxhash::{FxHashMap, FxHashSet};
use std::io::BufRead;

pub fn read_input(path: &str) -> (u32, u32, Vec<(u32, u32)>) {
    let file = std::fs::File::open(path)
        .unwrap_or_else(|error| panic!("cannot open input {}: {}", path, error));
    let reader = std::io::BufReader::new(file);
    let mut lines = reader.lines();
    let header = lines
        .next()
        .expect("input has no header line")
        .expect("cannot read header");
    let mut header_tokens = header.split_whitespace();
    let kind = header_tokens.next().expect("header missing kind");
    if kind != "p" {
        panic!("input header must start with 'p', got '{}'", kind);
    }
    let nodes: u32 = header_tokens
        .next()
        .expect("header missing node count")
        .parse()
        .expect("node count not a u32");
    let edge_count: u32 = header_tokens
        .next()
        .expect("header missing edge count")
        .parse()
        .expect("edge count not a u32");
    let mut edges = Vec::with_capacity(edge_count as usize);
    for line in lines {
        let line = line.expect("cannot read edge line");
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut tokens = trimmed.split_whitespace();
        let from: u32 = tokens
            .next()
            .expect("edge line missing from")
            .parse()
            .expect("from not a u32");
        let to: u32 = tokens
            .next()
            .expect("edge line missing to")
            .parse()
            .expect("to not a u32");
        edges.push((from, to));
    }
    (nodes, edge_count, edges)
}

fn encode_pair(from: u32, to: u32) -> u64 {
    ((from as u64) << 32) | (to as u64)
}

pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

pub fn pair_checksum(from: u32, to: u32) -> u64 {
    let mut holder = [0u8; 8];
    holder[..4].copy_from_slice(&from.to_le_bytes());
    holder[4..].copy_from_slice(&to.to_le_bytes());
    fnv1a64(&holder)
}

pub struct RefResult {
    pub derived: u64,
    pub checksum: u64,
}

pub fn ref_result(edges: &[(u32, u32)]) -> RefResult {
    let mut reachable: FxHashSet<u64> = FxHashSet::default();
    let mut edge_index: FxHashMap<u32, Vec<u32>> = FxHashMap::default();
    let mut delta: FxHashSet<u64> = FxHashSet::default();
    for (from, to) in edges {
        if from == to {
            continue;
        }
        let encoded = encode_pair(*from, *to);
        if reachable.insert(encoded) {
            delta.insert(encoded);
        }
        edge_index.entry(*to).or_default().push(*from);
    }
    loop {
        let mut next: FxHashSet<u64> = FxHashSet::default();
        for encoded in delta.iter() {
            let from = (encoded >> 32) as u32;
            let to = (encoded & 0xffff_ffff) as u32;
            if let Some(predecessors) = edge_index.get(&from) {
                for predecessor in predecessors {
                    let candidate = encode_pair(*predecessor, to);
                    if !reachable.contains(&candidate) {
                        next.insert(candidate);
                    }
                }
            }
        }
        if next.is_empty() {
            break;
        }
        for encoded in next.iter() {
            reachable.insert(*encoded);
        }
        delta = next;
    }
    let mut checksum: u64 = 0;
    for encoded in reachable.iter() {
        let from = (encoded >> 32) as u32;
        let to = (encoded & 0xffff_ffff) as u32;
        checksum ^= pair_checksum(from, to);
    }
    RefResult {
        derived: reachable.len() as u64,
        checksum,
    }
}

pub fn peak_rss_kb() -> i64 {
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    let result = unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
    if result != 0 {
        return -1;
    }
    let maxrss = usage.ru_maxrss;
    maxrss / 1024
}

pub fn derive_count_bitset(edges: &[(u32, u32)], node_count: u32) -> u64 {
    let nodes = node_count as usize;
    let words = (nodes + 63) / 64;
    let mut successors: Vec<Vec<u32>> = vec![Vec::new(); nodes];
    for (from, to) in edges {
        if *from >= node_count || *to >= node_count {
            panic!("edge id out of node range");
        }
        if *from >= *to {
            panic!("derive_count_bitset requires forward-ordered DAG edges");
        }
        successors[*from as usize].push(*to);
    }
    let mut reachable: Vec<Option<Vec<u64>>> = vec![None; nodes];
    let mut total = 0u64;
    for source in (0..nodes).rev() {
        let mut set = vec![0u64; words];
        let word_index = source / 64;
        let bit_index = source % 64;
        set[word_index] |= 1u64 << bit_index;
        for target in successors[source].iter() {
            if let Some(target_set) = &reachable[*target as usize] {
                for word in 0..words {
                    set[word] |= target_set[word];
                }
            }
        }
        let mut popcount = 0u64;
        for word in set.iter() {
            popcount += word.count_ones() as u64;
        }
        total += popcount - 1;
        reachable[source] = Some(set);
    }
    total
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semi_naive_stops_on_empty_delta() {
        let no_edges: Vec<(u32, u32)> = Vec::new();
        let result = ref_result(&no_edges);
        assert_eq!(result.derived, 0);
        assert_eq!(result.checksum, 0);
    }

    #[test]
    fn semi_naive_terminates_on_cycle() {
        let cycle = vec![(0u32, 1u32), (1u32, 0u32)];
        let result = ref_result(&cycle);
        assert_eq!(result.derived, 4);
    }

    #[test]
    fn checksum_matches_hand_computed_on_three_edge() {
        let edges = vec![(0u32, 1u32), (1u32, 2u32), (0u32, 2u32)];
        let result = ref_result(&edges);
        assert_eq!(result.derived, 3);
        assert_eq!(result.checksum, 0x29b29552ccf7a715);
        assert_eq!(format!("{:016x}", result.checksum), "29b29552ccf7a715");
    }

    #[test]
    fn checksum_is_order_independent() {
        let left = vec![(0u32, 1u32), (1u32, 2u32), (0u32, 2u32)];
        let right = vec![(0u32, 2u32), (0u32, 1u32), (1u32, 2u32)];
        assert_eq!(ref_result(&left).checksum, ref_result(&right).checksum);
    }

    #[test]
    fn bitset_counter_matches_reference_on_dag() {
        let edges = vec![(0u32, 1u32), (0u32, 2u32), (1u32, 3u32), (2u32, 3u32), (3u32, 4u32)];
        let node_count = 5;
        assert_eq!(
            derive_count_bitset(&edges, node_count),
            ref_result(&edges).derived
        );
    }
}
