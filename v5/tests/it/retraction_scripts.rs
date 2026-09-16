//! T3 "edit scripts" (`plans/2026-07-16-capstone-retraction-tests.feature`,
//! Feature "T3 edit scripts"; oracle defined in
//! `plans/2026-07-16-capstone-retraction-test-plan.md`'s "Core invariant").
//!
//! The core invariant: for any sequence of edits E1..En applied to a working
//! tree, `state(incremental engine after ticking E1..En)` equals
//! `state(fresh engine extracting the final tree)` for all 7 public call
//! rels, compared as decoded row sets. Every scenario below ticks an engine
//! per script step (real files on disk, real `Engine::tick`, no hand-built
//! deltas — `tests/it/call_golden.rs` and `tests/it/extract_cache.rs` are the
//! precedent for driving the engine this way from an integration test), then
//! diffs its 7 public call rels against a brand-new engine + db rooted at the
//! SAME directory, ticking the SAME final program once over whatever is on
//! disk now. Because the oracle shares the incremental engine's root path
//! verbatim, every repo/sym/file text cell is byte-identical between the two
//! — unlike `call_golden.rs`'s two-sandbox producer/consumer split, no
//! normalization step is needed.
//!
//! Deletion-heavy on purpose (per the test plan): insertion paths had years
//! of coverage under the legacy writer; retraction is the new surface P4/P5
//! opened up.

use sprefa_v5::ast::Program;
use sprefa_v5::{db, engine::Engine, lex, parse};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "retraction_scripts_{tag}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("git");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn init_git(dir: &Path) {
    git(dir, &["init", "-q"]);
    git(dir, &["config", "user.email", "t@example.com"]);
    git(dir, &["config", "user.name", "T"]);
}

/// The single-rev call-extraction program most scenarios tick: `seen` gates
/// the call family on `scan("WORK", ...)` appearing in a rule body (the same
/// shape `tests/it/extract_cache.rs`'s `PROG` uses — a relation must appear
/// in a body atom, closure edge, or query head for the family extraction to
/// run at all), and the 7 query heads are what makes every public call rel
/// non-lazy for this program.
const CALL_PROG: &str = r#"
rel seen(path: file).
seen(p) <- scan("WORK", "src/**/*.rs", p, rev), match(p, rev, /fn/, line).
? call_site(repo, caller, callee, file, line).
? call_kind(fn_sym, kind).
? call_edge(caller, callee, kind).
? call_edge_rev(caller, callee, kind, rev).
? call_def(repo, sym, kind, file, line, end).
? call_def_rev(repo, sym, kind, file, line, end, rev).
? call_name(sym, name).
"#;

/// Builds a `.dl` program scanning exactly the given revs (each a real git
/// revision string: a sha, `"HEAD"`, or `"WORK"`) into the `seen` gate rule
/// plus the same 7 query heads as [`CALL_PROG`]. Mirrors
/// `tests/it/extract_cache.rs`'s `RETRACT_PROG_TWO_REV`/
/// `RETRACT_PROG_WORK_ONLY` pair: dropping a rev's scan clause from the NEXT
/// tick's program (never editing a file) is what makes `_live_rev_scope`
/// lose that rev and fires `sweep_gone_call_inputs`.
fn rev_prog(revs: &[&str]) -> String {
    let mut src = String::from("rel seen(path: file).\n");
    for rev in revs {
        src.push_str(&format!(
            "seen(p) <- scan(\"{rev}\", \"src/**/*.rs\", p, rev), match(p, rev, /fn/, line).\n"
        ));
    }
    src.push_str(
        "? call_site(repo, caller, callee, file, line).\n\
         ? call_kind(fn_sym, kind).\n\
         ? call_edge(caller, callee, kind).\n\
         ? call_edge_rev(caller, callee, kind, rev).\n\
         ? call_def(repo, sym, kind, file, line, end).\n\
         ? call_def_rev(repo, sym, kind, file, line, end, rev).\n\
         ? call_name(sym, name).\n",
    );
    src
}

/// out_cols() order for each of the 7 public call rels — copied from
/// `tests/it/call_golden.rs`'s `REL_COLS` (`src/engine/family/<name>.rs` is
/// the source of truth both files must stay in sync with).
const REL_COLS: &[(&str, &[&str])] = &[
    ("call_site", &["repo", "caller", "callee", "file", "line"]),
    ("call_kind", &["fn", "kind"]),
    ("call_edge", &["caller", "callee", "kind"]),
    ("call_edge_rev", &["caller", "callee", "kind", "rev"]),
    ("call_def", &["repo", "sym", "kind", "file", "line", "end"]),
    (
        "call_def_rev",
        &["repo", "sym", "kind", "file", "line", "end", "rev"],
    ),
    ("call_name", &["sym", "name"]),
];

/// The six owned `_call_*` tables `sweep_sqlite_gone_call_inputs`
/// (`src/storage/call.rs`) deletes from, together with the SQL that counts
/// ONE rev's rows in each — `_call_raw_site`/`_call_resolution` carry no
/// `rev_sid` column of their own, so their count joins up to `_call_owner`.
/// `sprf_sym` (registered in `src/db.rs`) is a pure content hash, so it works
/// against a runtime-computed rev string with no prior interning needed.
const OWNED_CALL_TABLE_COUNT_SQL: &[(&str, &str)] = &[
    (
        "_call_owner",
        "SELECT COUNT(*) FROM _call_owner WHERE rev_sid = sprf_sym(?1)",
    ),
    (
        "_call_raw_site",
        "SELECT COUNT(*) FROM _call_raw_site s JOIN _call_owner o USING(owner_id) \
         WHERE o.rev_sid = sprf_sym(?1)",
    ),
    (
        "_call_resolution",
        "SELECT COUNT(*) FROM _call_resolution r \
         JOIN _call_raw_site s USING(site_id) JOIN _call_owner o USING(owner_id) \
         WHERE o.rev_sid = sprf_sym(?1)",
    ),
    (
        "_call_def",
        "SELECT COUNT(*) FROM _call_def WHERE rev_sid = sprf_sym(?1)",
    ),
    (
        "_call_def_bucket",
        "SELECT COUNT(*) FROM _call_def_bucket WHERE rev_sid = sprf_sym(?1)",
    ),
    (
        "_call_edge_support",
        "SELECT COUNT(*) FROM _call_edge_support WHERE rev_sid = sprf_sym(?1)",
    ),
];

/// Row count of each owned `_call_*` table scoped to one rev — the input
/// `sweep_retracts_gone_rev_from_twins_and_legacy`-style assertions read
/// before/after retiring a rev.
fn owned_table_counts(engine: &Engine, rev: &str) -> Vec<(&'static str, i64)> {
    OWNED_CALL_TABLE_COUNT_SQL
        .iter()
        .map(|&(table, sql)| {
            let rows = engine
                .query_sql(sql, &[serde_json::Value::String(rev.to_string())])
                .unwrap();
            (table, rows[0][0].as_i64().unwrap())
        })
        .collect()
}

fn call_prog() -> Program {
    parse::parse(lex::lex(CALL_PROG).unwrap()).unwrap()
}

fn engine_at(dir: &Path, db_name: &str) -> Engine {
    let conn = db::open(Some(dir.join(db_name).to_str().unwrap())).unwrap();
    Engine::new(conn, dir.to_path_buf())
}

fn cell_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Decode + sort all 7 public call rels off `engine`'s live tables — the
/// row-set shape the core invariant compares. Both the incremental engine
/// under test and the fresh oracle engine go through this same function, so a
/// column-order or decode drift can't sneak a false pass in.
fn dump_call_rels(engine: &Engine) -> BTreeMap<&'static str, Vec<String>> {
    let mut dump = BTreeMap::new();
    for &(rel, cols) in REL_COLS {
        let select = cols
            .iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("SELECT {select} FROM rel_{rel}_txt");
        let rows = engine.query_sql(&sql, &[]).unwrap();
        let mut lines: Vec<String> = rows
            .into_iter()
            .map(|row| {
                row.iter()
                    .map(cell_to_string)
                    .collect::<Vec<_>>()
                    .join("\t")
            })
            .collect();
        lines.sort();
        dump.insert(rel, lines);
    }
    dump
}

/// The oracle: a brand-new engine + db rooted at the SAME directory the
/// incremental engine under test just edited, ticking `prog_src` once over
/// whatever tree is on disk right now. `prog_src` must be the SAME program
/// text the incremental engine's last tick used (for the rev-retirement
/// scenarios that means the post-retirement program, not [`CALL_PROG`]) —
/// otherwise the oracle would extract a different rev scope than the
/// incremental engine settled on.
fn oracle_dump_for(dir: &Path, tag: &str, prog_src: &str) -> BTreeMap<&'static str, Vec<String>> {
    let mut engine = engine_at(dir, &format!("oracle_{tag}.db"));
    let prog = parse::parse(lex::lex(prog_src).unwrap()).unwrap();
    engine.tick(&prog, true).unwrap();
    dump_call_rels(&engine)
}

fn oracle_dump(dir: &Path, tag: &str) -> BTreeMap<&'static str, Vec<String>> {
    oracle_dump_for(dir, tag, CALL_PROG)
}

/// Panics naming the rel and the first differing row on a mismatch — the T3
/// assertion every scenario below funnels through.
fn assert_matches_oracle(
    scenario: &str,
    incremental: &BTreeMap<&'static str, Vec<String>>,
    oracle: &BTreeMap<&'static str, Vec<String>>,
) {
    for &(rel, _) in REL_COLS {
        let got = &incremental[rel];
        let want = &oracle[rel];
        if got == want {
            continue;
        }
        let detail = match got
            .iter()
            .zip(want.iter())
            .enumerate()
            .find(|(_, (g, w))| g != w)
        {
            Some((i, (g, w))) => format!("row {i}: incremental {g:?}, oracle {w:?}"),
            None if got.len() > want.len() => {
                format!(
                    "extra incremental row {}: {:?}",
                    want.len(),
                    got[want.len()]
                )
            }
            None => format!("missing oracle row {}: {:?}", got.len(), want[got.len()]),
        };
        panic!(
            "{scenario}: rel `{rel}` diverges from the fresh-engine oracle \
             ({} incremental rows, {} oracle rows) — {detail}",
            got.len(),
            want.len(),
        );
    }
}

/// "add then delete a file": a file added mid-script and ticked is deleted
/// and ticked again — its call_site/call_edge/call_def rows must be fully
/// gone, with the surviving file's own state untouched.
#[test]
fn add_then_delete_a_file() {
    let dir = sandbox("add_delete");
    fs::write(dir.join("src/a.rs"), "pub fn alpha() {}\n").unwrap();
    let mut engine = engine_at(&dir, "db");
    let prog = call_prog();
    engine.tick(&prog, true).unwrap();

    // Add a second file whose fn calls into the first.
    fs::write(dir.join("src/b.rs"), "pub fn beta() {\n    alpha();\n}\n").unwrap();
    engine.tick(&prog, true).unwrap();
    let after_add = dump_call_rels(&engine);
    assert!(
        !after_add["call_edge"].is_empty(),
        "fixture: beta->alpha must resolve before the delete"
    );

    // Delete it and tick again.
    fs::remove_file(dir.join("src/b.rs")).unwrap();
    engine.tick(&prog, true).unwrap();

    let incremental = dump_call_rels(&engine);
    assert!(
        incremental["call_site"].is_empty(),
        "alpha() has no calls of its own; no orphan call_site rows may remain: {:?}",
        incremental["call_site"]
    );
    assert!(
        incremental["call_edge"].is_empty(),
        "beta->alpha must be fully retracted: {:?}",
        incremental["call_edge"]
    );
    for &(rel, _) in REL_COLS {
        assert!(
            incremental[rel].iter().all(|row| !row.contains("beta")),
            "no orphan `{rel}` row for the deleted file's fn: {:?}",
            incremental[rel]
        );
    }

    let oracle = oracle_dump(&dir, "add_delete");
    assert_matches_oracle("add_then_delete_a_file", &incremental, &oracle);

    let _ = fs::remove_dir_all(&dir);
}

/// "delete only the callee": caller `f` calls `g` in another file; deleting
/// only g's file must retract the f->g call_edge while leaving f's own
/// call_site row (the raw syntactic occurrence, keyed on the interned call
/// NAME rather than g's def sym — see `src/engine/family/call_site.rs`)
/// untouched.
#[test]
fn delete_only_the_callee() {
    let dir = sandbox("delete_callee");
    fs::write(dir.join("src/caller.rs"), "pub fn f() {\n    g();\n}\n").unwrap();
    fs::write(dir.join("src/callee.rs"), "pub fn g() {}\n").unwrap();
    let mut engine = engine_at(&dir, "db");
    let prog = call_prog();
    engine.tick(&prog, true).unwrap();

    let before = dump_call_rels(&engine);
    assert!(
        !before["call_edge"].is_empty(),
        "fixture: f->g must resolve before the delete"
    );
    assert!(
        !before["call_site"].is_empty(),
        "fixture: f's call site must exist before the delete"
    );
    let site_rows_before = before["call_site"].clone();

    fs::remove_file(dir.join("src/callee.rs")).unwrap();
    engine.tick(&prog, true).unwrap();

    let incremental = dump_call_rels(&engine);
    assert!(
        incremental["call_edge"].is_empty(),
        "f->g must be retracted once g's file is gone: {:?}",
        incremental["call_edge"]
    );
    assert_eq!(
        incremental["call_site"], site_rows_before,
        "f's call_site row must survive the callee's deletion"
    );

    let oracle = oracle_dump(&dir, "delete_callee");
    assert_matches_oracle("delete_only_the_callee", &incremental, &oracle);

    let _ = fs::remove_dir_all(&dir);
}

/// "rename a function in one tick": the def AND every call site move
/// together in a single file rewrite, ticked once — no stale sym may carry
/// the old name, and the edge must retarget to the new one.
#[test]
fn rename_a_function_in_one_tick() {
    let dir = sandbox("rename");
    fs::write(
        dir.join("src/a.rs"),
        "pub fn alpha() {}\npub fn caller() {\n    alpha();\n}\n",
    )
    .unwrap();
    let mut engine = engine_at(&dir, "db");
    let prog = call_prog();
    engine.tick(&prog, true).unwrap();
    let before = dump_call_rels(&engine);
    assert!(
        before["call_name"]
            .iter()
            .any(|row| row.ends_with("\talpha")),
        "fixture: alpha must be a known def name before the rename"
    );

    // Rename in a single edit: def AND every call site move together, ticked once.
    fs::write(
        dir.join("src/a.rs"),
        "pub fn beta() {}\npub fn caller() {\n    beta();\n}\n",
    )
    .unwrap();
    engine.tick(&prog, true).unwrap();

    let incremental = dump_call_rels(&engine);

    let syms: Vec<&str> = incremental["call_name"]
        .iter()
        .map(|row| row.split('\t').next().unwrap())
        .collect();
    let unique_syms: HashSet<&str> = syms.iter().copied().collect();
    assert_eq!(
        syms.len(),
        unique_syms.len(),
        "no duplicate syms may exist in call_name: {:?}",
        incremental["call_name"]
    );
    assert!(
        !incremental["call_name"]
            .iter()
            .any(|row| row.ends_with("\talpha")),
        "no sym may still carry the old name `alpha`: {:?}",
        incremental["call_name"]
    );
    let beta_names: Vec<&String> = incremental["call_name"]
        .iter()
        .filter(|row| row.ends_with("\tbeta"))
        .collect();
    assert_eq!(
        beta_names.len(),
        1,
        "exactly one sym must carry the new name `beta`: {beta_names:?}"
    );
    let beta_sym = beta_names[0].split('\t').next().unwrap();
    assert!(
        incremental["call_edge"]
            .iter()
            .any(|row| row.split('\t').nth(1) == Some(beta_sym)),
        "call_edge must retarget to beta's sym {beta_sym}: {:?}",
        incremental["call_edge"]
    );

    let oracle = oracle_dump(&dir, "rename");
    assert_matches_oracle("rename_a_function_in_one_tick", &incremental, &oracle);

    let _ = fs::remove_dir_all(&dir);
}

/// "whitespace-only edit": a line-count-preserving edit (trailing whitespace
/// on an existing line, plus a trailing comment appended AFTER every def so
/// no line number shifts) changes the file's content hash — the tick must
/// register as changed — but extracts identical facts, so zero rows may move
/// in any public call rel. `flip_call_rels_via_router`'s per-family rerun
/// list is `pub(crate)`, unreachable from this integration-test crate, so
/// "families rerun with empty deltas" is verified at the only externally
/// observable level: the tick sees a real change (`TickReport::changed`) yet
/// produces byte-identical output.
#[test]
fn whitespace_only_edit() {
    let dir = sandbox("whitespace");
    fs::write(
        dir.join("src/a.rs"),
        "pub fn alpha() {}\npub fn caller() {\n    alpha();\n}\n",
    )
    .unwrap();
    let mut engine = engine_at(&dir, "db");
    let prog = call_prog();
    engine.tick(&prog, true).unwrap();
    let before = dump_call_rels(&engine);
    assert!(
        !before["call_edge"].is_empty(),
        "fixture: caller->alpha must resolve before the edit"
    );

    // Trailing whitespace on an existing line + a trailing comment AFTER
    // every def: the content hash moves but no def's (line, end) span does.
    fs::write(
        dir.join("src/a.rs"),
        "pub fn alpha() {}\npub fn caller() {\n    alpha();   \n}\n\n// trailing comment\n",
    )
    .unwrap();
    let report = engine.tick_report(&prog, true).unwrap();
    assert!(
        report.changed,
        "the file's content hash DID move, so the tick must register a change"
    );

    let after = dump_call_rels(&engine);
    assert_eq!(
        after, before,
        "zero rows may move in any public call rel for a whitespace-only edit"
    );

    let oracle = oracle_dump(&dir, "whitespace");
    assert_matches_oracle("whitespace_only_edit", &after, &oracle);

    let _ = fs::remove_dir_all(&dir);
}

/// "retired rev is swept from all six owned tables": exercises
/// `Engine::sweep_gone_call_inputs` (`src/engine/extract/call.rs`) the same
/// way `tests/it/extract_cache.rs`'s `sweep_retracts_gone_rev_from_twins_and_
/// legacy` retires a type/df rev — dropping a rev's `scan` rule from the
/// PROGRAM (never editing its files) is what makes `_live_rev_scope` lose it.
/// The fixture's revA def CALLS another revA def, so all six owned tables
/// (including `_call_resolution`/`_call_edge_support`, which a callee-less
/// def would leave vacuously empty) hold real rows before the sweep.
#[test]
fn retired_rev_is_swept_from_all_six_owned_tables() {
    let dir = sandbox("sweep_six");
    fs::write(
        dir.join("src/a.rs"),
        "pub fn helper_a() {}\npub fn caller_a() {\n    helper_a();\n}\n",
    )
    .unwrap();
    init_git(&dir);
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "revA"]);
    let head_sha = git(&dir, &["rev-parse", "HEAD"]);
    assert_eq!(
        head_sha.len(),
        40,
        "HEAD sha must be a 40-char commit sha: {head_sha:?}"
    );

    // WORK diverges with disjoint names, so it never masks revA's rows.
    fs::write(
        dir.join("src/a.rs"),
        "pub fn helper_w() {}\npub fn caller_w() {\n    helper_w();\n}\n",
    )
    .unwrap();

    let two_rev_src = rev_prog(&["WORK", "HEAD"]);
    let one_rev_src = rev_prog(&["WORK"]);
    let two_rev_prog = parse::parse(lex::lex(&two_rev_src).unwrap()).unwrap();
    let one_rev_prog = parse::parse(lex::lex(&one_rev_src).unwrap()).unwrap();

    let mut engine = engine_at(&dir, "db");
    engine.tick(&two_rev_prog, true).unwrap();

    let counts_before = owned_table_counts(&engine, &head_sha);
    for (table, count) in &counts_before {
        assert!(
            *count > 0,
            "fixture: `{table}` must hold rows for revA before the sweep: {counts_before:?}"
        );
    }

    // Drop revA's scan rule; `sweep_gone_call_inputs` runs as part of this tick.
    let report = engine.tick_report(&one_rev_prog, true).unwrap();
    assert!(
        report.changed,
        "retiring a rev is a real state change, not a no-op tick"
    );

    // Every owned table is at zero for revA. A wrong FK deletion order (child
    // before parent survives, or vice versa) would either leave orphan rows
    // here or `sweep_sqlite_gone_call_inputs`'s DELETEs would error outright
    // (both `tick_report` above and this loop would catch it) — so a clean
    // all-zero result IS the "FK dependency chain respected" proof.
    let counts_after = owned_table_counts(&engine, &head_sha);
    for (table, count) in &counts_after {
        assert_eq!(
            *count, 0,
            "`{table}` must hold zero rows for a retired rev: {counts_after:?}"
        );
    }

    let incremental = dump_call_rels(&engine);
    for &(rel, _) in REL_COLS {
        assert!(
            incremental[rel]
                .iter()
                .all(|row| !row.contains("helper_a") && !row.contains("caller_a")),
            "no orphan public `{rel}` row for the retired rev: {:?}",
            incremental[rel]
        );
    }

    let oracle = oracle_dump_for(&dir, "sweep_six", &one_rev_src);
    assert_matches_oracle(
        "retired_rev_is_swept_from_all_six_owned_tables",
        &incremental,
        &oracle,
    );

    let _ = fs::remove_dir_all(&dir);
}

/// "sweeping twice is idempotent": once a rev is retired and swept, ticking
/// the SAME post-retirement program again with nothing else changed must
/// move zero rows and register as a no-op tick.
#[test]
fn sweeping_twice_is_idempotent() {
    let dir = sandbox("sweep_idem");
    fs::write(
        dir.join("src/a.rs"),
        "pub fn helper_a() {}\npub fn caller_a() {\n    helper_a();\n}\n",
    )
    .unwrap();
    init_git(&dir);
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "revA"]);
    let head_sha = git(&dir, &["rev-parse", "HEAD"]);
    fs::write(
        dir.join("src/a.rs"),
        "pub fn helper_w() {}\npub fn caller_w() {\n    helper_w();\n}\n",
    )
    .unwrap();

    let two_rev_src = rev_prog(&["WORK", "HEAD"]);
    let one_rev_src = rev_prog(&["WORK"]);
    let two_rev_prog = parse::parse(lex::lex(&two_rev_src).unwrap()).unwrap();
    let one_rev_prog = parse::parse(lex::lex(&one_rev_src).unwrap()).unwrap();

    let mut engine = engine_at(&dir, "db");
    engine.tick(&two_rev_prog, true).unwrap();
    assert!(
        owned_table_counts(&engine, &head_sha)
            .iter()
            .all(|(_, n)| *n > 0),
        "fixture: revA must hold owned rows before the first sweep"
    );

    // First sweep: retires revA.
    let report_first = engine.tick_report(&one_rev_prog, true).unwrap();
    assert!(
        report_first.changed_rels.iter().any(|rel| rel == "file"),
        "fixture: retiring revA must move the `file` source rel: {:?}",
        report_first.changed_rels
    );
    let counts_after_first = owned_table_counts(&engine, &head_sha);
    assert!(
        counts_after_first.iter().all(|(_, n)| *n == 0),
        "first sweep must clear revA: {counts_after_first:?}"
    );
    let dump_after_first = dump_call_rels(&engine);

    // Second identical tick: nothing on disk or in the program moved. Only a
    // `report.changed` proxy is too coarse here — an UNRELATED scip_* digest
    // reports itself "changed" every tick in this sandbox (no scip index is
    // present), independent of the call family, so `report.changed` alone
    // would false-positive on every idempotent tick. The scoped proxy for
    // "no flip fires": none of the source rels the call family actually
    // reads (`file`/`content`, the `_file` projection the sweep keys off)
    // appear in the changed set, and (below) zero rows move anywhere the
    // sweep or the public call rels could show it.
    let report_second = engine.tick_report(&one_rev_prog, true).unwrap();
    let call_relevant: Vec<&String> = report_second
        .changed_rels
        .iter()
        .filter(|rel| {
            rel.as_str() == "file" || rel.as_str() == "content" || rel.starts_with("call")
        })
        .collect();
    assert!(
        call_relevant.is_empty(),
        "no flip fires: no call-relevant source rel may report changed on an idempotent second sweep \
         (scip_* noise aside): {:?}",
        report_second.changed_rels
    );

    let counts_after_second = owned_table_counts(&engine, &head_sha);
    assert_eq!(
        counts_after_second, counts_after_first,
        "zero rows may move on the idempotent second sweep: {counts_after_second:?}"
    );
    let dump_after_second = dump_call_rels(&engine);
    assert_eq!(
        dump_after_second, dump_after_first,
        "no public call rel may move on the idempotent second sweep"
    );

    let oracle = oracle_dump_for(&dir, "sweep_idem", &one_rev_src);
    assert_matches_oracle("sweeping_twice_is_idempotent", &dump_after_second, &oracle);

    let _ = fs::remove_dir_all(&dir);
}

/// "gone rev interleaved with a live rev sharing files": revA and revB are
/// real commits in the SAME history at the SAME path (`src/a.rs`), so they
/// genuinely "share file paths" — retiring revA must leave every one of
/// revB's owned rows byte-identical.
#[test]
fn gone_rev_interleaved_with_a_live_rev_sharing_files() {
    let dir = sandbox("interleaved");
    fs::write(
        dir.join("src/a.rs"),
        "pub fn helper_a() {}\npub fn caller_a() {\n    helper_a();\n}\n",
    )
    .unwrap();
    init_git(&dir);
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "revA"]);
    let rev_a = git(&dir, &["rev-parse", "HEAD"]);

    // revB is a CHILD commit at the SAME path with disjoint names, so its
    // rows are a clean per-rev marker.
    fs::write(
        dir.join("src/a.rs"),
        "pub fn helper_b() {}\npub fn caller_b() {\n    helper_b();\n}\n",
    )
    .unwrap();
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "revB"]);
    let rev_b = git(&dir, &["rev-parse", "HEAD"]);
    assert_ne!(
        rev_a, rev_b,
        "fixture: revA and revB must be distinct commits"
    );

    let two_rev_src = rev_prog(&[&rev_a, &rev_b]);
    let one_rev_src = rev_prog(&[&rev_b]);
    let two_rev_prog = parse::parse(lex::lex(&two_rev_src).unwrap()).unwrap();
    let one_rev_prog = parse::parse(lex::lex(&one_rev_src).unwrap()).unwrap();

    let mut engine = engine_at(&dir, "db");
    engine.tick(&two_rev_prog, true).unwrap();
    let revb_before = owned_table_counts(&engine, &rev_b);
    assert!(
        revb_before.iter().all(|(_, n)| *n > 0),
        "fixture: revB must hold owned rows before the sweep: {revb_before:?}"
    );

    // Retire revA; revB shares the same file path and must survive untouched.
    engine.tick(&one_rev_prog, true).unwrap();

    let revb_after = owned_table_counts(&engine, &rev_b);
    assert_eq!(
        revb_after, revb_before,
        "revB's owned rows must survive revA's retraction untouched: {revb_after:?}"
    );
    let rev_a_after = owned_table_counts(&engine, &rev_a);
    assert!(
        rev_a_after.iter().all(|(_, n)| *n == 0),
        "revA must be fully swept: {rev_a_after:?}"
    );

    let incremental = dump_call_rels(&engine);
    for &(rel, _) in REL_COLS {
        assert!(
            incremental[rel]
                .iter()
                .all(|row| !row.contains("helper_a") && !row.contains("caller_a")),
            "no leftover revA row in `{rel}`: {:?}",
            incremental[rel]
        );
    }

    let oracle = oracle_dump_for(&dir, "interleaved", &one_rev_src);
    assert_matches_oracle(
        "gone_rev_interleaved_with_a_live_rev_sharing_files",
        &incremental,
        &oracle,
    );

    let _ = fs::remove_dir_all(&dir);
}

/// "degenerate trees", Examples row "empty": a working tree with zero source
/// files must produce all-empty public call rels and no error.
#[test]
fn degenerate_tree_empty() {
    let dir = sandbox("degenerate_empty");
    // `sandbox` creates `src/` but no files land inside it.
    let mut engine = engine_at(&dir, "db");
    let prog = call_prog();
    engine.tick(&prog, true).unwrap();

    let incremental = dump_call_rels(&engine);
    for &(rel, _) in REL_COLS {
        assert!(
            incremental[rel].is_empty(),
            "empty tree: rel `{rel}` must be empty: {:?}",
            incremental[rel]
        );
    }

    let oracle = oracle_dump(&dir, "degenerate_empty");
    assert_matches_oracle("degenerate_tree_empty", &incremental, &oracle);

    let _ = fs::remove_dir_all(&dir);
}

/// "degenerate trees", Examples row "rust files with no calls": source files
/// with no `fn` at all (so no defs, no call sites, no edges) must also
/// produce all-empty public call rels.
#[test]
fn degenerate_tree_rust_files_with_no_calls() {
    let dir = sandbox("degenerate_no_calls");
    fs::write(
        dir.join("src/a.rs"),
        "pub struct Alpha { pub n: i64 }\npub const N: i64 = 1;\n",
    )
    .unwrap();
    let mut engine = engine_at(&dir, "db");
    let prog = call_prog();
    engine.tick(&prog, true).unwrap();

    let incremental = dump_call_rels(&engine);
    for &(rel, _) in REL_COLS {
        assert!(
            incremental[rel].is_empty(),
            "no-calls tree: rel `{rel}` must be empty: {:?}",
            incremental[rel]
        );
    }

    let oracle = oracle_dump(&dir, "degenerate_no_calls");
    assert_matches_oracle(
        "degenerate_tree_rust_files_with_no_calls",
        &incremental,
        &oracle,
    );

    let _ = fs::remove_dir_all(&dir);
}

/// "add and delete the same file in one tick": a file created AND removed
/// before the engine ever ticks again must be indistinguishable from a tick
/// where the file never existed — the engine only ever observes the
/// filesystem at tick-time.
#[test]
fn add_and_delete_the_same_file_in_one_tick() {
    let dir = sandbox("add_delete_same_tick");
    fs::write(dir.join("src/a.rs"), "pub fn alpha() {}\n").unwrap();
    let mut engine = engine_at(&dir, "db");
    let prog = call_prog();
    engine.tick(&prog, true).unwrap();
    let baseline = dump_call_rels(&engine);

    fs::write(dir.join("src/b.rs"), "pub fn beta() {\n    alpha();\n}\n").unwrap();
    fs::remove_file(dir.join("src/b.rs")).unwrap();
    engine.tick(&prog, true).unwrap();

    let incremental = dump_call_rels(&engine);
    assert_eq!(
        incremental, baseline,
        "a file created and removed before the next tick must be a net no-op"
    );

    let oracle = oracle_dump(&dir, "add_delete_same_tick");
    assert_matches_oracle(
        "add_and_delete_the_same_file_in_one_tick",
        &incremental,
        &oracle,
    );

    let _ = fs::remove_dir_all(&dir);
}
