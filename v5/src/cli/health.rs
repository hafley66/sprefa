//! `dl daemon health` — the storage-health report for every per-root db under
//! the daemon home. Read-only (SQLITE_OPEN_READONLY via `db::open_read_only`),
//! no socket, no lock: it answers while the daemon is live, wedged, or down,
//! the same file-trail idiom as `daemon why`/`daemon invocations`.
//!
//! Sections, in order (the 2026-07-19 autopsy queries, made repeatable):
//!   1. roots overview — every `<home>/roots/<key>` dir vs `roots.json`;
//!      unregistered dirs are ORPHANS (failure-modes class 14: agent-worktree
//!      pre-commit hooks cold-building ~600MB dbs) with a best-effort origin
//!      probe from the db's own `_repo`/`_program` rows.
//!   2. per-db buckets — one dbstat pass grouped into rel tables / internal
//!      tables / auto-demand `idx_` / engine `_*_idx` / pk autoindexes.
//!   3. per-rel ranking — table bytes + own-index bytes (sqlite_master
//!      tbl_name map) + row count, heaviest first.
//!   4. dupe probe — same-rowcount rel pairs proven identical (or not) by
//!      `EXCEPT` in both directions.
//!   5. copy rules — static rename-layer scan of the root's program set:
//!      single-atom all-var rules `a(x,y) <- b(x,y).` re-store an existing
//!      rel's rowset under a second name.
//!   6. db/corpus ratio — the class-17 verdict, inline.

use anyhow::Result;
use rusqlite::Connection;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::ast::{BodyItem, Item, Rule, Term};

/// Rel tables below this many rows are skipped by the dupe probe: tiny rels
/// pair up by coincidence and the finding would not move the storage needle.
const DUPE_MIN_ROWS: i64 = 500;
/// Cap on EXCEPT comparisons per equal-(rows, ncols) group, so a schema with
/// many same-shaped rels cannot go quadratic.
const DUPE_MAX_PAIRS_PER_GROUP: usize = 10;

pub fn run(args: &[String]) -> Result<i32> {
    let top: usize = flag_value(args, "--top")
        .and_then(|s| s.parse().ok())
        .unwrap_or(15);
    let only_root = flag_value(args, "--root").map(PathBuf::from);
    let skip_dupes = args.iter().any(|a| a == "--no-dupes");
    let home = crate::daemon::daemon_home();
    let records = crate::daemon::read_roots_json();

    println!("daemon home: {}", home.display());
    let orphans = report_roots_overview(&home, &records)?;
    for rec in &records {
        if let Some(only) = &only_root {
            let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
            if canon(only) != canon(&rec.root) {
                continue;
            }
        }
        let db_path = home.join("roots").join(&rec.key).join("db.sqlite");
        if !db_path.is_file() {
            println!("\n== {} ({}) ==\n  no db file", rec.key, rec.root.display());
            continue;
        }
        report_db(&rec.key, &rec.root, &db_path, top, skip_dupes)?;
    }
    if !orphans.is_empty() {
        let total_mb: f64 = orphans.iter().map(|(_, b)| *b as f64 / 1e6).sum();
        println!(
            "\norphan cleanup ({} dir(s), {:.0}MB):",
            orphans.len(),
            total_mb
        );
        println!(
            "  cd {} && rm -rf {}",
            home.join("roots").display(),
            orphans
                .iter()
                .map(|(k, _)| k.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        );
    }
    Ok(0)
}

/// Section 1: every dir under `<home>/roots` against the registered set.
/// Returns the orphan (key, bytes) list for the closing cleanup hint.
fn report_roots_overview(
    home: &Path,
    records: &[crate::daemon::RootRecord],
) -> Result<Vec<(String, u64)>> {
    let by_key: BTreeMap<&str, &crate::daemon::RootRecord> =
        records.iter().map(|r| (r.key.as_str(), r)).collect();
    let roots_dir = home.join("roots");
    let mut orphans: Vec<(String, u64)> = Vec::new();
    println!("\nKEY\tDB_MB\tAGE\tSTATUS");
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&roots_dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    for dir in dirs {
        let key = dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let bytes = dir_bytes(&dir);
        let age = dir_mtime_age(&dir);
        match by_key.get(key.as_str()) {
            Some(rec) => println!(
                "{key}\t{:.1}\t{age}\tregistered {}",
                bytes as f64 / 1e6,
                rec.root.display()
            ),
            None => {
                let origin = orphan_origin(&dir.join("db.sqlite"));
                println!("{key}\t{:.1}\t{age}\tORPHAN{origin}", bytes as f64 / 1e6);
                orphans.push((key, bytes));
            }
        }
    }
    // A registered root whose dir is gone is worth a line too (cold on next tick).
    for rec in records {
        if !roots_dir.join(&rec.key).is_dir() {
            println!(
                "{}\t-\t-\tregistered {} (no dir yet)",
                rec.key,
                rec.root.display()
            );
        }
    }
    Ok(orphans)
}

/// Best-effort origin of an unregistered db: the corpus root its own `_repo`
/// row recorded, else the first loaded program path. Empty string if the db
/// is unreadable or carries neither.
fn orphan_origin(db_path: &Path) -> String {
    let Ok(conn) = crate::db::open_read_only(&db_path.to_string_lossy()) else {
        return String::new();
    };
    let probe = |sql: &str| -> Option<String> {
        conn.query_row(sql, [], |r| r.get::<_, String>(0))
            .ok()
            .filter(|s| !s.is_empty())
    };
    probe("SELECT root FROM _repo WHERE root != '' LIMIT 1")
        .or_else(|| probe("SELECT path FROM _program LIMIT 1"))
        .map(|s| format!(" (from {s})"))
        .unwrap_or_default()
}

/// Sections 2-6 for one registered root's db.
fn report_db(key: &str, root: &Path, db_path: &Path, top: usize, skip_dupes: bool) -> Result<()> {
    let conn = crate::db::open_read_only(&db_path.to_string_lossy())?;
    // One read transaction for the whole report: under WAL each autocommit
    // statement would otherwise get its own snapshot, and a live daemon tick
    // between them makes rows and dbstat bytes disagree (seen: a mid-rebuild
    // rel reporting 0 rows against 50MB of pages).
    conn.execute_batch("BEGIN")?;
    let file_bytes = std::fs::metadata(db_path).map(|m| m.len()).unwrap_or(0);
    println!("\n== {key} ({}) ==", root.display());

    let pragma = |name: &str| -> i64 {
        conn.query_row(&format!("PRAGMA {name}"), [], |r| r.get(0))
            .unwrap_or(0)
    };
    let (page_size, page_count, freelist) = (
        pragma("page_size"),
        pragma("page_count"),
        pragma("freelist_count"),
    );
    println!("file {:.1}MB  pages {page_count}x{page_size}  freelist {freelist} ({:.1}MB reclaimable by VACUUM)",
        file_bytes as f64 / 1e6, (freelist * page_size) as f64 / 1e6);

    // One dbstat pass: bytes per object, joined to sqlite_master for its kind
    // and owning table. Autoindexes carry NULL sql but do appear in both.
    let mut objects: Vec<(String, String, String, i64)> = Vec::new(); // (name, type, tbl_name, bytes)
    {
        let mut stmt = conn.prepare(
            "SELECT s.name, m.type, m.tbl_name, s.bytes FROM \
               (SELECT name, sum(pgsize) AS bytes FROM dbstat GROUP BY name) s \
             LEFT JOIN sqlite_master m ON m.name = s.name",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                r.get::<_, i64>(3)?,
            ))
        })?;
        for row in rows {
            objects.push(row?);
        }
    }

    // Section 2: buckets.
    let mut buckets: BTreeMap<&str, (i64, usize)> = BTreeMap::new();
    for (name, kind, _tbl, bytes) in &objects {
        let bucket = match (kind.as_str(), name) {
            ("table", n) if n.starts_with("rel_") => "rel tables",
            ("table", n) if n.starts_with('_') => "internal tables",
            ("table", _) => "other tables",
            ("index", n) if n.starts_with("idx_") => "auto-demand idx_",
            ("index", n) if n.starts_with("sqlite_autoindex_") => "pk autoindexes",
            ("index", _) => "engine indexes",
            _ => "other",
        };
        let e = buckets.entry(bucket).or_default();
        e.0 += bytes;
        e.1 += 1;
    }
    let mut bucket_rows: Vec<(&str, i64, usize)> =
        buckets.into_iter().map(|(k, (b, n))| (k, b, n)).collect();
    bucket_rows.sort_by_key(|(_, b, _)| -b);
    println!("\nBUCKET\tMB\tOBJECTS");
    for (bucket, bytes, n) in &bucket_rows {
        println!("{bucket}\t{:.1}\t{n}", *bytes as f64 / 1e6);
    }

    // Section 3: per-table ranking, own indexes attached via tbl_name.
    let mut table_bytes: BTreeMap<&str, (i64, i64)> = BTreeMap::new(); // tbl -> (data, idx)
    for (name, kind, tbl, bytes) in &objects {
        match kind.as_str() {
            "table" => table_bytes.entry(name).or_default().0 += bytes,
            "index" => table_bytes.entry(tbl).or_default().1 += bytes,
            _ => {}
        }
    }
    let mut ranked: Vec<(&str, i64, i64)> = table_bytes
        .into_iter()
        .map(|(t, (d, i))| (t, d, i))
        .collect();
    ranked.sort_by_key(|(_, d, i)| -(d + i));
    println!("\nTABLE\tROWS\tDATA_MB\tIDX_MB\tTOTAL_MB");
    for (tbl, data, idx) in ranked.iter().take(top) {
        let rows = count_rows(&conn, tbl).unwrap_or(-1);
        println!(
            "{tbl}\t{rows}\t{:.1}\t{:.1}\t{:.1}",
            *data as f64 / 1e6,
            *idx as f64 / 1e6,
            (*data + *idx) as f64 / 1e6
        );
    }

    // Sections 4+5 want the program's rel names; parse failures degrade to
    // db-only reporting rather than killing the report.
    if !skip_dupes {
        report_dupes(&conn, &ranked)?;
    }
    report_copy_rules(root);

    conn.execute_batch("COMMIT")?;

    // Section 6: the class-17 ratio verdict, inline (same walk as db_ratio).
    let corpus = crate::db_ratio::corpus_bytes(root);
    if corpus > 0 {
        println!(
            "\ndb/corpus ratio: {:.1}x  (db {:.1}MB / corpus {:.1}MB)",
            file_bytes as f64 / corpus as f64,
            file_bytes as f64 / 1e6,
            corpus as f64 / 1e6
        );
    }
    Ok(())
}

/// Section 4: rel tables sharing an exact (rowcount, column-list-length) pair
/// get an `EXCEPT` in both directions; 0 rows both ways = identical rowsets
/// (the port_of_reach == port_of_reach_rec finding, made standing).
fn report_dupes(conn: &Connection, ranked: &[(&str, i64, i64)]) -> Result<()> {
    let mut by_count: BTreeMap<i64, Vec<&str>> = BTreeMap::new();
    for (tbl, _, _) in ranked {
        if !tbl.starts_with("rel_") {
            continue;
        }
        let Ok(rows) = count_rows(conn, tbl) else {
            continue;
        };
        if rows >= DUPE_MIN_ROWS {
            by_count.entry(rows).or_default().push(tbl);
        }
    }
    let mut printed_header = false;
    for (rows, tables) in by_count.iter().filter(|(_, t)| t.len() > 1) {
        let mut pairs = 0usize;
        for i in 0..tables.len() {
            for j in (i + 1)..tables.len() {
                if pairs >= DUPE_MAX_PAIRS_PER_GROUP {
                    break;
                }
                let (a, b) = (tables[i], tables[j]);
                let (cols_a, cols_b) = (data_columns(conn, a)?, data_columns(conn, b)?);
                if cols_a.len() != cols_b.len() {
                    continue;
                }
                pairs += 1;
                let diff = |x: &str, xc: &[String], y: &str, yc: &[String]| -> bool {
                    let sql = format!(
                        "SELECT EXISTS(SELECT 1 FROM (SELECT {} FROM {x} EXCEPT SELECT {} FROM {y}))",
                        xc.join(", "), yc.join(", "));
                    conn.query_row(&sql, [], |r| r.get::<_, bool>(0))
                        .unwrap_or(true)
                };
                let identical = !diff(a, &cols_a, b, &cols_b) && !diff(b, &cols_b, a, &cols_a);
                if !printed_header {
                    println!("\nDUPE PROBE (rel pairs with equal rowcount)");
                    printed_header = true;
                }
                if identical {
                    println!("{a} == {b}  ({rows} rows, identical rowsets — a rename/copy layer)");
                } else {
                    println!("{a} vs {b}  ({rows} rows, differ)");
                }
            }
        }
    }
    Ok(())
}

/// Column names of `tbl` minus the universal `__src` padding column, in
/// declared order — the positional shape `EXCEPT` compares.
fn data_columns(conn: &Connection, tbl: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({tbl})"))?;
    let cols = stmt
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(|c| c.ok())
        .filter(|c| c != "__src")
        .collect();
    Ok(cols)
}

/// Section 5: static rename-layer scan over the root's discovery program set.
/// Best-effort: a root with no `.dl/` chain, or a program that fails to
/// parse, degrades to silence (the db sections already printed).
fn report_copy_rules(root: &Path) {
    let Ok(files) = crate::resolve_programs(&[], root) else {
        return;
    };
    let Ok((prog, _diags, _display)) = crate::prepare_paths(&files) else {
        return;
    };
    // Dedup: `use` splicing can merge the same file's items into the flat
    // program more than once; one finding per (head, body, origin).
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut printed_header = false;
    for item in &prog.items {
        let Item::Rule(rule) = item else { continue };
        let Some((head, body)) = copy_shape(rule) else {
            continue;
        };
        let origin = rule
            .origin
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| "<inline>".to_string());
        if !seen.insert(format!("{head}\t{body}\t{origin}")) {
            continue;
        }
        if !printed_header {
            println!("\nCOPY RULES (single-atom all-var rules re-storing an existing rowset)");
            printed_header = true;
        }
        println!("{head} <- {body}  ({origin})");
    }
}

/// `Some((head_rel, body_rel))` iff `rule` is a pure copy layer: one positive
/// body atom, no aggregation/temporal modifier, every term a variable, head
/// and body binding the same distinct variable set. Such a head's rowset is a
/// (possibly column-permuted) duplicate of the body rel.
fn copy_shape(rule: &Rule) -> Option<(&str, &str)> {
    if rule.temporal.is_some() || rule.has_agg() {
        return None;
    }
    let [BodyItem::Pos(atom)] = rule.body.as_slice() else {
        return None;
    };
    if atom.terms.len() != rule.head.terms.len() {
        return None;
    }
    let head_vars = all_vars(&rule.head.terms)?;
    let body_vars = all_vars(&atom.terms)?;
    let distinct = |v: &[&str]| {
        let set: std::collections::BTreeSet<&&str> = v.iter().collect();
        set.len() == v.len()
    };
    if !distinct(&head_vars) || !distinct(&body_vars) {
        return None;
    }
    let same_set = {
        let (mut h, mut b) = (head_vars.clone(), body_vars.clone());
        h.sort_unstable();
        b.sort_unstable();
        h == b
    };
    if same_set {
        Some((rule.head.rel.as_str(), atom.rel.as_str()))
    } else {
        None
    }
}

/// All terms as variable names, or `None` if any term is not a plain var.
fn all_vars(terms: &[Term]) -> Option<Vec<&str>> {
    terms
        .iter()
        .map(|t| match t {
            Term::Var(v) => Some(v.as_str()),
            _ => None,
        })
        .collect()
}

fn count_rows(conn: &Connection, tbl: &str) -> Result<i64> {
    Ok(conn.query_row(&format!("SELECT count(*) FROM {tbl}"), [], |r| r.get(0))?)
}

/// Recursive byte sum of a directory (a root dir holds db.sqlite + -wal/-shm).
fn dir_bytes(dir: &Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if let Ok(m) = entry.metadata() {
                total += m.len();
            }
        }
    }
    total
}

/// Human age of a dir's mtime: "3h", "2d", "41m".
fn dir_mtime_age(dir: &Path) -> String {
    let Some(mtime) = std::fs::metadata(dir).ok().and_then(|m| m.modified().ok()) else {
        return "-".to_string();
    };
    let secs = SystemTime::now()
        .duration_since(mtime)
        .map(|d| d.as_secs())
        .or_else(|_| mtime.duration_since(UNIX_EPOCH).map(|_| 0))
        .unwrap_or(0);
    match secs {
        s if s < 3600 => format!("{}m", s / 60),
        s if s < 172_800 => format!("{}h", s / 3600),
        s => format!("{}d", s / 86_400),
    }
}

/// Value following `name` in `args` (e.g. `--top 30`).
fn flag_value<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_rules(src: &str) -> Vec<Rule> {
        let prog = crate::parse::parse(crate::lex::lex(src).unwrap()).unwrap();
        prog.items
            .into_iter()
            .filter_map(|i| match i {
                Item::Rule(r) => Some(r),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn copy_shape_flags_pure_renames_and_permutations_only() {
        let rules = parse_rules(
            "rel base(path: str, line: int).\n\
             copy(path, line) <- base(path, line).\n\
             swapped(line, path) <- base(path, line).\n\
             projected(path) <- base(path, _).\n\
             constant(path, line) <- base(path, line), line > 3.\n\
             joined(path, line) <- base(path, line), base(path, line).\n",
        );
        let found: Vec<Option<(&str, &str)>> = rules.iter().map(copy_shape).collect();
        assert_eq!(found[0], Some(("copy", "base")));
        assert_eq!(
            found[1],
            Some(("swapped", "base")),
            "a permutation still re-stores the rowset"
        );
        assert_eq!(found[2], None, "a projection drops a column, not a copy");
        assert_eq!(found[3], None, "a constraint filters, not a copy");
        assert_eq!(found[4], None, "two atoms = a join, not a copy");
    }
}
