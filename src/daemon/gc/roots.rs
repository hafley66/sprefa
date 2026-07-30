//! Reachability roots beyond the caller-supplied `RelDecl` set: kinds 2 and 3
//! of the argument below, plus the chunked-insert helper that turns a root
//! list into `_gc_reachable` rows without tripping SQLite's compound-SELECT
//! ceiling. `sweep` (`../gc.rs`) owns kind 1 (interned rel columns from
//! decls) and calls into this module for the rest.
//!
//! ## Reachability argument (read before touching this file)
//!
//! A `_strings.id` is content-addressed (`StringId::of`, `src/spine.rs`): the
//! same text always hashes to the same id, everywhere, forever. A `_strings`
//! row is LIVE iff some value currently stored somewhere is that id. Three
//! kinds of place can store one:
//!
//!  1. An **interned rel column** — `Col::interned()` (`src/ast.rs`) is true
//!     for a textish, non-`raw` column; `Engine::declare` stores it as SQLite
//!     INTEGER holding `StringId::sqlite()`. Whether a given INTEGER column is
//!     one of these is NOT visible from the schema alone — a plain `int`
//!     column looks identical on disk, and (unlike case 2 below) `rel_*`
//!     tables do not declare this as a SQL foreign key. It is only knowable
//!     from the `.dl` program's own `RelDecl`s (`Col.ty`/`Col.raw`), so
//!     `sweep` gets its column list from `crate::prepare_paths` (user rels)
//!     `+` `crate::engine::all_builtin_decls()` (every built-in rel). Never a
//!     hand-maintained table list — the corpus's rel set is dynamic (users
//!     declare rels) — restricted to tables actually present in the db, so a
//!     stale decl for a since-dropped rel is a no-op.
//!  2. **Any column anywhere declaring `REFERENCES _strings(id)`** —
//!     discovered by `fk_roots` via `pragma_foreign_key_list` against EVERY
//!     table in the db, not a hardcoded set.
//!
//!     Incident: the first cut of this module had no such mechanism; its
//!     internal-table roots were a HAND-ENUMERATED list of three tables
//!     (`_where_bytes`, `_embeddings`, `_node_embeddings`). Six more internal
//!     tables — `_call_owner`, `_call_raw_site`, `_call_resolution`,
//!     `_call_edge_support`, `_call_def_bucket`, `_call_def`
//!     (`src/storage/call.rs`) — declare 26 columns that are `_strings.id`
//!     references and were never in that list. On the live sprefa root db,
//!     `_call_owner.fact_digest_sid` alone held 626 ids present in NO
//!     `rel_*` table and NO `_where_bytes` row; the first sweep classified
//!     all 626 as orphans. This is the exact failure the module's own
//!     stated rule ("an unprovable column is treated as live") was written
//!     to prevent — the rule was correct, the hand-enumeration implementing
//!     it was not. Every one of those 26 columns declares
//!     `REFERENCES _strings(id)` in its own `CREATE TABLE`: a
//!     machine-checkable fact already sitting in the schema, so the root set
//!     is now discovered from `pragma_foreign_key_list`, not curated by
//!     hand. This is what covers `_call_owner`/`_call_raw_site`/
//!     `_call_resolution`/`_call_edge_support`/`_call_def_bucket`/
//!     `_call_def` today and any FUTURE table that declares the same FK,
//!     with no code change here required.
//!  3. **A column that stores a `_strings.id` WITHOUT declaring the FK** —
//!     the only way such a column enters the root set is by name, hand-added
//!     in `undeclared_roots`, because nothing in the schema can surface it
//!     automatically. This is fragile in the same way the pre-incident hand
//!     list was, so it is kept as small as verification allows and every
//!     entry is justified. Confirmed by an actual query over a schema built
//!     from the verbatim `CREATE TABLE` text in `src/engine/meta.rs` and
//!     `src/storage/call.rs` (not by reading writer code and trusting
//!     memory):
//!
//!     ```sql
//!     SELECT m.name, p.name, p.type FROM sqlite_master m
//!     JOIN pragma_table_info(m.name) p
//!     WHERE m.type='table'
//!       AND (p.name LIKE '%sid' OR p.name LIKE '%_id' OR p.name = 'sym'
//!            OR p.name LIKE '%string%' OR p.name = 'node')
//!       AND NOT EXISTS (SELECT 1 FROM pragma_foreign_key_list(m.name) fk
//!                       WHERE fk."table" = '_strings' AND fk."from" = p.name);
//!     ```
//!
//!     Result against the full production schema (every `CREATE TABLE` in
//!     `meta.rs` + `call.rs`'s `SQLITE_CALL_DELTA_SCHEMA`):
//!
//!     ```text
//!     _call_owner|owner_id|INTEGER        -- _call_owner's own surrogate PK,
//!     _call_raw_site|owner_id|INTEGER     -- REFERENCES _call_owner/_call_raw_site
//!     _call_raw_site|site_id|INTEGER      -- (read: these DO declare an FK,
//!     _call_resolution|site_id|INTEGER    -- just not one targeting _strings)
//!     _embeddings|sid|TEXT                -- known undeclared, see below
//!     _node_embeddings|node|TEXT          -- known undeclared, see below
//!     _ref|oid|TEXT                       -- a git object id (sha hex), not a StringId
//!     _where_bytes|file_id|TEXT           -- a FileId (content hash), not a StringId
//!     _where_bytes|string_id|INTEGER      -- known undeclared, see below
//!     ```
//!
//!     The three genuine cases (`_where_bytes.string_id`, `_embeddings.sid`,
//!     `_node_embeddings.node`) are exactly the three `undeclared_roots`
//!     scans; nothing else in that result is a `_strings` reference under a
//!     different name. This name-pattern query is a heuristic cross-check,
//!     not a proof — a column that stores a `_strings.id` under a name
//!     matching none of `%sid`/`%_id`/`sym`/`%string%`/`node` would not
//!     surface here. That residual gap is the reason mechanism (2) — the FK
//!     declaration — is the PRIMARY discovery path and this hand list is
//!     explicitly the fallback of last resort, not the other way around.
//!
//! `_embeddings.sid` and `_node_embeddings.node` hold the decimal-text form
//! of a `StringId` (`CAST(s.id AS TEXT)`, `src/rels/embed.rs`);
//! `_node_embeddings.node` is documented as "a sym / file / whatever the edge
//! rel carries" and this module cannot prove every value is a `StringId`, so
//! per the standing rule (unprovable column -> treat as live) both are
//! scanned. `CAST(x AS INTEGER)` on a non-numeric string yields 0, which only
//! adds noise to protecting the already-always-kept sentinel id 0 — a
//! scanned root can only ADD entries to the reachable set, so being generous
//! here is safe by construction, never a source of over-deletion.
//!
//! Every other internal table this module's author read (`_file`, `_prov`,
//! `_reldigest`, `_program`, `_repo`, `_ref`, `_rev_log`, `_query_log`,
//! `pending_effect`, `_node_path`, `_files`, `_cold_node`, `_shapes`, ...)
//! stores plain TEXT (paths, hashes, JSON, digests, already-decoded template
//! args) that never round-trips through `_strings`; the heuristic query above
//! is the check that this belief is not just "I read the writers" — every
//! column it flagged has an explicit explanation in the table above, and none
//! of the flagged columns is an undeclared `_strings` reference beyond the
//! three already scanned. `src/engine/ownership.rs`'s `_*_v1` shadow tables
//! are `#[cfg(test)]`-only prototypes never created in a real db and are
//! excluded.

use anyhow::Result;

use crate::db::Db;

/// Root SELECT terms per `INSERT ... SELECT id FROM (... UNION ALL ...)`
/// statement. SQLite's `SQLITE_MAX_COMPOUND_SELECT` defaults to 500, but it
/// is a compile-time option a build can lower, and the real `instant` root
/// measured at 437 terms — already close enough that a modestly larger
/// corpus would tip it over. 100 leaves wide headroom rather than hugging
/// the ceiling.
pub(super) const CHUNK_SIZE: usize = 100;

/// Insert every root SELECT's ids into the caller's `_gc_reachable` temp
/// table, chunked to stay comfortably under `SQLITE_MAX_COMPOUND_SELECT`:
/// one `INSERT ... SELECT` per `CHUNK_SIZE`-term chunk, all within the
/// caller's transaction (`Db::transact` in `sweep`). This is purely a
/// statement-shape change — every root in `roots` lands in exactly one
/// chunk's `UNION ALL`, so chunking cannot narrow the reachable set. Still
/// set-based: a chunk is itself one multi-row `INSERT ... SELECT`, never a
/// per-row write.
///
/// Incident 2: the pre-chunking version joined every root into ONE
/// `UNION ALL` and ran it as a single `INSERT`. On the real sprefa and
/// smashy roots (more rels declared than `instant`, which happened to fit
/// at 437 terms) the statement was rejected outright with "too many terms
/// in compound SELECT" before it ran. `sweep` correctly REFUSED rather than
/// silently truncating the root set and deleting almost the whole
/// dictionary — the refusal is what made this a bug report instead of an
/// incident — but the collector was inoperable on the two real dbs whose
/// orphan counts motivated this module.
pub(super) fn insert_reachable_chunks(db: &Db, roots: &[String]) -> Result<()> {
    for chunk in roots.chunks(CHUNK_SIZE) {
        let union_sql = chunk.join(" UNION ALL ");
        db.exec_on(
            "_gc_reachable",
            &format!("INSERT OR IGNORE INTO _gc_reachable SELECT id FROM ({union_sql})"),
        )?;
    }
    Ok(())
}

/// Reachability kind 2: every (table, column) anywhere in `db` whose schema
/// declares `REFERENCES _strings(id)`, via `pragma_foreign_key_list` — the
/// structured form (preferred over regexing `sqlite_master.sql` text). Scans
/// EVERY table, not a hardcoded set: this is what covers `_call_owner` /
/// `_call_raw_site` / `_call_resolution` / `_call_edge_support` /
/// `_call_def_bucket` / `_call_def` today and any future table that declares
/// the same FK, with no code change here.
pub(super) fn fk_roots(db: &Db) -> Result<Vec<(String, String)>> {
    let all_tables: Vec<String> = db
        .schema_objects(&["%"])?
        .into_iter()
        .filter(|(_, kind)| kind == "table")
        .map(|(name, _)| name)
        .collect();
    let mut out = Vec::new();
    for table in all_tables {
        let cols: Vec<String> = db.query_rows(
            "pragma_foreign_key_list",
            "SELECT \"from\" FROM pragma_foreign_key_list(?1) WHERE \"table\" = '_strings'",
            &[table.as_str().into()],
            |row| Ok(row.get::<_, String>(0)?),
        )?;
        for col in cols {
            out.push((table.clone(), col));
        }
    }
    Ok(out)
}

/// Reachability kind 3: known cases that store a `_strings.id` WITHOUT
/// declaring the FK, so `fk_roots` cannot see them. See the module doc's
/// receipt query for how this list was verified rather than assumed. Each
/// entry is skipped (not an error) when the table/column is absent — a bare
/// fixture db in a test, or an older schema.
pub(super) fn undeclared_roots(db: &Db) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    if db.column_exists("_where_bytes", "string_id")? {
        out.push(("_where_bytes".to_string(), "string_id".to_string()));
    }
    if db.column_exists("_embeddings", "sid")? {
        out.push((
            "_embeddings".to_string(),
            "CAST(sid AS INTEGER)".to_string(),
        ));
    }
    if db.column_exists("_node_embeddings", "node")? {
        out.push((
            "_node_embeddings".to_string(),
            "CAST(node AS INTEGER)".to_string(),
        ));
    }
    Ok(out)
}
