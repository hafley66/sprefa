//! `dl daemon gc` — garbage collection for the `_strings` intern dictionary.
//!
//! Measured on the live sprefa root: 314,892 of 942,763 `_strings` rows (33.4%,
//! ~15MB) are orphaned — leftovers of a since-removed id-salting scheme. No
//! source in this repo ever collected them. This module is the collector.
//!
//! Reachability is the hard part; see `gc::roots` (`src/daemon/gc/roots.rs`)
//! for the full argument, including the incident where the first version of
//! this sweep hand-enumerated internal tables, missed six of them (26
//! `_strings.id`-referencing columns in `src/storage/call.rs`), and would
//! have deleted 626 live rows on the real sprefa root. The fix there:
//! discover roots from the schema itself (`pragma_foreign_key_list`), not a
//! curated list.
//!
//! **Compile-time literals do not create a fourth reachability kind.**
//! `src/lower.rs`'s `sym_lit` hashes a text literal at compile time for an
//! EQUALITY filter only (`{cell} = <hash>`, an int compare — grep `sym_lit(`
//! call sites, all `wheres.push`/`sub.push` on a `WHERE`/join condition); it
//! never decodes, so a literal comparison never needs the row to exist,
//! whether or not the comparison ever matches a live row. A literal WRITTEN
//! into an interned column instead lowers through `sprf_sym_intern`
//! (`head_term_sql`), which re-queues the text for the next `flush_syms` —
//! so any literal that lands in a live row keeps re-interning itself every
//! tick a rule with that head fires, independent of this sweep. A literal
//! that is neither compared nor written to an interned column lowers
//! through `lit_sql`/`term_sql` as an inline SQL string constant and never
//! touches `_strings` at all. There is no path by which a swept id can be
//! "resurrected" by a query expecting to decode it — every decode
//! (`sym_decode`, the `_txt` views, `engine/lens.rs`, `engine/derive.rs`)
//! reads an id FROM a stored cell this sweep already scanned as a root.
//!
//! ## Concurrency / timing
//!
//! The whole sweep — compute the reachable set, then delete — runs inside one
//! `Db::transact` (`BEGIN IMMEDIATE` under WAL). Mid-tick, a row can exist in
//! a rel table whose interned column carries an id `sprf_sym_intern` has
//! QUEUED but `flush_syms` has not yet flushed; the reachable-set scan reads
//! the rel table's actual committed data at snapshot time, so that id is
//! marked reachable regardless (there is nothing yet in `_strings` for the
//! sweep to wrongly delete). This module is a maintenance verb, never called
//! from a tick — see `run` below — so it is meant to be run with the daemon
//! quiescent, same convention as the standing VACUUM step in the task ledger.
//!
//! ## Statement shape: chunked inserts, not one unbounded UNION ALL
//!
//! Incident 2 (found by an end-to-end run against the real sprefa/smashy/
//! instant roots, after the FK fix above landed): the first version joined
//! every root into ONE `UNION ALL` and ran it as a single `INSERT`. SQLite
//! caps compound SELECT terms at `SQLITE_MAX_COMPOUND_SELECT` (default 500,
//! itself a compile-time option a build can lower). `instant` happened to
//! fit at 437 terms; `sprefa` and `smashy` declare more rels, exceeded 500,
//! and the statement was rejected with "too many terms in compound SELECT"
//! before it ran — the collector was inoperable on the two real dbs whose
//! orphan counts motivated this module, even though `sweep` correctly
//! REFUSED rather than silently truncating the root set (a truncated set
//! would have marked most live strings unreachable and deleted almost the
//! whole dictionary under `--apply`). `gc::roots::insert_reachable_chunks`
//! now issues one `INSERT ... SELECT` per 100-term chunk, comfortably under
//! the ceiling rather than up against it, all inside the same transaction —
//! every root is still scanned; only the statement shape changes.

mod roots;

use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::ast::{Item, RelDecl};
use crate::db::Db;

/// Result of one sweep pass, dry-run or applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SweepReport {
    /// Interned `RelDecl` (table, column) pairs scanned (reachability kind 1).
    pub decl_columns_scanned: usize,
    /// Columns discovered via `pragma_foreign_key_list` declaring
    /// `REFERENCES _strings(id)`, anywhere in the db (reachability kind 2).
    pub fk_columns_scanned: usize,
    /// Known-undeclared roots scanned (reachability kind 3): up to
    /// `_where_bytes.string_id`, `_embeddings.sid`, `_node_embeddings.node`.
    pub undeclared_columns_scanned: usize,
    /// `_strings` rows before the sweep, excluding the id=0 sentinel (never a
    /// candidate: `StringId::EMPTY`, always present, never orphaned).
    pub strings_total: i64,
    /// Rows unreferenced by any scanned root — the sweep's target set.
    pub orphans: i64,
    /// `SUM(length(content))` over the orphan rows: a lower-bound byte
    /// estimate (no row/page/index overhead), so the real reclaim after a
    /// separate VACUUM is larger than this number.
    pub orphan_bytes: i64,
    /// Whether `_strings` was actually modified. `false` = dry run: every
    /// number above was computed but nothing was deleted.
    pub applied: bool,
}

/// `dl daemon gc [--root PATH] [--apply] [--dry-run]`. Dry run (report only)
/// is the default; `--apply` is required to actually delete. Iterates every
/// registered root like `dl daemon health`, same file-trail-only posture
/// otherwise — but unlike `health`, this verb WRITES when `--apply` is given,
/// so it takes a real write connection (`crate::db::open`, not
/// `open_read_only`) and is expected to contend with (or be run instead of) a
/// live daemon, exactly like the standing VACUUM step it complements.
pub fn run(args: &[String]) -> Result<i32> {
    let apply = args.iter().any(|a| a == "--apply");
    let only_root = flag_value(args, "--root").map(PathBuf::from);
    let home = crate::daemon::daemon_home();
    let records = crate::daemon::read_roots_json();

    println!("daemon home: {}", home.display());
    println!(
        "mode: {}",
        if apply {
            "APPLY (deleting orphans)"
        } else {
            "DRY RUN (--apply to delete)"
        }
    );

    let mut matched = false;
    for rec in &records {
        if let Some(only) = &only_root {
            let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
            if canon(only) != canon(&rec.root) {
                continue;
            }
        }
        matched = true;
        let db_path = home.join("roots").join(&rec.key).join("db.sqlite");
        if !db_path.is_file() {
            println!("\n== {} ({}) ==\n  no db file", rec.key, rec.root.display());
            continue;
        }
        println!("\n== {} ({}) ==", rec.key, rec.root.display());
        match run_for_root(&rec.root, &db_path, apply) {
            Ok(report) => print_report(&report),
            Err(err) => {
                // A reachability computation that fails is refused, never
                // silently degraded — the health report's "best effort,
                // silence on failure" posture is wrong here: proceeding on a
                // partial rel-decl picture is exactly how this sweeps a
                // column it did not know was interned.
                println!("  REFUSED: {err:#}");
            }
        }
    }
    if !matched {
        println!("no matching registered root");
        return Ok(1);
    }
    Ok(0)
}

fn print_report(report: &SweepReport) {
    println!(
        "  scanned {} decl column(s) + {} FK-declared column(s) + {} known-undeclared root(s)",
        report.decl_columns_scanned, report.fk_columns_scanned, report.undeclared_columns_scanned
    );
    println!(
        "  _strings: {} total, {} orphaned (~{:.1}MB by content bytes alone)",
        report.strings_total,
        report.orphans,
        report.orphan_bytes as f64 / 1e6
    );
    if report.applied {
        println!(
            "  deleted {} row(s) — run VACUUM separately to reclaim pages",
            report.orphans
        );
    } else {
        println!("  (dry run: nothing deleted; re-run with --apply)");
    }
}

/// Open the root's declared program plus the built-in catalog, open its db
/// read-write, and run the sweep. The program resolve/parse step MUST
/// succeed: a partial or failed program read means an incomplete `RelDecl`
/// set, and an incomplete set is exactly what turns this sweep into data
/// loss (a live interned column this run never learned about). Failure here
/// is a refusal, not a degraded-but-proceeding report.
fn run_for_root(root: &Path, db_path: &Path, apply: bool) -> Result<SweepReport> {
    crate::daemon::apply_process_budget();
    let files = crate::resolve_programs(&[], root)
        .with_context(|| format!("resolving the program set for {}", root.display()))?;
    let (prog, _diags, _display) = crate::prepare_paths(&files)
        .with_context(|| format!("parsing the program set for {}", root.display()))?;
    let mut decls: Vec<RelDecl> = prog
        .items
        .into_iter()
        .filter_map(|item| match item {
            Item::Rel(decl) => Some(decl),
            _ => None,
        })
        .collect();
    decls.extend(crate::engine::all_builtin_decls());

    let db = crate::db::open(Some(&db_path.to_string_lossy()))
        .with_context(|| format!("opening {}", db_path.display()))?;
    sweep(&db, &decls, apply)
}

/// The testable core: given an already-open db and a caller-supplied `RelDecl`
/// set (reachability kind 1), compute the orphan set and, if `apply`, delete
/// it — all inside one transaction. Reachability kinds 2 (`roots::fk_roots`)
/// and 3 (`roots::undeclared_roots`) are always discovered fresh from `db`
/// itself, never from the caller. Never called on a hot path; this is only
/// ever `run_for_root`'s callee or a test's direct callee.
pub fn sweep(db: &Db, decls: &[RelDecl], apply: bool) -> Result<SweepReport> {
    let existing_rel_tables: BTreeSet<String> = db
        .schema_objects(&["rel_%"])?
        .into_iter()
        .filter(|(_, kind)| kind == "table")
        .map(|(name, _)| name)
        .collect();

    let mut root_selects: Vec<String> = Vec::new();
    let mut decl_columns_scanned = 0usize;
    let mut seen_tables: BTreeSet<String> = BTreeSet::new();
    for decl in decls {
        let table = crate::lower::tbl(&decl.name);
        if !existing_rel_tables.contains(&table) {
            continue;
        }
        if !seen_tables.insert(table.clone()) {
            continue;
        }
        for col in &decl.cols {
            if col.interned() {
                // Storage normalization (2026-07-21): an interned cell now stores
                // a dense `_sym_dict` surrogate, NOT the `_strings` hash it
                // decodes to, so resolve dense -> `sym_hash` to reach the real
                // `_strings.id`. `COALESCE(..., cell)` keeps a raw-hash cell
                // (a coord id in a `text` column, or the rare in-range value)
                // reachable by passing through. This follows only LIVE references
                // — a string with no live cell stays collectable, so the dict
                // does not pin every string it ever saw.
                root_selects.push(format!(
                    "SELECT COALESCE((SELECT sym_hash FROM _sym_dict WHERE _sym_dict.id = t.\"{col}\"), t.\"{col}\") AS id \
                     FROM {table} t WHERE t.\"{col}\" IS NOT NULL",
                    col = col.name
                ));
                decl_columns_scanned += 1;
            }
        }
    }

    let fk_columns = roots::fk_roots(db)?;
    // Requirement: a discovery mechanism that silently finds nothing is how
    // this class of bug ships a second time. A real db always carries at
    // least the `_strings` self-reference-free schema this project ships
    // (the `_call_*` family declares 26 such columns) — zero here means the
    // discovery query itself broke, not that reachability is legitimately
    // empty, so this refuses independently of whether `root_selects` overall
    // is non-empty via decl columns.
    anyhow::ensure!(
        !fk_columns.is_empty(),
        "schema-derived FK discovery (pragma_foreign_key_list) found zero columns \
         referencing _strings anywhere in this db — refusing: a discovery mechanism \
         that silently finds nothing is exactly how this class of bug ships"
    );
    let fk_columns_scanned = fk_columns.len();
    for (table, col) in &fk_columns {
        root_selects.push(format!(
            "SELECT \"{col}\" AS id FROM \"{table}\" WHERE \"{col}\" IS NOT NULL"
        ));
    }

    let undeclared = roots::undeclared_roots(db)?;
    let undeclared_columns_scanned = undeclared.len();
    for (table, expr) in &undeclared {
        root_selects.push(format!("SELECT {expr} AS id FROM {table}"));
    }

    anyhow::ensure!(
        !root_selects.is_empty(),
        "no interned column or FK root found — refusing to sweep with an empty reachability set"
    );

    db.transact(|| {
        db.execute_batch_on("_gc_reachable",
            "CREATE TEMP TABLE _gc_reachable (id INTEGER PRIMARY KEY)")?;
        // Chunked, not one unbounded UNION ALL — see the module doc's
        // "Statement shape" section. Still set-based: each chunk is one
        // multi-row INSERT ... SELECT, never a per-row write.
        roots::insert_reachable_chunks(db, &root_selects)?;

        let (strings_total, orphans, orphan_bytes): (i64, i64, i64) = db.query_one(
            "_strings",
            "SELECT \
               (SELECT count(*) FROM _strings WHERE id != 0), \
               (SELECT count(*) FROM _strings WHERE id != 0 AND id NOT IN (SELECT id FROM _gc_reachable)), \
               (SELECT coalesce(sum(length(content)), 0) FROM _strings \
                  WHERE id != 0 AND id NOT IN (SELECT id FROM _gc_reachable))",
            &[],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;

        if apply && orphans > 0 {
            let deleted = db.exec_on("_strings",
                "DELETE FROM _strings WHERE id != 0 AND id NOT IN (SELECT id FROM _gc_reachable)")?;
            anyhow::ensure!(deleted as i64 == orphans,
                "sweep deleted {deleted} rows but the pre-count said {orphans} — refusing to trust this run");
        }
        db.execute_batch_on("_gc_reachable", "DROP TABLE _gc_reachable")?;

        Ok(SweepReport {
            decl_columns_scanned, fk_columns_scanned, undeclared_columns_scanned,
            strings_total, orphans, orphan_bytes, applied: apply,
        })
    })
}

/// Value following `name` in `args` (e.g. `--root /path`).
fn flag_value<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}
