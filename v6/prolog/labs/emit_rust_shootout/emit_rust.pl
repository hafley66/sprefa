% emit_rust_shootout: lower the two-rule reachability program below to a
% standalone, fully specialized main.rs. The recursive rule becomes one
% semi-naive delta-join loop that feeds back until the delta is empty.
%
% Run from this directory:
%   swipl -g main -t halt emit_rust.pl
% Writes v6/labs/exec_shootout/mono/src/main.rs, the file the harness lane
% builds. The emitted binary reads MONO_TRACE for per-round stderr logs.

:- use_module(library(lists)).

% ---------------------------------------------------------------------------
% Program facts: the input this emitter lowers. Ids are u32; no negation.
% ---------------------------------------------------------------------------
rule(1,
     head(reachable, [source, target]),
     [edge(source, target)]).
rule(2,
     head(reachable, [source, target]),
     [reachable(source, mid), edge(mid, target)]).

% The seed rule (rule 1) has one base body atom on the index relation and a
% head on the set relation. The recursive rule (rule 2) joins the set
% relation (delta side) to the index relation. The shared column is the join
% key. These two extractions drive every specialized emit below.
seed_columns(SetRel, IndexRel, Source, Target) :-
    rule(1, head(SetRel, [Source, Target]), [IndexAtom]),
    functor(IndexAtom, IndexRel, _).
loop_columns(SetRel, IndexRel, Source, JoinKey, Target) :-
    rule(2, head(SetRel, [Source, Target]), [DeltaAtom, IndexAtom]),
    functor(DeltaAtom, SetRel, _),
    functor(IndexAtom, IndexRel, _),
    IndexAtom =.. [_, JoinKey, Target].

target_path(Path) :-
    prolog_to_os_filename('../../../labs/exec_shootout/mono/src/main.rs', Path).

% Rust type names uppercase the relation atom's first letter.
capitalize(In, Out) :-
    atom_codes(In, [First | Rest]),
    ( First >= 0'a, First =< 0'z ->
        Upper is First - 32
    ;   Upper = First ),
    atom_codes(Out, [Upper | Rest]).

main :-
    seed_columns(SetRel, IndexRel, Source, Target),
    loop_columns(SetRel, IndexRel, Source, JoinKey, Target),
    emit_text(Text, SetRel, IndexRel, Source, JoinKey, Target),
    target_path(Path),
    open(Path, write, Stream),
    format(Stream, '~s~n', [Text]),
    close(Stream),
    write('wrote '), write(Path), nl.

% ---------------------------------------------------------------------------
% text assembly
% ---------------------------------------------------------------------------
emit_text(Text, SetRel, IndexRel, Source, JoinKey, Target) :-
    phrase(emit_lines(SetRel, IndexRel, Source, JoinKey, Target), Lines),
    atomic_list_concat(Lines, '\n', Text).

emit_lines(SetRel, IndexRel, Source, JoinKey, Target) -->
    { capitalize(SetRel, CapSet),
      capitalize(IndexRel, CapIndex),
      format(string(S0), 'type ~w = Vec<FxHashSet<u32>>;', [CapSet]),
      format(string(S1), 'type ~wIndex = FxHashMap<u32, Vec<u32>>;', [CapIndex]) },
    [ "use std::fs::File;",
      "use std::io::{BufRead, BufReader};",
      "use std::time::Instant;",
      "",
      "use fxhash::{FxHashMap, FxHashSet};",
      "",
      "// The relation is seen-set sharded per source node: one FxHashSet of",
      "// targets per source. A membership test touches one bucket, never the",
      "// whole derived set.",
      S0,
      S1,
      "",
      "struct State {",
      "    edges: usize,",
      "    index: EdgeIndex,",
      "    seen: Reachable,",
      "    delta: Vec<(u32, u32)>,",
      "}",
      "",
      "fn main() {",
      "    let input = find_input_arg();",
      "    let trace = std::env::var(\"MONO_TRACE\").is_ok();",
      "    let start_at = Instant::now();",
      "",
      "    let mut state = load(&input);",
      "",
      "    let loaded_ms = start_at.elapsed().as_millis() as i64;",
      "    println!(\"{{\\\"event\\\":\\\"loaded\\\",\\\"edges\\\":{},\\\"ms\\\":{}}}\", state.edges, loaded_ms);",
      "",
      "    let fix_start = Instant::now();",
      "    derive(&mut state, trace);",
      "    let fixpoint_ms = fix_start.elapsed().as_millis() as i64;",
      "    println!(\"{{\\\"event\\\":\\\"fixpoint\\\",\\\"derived\\\":{},\\\"ms\\\":{}}}\", derived_count(&state.seen), fixpoint_ms);",
      "",
      "    let checksum = checksum(&state.seen);",
      "    let peak_rss_kb = peak_rss_kb();",
      "    println!(\"{{\\\"event\\\":\\\"done\\\",\\\"checksum\\\":\\\"{checksum:016x}\\\",\\\"peak_rss_kb\\\":{peak_rss_kb}}}\");",
      "}",
      "",
      "fn find_input_arg() -> String {",
      "    let args: Vec<String> = std::env::args().collect();",
      "    let mut input = None;",
      "    let mut index = 1;",
      "    while index < args.len() {",
      "        match args[index].as_str() {",
      "            \"--input\" => {",
      "                index += 1;",
      "                input = Some(args.get(index).expect(\"--input needs a path\").clone());",
      "            }",
      "            \"--threads\" => {",
      "                index += 1;",
      "            }",
      "            other => panic!(\"unknown argument: {other}\"),",
      "        }",
      "        index += 1;",
      "    }",
      "    input.expect(\"--input <path> is required\")",
      "}",
      "",
      "fn insert_pair(seen: &mut Reachable, source: u32, target: u32) -> bool {",
      "    let bucket = source as usize;",
      "    if seen.len() <= bucket {",
      "        seen.resize(bucket + 1, FxHashSet::default());",
      "    }",
      "    seen[bucket].insert(target)",
      "}",
      "",
      "fn load(path: &str) -> State {",
      "    let file = File::open(path).unwrap_or_else(|why| panic!(\"open {path}: {why}\"));",
      "    let reader = BufReader::new(file);",
      "    let mut index: EdgeIndex = FxHashMap::default();",
      "    let mut seen: Reachable = Vec::new();",
      "    let mut delta = Vec::<(u32, u32)>::new();",
      "    let mut edges = 0usize;",
      "",
      "    for (line_no, line) in reader.lines().enumerate() {",
      "        let line = line.unwrap();",
      "        let line = line.trim();",
      "        if line.is_empty() {",
      "            continue;",
      "        }",
      "        let mut parts = line.split_whitespace();",
      "        let first = parts.next().expect(\"line has a token\");",
      "        if first == \"p\" {",
      "            // Header: p <nodes> <edges>. Node count is not used by either rule.",
      "            continue;",
      "        }",
      "        if first.chars().all(|ch| ch.is_ascii_digit()) {",
      "            let tail: u32 = first.parse().expect(\"edge tail is a u32\");",
      "            let head: u32 = parts.next().expect(\"edge head\").parse().expect(\"edge head is a u32\");",
      "            index.entry(tail).or_default().push(head);",
      "            if insert_pair(&mut seen, tail, head) {",
      "                edges += 1;",
      "                delta.push((tail, head));",
      "            }",
      "        } else {",
      "            panic!(\"unexpected token {first} on line {}\", line_no + 1);",
      "        }",
      "    }",
      "    State { edges, index, seen, delta }",
      "}",
      "" ],
      loop_lines(Source, JoinKey, Target),
      [ "",
      "fn derived_count(seen: &Reachable) -> i64 {",
      "    seen.iter().fold(0i64, |count, bucket| count + bucket.len() as i64)",
      "}",
      "",
      "// fnv1a64 over the 8 little-endian bytes of (source, target), source first.",
      "fn fnv1a64(source: u32, target: u32) -> u64 {",
      "    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;",
      "    for byte in source.to_le_bytes().iter().chain(target.to_le_bytes().iter()) {",
      "        hash ^= *byte as u64;",
      "        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);",
      "    }",
      "    hash",
      "}",
      "",
      "fn checksum(seen: &Reachable) -> u64 {",
      "    let mut xor = 0u64;",
      "    for (source, bucket) in seen.iter().enumerate() {",
      "        for &target in bucket {",
      "            xor ^= fnv1a64(source as u32, target);",
      "        }",
      "    }",
      "    xor",
      "}",
      "",
      "fn peak_rss_kb() -> i64 {",
      "    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();",
      "    let ok = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };",
      "    if ok != 0 {",
      "        return -1;",
      "    }",
      "    let usage = unsafe { usage.assume_init() };",
      "    // macOS reports ru_maxrss in bytes; the contract says /1024 for KB.",
      "    (usage.ru_maxrss / 1024) as i64",
      "}",
      "",
      "#[cfg(test)]",
      "mod tests {",
      "    use super::*;",
      "",
      "    #[test]",
      "    fn chain_three_seeds_and_joins() {",
      "        let path = \"/tmp/mono_chain.txt\";",
      "        std::fs::write(path, \"p 3 2\\n1 2\\n2 3\\n\").unwrap();",
      "        let mut state = load(path);",
      "        derive(&mut state, false);",
      "        assert_eq!(derived_count(&state.seen), 3);",
      "        assert_eq!(checksum(&state.seen), 0xa6b23e50ed0dd5c5);",
      "        std::fs::remove_file(path).unwrap();",
      "    }",
      "",
      "    #[test]",
      "    fn stops_on_empty_delta() {",
      "        let path = \"/tmp/mono_stop.txt\";",
      "        std::fs::write(path, \"p 2 1\\n1 2\\n\").unwrap();",
      "        let mut state = load(path);",
      "        derive(&mut state, false);",
      "        assert_eq!(derived_count(&state.seen), 1);",
      "        assert!(state.delta.is_empty());",
      "        std::fs::remove_file(path).unwrap();",
      "    }",
      "}",
      "" ].

% The recursive rule's delta join: seed rule copied edges into reachable and
% delta; each round joins the delta on its join column against the index and
% feeds back until a round yields nothing.
loop_lines(Source, JoinKey, Target) -->
    { format(string(L0), 'fn derive(state: &mut State, trace: bool) {', []),
      format(string(L1), '    let mut round = 0u32;', []),
      format(string(L2), '    loop {', []),
      format(string(L3), '        let mut next = Vec::<(u32, u32)>::new();', []),
      format(string(L4), '        round += 1;', []),
      format(string(L5), '        for &(~w, ~w) in &state.delta {', [Source, JoinKey]),
      format(string(L6), '            if let Some(outgoing) = state.index.get(&~w) {', [JoinKey]),
      format(string(L7), '                for &~w in outgoing {', [Target]),
      format(string(L8), '                    if insert_pair(&mut state.seen, ~w, ~w) {', [Source, Target]),
      format(string(L9), '                        next.push((~w, ~w));', [Source, Target]),
      format(string(L10), '                    }', []),
      format(string(L11), '                }', []),
      format(string(L12), '            }', []),
      format(string(L13), '            if trace {', []),
      format(string(L14), '                eprintln!("mono: round={round} delta={}", next.len());', []),
      format(string(L15), '            }', []),
      format(string(L16), '        }', []),
      format(string(L17), '        if next.is_empty() {', []),
      format(string(L18), '            break;', []),
      format(string(L19), '        }', []),
      format(string(L20), '        state.delta = next;', []),
      format(string(L21), '    }', []),
      format(string(L22), '    state.delta = Vec::new();', []),
      format(string(L23), '}', []) },
    [ L0, L1, L2, L3, L4, L5, L6, L7, L8, L9, L10, L11, L12, L16, L13,
      L14, L15, L17, L18, L19, L20, L21, L22, L23 ].
