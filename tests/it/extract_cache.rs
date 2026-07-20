//! Perf gap A structural proof: the type/call/dataflow extractors skip a warm
//! no-change tick entirely (the persisted `extract:*` input digest) and, when a
//! file DOES change, re-parse only the moved content (the per-file fact cache
//! keyed on (repo, path, content hash)). `Engine::extract_files_parsed` is the
//! instrumentation counter — cache misses only — so a regression back to
//! full-corpus re-parse fails here, not in a profile trace.

use sprefa_v5::{db, engine::Engine, lex, parse};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("extract_cache_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

const PROG: &str = r#"
rel seen(path: file).
seen(p) <- scan("WORK", "src/**/*.rs", p, rev), match(p, rev, /fn/, line).
? type_entity(repo, sym, name, kind, parent, file, line).
? call_def(repo, sym, kind, file, line, end).
? df_node(id, kind, var, fn, file, line).
"#;

#[test]
fn production_bundle_ab_matches_rows_and_reduces_physical_parses() {
    let run = |tag: &str, separate: bool| {
        let d = sandbox(tag);
        fs::write(d.join("src/a.rs"),
            "pub struct Alpha { pub n: u32 }\npub fn alpha() -> Alpha { Alpha { n: helper() } }\nfn helper() -> u32 { 1 }\n").unwrap();
        let prog = parse::parse(lex::lex(PROG).unwrap()).unwrap();
        let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
        let mut eng = Engine::new(conn, d);
        eng.force_separate_analysis_extractors.set(separate);
        eng.tick(&prog, true).unwrap();
        let rows = [
            count(&eng, "SELECT COUNT(*) FROM rel_type_entity_txt"),
            count(&eng, "SELECT COUNT(*) FROM rel_call_def_txt"),
            count(&eng, "SELECT COUNT(*) FROM rel_df_node_txt"),
        ];
        (eng.extract_files_parsed.get(), rows)
    };

    let (separate_parses, separate_rows) = run("ab_separate", true);
    let (bundle_parses, bundle_rows) = run("ab_bundle", false);
    assert_eq!(separate_rows, bundle_rows, "production A/B rows must match");
    assert_eq!(separate_parses, 3, "legacy arm parses once per family");
    assert_eq!(bundle_parses, 1, "bundle arm parses once per file");
}

#[test]
fn warm_tick_skips_extraction_and_edit_reparses_only_the_moved_file() {
    let d = sandbox("warm");
    fs::write(d.join("src/a.rs"), "pub struct Alpha { pub n: u32 }\npub fn alpha() -> Alpha { Alpha { n: helper() } }\nfn helper() -> u32 { 1 }\n").unwrap();
    fs::write(d.join("src/b.rs"), "pub struct Beta { pub s: String }\npub fn beta() -> Beta { Beta { s: String::new() } }\n").unwrap();

    let prog = parse::parse(lex::lex(PROG).unwrap()).unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());

    // Cold tick: the Rust bundle parses each file once and primes all three
    // family caches without retaining source or ASTs.
    eng.tick(&prog, true).unwrap();
    let cold = eng.extract_files_parsed.get();
    assert_eq!(cold, 2, "cold tick parses each Rust file once for all three families");

    // Warm no-change tick: the extract:* digests match, no family parses.
    eng.tick(&prog, true).unwrap();
    assert_eq!(eng.extract_files_parsed.get(), cold,
        "a no-change tick must not re-parse any file");
    // ... and the rows are still served from the db (skip, not wipe).
    let n: i64 = eng.query_sql("SELECT COUNT(*) FROM rel_type_entity_txt", &[]).unwrap().len() as i64;
    assert!(n > 0, "type_entity rows survive the skipped refresh");

    // Edit ONE file: each family re-parses only it (the other file is a
    // (path, content hash) cache hit). The new content has a different byte
    // length — reconcile's (mtime secs, size) fast path can't see a same-
    // second same-size rewrite.
    fs::write(d.join("src/a.rs"), "pub struct Alpha { pub n: u64, pub extra: bool }\npub fn alpha() -> Alpha { Alpha { n: helper(), extra: true } }\nfn helper() -> u64 { 2 }\n").unwrap();
    eng.tick_paths(&prog, &[d.join("src/a.rs")], true).unwrap();
    assert_eq!(eng.extract_files_parsed.get(), cold + 1,
        "an edit re-parses the moved Rust file once for all three families");
    assert!(count(&eng, "SELECT COUNT(*) FROM rel_type_entity_txt") > 0);
    assert!(count(&eng, "SELECT COUNT(*) FROM rel_call_def_txt") > 0);
    assert!(count(&eng, "SELECT COUNT(*) FROM rel_df_node_txt") > 0);
}

#[test]
fn fresh_process_over_a_warm_db_still_skips() {
    let d = sandbox("crossproc");
    fs::write(d.join("src/a.rs"), "pub struct Gamma { pub n: u32 }\npub fn gamma() -> u32 { 3 }\n").unwrap();
    let prog = parse::parse(lex::lex(PROG).unwrap()).unwrap();

    {
        let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
        let mut eng = Engine::new(conn, d.clone());
        eng.tick(&prog, true).unwrap();
        assert!(eng.extract_files_parsed.get() > 0);
    }
    // Same db, new Engine (the one-shot rerun shape): the persisted digest
    // skips the parse even though the in-memory fact cache is cold.
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    eng.tick(&prog, true).unwrap();
    assert_eq!(eng.extract_files_parsed.get(), 0,
        "a fresh process over an unchanged db parses nothing");
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().expect("git");
    assert!(out.status.success(), "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr));
}

/// The revs that carry a stored `type` extract digest. The key form is
/// `extract:<exe>:type:<rev>` (the running binary's stamp is a namespace
/// segment so two binaries sharing a cache keep separate skip-state); rev is
/// the trailing segment, so this stays exe-agnostic.
fn type_digest_revs(eng: &Engine) -> Vec<String> {
    eng.query_sql(
        "SELECT rel FROM _reldigest WHERE rel LIKE 'extract:%:type:%' ORDER BY rel", &[])
        .unwrap().into_iter()
        .map(|r| r[0].as_str().unwrap().rsplit(':').next().unwrap().to_string()).collect()
}

/// The full stored digest key for one family+rev in the current exe namespace.
fn digest_key_for(eng: &Engine, family: &str, rev: &str) -> String {
    let like = format!("extract:%:{family}:{rev}");
    let rows = eng.query_sql(
        "SELECT rel FROM _reldigest WHERE rel LIKE ?1 LIMIT 1",
        &[serde_json::Value::String(like)]).unwrap();
    rows.into_iter().next()
        .unwrap_or_else(|| panic!("no digest key for {family}:{rev}"))[0]
        .as_str().unwrap().to_string()
}

fn digest_of(eng: &Engine, key: &str) -> String {
    let rows = eng.query_sql(
        "SELECT digest FROM _reldigest WHERE rel = ?1",
        &[serde_json::Value::String(key.to_string())]).unwrap();
    rows[0][0].as_str().unwrap().to_string()
}

const REV_PROG: &str = r#"
rel seen(path: file).
seen(p) <- scan("WORK", "src/**/*.rs", p, rev), match(p, rev, /fn/, line).
seen(p) <- scan("HEAD", "src/**/*.rs", p, rev), match(p, rev, /fn/, line).
? type_entity(repo, sym, name, kind, parent, file, line).
"#;

/// D5.1: the input digest keys on (family, rev). A corpus with two revs (WORK +
/// one committed HEAD) ticks clean; a WORK edit moves ONLY the WORK rev's
/// digest, leaving the committed rev's digest frozen; a warm no-change tick
/// still skips every family.
#[test]
fn per_rev_digest_isolates_a_work_edit_from_the_committed_rev() {
    let d = sandbox("tworev");
    fs::write(d.join("src/a.rs"),
        "pub struct Alpha { pub n: u32 }\npub fn alpha() -> Alpha { Alpha { n: 1 } }\n").unwrap();
    git(&d, &["init", "-q"]);
    git(&d, &["config", "user.email", "t@example.com"]);
    git(&d, &["config", "user.name", "T"]);
    git(&d, &["add", "."]);
    git(&d, &["commit", "-q", "-m", "head"]);

    let prog = parse::parse(lex::lex(REV_PROG).unwrap()).unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());

    // (a) the two-rev corpus ticks and lands one type digest per rev.
    eng.tick(&prog, true).unwrap();
    let revs = type_digest_revs(&eng);
    assert_eq!(revs.len(), 2, "one type digest per rev (WORK + HEAD sha): {revs:?}");
    assert!(revs.contains(&"WORK".to_string()));
    let work_key = digest_key_for(&eng, "type", "WORK");
    let head_rev = revs.iter().find(|r| *r != "WORK").unwrap().clone();
    let head_key = digest_key_for(&eng, "type", &head_rev);
    let n: usize = eng.query_sql("SELECT COUNT(*) FROM rel_type_entity_txt", &[]).unwrap()
        .into_iter().next().unwrap()[0].as_i64().unwrap() as usize;
    assert!(n > 0, "type_entity rows land across both revs");

    let work0 = digest_of(&eng, &work_key);
    let head0 = digest_of(&eng, &head_key);

    // (c) warm no-change tick: no family re-parses, no digest moves.
    let parsed = eng.extract_files_parsed.get();
    eng.tick(&prog, true).unwrap();
    assert_eq!(eng.extract_files_parsed.get(), parsed, "a no-change tick re-parses nothing");
    assert_eq!(digest_of(&eng, &work_key), work0);
    assert_eq!(digest_of(&eng, &head_key), head0);

    // (b) edit the WORK file: only the WORK rev's digest moves; the committed
    // rev's content is unchanged, so its digest is frozen.
    fs::write(d.join("src/a.rs"),
        "pub struct Alpha { pub n: u64, pub extra: bool }\npub fn alpha() -> Alpha { Alpha { n: 2, extra: true } }\n").unwrap();
    eng.tick(&prog, true).unwrap();
    assert_ne!(digest_of(&eng, &work_key), work0, "WORK digest moves on a WORK edit");
    assert_eq!(digest_of(&eng, &head_key), head0, "committed rev digest stays frozen");
}

/// A single-column query result as a set (dedups + order-free compare).
/// Stringifies whatever JSON type the cell came back as — a `sym`-typed id
/// column (df_node_rev.id and kin, INTEGER as of the intern-key arc) reads
/// back as a JSON Number, not a String.
fn col_set(eng: &Engine, sql: &str) -> BTreeSet<String> {
    eng.query_sql(sql, &[]).unwrap().into_iter()
        .map(|r| match &r[0] {
            serde_json::Value::String(s) => s.clone(),
            v => v.to_string(),
        })
        .collect()
}

fn count(eng: &Engine, sql: &str) -> i64 {
    eng.query_sql(sql, &[]).unwrap().into_iter().next().unwrap()[0].as_i64().unwrap()
}

const TWIN_PROG: &str = r#"
rel seen(path: file).
seen(p) <- scan("WORK", "src/**/*.rs", p, rev), match(p, rev, /fn/, line).
seen(p) <- scan("HEAD", "src/**/*.rs", p, rev), match(p, rev, /fn/, line).
? type_entity_rev(repo, sym, name, kind, parent, file, line, rev).
? type_link_rev(src, dst, kind, rev).
? call_def_rev(repo, sym, kind, file, line, end, rev).
"#;

/// D5.2 + D5.3: the type/call twins carry a `rev` COLUMN, never folded into the
/// sym. A two-rev corpus (WORK + committed HEAD of identical content) populates
/// type_entity_rev / type_link_rev / call_def_rev at BOTH rev values, with the
/// SAME sym strings across revs (the diff's whole premise). The legacy rels are
/// the rev-deduped projection of their twins.
#[test]
fn type_and_call_twins_carry_both_revs_with_stable_syms() {
    let d = sandbox("twins");
    fs::write(d.join("src/a.rs"),
        "pub struct Gadget { pub n: i64 }\n\
         pub struct Widget { pub part: Gadget }\n\
         pub fn make() -> Widget { Widget { part: Gadget { n: 1 } } }\n").unwrap();
    git(&d, &["init", "-q"]);
    git(&d, &["config", "user.email", "t@example.com"]);
    git(&d, &["config", "user.name", "T"]);
    git(&d, &["add", "."]);
    git(&d, &["commit", "-q", "-m", "head"]);

    let prog = parse::parse(lex::lex(TWIN_PROG).unwrap()).unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    eng.tick(&prog, true).unwrap();

    // (a) each twin spans both revs (WORK + the committed HEAD sha).
    let ent_revs = col_set(&eng, "SELECT DISTINCT rev FROM rel_type_entity_rev_txt");
    assert_eq!(ent_revs.len(), 2, "type_entity_rev spans WORK + HEAD: {ent_revs:?}");
    assert!(ent_revs.contains("WORK"), "WORK rev present: {ent_revs:?}");
    assert_eq!(col_set(&eng, "SELECT DISTINCT rev FROM rel_call_def_rev_txt").len(), 2,
        "call_def_rev spans both revs");
    assert_eq!(col_set(&eng, "SELECT DISTINCT rev FROM rel_type_link_rev_txt").len(), 2,
        "type_link_rev spans both revs");

    // (a, the crux) the sym strings are IDENTICAL across revs for unchanged code
    // — rev is a column, never folded into the sym, so a diff compares the same
    // string at both revs. WORK and the committed HEAD have byte-identical
    // content, so the two rev projections must be equal AND non-empty.
    let work_ent = col_set(&eng, "SELECT sym FROM rel_type_entity_rev_txt WHERE rev = 'WORK'");
    let head_ent = col_set(&eng, "SELECT sym FROM rel_type_entity_rev_txt WHERE rev <> 'WORK'");
    assert!(!work_ent.is_empty(), "type_entity_rev has WORK syms");
    assert_eq!(work_ent, head_ent, "type_entity syms are identical across revs");

    let work_def = col_set(&eng, "SELECT sym FROM rel_call_def_rev_txt WHERE rev = 'WORK'");
    let head_def = col_set(&eng, "SELECT sym FROM rel_call_def_rev_txt WHERE rev <> 'WORK'");
    assert!(!work_def.is_empty(), "call_def_rev has WORK syms (fn make)");
    assert_eq!(work_def, head_def, "call_def syms are identical across revs");

    let work_link = col_set(&eng, "SELECT src || char(31) || dst || char(31) || kind FROM rel_type_link_rev_txt WHERE rev = 'WORK'");
    let head_link = col_set(&eng, "SELECT src || char(31) || dst || char(31) || kind FROM rel_type_link_rev_txt WHERE rev <> 'WORK'");
    assert!(!work_link.is_empty(), "type_link_rev has WORK edges (Widget -> Gadget)");
    assert_eq!(work_link, head_link, "type_link edges are identical across revs");

    // (b) each legacy rel equals the rev-deduped projection (drop rev) of its
    // twin — full-row equality, not just a count.
    assert_eq!(
        col_set(&eng, "SELECT repo||char(31)||sym||char(31)||name||char(31)||kind||char(31)||parent||char(31)||file||char(31)||line FROM rel_type_entity_txt"),
        col_set(&eng, "SELECT DISTINCT repo||char(31)||sym||char(31)||name||char(31)||kind||char(31)||parent||char(31)||file||char(31)||line FROM rel_type_entity_rev_txt"),
        "legacy type_entity = rev-deduped projection of type_entity_rev");
    assert_eq!(
        col_set(&eng, "SELECT src||char(31)||dst||char(31)||kind FROM rel_type_link_txt"),
        col_set(&eng, "SELECT DISTINCT src||char(31)||dst||char(31)||kind FROM rel_type_link_rev_txt"),
        "legacy type_link = rev-deduped projection of type_link_rev");
    assert_eq!(
        col_set(&eng, "SELECT repo||char(31)||sym||char(31)||kind||char(31)||file||char(31)||line||char(31)||end FROM rel_call_def_txt"),
        col_set(&eng, "SELECT DISTINCT repo||char(31)||sym||char(31)||kind||char(31)||file||char(31)||line||char(31)||end FROM rel_call_def_rev_txt"),
        "legacy call_def = rev-deduped projection of call_def_rev");
    // sanity: the legacy rels are non-empty (the projection actually ran).
    assert!(count(&eng, "SELECT COUNT(*) FROM rel_type_entity_txt") > 0);
    assert!(count(&eng, "SELECT COUNT(*) FROM rel_call_def_txt") > 0);
}

const DF_TWIN_PROG: &str = r#"
rel seen(path: file).
seen(p) <- scan("WORK", "src/**/*.rs", p, rev), match(p, rev, /fn/, line).
seen(p) <- scan("HEAD", "src/**/*.rs", p, rev), match(p, rev, /fn/, line).
? df_node_rev(id, kind, var, fn, file, line, rev).
? df_node_repo_rev(id, repo, rev).
? df_arg_rev(call, pos, arg, rev).
? df_field_rev(id, field, value, rev).
"#;

/// D5.4, as amended by the composite-key arc: the df twins keep `rev` as a
/// plain trailing COLUMN, like type/call syms, and no longer fold it into the
/// id via a string salt. A byte-identical file at two revs therefore yields the
/// SAME interned `file:line:col` id at both, and table integrity comes from
/// `df_node_rev`'s `PRIMARY KEY (id, rev)` rather than from making the ids
/// disjoint. Legacy `df_node` holds that same id set, rev-deduped. And every
/// id-valued twin column (df_node_repo_rev.id, df_arg_rev.call,
/// df_field_rev.id) joins df_node_rev.id within its own rev — unchanged by the
/// desalting, since those columns always carried whatever id df_node_rev did.
#[test]
fn df_twins_key_on_id_rev_and_join_within_a_rev() {
    let d = sandbox("dftwin");
    fs::write(d.join("src/a.rs"),
        "pub struct Gadget { pub n: i64 }\n\
         pub struct Widget { pub part: Gadget }\n\
         pub fn helper(x: i64) -> i64 { x }\n\
         pub fn make() -> Widget { Widget { part: Gadget { n: helper(1) } } }\n").unwrap();
    git(&d, &["init", "-q"]);
    git(&d, &["config", "user.email", "t@example.com"]);
    git(&d, &["config", "user.name", "T"]);
    git(&d, &["add", "."]);
    git(&d, &["commit", "-q", "-m", "head"]);

    let prog = parse::parse(lex::lex(DF_TWIN_PROG).unwrap()).unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    eng.tick(&prog, true).unwrap();

    // (a) df_node ids are SHARED across the two revs, both non-empty. The file
    // is byte-identical at WORK and HEAD, so every id must appear at both —
    // this is the exact assertion the old salt inverted, and reading it as a
    // join is the point: an id-join across revs is now meaningful.
    let work_ids = col_set(&eng, "SELECT id FROM rel_df_node_rev_txt WHERE rev = 'WORK'");
    let head_ids = col_set(&eng, "SELECT id FROM rel_df_node_rev_txt WHERE rev <> 'WORK'");
    assert!(!work_ids.is_empty(), "df_node_rev has WORK ids");
    assert!(!head_ids.is_empty(), "df_node_rev has committed-rev ids");
    assert_eq!(work_ids, head_ids,
        "byte-identical content at two revs interns to the SAME df_node ids (no rev salt)");

    // Disjointness now lives in the composite PRIMARY KEY (id, rev), not in the
    // id: each rev keeps its OWN row for a shared id, so the twin's row count
    // is the sum of the per-rev counts even though the id sets are equal.
    let total_rows = count(&eng, "SELECT COUNT(*) FROM rel_df_node_rev_txt");
    let work_rows = count(&eng, "SELECT COUNT(*) FROM rel_df_node_rev_txt WHERE rev = 'WORK'");
    let head_rows = count(&eng, "SELECT COUNT(*) FROM rel_df_node_rev_txt WHERE rev <> 'WORK'");
    assert_eq!(total_rows, work_rows + head_rows,
        "PRIMARY KEY (id, rev) keeps one row per rev for a shared id");

    // legacy df_node keeps the same ids, rev-deduped (one row per id despite
    // two identical revs) — the single-rev daemon sees today's behavior. No
    // recovery join is needed any more: the twin id IS the legacy id.
    let legacy = col_set(&eng, "SELECT id FROM rel_df_node_txt");
    assert_eq!(legacy, work_ids, "legacy df_node = the same ids, rev-deduped");

    // (b) each id-valued twin column joins df_node_rev.id WITHIN its rev (the salt
    // is applied identically to every column that joins on node identity).
    assert!(count(&eng,
        "SELECT COUNT(*) FROM rel_df_node_repo_rev_txt r \
         JOIN rel_df_node_rev_txt n ON r.id = n.id AND r.rev = n.rev") > 0,
        "df_node_repo_rev.id joins df_node_rev.id within a rev");
    assert!(count(&eng,
        "SELECT COUNT(*) FROM rel_df_arg_rev_txt a \
         JOIN rel_df_node_rev_txt n ON a.call = n.id AND a.rev = n.rev") > 0,
        "df_arg_rev.call joins df_node_rev.id within a rev (helper(1) call site)");
    assert!(count(&eng,
        "SELECT COUNT(*) FROM rel_df_field_rev_txt f \
         JOIN rel_df_node_rev_txt n ON f.id = n.id AND f.rev = n.rev") > 0,
        "df_field_rev.id joins df_node_rev.id within a rev (struct-literal field)");
}

const RETRACT_PROG_TWO_REV: &str = r#"
rel seen(path: file).
seen(p) <- scan("WORK", "src/**/*.rs", p, rev), match(p, rev, /fn/, line).
seen(p) <- scan("HEAD", "src/**/*.rs", p, rev), match(p, rev, /fn/, line).
? type_entity_rev(repo, sym, name, kind, parent, file, line, rev).
? call_def_rev(repo, sym, kind, file, line, end, rev).
? df_node_rev(id, kind, var, fn, file, line, rev).
"#;

const RETRACT_PROG_WORK_ONLY: &str = r#"
rel seen(path: file).
seen(p) <- scan("WORK", "src/**/*.rs", p, rev), match(p, rev, /fn/, line).
? type_entity_rev(repo, sym, name, kind, parent, file, line, rev).
? call_def_rev(repo, sym, kind, file, line, end, rev).
? df_node_rev(id, kind, var, fn, file, line, rev).
"#;

/// D5.5: a rev that stops being scanned (here the `scan("HEAD", ...)` rule is
/// dropped from the program entirely, not a file edit) must disappear from
/// every `_rev` twin, the rev-deduped legacy union, AND the `extract:<family>:
/// <rev>` digest key -- all in the SAME tick that retires it. The retired
/// rev's entity/fn (`OldOnly`/`old_only_fn`) is unique to the committed rev
/// (WORK never has it, by construction), so its presence/absence is a clean
/// per-rev marker. Without the sweep's own unconditional legacy rebuild, the
/// type/call families would never revisit their internal `rebuild_legacy_*`
/// call this tick (their per-rev digest early return sees the still-live WORK
/// rev unchanged and skips the whole family), so the retired rev's rows would
/// linger in legacy — this test is the regression guard for that gap.
#[test]
fn sweep_retracts_gone_rev_from_twins_and_legacy() {
    let d = sandbox("sweep");
    fs::write(d.join("src/a.rs"),
        "pub struct OldOnly { pub n: i64 }\npub fn old_only_fn() -> i64 { 1 }\n").unwrap();
    git(&d, &["init", "-q"]);
    git(&d, &["config", "user.email", "t@example.com"]);
    git(&d, &["config", "user.name", "T"]);
    git(&d, &["add", "."]);
    git(&d, &["commit", "-q", "-m", "head"]);
    // WORK diverges from HEAD with disjoint names, so each rev's rows are a
    // clean, unambiguous marker.
    fs::write(d.join("src/a.rs"),
        "pub struct NewOnly { pub n: i64 }\npub fn new_only_fn() -> i64 { 2 }\n").unwrap();

    let prog_both = parse::parse(lex::lex(RETRACT_PROG_TWO_REV).unwrap()).unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());

    // (a) both revs scanned: OldOnly (HEAD-only) and NewOnly (WORK-only) both
    // land in the twins, the legacy union, and a per-rev digest exists.
    eng.tick(&prog_both, true).unwrap();
    let ent_revs = col_set(&eng, "SELECT DISTINCT rev FROM rel_type_entity_rev_txt");
    assert_eq!(ent_revs.len(), 2, "two revs present before retraction: {ent_revs:?}");
    let head_rev = ent_revs.iter().find(|r| *r != "WORK").unwrap().clone();

    assert!(count(&eng, "SELECT COUNT(*) FROM rel_type_entity_rev_txt WHERE name = 'OldOnly'") > 0);
    assert!(count(&eng, "SELECT COUNT(*) FROM rel_type_entity_rev_txt WHERE name = 'NewOnly'") > 0);
    assert!(count(&eng, "SELECT COUNT(*) FROM rel_type_entity_txt WHERE name = 'OldOnly'") > 0,
        "legacy type_entity has the HEAD-only entity before retraction");
    assert!(count(&eng, "SELECT COUNT(*) FROM rel_call_def_rev_txt WHERE sym LIKE '%old_only_fn%'") > 0);
    assert!(count(&eng, "SELECT COUNT(*) FROM rel_call_def_txt WHERE sym LIKE '%old_only_fn%'") > 0,
        "legacy call_def has the HEAD-only fn before retraction");
    assert!(count(&eng, &format!(
        "SELECT COUNT(*) FROM rel_df_node_rev_txt WHERE rev = '{head_rev}'")) > 0,
        "df_node_rev has HEAD rows before retraction");

    // Digest keys are `extract:<exe>:<family>:<rev>` (see `digest_key_for`), so
    // match the family+rev across any exe namespace.
    let digest_like = |fam: &str| format!("extract:%:{fam}:{head_rev}");
    for fam in ["type", "call", "dataflow"] {
        assert!(count(&eng, &format!(
            "SELECT COUNT(*) FROM _reldigest WHERE rel LIKE '{}'", digest_like(fam))) > 0,
            "{fam}:{head_rev} digest exists before retraction");
    }

    // (b) drop the HEAD scan rule entirely (the program changes, WORK's file
    // content does not) and re-tick.
    let prog_work_only = parse::parse(lex::lex(RETRACT_PROG_WORK_ONLY).unwrap()).unwrap();
    eng.tick(&prog_work_only, true).unwrap();

    // the retired rev's rows are gone from every twin...
    let expected_revs: BTreeSet<String> = ["WORK".to_string()].into_iter().collect();
    assert_eq!(col_set(&eng, "SELECT DISTINCT rev FROM rel_type_entity_rev_txt"), expected_revs,
        "only WORK remains in type_entity_rev after retraction");
    assert_eq!(count(&eng, "SELECT COUNT(*) FROM rel_type_entity_rev_txt WHERE name = 'OldOnly'"), 0);
    assert_eq!(count(&eng, &format!(
        "SELECT COUNT(*) FROM rel_call_def_rev_txt WHERE rev = '{head_rev}'")), 0);
    assert_eq!(count(&eng, &format!(
        "SELECT COUNT(*) FROM rel_df_node_rev_txt WHERE rev = '{head_rev}'")), 0);

    // ...and from the legacy union, in the SAME tick (the sweep rebuilds
    // legacy directly rather than waiting for a family to re-run on its own).
    assert_eq!(count(&eng, "SELECT COUNT(*) FROM rel_type_entity_txt WHERE name = 'OldOnly'"), 0,
        "legacy type_entity drops the retired rev's entity this tick");
    assert_eq!(count(&eng, "SELECT COUNT(*) FROM rel_call_def_txt WHERE sym LIKE '%old_only_fn%'"), 0,
        "legacy call_def drops the retired rev's def this tick");

    // ...and the stale per-rev digest keys are swept too.
    for fam in ["type", "call", "dataflow"] {
        assert_eq!(count(&eng, &format!(
            "SELECT COUNT(*) FROM _reldigest WHERE rel LIKE '{}'", digest_like(fam))), 0,
            "{fam}:{head_rev} digest is swept once its rev is gone");
    }

    // WORK's own rows/legacy/digest survive the sweep untouched.
    assert!(count(&eng, "SELECT COUNT(*) FROM rel_type_entity_rev_txt WHERE name = 'NewOnly'") > 0,
        "WORK's entity survives the sweep");
    assert!(count(&eng, "SELECT COUNT(*) FROM rel_type_entity_txt WHERE name = 'NewOnly'") > 0,
        "legacy type_entity still populated for WORK after the sweep");
    assert!(count(&eng, "SELECT COUNT(*) FROM rel_call_def_txt WHERE sym LIKE '%new_only_fn%'") > 0,
        "legacy call_def still populated for WORK after the sweep");
    assert!(count(&eng, "SELECT COUNT(*) FROM _reldigest WHERE rel LIKE 'extract:%:type:WORK'") > 0,
        "WORK's own digest key survives the sweep");
}
