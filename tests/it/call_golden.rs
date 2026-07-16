//! P3 (capstone cutover, `plans/2026-07-16-capstone-cutover.md`) froze the 7
//! public call rels as text-decoded goldens before P4 deleted the legacy
//! writer. The goldens are the acceptance baseline P4 (the sole-writer cut)
//! and P5 verify against — the family router is now the ONLY producer of
//! every public call rel, no gate involved.
//!
//! The fixture is ONE git checkout carrying both a committed rev and a
//! divergent working tree, so the rev union is exactly {HEAD-sha, "WORK"} —
//! the asymmetry that pins multi-rev coverage: `helper`/`leaf` sit at the same
//! line in both revs (their `call_def` rows collapse to one, `call_def_rev`
//! keeps two), while `run`/`other` genuinely change shape (a call site moves
//! from `run` to `other`) and `extra` is WORK-only.
//!
//! Two values in the text-decoded rows are nondeterministic across regens:
//! the committed rev resolves to a 40-char SHA that changes with the commit
//! timestamp, and the sandbox dir's basename (which embeds `process::id()`
//! and leaks into `repo`/`sym`/`file` values). `normalize` replaces both with
//! fixed placeholders — applied identically by the producer and the consumer,
//! so a regen is byte-stable modulo those two tokens.

use sprefa_v5::{db, engine::Engine, lex, parse};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("call_golden_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().expect("git");
    assert!(out.status.success(), "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr));
}

fn init_git(d: &Path) {
    git(d, &["init", "-q"]);
    git(d, &["config", "user.email", "t@example.com"]);
    git(d, &["config", "user.name", "T"]);
}

/// The rev-pair extraction program: a data-driven scan over both the
/// committed rev ("HEAD") and the working tree ("WORK") — the same
/// `diff_pair` idiom `tests/it/graph_diff_rev.rs` uses to hit both revs in one
/// checkout — plus a query head for each of the 7 public call rels. A
/// relation must appear in a body atom, closure edge, or query head for
/// `call_rels_used` to gate the family extraction on; a bare golden dump over
/// `rel_call_def_txt` etc. without these heads would find every owned table
/// (and its text-decoded view) empty.
const CALL_PROG: &str = r#"
rel diff_pair(base_rev: text, head_rev: text).
diff_pair("HEAD", "WORK").

rel diff_seen(path: file).
diff_seen(path) <- diff_pair(_, head_ref), scan(head_ref, "src/**/*.rs", path, rev).
diff_seen(path) <- diff_pair(base_ref, _), scan(base_ref, "src/**/*.rs", path, rev).

? call_site(repo, caller, callee, file, line).
? call_kind(fn_sym, kind).
? call_edge(caller, callee, kind).
? call_edge_rev(caller, callee, kind, rev).
? call_def(repo, sym, kind, file, line, end).
? call_def_rev(repo, sym, kind, file, line, end, rev).
? call_name(sym, name).
"#;

/// BASE (committed): `run` calls `helper` then `leaf`; `other` is empty.
/// `helper` and `leaf` each sit on their own line, unchanged by WORK below.
/// `writer`/`reader` call the bare names `execute`/`query_row`, unresolved but
/// classified by `classify_call_kind` (`src/engine/decls.rs`) — one per branch
/// of that allowlist, so `call_kind` pins both "write" and "read". Both stay
/// byte-identical across the two revs.
const BASE_SRC: &str = "\
pub fn helper() {}

pub fn leaf() {}

pub fn run() {
    helper();
    leaf();
}

pub fn other() {}

pub fn writer() {
    execute();
}

pub fn reader() {
    query_row();
}
";

/// WORK: the `leaf()` call moves from `run` into `other` (a call-site move
/// across the whole def, changing `run`'s and `other`'s spans — their
/// `call_def` rows genuinely differ per rev), a new fn `extra` is added
/// (WORK-only), `helper`/`leaf` are untouched at their original lines (so
/// their `call_def` rows collapse across revs while `call_def_rev` keeps
/// both), and `writer`/`reader` are untouched too.
const WORK_SRC: &str = "\
pub fn helper() {}

pub fn leaf() {}

pub fn run() {
    helper();
}

pub fn other() {
    leaf();
}

pub fn extra() {
    helper();
}

pub fn writer() {
    execute();
}

pub fn reader() {
    query_row();
}
";

/// out_cols() order for each of the 7 public call rels, from
/// `src/engine/family/<name>.rs`.
const REL_COLS: &[(&str, &[&str])] = &[
    ("call_site", &["repo", "caller", "callee", "file", "line"]),
    ("call_kind", &["fn", "kind"]),
    ("call_edge", &["caller", "callee", "kind"]),
    ("call_edge_rev", &["caller", "callee", "kind", "rev"]),
    ("call_def", &["repo", "sym", "kind", "file", "line", "end"]),
    ("call_def_rev", &["repo", "sym", "kind", "file", "line", "end", "rev"]),
    ("call_name", &["sym", "name"]),
];

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/call_golden")
}

/// Replace the two unstable tokens with fixed placeholders. Applied per-cell,
/// before joining a row, identically by the producer and the consumer.
fn normalize(cell: &str, head_sha: &str, root_name: &str) -> String {
    cell.replace(head_sha, "<BASE>").replace(root_name, "<ROOT>")
}

fn cell_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Build the git fixture (BASE committed, WORK divergent), run `CALL_PROG` to
/// a fixpoint, and dump + normalize all 7 public call rels through
/// `rel_<name>_txt`. One helper shared by the producer and the consumer so
/// they cannot drift: same fixture shape, same normalizer, same column order.
/// Each cell is normalized before the row is joined with `\t`; rows are
/// sorted lexicographically (the golden's on-disk order).
fn build_and_dump(tag: &str) -> BTreeMap<&'static str, Vec<String>> {
    let dir = sandbox(tag);
    fs::write(dir.join("src/a.rs"), BASE_SRC).unwrap();
    init_git(&dir);
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "base"]);
    let head_sha_out = Command::new("git")
        .arg("-C").arg(&dir).args(["rev-parse", "HEAD"])
        .output().expect("git rev-parse HEAD");
    assert!(head_sha_out.status.success(), "git rev-parse HEAD failed");
    let head_sha = String::from_utf8_lossy(&head_sha_out.stdout).trim().to_string();
    assert_eq!(head_sha.len(), 40, "HEAD sha must be a 40-char commit sha: {head_sha:?}");

    // WORK diverges from the commit.
    fs::write(dir.join("src/a.rs"), WORK_SRC).unwrap();

    let root_name = dir.file_name().unwrap().to_str().unwrap().to_string();

    let prog = parse::parse(lex::lex(CALL_PROG).unwrap()).unwrap();
    let conn = db::open(Some(dir.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, dir.clone());
    // Same convergence loop as graph_diff_rev.rs: the diff_pair fact lands
    // tick 1, the data-driven scans + call extraction land tick 2, derived
    // rows converge after. Tick to a fixpoint, the daemon's steady state.
    for _ in 0..4 { eng.tick(&prog, true).unwrap(); }

    let mut dump: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
    for &(rel, cols) in REL_COLS {
        let select = cols.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
        let sql = format!("SELECT {select} FROM rel_{rel}_txt");
        let rows = eng.query_sql(&sql, &[]).unwrap();
        let mut lines: Vec<String> = rows
            .into_iter()
            .map(|row| {
                row.iter()
                    .map(|cell| normalize(&cell_to_string(cell), &head_sha, &root_name))
                    .collect::<Vec<_>>()
                    .join("\t")
            })
            .collect();
        lines.sort();
        dump.insert(rel, lines);
    }

    let _ = fs::remove_dir_all(&dir);
    dump
}

fn golden_path(rel: &str) -> PathBuf {
    fixtures_dir().join(format!("{rel}.tsv"))
}

fn load_goldens() -> BTreeMap<&'static str, Vec<String>> {
    let mut goldens = BTreeMap::new();
    for &(rel, _) in REL_COLS {
        let path = golden_path(rel);
        let content = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read golden {}: {e}", path.display()));
        let lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
        goldens.insert(rel, lines);
    }
    goldens
}

/// Panics naming the rel and the first differing row (or the first row past
/// the shorter side, on a length mismatch).
fn assert_matches_golden(rel: &str, got: &[String], golden: &[String], label: &str) {
    if got == golden {
        return;
    }
    let detail = match got.iter().zip(golden.iter()).enumerate().find(|(_, (g, w))| g != w) {
        Some((i, (g, w))) => format!("row {i}: got {g:?}, want {w:?}"),
        None if got.len() > golden.len() => {
            format!("extra got row {}: {:?}", golden.len(), got[golden.len()])
        }
        None => format!("missing golden row {}: {:?}", got.len(), golden[got.len()]),
    };
    panic!(
        "call_family_matches_golden[{label}]: rel `{rel}` diverged from golden ({} got rows, {} golden rows) — {detail}",
        got.len(), golden.len()
    );
}

/// Producer (Deliverable 1): dumps the 7 rels through the family router — the
/// sole writer of every public call rel post-P4. Run explicitly (`--ignored`)
/// to regenerate; never runs as part of the default suite.
#[test]
#[ignore]
fn regen_call_goldens() {
    let dump = build_and_dump("regen");

    let dir = fixtures_dir();
    fs::create_dir_all(&dir).unwrap();
    for &(rel, _) in REL_COLS {
        let lines = dump.get(rel).expect("dump covers every rel in REL_COLS");
        assert!(!lines.is_empty(), "rel `{rel}` produced zero rows; a vacuous golden is worse than none");
        let content = format!("{}\n", lines.join("\n"));
        fs::write(golden_path(rel), content).unwrap();
        println!("wrote {} rows to {}", lines.len(), golden_path(rel).display());
    }
}

/// Consumer (Deliverable 2): the SAME fixture + dump, compared against the
/// frozen goldens. Post-P4 there is only one state to check — the family
/// router is the sole writer of every public call rel, no gate involved —
/// collapsed from the pre-cutover both-gate-states parameterization.
#[test]
fn call_family_matches_golden() {
    let goldens = load_goldens();
    let dump = build_and_dump("consumer");
    for &(rel, _) in REL_COLS {
        assert_matches_golden(rel, &dump[rel], &goldens[rel], "family router");
    }
}
