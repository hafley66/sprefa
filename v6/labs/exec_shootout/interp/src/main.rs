// IR interpreter for the exec_shootout reachability workload. The program
// lives as DATA; the loop re-reads the rules each batch, nothing specialized.

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use std::time::Instant;

#[cfg(feature = "mimalloc-global")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(feature = "jemalloc-global")]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

const ALLOCATOR_NAME: &str = if cfg!(feature = "mimalloc-global") {
    "mimalloc"
} else if cfg!(feature = "jemalloc-global") {
    "jemalloc"
} else {
    "system"
};

type NodeId = u32;
// Inline for arity <= 4, which is every relation this IR carries. A heap
// Vec per derived row put 39.5% of a profiled run inside malloc/free.
type Tuple = SmallVec<[u32; 4]>;

// ----- term, atom, rule: the IR -------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Term {
    Variable(usize),
    // Constants are part of the generic IR; this workload binds no literals.
    #[allow(dead_code)]
    Constant(NodeId),
}

#[derive(Clone, Debug)]
struct Atom {
    relation: usize,
    args: Vec<Term>,
}

#[derive(Clone, Debug)]
struct Rule {
    head_relation: usize,
    head_args: Vec<Term>,
    body: Vec<Atom>,
}

// ----- generic tuple store -------------------------------------

struct RelationData {
    arity: usize,
    rows: Vec<Tuple>,
    members: FxHashSet<Tuple>,
    index: Vec<FxHashMap<NodeId, Vec<usize>>>,
}

impl RelationData {
    fn new(arity: usize, indexed: bool) -> Self {
        RelationData {
            arity,
            rows: Vec::new(),
            members: FxHashSet::default(),
            index: if indexed {
                (0..arity).map(|_| FxHashMap::default()).collect()
            } else {
                Vec::new()
            },
        }
    }

    fn insert(&mut self, tuple: Tuple) -> bool {
        if !self.members.insert(tuple.clone()) {
            return false;
        }
        let row = self.rows.len();
        if self.index.len() == self.arity {
            for (column, table) in self.index.iter_mut().enumerate() {
                table.entry(tuple[column]).or_default().push(row);
            }
        }
        self.rows.push(tuple);
        true
    }
}

struct Program {
    relations: Vec<RelationData>,
    rules: Vec<Rule>,
}

// ----- executor -------------------------------------------------

// Each body atom reads its relation either in full or only the delta rows.
enum RowSource<'rows> {
    All,
    Delta(&'rows [usize]),
}

// Recursively match body atoms from `position` onward against the binding,
// appending a head tuple to `outputs` once every atom has matched.
fn match_body(
    program: &Program,
    body: &[Atom],
    body_sources: &[RowSource],
    bindings: &mut [Option<NodeId>],
    position: usize,
    head_args: &[Term],
    outputs: &mut Vec<Tuple>,
) {
    if position == body.len() {
        let head: Tuple = head_args
            .iter()
            .map(|term| match term {
                Term::Variable(variable) => bindings[*variable].expect("head variable is bound"),
                Term::Constant(value) => *value,
            })
            .collect();
        outputs.push(head);
        return;
    }

    let atom = &body[position];
    let relation = &program.relations[atom.relation];

    // Column constraints come from constants and already-bound variables;
    // free variables are bound by whichever tuple matches here.
    let mut constraints: SmallVec<[(usize, NodeId); 4]> = SmallVec::new();
    for (column, term) in atom.args.iter().enumerate() {
        match term {
            Term::Variable(variable) => {
                if let Some(value) = bindings[*variable] {
                    constraints.push((column, value));
                }
            }
            Term::Constant(value) => constraints.push((column, *value)),
        }
    }

    // The delta atom scans its listed rows; any constrained column probes the
    // relation index; an all-free atom scans every row once.
    let mut candidates: Vec<usize> = Vec::new();
    match body_sources[position] {
        RowSource::All => {
            if let Some((column, value)) = constraints.first() {
                if let Some(rows) = relation.index[*column].get(value) {
                    candidates.extend_from_slice(rows);
                }
            } else {
                candidates.extend(0..relation.rows.len());
            }
        }
        RowSource::Delta(rows) => {
            candidates.extend_from_slice(rows);
        }
    }

    for row in candidates {
        let tuple = &relation.rows[row];
        let mut compatible = true;
        for (column, value) in &constraints {
            if tuple[*column] != *value {
                compatible = false;
                break;
            }
        }
        if !compatible {
            continue;
        }

        let mut bound_here: SmallVec<[usize; 4]> = SmallVec::new();
        for (column, term) in atom.args.iter().enumerate() {
            if let Term::Variable(variable) = term {
                if bindings[*variable].is_none() {
                    bindings[*variable] = Some(tuple[column]);
                    bound_here.push(*variable);
                }
            }
        }

        match_body(
            program,
            body,
            body_sources,
            bindings,
            position + 1,
            head_args,
            outputs,
        );

        for variable in &bound_here {
            bindings[*variable] = None;
        }
    }
}

fn max_variable_id(rule: &Rule) -> usize {
    let body_terms = rule.body.iter().flat_map(|atom| atom.args.iter());
    rule.head_args
        .iter()
        .chain(body_terms)
        .filter_map(|term| match term {
            Term::Variable(variable) => Some(*variable),
            Term::Constant(_) => None,
        })
        .max()
        .unwrap_or(0)
}

// Compute all head tuples a rule can derive against per-atom row sources.
fn evaluate_rule(program: &Program, rule: &Rule, sources: &[RowSource]) -> Vec<Tuple> {
    // Variable ids are dense, so bindings index directly instead of hashing.
    let mut bindings = vec![None; max_variable_id(rule) + 1];
    let mut outputs = Vec::new();
    match_body(
        program,
        &rule.body,
        sources,
        &mut bindings,
        0,
        &rule.head_args,
        &mut outputs,
    );
    outputs
}

// ----- program construction (the exhibit: program as data) ------

fn build_program() -> Program {
    // relation 0: edge(x, y), indexed for joins on any column.
    let edge = RelationData::new(2, true);
    // relation 1: reachable(x, y), indexed on both columns.
    let reachable = RelationData::new(2, true);

    let mut rules = Vec::new();

    // rule 0: reachable(x, y) <- edge(x, y)
    rules.push(Rule {
        head_relation: 1,
        head_args: vec![Term::Variable(0), Term::Variable(1)],
        body: vec![Atom {
            relation: 0,
            args: vec![Term::Variable(0), Term::Variable(1)],
        }],
    });

    // rule 1: reachable(x, z) <- reachable(x, y), edge(y, z)
    rules.push(Rule {
        head_relation: 1,
        head_args: vec![Term::Variable(0), Term::Variable(2)],
        body: vec![
            Atom {
                relation: 1,
                args: vec![Term::Variable(0), Term::Variable(1)],
            },
            Atom {
                relation: 0,
                args: vec![Term::Variable(1), Term::Variable(2)],
            },
        ],
    });

    Program {
        relations: vec![edge, reachable],
        rules,
    }
}

// Run semi-naive evaluation: seed from the non-recursive rule, then fold the
// recursive rule until a batch contributes nothing (returns derived count).
fn semi_naive(program: &mut Program) -> usize {
    let idb_relations: FxHashSet<usize> = program
        .rules
        .iter()
        .map(|rule| rule.head_relation)
        .collect();

    let is_recursive = |rule: &Rule| {
        rule.body
            .iter()
            .any(|atom| idb_relations.contains(&atom.relation))
    };

    // Seed: each non-recursive rule inserts its head once.
    let mut delta: Vec<Vec<usize>> = vec![Vec::new(); program.relations.len()];
    for rule in &program.rules {
        if is_recursive(rule) {
            continue;
        }
        let sources: Vec<RowSource> = rule.body.iter().map(|_| RowSource::All).collect();
        for head in evaluate_rule(program, rule, &sources) {
            let relation = &mut program.relations[rule.head_relation];
            if relation.insert(head) {
                let row = relation.rows.len() - 1;
                delta[rule.head_relation].push(row);
            }
        }
    }

    // Fold: each recursive rule runs with the idb atom read from the delta.
    loop {
        let mut next_delta: Vec<Vec<usize>> = vec![Vec::new(); program.relations.len()];
        let mut any_new = false;
        for rule in &program.rules {
            if !is_recursive(rule) {
                continue;
            }
            let sources: Vec<RowSource> = rule
                .body
                .iter()
                .map(|atom| {
                    if atom.relation == rule.head_relation {
                        RowSource::Delta(&delta[rule.head_relation])
                    } else {
                        RowSource::All
                    }
                })
                .collect();
            for head in evaluate_rule(program, rule, &sources) {
                let relation = &mut program.relations[rule.head_relation];
                if relation.insert(head) {
                    let row = relation.rows.len() - 1;
                    next_delta[rule.head_relation].push(row);
                    any_new = true;
                }
            }
        }
        if !any_new {
            break;
        }
        delta = next_delta;
    }

    let derived_idb = *idb_relations.iter().next().unwrap();
    program.relations[derived_idb].rows.len()
}

// ----- checksum and timing --------------------------------------

fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET_BASIS;
    for &byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

fn pair_checksum(left: u32, right: u32) -> u64 {
    let mut bytes = Vec::with_capacity(8);
    bytes.extend_from_slice(&left.to_le_bytes());
    bytes.extend_from_slice(&right.to_le_bytes());
    fnv1a64(&bytes)
}

fn closure_checksum(rows: &[Tuple]) -> u64 {
    let mut checksum: u64 = 0;
    for tuple in rows {
        checksum ^= pair_checksum(tuple[0], tuple[1]);
    }
    checksum
}

unsafe fn peak_rss_kb() -> i64 {
    let mut usage: libc::rusage = std::mem::zeroed();
    libc::getrusage(libc::RUSAGE_SELF, &mut usage);
    // macOS reports ru_maxrss in bytes; divide by 1024 to get KiB.
    #[cfg(target_os = "macos")]
    {
        usage.ru_maxrss / 1024
    }
    #[cfg(not(target_os = "macos"))]
    {
        usage.ru_maxrss
    }
}

// ----- input and the three JSONL events --------------------------

fn parse_input(contents: &str, program: &mut Program) -> Result<usize, String> {
    let mut lines = contents.lines();
    let header = lines
        .next()
        .ok_or_else(|| "input file is empty".to_string())?;
    let mut header_fields = header.split_whitespace();
    let kind = header_fields.next().unwrap_or("");
    if kind != "p" {
        return Err(format!("first token must be 'p', got {kind}"));
    }
    let _nodes = header_fields
        .next()
        .and_then(|field| field.parse::<usize>().ok())
        .ok_or_else(|| "missing node count in header".to_string())?;
    let _edges = header_fields
        .next()
        .and_then(|field| field.parse::<usize>().ok())
        .ok_or_else(|| "missing edge count in header".to_string())?;

    let mut inserted = 0usize;
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut fields = trimmed.split_whitespace();
        let left = fields
            .next()
            .and_then(|field| field.parse::<NodeId>().ok())
            .ok_or_else(|| "edge line missing left node".to_string())?;
        let right = fields
            .next()
            .and_then(|field| field.parse::<NodeId>().ok())
            .ok_or_else(|| "edge line missing right node".to_string())?;
        if program.relations[0].insert(Tuple::from_slice(&[left, right])) {
            inserted += 1;
        }
    }

    // Duplicate edges are deduped; the loaded count is the distinct set.
    Ok(inserted)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut input_path: Option<&str> = None;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--input" => {
                index += 1;
                input_path = args.get(index).map(String::as_str);
            }
            flag => {
                eprintln!("unrecognized flag {flag}");
                std::process::exit(1);
            }
        }
        index += 1;
    }
    let input_path = match input_path {
        Some(path) => path,
        None => {
            eprintln!("usage: interp --input <path>");
            std::process::exit(1);
        }
    };

    eprintln!("interp: allocator={ALLOCATOR_NAME}");

    let loaded_clock = Instant::now();
    let mut program = build_program();
    let contents = match std::fs::read_to_string(input_path) {
        Ok(contents) => contents,
        Err(error) => {
            eprintln!("cannot read input {input_path}: {error}");
            std::process::exit(1);
        }
    };
    let edge_count = match parse_input(&contents, &mut program) {
        Ok(count) => count,
        Err(message) => {
            eprintln!("input error: {message}");
            std::process::exit(1);
        }
    };
    let load_ms = loaded_clock.elapsed().as_millis() as i64;

    let fixpoint_clock = Instant::now();
    let derived = semi_naive(&mut program);
    let fixpoint_ms = fixpoint_clock.elapsed().as_millis() as i64;

    let checksum = closure_checksum(&program.relations[1].rows);
    let peak_rss_kb = unsafe { peak_rss_kb() };

    println!("{{\"event\":\"loaded\",\"edges\":{edge_count},\"ms\":{load_ms}}}");
    println!("{{\"event\":\"fixpoint\",\"derived\":{derived},\"ms\":{fixpoint_ms}}}");
    println!(
        "{{\"event\":\"done\",\"checksum\":\"{checksum:016x}\",\"peak_rss_kb\":{peak_rss_kb}}}"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_graph(edge_lines: &str, edge_count: usize) -> (usize, u64) {
        let mut program = build_program();
        let contents = format!("p 1 {edge_count}\n{edge_lines}");
        parse_input(&contents, &mut program).expect("parse");
        let derived = semi_naive(&mut program);
        let checksum = closure_checksum(&program.relations[1].rows);
        (derived, checksum)
    }

    // A 3-edge chain 1->2->3->4 plus a loop closes to six reachable pairs.
    const THREE_EDGE: &str = "1 2\n2 3\n3 4\n";

    #[test]
    fn semi_naive_stops_on_empty_delta() {
        // Two disconnected edges produce no transitive extension; the fold
        // must halt after a batch that contributes no new pairs.
        let (derived, _) = run_graph("1 2\n3 4\n", 2);
        assert_eq!(derived, 2);
    }

    #[test]
    fn checksum_matches_hand_computed_three_edge() {
        let (derived, checksum) = run_graph(THREE_EDGE, 3);
        assert_eq!(derived, 6);
        // XOR of fnv1a64(u_le ++ v_le) over the six reachable pairs of the
        // chain 1->2->3->4, computed independently in the report.
        assert_eq!(checksum, 0x8e5a666c164d62c4);
    }

    #[test]
    fn events_parse_as_json() {
        let (derived, checksum) = run_graph(THREE_EDGE, 3);
        let loaded = format!("{{\"event\":\"loaded\",\"edges\":3,\"ms\":2}}");
        let fixpoint = format!("{{\"event\":\"fixpoint\",\"derived\":{derived},\"ms\":3}}");
        let done =
            format!("{{\"event\":\"done\",\"checksum\":\"{checksum:016x}\",\"peak_rss_kb\":4096}}");
        let loaded_fields = parse_json_fields(&loaded);
        let fixpoint_fields = parse_json_fields(&fixpoint);
        let done_fields = parse_json_fields(&done);
        assert_eq!(
            loaded_fields.get("event").map(String::as_str),
            Some("loaded")
        );
        assert_eq!(loaded_fields.get("edges").map(String::as_str), Some("3"));
        assert_eq!(
            fixpoint_fields.get("event").map(String::as_str),
            Some("fixpoint")
        );
        assert_eq!(
            fixpoint_fields.get("derived").map(String::as_str),
            Some("6")
        );
        assert_eq!(done_fields.get("event").map(String::as_str), Some("done"));
        assert_eq!(
            done_fields.get("checksum").map(String::as_str),
            Some(format!("{checksum:016x}").as_str())
        );
    }

    fn parse_json_fields(line: &str) -> FxHashMap<String, String> {
        let mut fields = FxHashMap::default();
        let trimmed = line.trim_start_matches('{').trim_end_matches('}');
        for pair in trimmed.split(',') {
            let mut parts = pair.splitn(2, ':');
            let key = parts.next().unwrap_or("").trim_matches('"');
            let value = parts.next().unwrap_or("").trim_matches('"');
            fields.insert(key.to_string(), value.to_string());
        }
        fields
    }
}
