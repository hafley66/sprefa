// Monomorphized two-rule semi-naive reachability. Concrete u32 pairs, a
// concrete FxHashMap y-join index, the rule loop spelled out (see CONTRACT).

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::time::Instant;

use fxhash::{FxHashMap, FxHashSet};

use crate::payload::Pair;

mod payload {
    // A derived reachable pair packed into one u32 for pointer-sized hashing.
    #[derive(Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Pair(u64);

    impl Pair {
        pub fn new(u: u32, v: u32) -> Self {
            Pair(((u as u64) << 32) | (v as u64))
        }

        pub fn u(self) -> u32 {
            (self.0 >> 32) as u32
        }

        pub fn v(self) -> u32 {
            self.0 as u32
        }
    }
}

// Reading rule 2 "reachable(x, z) <- reachable(x, y), edge(y, z)" joins each
// delta row on its y; the edge index maps a y to the z targets it reaches.
type EdgeIndex = FxHashMap<u32, Vec<u32>>;

struct Loaded {
    edges: usize,
    index: EdgeIndex,
    derived: FxHashSet<Pair>,
    delta: Vec<Pair>,
}

fn main() {
    let input = parse_args();
    let start_at = Instant::now();

    let loaded = load(&input);

    let loaded_ms = start_at.elapsed().as_millis() as i64;
    let json = json_loaded(loaded.edges as i64, loaded_ms);
    println!("{json}");

    let fix_start = Instant::now();
    let closure = derive(loaded);
    let fixpoint_ms = fix_start.elapsed().as_millis() as i64;

    let check = checksum(&closure.derived);
    let derived_count = closure.derived.len() as i64;
    let json = json_fixpoint(derived_count, fixpoint_ms);
    println!("{json}");

    let peak_rss = peak_rss_kb();
    let json = json_done(check, peak_rss);
    println!("{json}");
}

fn parse_args() -> String {
    let args: Vec<String> = std::env::args().collect();
    let mut input = None;
    let mut index = 1;
    while index < args.len() {
        let arg = &args[index];
        match arg.as_str() {
            "--input" => {
                index += 1;
                input = Some(args.get(index).expect("--input needs a path").clone());
            }
            "--threads" => {
                // Reserved by contract; only 1 is in scope for this lane.
                index += 1;
            }
            other => panic!("unknown argument: {other}"),
        }
        index += 1;
    }
    input.expect("--input <path> is required")
}

fn load(path: &str) -> Loaded {
    let file = File::open(path).unwrap_or_else(|why| panic!("open {path}: {why}"));
    let reader = BufReader::new(file);
    let mut index: EdgeIndex = FxHashMap::default();
    let mut derived = FxHashSet::default();
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
            // Header: p <nodes> <edges>. Node count is not needed by either rule.
            continue;
        }
        if first.chars().all(|ch| ch.is_ascii_digit()) {
            let y: u32 = first.parse().expect("edge tail is a u32");
            let z: u32 = parts.next().expect("edge head").parse().expect("edge head is a u32");
            index.entry(y).or_default().push(z);
            let row = Pair::new(y, z);
            if derived.insert(row) {
                edges += 1;
            }
        } else {
            panic!("unexpected token {first} on line {}", line_no + 1);
        }
    }

    let delta: Vec<Pair> = derived.iter().copied().collect();
    Loaded { edges, index, derived, delta }
}

fn derive(mut state: Loaded) -> Loaded {
    // Unrolled rule body: only rule 2 can add rows (rule 1 only seeds), so the
    // semi-naive loop is a single delta-join that feeds back until empty.
    loop {
        let mut next = Vec::<Pair>::new();
        for &reachable in &state.delta {
            let x = reachable.u();
            let y = reachable.v();
            if let Some(zs) = state.index.get(&y) {
                for &z in zs {
                    let joined = Pair::new(x, z);
                    if state.derived.insert(joined) {
                        next.push(joined);
                    }
                }
            }
        }
        if next.is_empty() {
            // Semi-naive stop: the last delta batch joined nothing new, so
            // the fixpoint is reached and the pending delta is empty.
            state.delta = Vec::new();
            return state;
        }
        state.delta = next;
    }
}

// fnv1a64 over the 8 little-endian bytes of (u, v), u first then v, exactly
// as the contract defines; the xor fold is order-independent by construction.
fn fnv1a64(pair: Pair) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in pair.u().to_le_bytes().iter().chain(pair.v().to_le_bytes().iter()) {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn checksum(derived: &FxHashSet<Pair>) -> u64 {
    derived.iter().fold(0u64, |xor, pair| xor ^ fnv1a64(*pair))
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

fn json_loaded(edges: i64, ms: i64) -> String {
    format!("{{\"event\":\"loaded\",\"edges\":{edges},\"ms\":{ms}}}")
}

fn json_fixpoint(derived: i64, ms: i64) -> String {
    format!("{{\"event\":\"fixpoint\",\"derived\":{derived},\"ms\":{ms}}}")
}

fn json_done(checksum: u64, peak_rss_kb: i64) -> String {
    format!(
        "{{\"event\":\"done\",\"checksum\":\"{checksum:016x}\",\"peak_rss_kb\":{peak_rss_kb}}}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semi_naive_stops_on_empty_delta() {
        // edge (1,2) only: rule 1 seeds reachable(1,2); no node 2 has an
        // outgoing edge, so the first delta-join yields nothing.
        let path = "/tmp/mono_stop_test.txt";
        let text = "p 2 1\n1 2\n";
        std::fs::write(path, text).unwrap();
        let state = load(path);
        let closed = derive(state);
        assert_eq!(closed.derived.len(), 1);
        assert!(closed.delta.is_empty());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn checksum_matches_hand_computed() {
        // Graph: 1->2, 2->3, 1->3. reachable = {(1,2),(2,3),(1,3)}.
        let path = "/tmp/mono_check_test.txt";
        let text = "p 3 3\n1 2\n2 3\n1 3\n";
        std::fs::write(path, text).unwrap();
        let state = load(path);
        let closed = derive(state);
        assert_eq!(closed.derived.len(), 3);
        let check = checksum(&closed.derived);
        let hand = fnv1a64(Pair::new(1, 2))
            ^ fnv1a64(Pair::new(2, 3))
            ^ fnv1a64(Pair::new(1, 3));
        assert_eq!(check, hand);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn events_parse_as_json() {
        let loaded = json_loaded(10, 5);
        let fixpoint = json_fixpoint(42, 9);
        let done = json_done(0xdead_beef_u64, 1234);
        // Parse as JSON with a hand-rolled parser; no json dep is allowed by
        // the shootout contract (fxhash + libc only).
        let loaded = parse_object(&loaded);
        let fixpoint = parse_object(&fixpoint);
        let done = parse_object(&done);
        assert_eq!(loaded.get("event").unwrap().as_str(), "loaded");
        assert_eq!(loaded.get("edges").unwrap().as_num(), 10);
        assert_eq!(fixpoint.get("event").unwrap().as_str(), "fixpoint");
        assert_eq!(fixpoint.get("derived").unwrap().as_num(), 42);
        assert_eq!(done.get("event").unwrap().as_str(), "done");
        assert_eq!(done.get("checksum").unwrap().as_str(), "00000000deadbeef");
    }

    // A JSON value limited to string/i64 fields, parsed without a json crate.
    #[derive(Debug, Clone, PartialEq)]
    enum Json {
        Str(String),
        Num(i64),
    }

    impl Json {
        fn as_str(&self) -> &str {
            match self {
                Json::Str(s) => s,
                Json::Num(_) => panic!("expected string"),
            }
        }

        fn as_num(&self) -> i64 {
            match self {
                Json::Num(n) => *n,
                Json::Str(_) => panic!("expected number"),
            }
        }
    }

    #[derive(Clone, Copy)]
    struct Cursor<'a> {
        bytes: &'a [u8],
        pos: usize,
    }

    impl<'a> Cursor<'a> {
        fn peek(&self) -> u8 {
            self.bytes[self.pos]
        }

        fn bump(&mut self) {
            self.pos += 1;
        }
    }

    fn parse_object(input: &str) -> std::collections::HashMap<String, Json> {
        let cursor = Cursor { bytes: input.as_bytes(), pos: 0 };
        let (cursor, out) = object(cursor);
        assert_eq!(cursor.pos, input.len(), "whole input consumed");
        out
    }

    // object: { "key" : value (, "key" : value)* }
    fn object(mut cursor: Cursor) -> (Cursor, std::collections::HashMap<String, Json>) {
        assert_eq!(cursor.peek(), b'{');
        cursor.bump();
        let mut out = std::collections::HashMap::new();
        while cursor.peek() != b'}' {
            assert_eq!(cursor.peek(), b'"');
            let (next, key) = string(cursor);
            cursor = next;
            assert_eq!(cursor.peek(), b':');
            cursor.bump();
            let value;
            (cursor, value) = value_of(cursor);
            out.insert(key, value);
            if cursor.peek() == b',' {
                cursor.bump();
            }
        }
        cursor.bump();
        (cursor, out)
    }

    fn string(cursor_in: Cursor) -> (Cursor, String) {
        let mut cursor = cursor_in;
        assert_eq!(cursor.peek(), b'"');
        cursor.bump();
        let start = cursor.pos;
        while cursor.peek() != b'"' {
            cursor.bump();
        }
        let raw = String::from_utf8(cursor.bytes[start..cursor.pos].to_vec()).unwrap();
        cursor.bump();
        (cursor, raw)
    }

    fn value_of(cursor_in: Cursor) -> (Cursor, Json) {
        let mut cursor = cursor_in;
        match cursor.peek() {
            b'"' => {
                let value;
                (cursor, value) = string(cursor);
                (cursor, Json::Str(value))
            }
            b'-' | b'0'..=b'9' => {
                let start = cursor.pos;
                if cursor.peek() == b'-' {
                    cursor.bump();
                }
                let digits = cursor.pos;
                while cursor.pos < cursor.bytes.len() && cursor.bytes[cursor.pos].is_ascii_digit() {
                    cursor.bump();
                }
                assert!(cursor.pos > digits, "number has digits");
                let raw = std::str::from_utf8(&cursor.bytes[start..cursor.pos]).unwrap();
                let n: i64 = raw.parse().unwrap();
                (cursor, Json::Num(n))
            }
            other => panic!("unexpected value byte {other}"),
        }
    }
}
