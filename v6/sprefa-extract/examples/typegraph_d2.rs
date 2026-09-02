//! Node identity is (path, name), never the bare name: two structs sharing a
//! name in two files are two nodes, which is what the wire's split paths say.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use sprefa_extract::{
    dispatch, flatten, resolve_project, FamilyMask, FamilyTag, FlatFact, ResolveArms,
    ResolveRequest, ScipMode,
};

/// At most this many shapes on one board. Over budget splits, never crams.
const SHAPE_BUDGET: usize = 24;

/// A node key. `path` is as the resolve was invoked with it.
type Key = (String, String);

struct Args {
    root: PathBuf,
    entry: String,
    out: PathBuf,
}

fn parse_args() -> Result<Args, String> {
    let mut root = None;
    let mut entry = None;
    let mut out = None;
    let mut argv = std::env::args().skip(1);
    while let Some(flag) = argv.next() {
        match flag.as_str() {
            "--root" => root = Some(PathBuf::from(argv_next(&mut argv, &flag)?)),
            "--entry" => entry = Some(argv_next(&mut argv, &flag)?),
            "--out" => out = Some(PathBuf::from(argv_next(&mut argv, &flag)?)),
            other => return Err(format!("unknown flag {other}")),
        }
    }
    Ok(Args {
        root: root.ok_or("--root DIR is required")?,
        entry: entry.ok_or("--entry PATH::NAME is required")?,
        out: out.ok_or("--out DIR is required")?,
    })
}

fn argv_next(argv: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    argv.next().ok_or_else(|| format!("{flag} takes a value"))
}

/// Every `.rs` under `root`, `target/` excluded, sorted.
fn rust_files(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().is_some_and(|name| name == "target") {
                    continue;
                }
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// (path, name) -> entity kind, from the phase-1 type nodes of each file.
fn node_kinds(paths: &[PathBuf]) -> BTreeMap<Key, String> {
    let mut kinds = BTreeMap::new();
    for path in paths {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let key = path.to_string_lossy().to_string();
        let Some(output) = dispatch(&key, &bytes, FamilyMask::ALL) else {
            continue;
        };
        for fact in flatten(&output) {
            if let FlatFact::Node {
                family: FamilyTag::Type,
                kind,
                name: Some(name),
                ..
            } = fact
            {
                kinds.entry((key.clone(), name)).or_insert(kind);
            }
        }
    }
    kinds
}

/// One resolved type reference, owner to target.
struct TypeEdge {
    src: Key,
    dst: Key,
    kind: String,
}

fn type_edges(paths: &[PathBuf]) -> Result<Vec<TypeEdge>, String> {
    let request = ResolveRequest {
        paths,
        arms: ResolveArms {
            call: false,
            types: true,
            flow: false,
        },
        scip: ScipMode::Off,
        project_root: None,
        scip_records: Default::default(),
        occurrence_text: false,
        rust_checker: None,
        ts_checker: None,
        witness: false,
    };
    let facts = resolve_project(&request).map_err(|err| err.to_string())?;
    Ok(facts
        .into_iter()
        .filter_map(|fact| match fact {
            FlatFact::ResolvedTypeEdge {
                owner_path,
                owner_name: Some(owner_name),
                target_path,
                target_name: Some(target_name),
                kind,
                ..
            } => Some(TypeEdge {
                src: (owner_path, owner_name),
                dst: (target_path, target_name),
                kind,
            }),
            _ => None,
        })
        .collect())
}

/// Breadth-first from `entry`, following edges owner to target. Returns each
/// reached key with its hop distance.
fn reachable(entry: &Key, edges: &[TypeEdge]) -> BTreeMap<Key, usize> {
    let mut out_edges: BTreeMap<&Key, Vec<&Key>> = BTreeMap::new();
    for edge in edges {
        out_edges.entry(&edge.src).or_default().push(&edge.dst);
    }
    let mut hops = BTreeMap::new();
    hops.insert(entry.clone(), 0usize);
    let mut queue = VecDeque::from([entry.clone()]);
    while let Some(current) = queue.pop_front() {
        let hop = hops[&current];
        for next in out_edges.get(&current).into_iter().flatten() {
            if !hops.contains_key(*next) {
                hops.insert((*next).clone(), hop + 1);
                queue.push_back((*next).clone());
            }
        }
    }
    hops
}

/// A d2 identifier: everything outside `[A-Za-z0-9_]` becomes `_`.
fn ident(key: &Key) -> String {
    let raw = format!("{}__{}", key.0, key.1);
    raw.chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

/// d2 label text: the bare name, truncated so no shape carries a paragraph.
fn label(key: &Key) -> String {
    let name = &key.1;
    if name.chars().count() <= 40 {
        return name.clone();
    }
    let head: String = name.chars().take(39).collect();
    format!("{head}…")
}

/// Fill per `TypeEntityKind` slug. Unknown kinds take the last row.
fn fill_for(kind: &str) -> &'static str {
    match kind {
        "struct" | "class" => "#1f4e79",
        "enum" => "#7b3f00",
        "trait" | "interface" => "#4b2e83",
        "alias" => "#3f5f3f",
        "function" | "method" => "#5a5a2d",
        _ => "#5a3a5a",
    }
}

/// `direction: down` is what makes the board wider than tall: a type graph fans
/// out, so downward ranks put siblings side by side. One line per shape.
fn board(nodes: &[(Key, String)], edges: &[&TypeEdge]) -> String {
    let mut text = String::from("direction: down\n\n");
    for (key, kind) in nodes {
        text.push_str(&format!(
            "{}: {} {{ shape: rectangle; style.fill: \"{}\"; style.font-color: \"#ffffff\" }}\n",
            ident(key),
            label(key),
            fill_for(kind)
        ));
    }
    text.push('\n');
    let present: BTreeSet<&Key> = nodes.iter().map(|(key, _)| key).collect();
    for edge in edges {
        if present.contains(&edge.src) && present.contains(&edge.dst) {
            text.push_str(&format!(
                "{} -> {}: {}\n",
                ident(&edge.src),
                ident(&edge.dst),
                edge.kind
            ));
        }
    }
    text
}

/// Split the reached set into boards: hop 0 and 1 together, then one hop per
/// board, then chunked when a hop alone exceeds the budget.
fn boards(hops: &BTreeMap<Key, usize>, kinds: &BTreeMap<Key, String>) -> Vec<Vec<(Key, String)>> {
    let mut by_band: BTreeMap<usize, Vec<(Key, String)>> = BTreeMap::new();
    for (key, hop) in hops {
        let band = if *hop <= 1 { 0 } else { *hop - 1 };
        let kind = kinds
            .get(key)
            .cloned()
            .unwrap_or_else(|| "struct".to_string());
        by_band.entry(band).or_default().push((key.clone(), kind));
    }
    let mut out = Vec::new();
    for (_, band) in by_band {
        for chunk in band.chunks(SHAPE_BUDGET) {
            out.push(chunk.to_vec());
        }
    }
    out
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let (entry_path, entry_name) = args
        .entry
        .rsplit_once("::")
        .ok_or("--entry must be PATH::NAME")?;
    let entry: Key = (entry_path.to_string(), entry_name.to_string());

    let paths = rust_files(&args.root);
    if paths.is_empty() {
        return Err(format!("no .rs files under {}", args.root.display()));
    }
    let kinds = node_kinds(&paths);
    if !kinds.contains_key(&entry) {
        let near: Vec<&String> = kinds
            .keys()
            .filter(|(_, name)| name == &entry.1)
            .map(|(path, _)| path)
            .take(5)
            .collect();
        return Err(format!(
            "no type node {}::{} ; the same name appears in: {near:?}",
            entry.0, entry.1
        ));
    }
    let edges = type_edges(&paths)?;
    let hops = reachable(&entry, &edges);

    std::fs::create_dir_all(&args.out).map_err(|err| err.to_string())?;
    let edge_refs: Vec<&TypeEdge> = edges.iter().collect();
    for (index, nodes) in boards(&hops, &kinds).into_iter().enumerate() {
        let text = board(&nodes, &edge_refs);
        let drawn = text.lines().filter(|line| line.contains(" -> ")).count();
        let path = args.out.join(format!("typegraph.{index}.d2"));
        std::fs::write(&path, text).map_err(|err| err.to_string())?;
        println!("{} shapes={} edges={}", path.display(), nodes.len(), drawn);
    }
    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("typegraph_d2: {err}"); // @eprintln-ok: example CLI usage line
        std::process::exit(2);
    }
}
