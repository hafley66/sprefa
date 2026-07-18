//! Test-only positive-acyclic Boolean incremental-view fixture.
//!
//! This module deliberately uses a private in-memory SQLite schema. It does not
//! participate in production execution; it proves candidate revalidation and
//! public-presence transitions before the runtime is wired to a typed plan.

use crate::db::{self, Db, SqlVal};
use anyhow::Result;
use std::collections::BTreeSet;

/// SQLite's default compiled bound-parameter ceiling is 999; stay under it so a
/// batched write stays a single statement across realistic touched-candidate
/// sets, and chunks (never a per-row write) beyond it.
const MAX_BOUND_PARAMS: usize = 800;

/// Representative shapes recorded in `executed_mutations` for the derived-rel
/// deletes. They keep the scoped `WHERE` so `has_whole_derived_delete` still
/// distinguishes a targeted delete from a whole-table wipe after batching.
const DELETE_PROJECTED_SCOPED: &str = "DELETE FROM projected WHERE sym IN (VALUES ...)";
const DELETE_JOINED_SCOPED: &str = "DELETE FROM joined WHERE (sym,payload) IN (VALUES ...)";
const DELETE_OUT_SCOPED: &str = "DELETE FROM out WHERE sym IN (VALUES ...)";

#[derive(Clone, Debug)]
pub(crate) enum InputChange {
    Src {
        repo: String,
        sym: String,
        kind: String,
        diff: i64,
    },
    Stable {
        sym: String,
        payload: String,
        diff: i64,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DeltaMetrics {
    pub(crate) delta_input_rows: usize,
    pub(crate) candidate_rows: usize,
    pub(crate) public_plus: usize,
    pub(crate) public_minus: usize,
    pub(crate) rules_run: usize,
    pub(crate) peak_queued_rows: usize,
    pub(crate) peak_queued_estimated_bytes: usize,
    /// Number of INSERT/DELETE statements this generation actually executed.
    /// Batched: a handful of chunked writes, NOT one per changed row. The N+1
    /// guard test reads this to prove the per-row write path stayed gone.
    pub(crate) write_statements: usize,
}

impl DeltaMetrics {
    fn observe_queue(&mut self, rows: usize, estimated_bytes: usize) {
        self.peak_queued_rows = self.peak_queued_rows.max(rows);
        self.peak_queued_estimated_bytes = self.peak_queued_estimated_bytes.max(estimated_bytes);
    }

    fn transition(&mut self, present: bool) {
        if present {
            self.public_plus += 1;
        } else {
            self.public_minus += 1;
        }
    }
}

pub(crate) struct BooleanDeltaFixture {
    db: Db,
    generation: u64,
    executed_mutations: Vec<&'static str>,
    write_statements: usize,
}

impl BooleanDeltaFixture {
    pub(crate) fn new() -> Result<Self> {
        let db = db::open(None)?;
        db.execute_batch_on(
            "src",
            "
            PRAGMA foreign_keys=ON;
            PRAGMA temp_store=FILE;
            CREATE TABLE src(
                repo TEXT NOT NULL,
                sym TEXT NOT NULL,
                kind TEXT NOT NULL,
                PRIMARY KEY(repo,sym,kind)
            ) WITHOUT ROWID;
            CREATE INDEX src_by_sym_kind ON src(sym,kind);
            CREATE TABLE stable(
                sym TEXT NOT NULL,
                payload TEXT NOT NULL,
                PRIMARY KEY(sym,payload)
            ) WITHOUT ROWID;
            CREATE TABLE projected(sym TEXT PRIMARY KEY) WITHOUT ROWID;
            CREATE TABLE joined(
                sym TEXT NOT NULL,
                payload TEXT NOT NULL,
                PRIMARY KEY(sym,payload)
            ) WITHOUT ROWID;
            CREATE TABLE out(sym TEXT PRIMARY KEY) WITHOUT ROWID;
        ",
        )?;
        Ok(Self {
            db,
            generation: 0,
            executed_mutations: Vec::new(),
            write_statements: 0,
        })
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn apply_generation(&mut self, changes: &[InputChange]) -> Result<DeltaMetrics> {
        let mut metrics = DeltaMetrics::default();
        let mut changed_src_syms = BTreeSet::new();
        let mut changed_stable_syms = BTreeSet::new();
        self.write_statements = 0;
        self.db.begin_immediate()?;

        // --- input deltas ---------------------------------------------------
        // Group the incoming deltas into per-(rel, direction) sets and flush
        // each set as ONE batched write instead of a write per change. The
        // membership probe against the pre-batch db state reproduces the old
        // per-change `changes()` accounting for the distinct deltas the
        // InputChange stream models (BTreeSet dedups a repeated change; a
        // same-row +/- pair inside one generation is outside that contract).
        // BTreeSet iteration order keeps the batched rows deterministic.
        let mut src_add: BTreeSet<(String, String, String)> = BTreeSet::new();
        let mut src_del: BTreeSet<(String, String, String)> = BTreeSet::new();
        let mut stable_add: BTreeSet<(String, String)> = BTreeSet::new();
        let mut stable_del: BTreeSet<(String, String)> = BTreeSet::new();
        for change in changes {
            match change {
                InputChange::Src {
                    repo,
                    sym,
                    kind,
                    diff: 1,
                } => {
                    if !self.src_present(repo, sym, kind)?
                        && src_add.insert((repo.clone(), sym.clone(), kind.clone()))
                    {
                        metrics.delta_input_rows += 1;
                        changed_src_syms.insert(sym.clone());
                    }
                }
                InputChange::Src {
                    repo,
                    sym,
                    kind,
                    diff: -1,
                } => {
                    if self.src_present(repo, sym, kind)?
                        && src_del.insert((repo.clone(), sym.clone(), kind.clone()))
                    {
                        metrics.delta_input_rows += 1;
                        changed_src_syms.insert(sym.clone());
                    }
                }
                InputChange::Stable {
                    sym,
                    payload,
                    diff: 1,
                } => {
                    if !self.stable_present(sym, payload)?
                        && stable_add.insert((sym.clone(), payload.clone()))
                    {
                        metrics.delta_input_rows += 1;
                        changed_stable_syms.insert(sym.clone());
                    }
                }
                InputChange::Stable {
                    sym,
                    payload,
                    diff: -1,
                } => {
                    if self.stable_present(sym, payload)?
                        && stable_del.insert((sym.clone(), payload.clone()))
                    {
                        metrics.delta_input_rows += 1;
                        changed_stable_syms.insert(sym.clone());
                    }
                }
                _ => anyhow::bail!("InputChange diff must be +1 or -1"),
            }
        }
        let src_add_rows = triples(&src_add);
        let src_del_rows = triples(&src_del);
        let stable_add_rows = doubles(&stable_add);
        let stable_del_rows = doubles(&stable_del);
        self.batch_insert("src", "INSERT OR IGNORE INTO src(repo,sym,kind)", 3, &src_add_rows)?;
        self.batch_delete("src", &["repo", "sym", "kind"], &src_del_rows)?;
        self.batch_insert("stable", "INSERT OR IGNORE INTO stable(sym,payload)", 2, &stable_add_rows)?;
        self.batch_delete("stable", &["sym", "payload"], &stable_del_rows)?;

        // --- projected (source-driven presence) -----------------------------
        let projected_candidates: Vec<String> = changed_src_syms.into_iter().collect();
        metrics.candidate_rows += projected_candidates.len();
        metrics.observe_queue(
            projected_candidates.len(),
            string_bytes(&projected_candidates),
        );
        let mut projected_transitions = BTreeSet::new();
        let mut projected_insert: Vec<Vec<String>> = Vec::new();
        let mut projected_delete: Vec<Vec<String>> = Vec::new();
        if !projected_candidates.is_empty() {
            metrics.rules_run += 1;
            for sym in &projected_candidates {
                let want: bool = self.db.query_one(
                    "src",
                    "SELECT EXISTS(SELECT 1 FROM src WHERE sym=?1 AND kind='keep')",
                    &[SqlVal::from(sym)],
                    |row| Ok(row.get::<_, bool>(0)?),
                )?;
                let have = self
                    .db
                    .query_opt(
                        "projected",
                        "SELECT 1 FROM projected WHERE sym=?1",
                        &[SqlVal::from(sym)],
                        |_| Ok(true),
                    )?
                    .unwrap_or(false);
                if want != have {
                    if want {
                        projected_insert.push(vec![sym.clone()]);
                    } else {
                        projected_delete.push(vec![sym.clone()]);
                    }
                    metrics.transition(want);
                    projected_transitions.insert(sym.clone());
                }
            }
        }
        self.batch_insert("projected", "INSERT INTO projected(sym)", 1, &projected_insert)?;
        if !projected_delete.is_empty() {
            self.executed_mutations.push(DELETE_PROJECTED_SCOPED);
        }
        self.batch_delete("projected", &["sym"], &projected_delete)?;

        // --- joined candidate gather (read-only; NOT a per-row write) --------
        // This loop only SELECTs, filling the in-memory candidate set; there is
        // no db mutation to batch. It stays row-at-a-time on purpose.
        let joined_syms: BTreeSet<String> = projected_transitions
            .into_iter()
            .chain(changed_stable_syms)
            .collect();
        let mut joined_candidates = BTreeSet::new();
        for sym in joined_syms {
            for payload in self.db.query_rows(
                "joined",
                "SELECT payload FROM joined WHERE sym=?1",
                &[SqlVal::from(&sym)],
                |row| Ok(row.get::<_, String>(0)?),
            )? {
                joined_candidates.insert((sym.clone(), payload));
            }
            for payload in self.db.query_rows(
                "stable",
                "SELECT payload FROM stable WHERE sym=?1",
                &[SqlVal::from(&sym)],
                |row| Ok(row.get::<_, String>(0)?),
            )? {
                joined_candidates.insert((sym.clone(), payload));
            }
        }

        // --- joined (projected AND stable) ----------------------------------
        metrics.candidate_rows += joined_candidates.len();
        metrics.observe_queue(joined_candidates.len(), pair_bytes(&joined_candidates));
        let mut joined_transition_syms = BTreeSet::new();
        let mut joined_insert: Vec<Vec<String>> = Vec::new();
        let mut joined_delete: Vec<Vec<String>> = Vec::new();
        if !joined_candidates.is_empty() {
            metrics.rules_run += 1;
            for (sym, payload) in &joined_candidates {
                let want: bool = self.db.query_one(
                    "projected",
                    "SELECT EXISTS(SELECT 1 FROM projected WHERE sym=?1) AND EXISTS(SELECT 1 FROM stable WHERE sym=?1 AND payload=?2)",
                    &[SqlVal::from(sym), SqlVal::from(payload)],
                    |row| Ok(row.get::<_, bool>(0)?),
                )?;
                let have = self
                    .db
                    .query_opt(
                        "joined",
                        "SELECT 1 FROM joined WHERE sym=?1 AND payload=?2",
                        &[SqlVal::from(sym), SqlVal::from(payload)],
                        |_| Ok(true),
                    )?
                    .unwrap_or(false);
                if want != have {
                    if want {
                        joined_insert.push(vec![sym.clone(), payload.clone()]);
                    } else {
                        joined_delete.push(vec![sym.clone(), payload.clone()]);
                    }
                    metrics.transition(want);
                    joined_transition_syms.insert(sym.clone());
                }
            }
        }
        self.batch_insert("joined", "INSERT INTO joined(sym,payload)", 2, &joined_insert)?;
        if !joined_delete.is_empty() {
            self.executed_mutations.push(DELETE_JOINED_SCOPED);
        }
        self.batch_delete("joined", &["sym", "payload"], &joined_delete)?;

        // --- out (joined emits) ---------------------------------------------
        let out_candidates: Vec<String> = joined_transition_syms.into_iter().collect();
        metrics.candidate_rows += out_candidates.len();
        metrics.observe_queue(out_candidates.len(), string_bytes(&out_candidates));
        let mut out_insert: Vec<Vec<String>> = Vec::new();
        let mut out_delete: Vec<Vec<String>> = Vec::new();
        if !out_candidates.is_empty() {
            metrics.rules_run += 1;
            for sym in &out_candidates {
                let want: bool = self.db.query_one(
                    "joined",
                    "SELECT EXISTS(SELECT 1 FROM joined WHERE sym=?1 AND payload LIKE 'emit:%')",
                    &[SqlVal::from(sym)],
                    |row| Ok(row.get::<_, bool>(0)?),
                )?;
                let have = self
                    .db
                    .query_opt(
                        "out",
                        "SELECT 1 FROM out WHERE sym=?1",
                        &[SqlVal::from(sym)],
                        |_| Ok(true),
                    )?
                    .unwrap_or(false);
                if want != have {
                    if want {
                        out_insert.push(vec![sym.clone()]);
                    } else {
                        out_delete.push(vec![sym.clone()]);
                    }
                    metrics.transition(want);
                }
            }
        }
        self.batch_insert("out", "INSERT INTO out(sym)", 1, &out_insert)?;
        if !out_delete.is_empty() {
            self.executed_mutations.push(DELETE_OUT_SCOPED);
        }
        self.batch_delete("out", &["sym"], &out_delete)?;

        self.db.commit()?;
        self.generation += 1;
        metrics.write_statements = self.write_statements;
        Ok(metrics)
    }

    fn src_present(&self, repo: &str, sym: &str, kind: &str) -> Result<bool> {
        Ok(self
            .db
            .query_opt(
                "src",
                "SELECT 1 FROM src WHERE repo=?1 AND sym=?2 AND kind=?3",
                &[SqlVal::from(repo), SqlVal::from(sym), SqlVal::from(kind)],
                |_| Ok(()),
            )?
            .is_some())
    }

    fn stable_present(&self, sym: &str, payload: &str) -> Result<bool> {
        Ok(self
            .db
            .query_opt(
                "stable",
                "SELECT 1 FROM stable WHERE sym=?1 AND payload=?2",
                &[SqlVal::from(sym), SqlVal::from(payload)],
                |_| Ok(()),
            )?
            .is_some())
    }

    /// One INSERT statement per chunk (chunked under `MAX_BOUND_PARAMS`), never
    /// one per row. `head` is everything before `VALUES`; `arity` its columns;
    /// `rel` names the table `head` inserts into (N+1 counter key).
    fn batch_insert(
        &mut self,
        rel: &str,
        head: &str,
        arity: usize,
        rows: &[Vec<String>],
    ) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let per_stmt = (MAX_BOUND_PARAMS / arity).max(1);
        let tuple = format!("({})", vec!["?"; arity].join(","));
        for chunk in rows.chunks(per_stmt) {
            let values = vec![tuple.as_str(); chunk.len()].join(",");
            let sql = format!("{head} VALUES {values}");
            let flat: Vec<SqlVal> = chunk.iter().flatten().map(SqlVal::from).collect();
            self.db.exec_params(rel, &sql, &flat)?;
            self.write_statements += 1;
        }
        Ok(())
    }

    /// One DELETE statement per chunk, keyed by an N-column tuple `IN (VALUES
    /// ...)`. Never one delete per row.
    fn batch_delete(&mut self, table: &str, cols: &[&str], rows: &[Vec<String>]) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let arity = cols.len();
        let per_stmt = (MAX_BOUND_PARAMS / arity).max(1);
        let key = if arity == 1 {
            cols[0].to_string()
        } else {
            format!("({})", cols.join(","))
        };
        let tuple = format!("({})", vec!["?"; arity].join(","));
        for chunk in rows.chunks(per_stmt) {
            let values = vec![tuple.as_str(); chunk.len()].join(",");
            let sql = format!("DELETE FROM {table} WHERE {key} IN (VALUES {values})");
            let flat: Vec<SqlVal> = chunk.iter().flatten().map(SqlVal::from).collect();
            self.db.exec_params(table, &sql, &flat)?;
            self.write_statements += 1;
        }
        Ok(())
    }

    pub(crate) fn has_whole_derived_delete(&self) -> bool {
        self.executed_mutations.iter().any(|sql| {
            (sql.starts_with("DELETE FROM projected")
                || sql.starts_with("DELETE FROM joined")
                || sql.starts_with("DELETE FROM out"))
                && !sql.contains(" WHERE ")
        })
    }

    fn assert_clean_rebuild_parity(&self) -> Result<()> {
        let expected_projected = strings(
            &self.db,
            "src",
            "SELECT DISTINCT sym FROM src WHERE kind='keep' ORDER BY sym",
        )?;
        let actual_projected = strings(&self.db, "projected", "SELECT sym FROM projected ORDER BY sym")?;
        assert_eq!(
            actual_projected, expected_projected,
            "projected differs from clean rebuild"
        );
        let expected_joined = pairs(&self.db, "src",
            "SELECT DISTINCT p.sym,s.payload FROM (SELECT DISTINCT sym FROM src WHERE kind='keep') p JOIN stable s USING(sym) ORDER BY p.sym,s.payload")?;
        let actual_joined = pairs(
            &self.db,
            "joined",
            "SELECT sym,payload FROM joined ORDER BY sym,payload",
        )?;
        assert_eq!(
            actual_joined, expected_joined,
            "joined differs from clean rebuild"
        );
        let expected_out = strings(&self.db, "src",
            "SELECT DISTINCT p.sym FROM (SELECT DISTINCT sym FROM src WHERE kind='keep') p JOIN stable s USING(sym) WHERE s.payload LIKE 'emit:%' ORDER BY p.sym")?;
        let actual_out = strings(&self.db, "out", "SELECT sym FROM out ORDER BY sym")?;
        assert_eq!(actual_out, expected_out, "out differs from clean rebuild");
        Ok(())
    }

    fn seed_clean_unrelated(&mut self, count: usize) -> Result<()> {
        self.db.execute_batch_on("src", &format!(
            "
            WITH RECURSIVE n(i) AS (VALUES(1) UNION ALL SELECT i+1 FROM n WHERE i<{count})
              INSERT INTO src SELECT 'bulk',printf('u%05d',i),'keep' FROM n;
            WITH RECURSIVE n(i) AS (VALUES(1) UNION ALL SELECT i+1 FROM n WHERE i<{count})
              INSERT INTO stable SELECT printf('u%05d',i),printf('emit:%05d',i) FROM n;
            INSERT INTO projected SELECT DISTINCT sym FROM src WHERE kind='keep';
            INSERT INTO joined SELECT p.sym,s.payload FROM projected p JOIN stable s USING(sym);
            INSERT INTO out SELECT DISTINCT sym FROM joined WHERE payload LIKE 'emit:%';
        "
        ))?;
        Ok(())
    }
}

fn doubles(rows: &BTreeSet<(String, String)>) -> Vec<Vec<String>> {
    rows.iter()
        .map(|(first, second)| vec![first.clone(), second.clone()])
        .collect()
}

fn triples(rows: &BTreeSet<(String, String, String)>) -> Vec<Vec<String>> {
    rows.iter()
        .map(|(first, second, third)| vec![first.clone(), second.clone(), third.clone()])
        .collect()
}

fn string_bytes(rows: &[String]) -> usize {
    rows.iter()
        .map(|s| s.len() + std::mem::size_of::<String>())
        .sum()
}

fn pair_bytes(rows: &BTreeSet<(String, String)>) -> usize {
    rows.iter()
        .map(|(a, b)| a.len() + b.len() + 2 * std::mem::size_of::<String>())
        .sum()
}

fn strings(db: &Db, rel: &str, sql: &str) -> Result<Vec<String>> {
    db.query_rows(rel, sql, &[], |row| Ok(row.get(0)?))
}

fn pairs(db: &Db, rel: &str, sql: &str) -> Result<Vec<(String, String)>> {
    db.query_rows(rel, sql, &[], |row| Ok((row.get(0)?, row.get(1)?)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src(repo: &str, sym: &str, kind: &str, diff: i64) -> InputChange {
        InputChange::Src {
            repo: repo.into(),
            sym: sym.into(),
            kind: kind.into(),
            diff,
        }
    }

    fn stable(sym: &str, payload: &str, diff: i64) -> InputChange {
        InputChange::Stable {
            sym: sym.into(),
            payload: payload.into(),
            diff,
        }
    }

    #[test]
    fn clean_rebuild_parity_after_each_add_and_delete() -> Result<()> {
        let mut f = BooleanDeltaFixture::new()?;
        for changes in [
            vec![src("r1", "s", "keep", 1)],
            vec![stable("s", "emit:value", 1)],
            vec![src("r2", "s", "keep", 1)],
            vec![src("r1", "s", "keep", -1)],
            vec![stable("s", "emit:value", -1)],
            vec![src("r2", "s", "keep", -1)],
        ] {
            f.apply_generation(&changes)?;
            f.assert_clean_rebuild_parity()?;
            assert!(!f.has_whole_derived_delete());
        }
        assert_eq!(f.generation(), 6);
        Ok(())
    }

    #[test]
    fn deleting_one_duplicate_projection_witness_emits_nothing() -> Result<()> {
        let mut f = BooleanDeltaFixture::new()?;
        f.apply_generation(&[
            src("r1", "s", "keep", 1),
            src("r2", "s", "keep", 1),
            stable("s", "emit:value", 1),
        ])?;
        let metrics = f.apply_generation(&[src("r1", "s", "keep", -1)])?;
        assert_eq!(metrics.public_plus, 0);
        assert_eq!(metrics.public_minus, 0);
        assert_eq!(metrics.candidate_rows, 1);
        assert_eq!(metrics.rules_run, 1);
        f.assert_clean_rebuild_parity()
    }

    #[test]
    fn simultaneous_source_changes_produce_each_public_transition_once() -> Result<()> {
        let mut f = BooleanDeltaFixture::new()?;
        let added =
            f.apply_generation(&[stable("s", "emit:value", 1), src("r", "s", "keep", 1)])?;
        assert_eq!((added.public_plus, added.public_minus), (3, 0));
        f.assert_clean_rebuild_parity()?;
        let removed =
            f.apply_generation(&[src("r", "s", "keep", -1), stable("s", "emit:value", -1)])?;
        assert_eq!((removed.public_plus, removed.public_minus), (0, 3));
        f.assert_clean_rebuild_parity()
    }

    #[test]
    fn filter_without_output_stops_public_propagation() -> Result<()> {
        let mut f = BooleanDeltaFixture::new()?;
        let metrics =
            f.apply_generation(&[src("r", "s", "keep", 1), stable("s", "ignore:value", 1)])?;
        assert_eq!((metrics.public_plus, metrics.public_minus), (2, 0));
        assert!(strings(&f.db, "out", "SELECT sym FROM out")?.is_empty());
        f.assert_clean_rebuild_parity()
    }

    // N+1 structural guard, sibling to tests/it/derived_intern_n1.rs. A single
    // generation touches K distinct symbols, each cascading a public row into
    // all three derived rels — 2*K input rows written plus 3*K derived rows. A
    // per-row write path issues one INSERT/DELETE statement per row (>= 5*K).
    // Batched, the whole generation collapses to a handful of chunked writes,
    // structurally below even the 2*K change count. Read off the write-statement
    // counter, not a timing vibe; restoring any per-row `db.execute` inside a
    // loop trips the `< changes.len()` bound.
    #[test]
    fn batched_writes_do_not_scale_with_change_count() -> Result<()> {
        const K: usize = 200;
        let mut f = BooleanDeltaFixture::new()?;
        let mut changes = Vec::with_capacity(2 * K);
        for i in 0..K {
            let sym = format!("sym{i:05}");
            changes.push(src("r", &sym, "keep", 1));
            changes.push(stable(&sym, "emit:value", 1));
        }
        let metrics = f.apply_generation(&changes)?;

        // Every symbol reached a public row in projected, joined, and out.
        assert_eq!(metrics.public_plus, 3 * K, "expected a full cascade per symbol");
        assert_eq!(metrics.delta_input_rows, 2 * K);
        assert!(
            changes.len() > 64,
            "fixture must write enough rows to expose the N+1 (got {})",
            changes.len()
        );
        // The structural guarantee: 5*K rows written this generation must NOT
        // cost 5*K write statements. Pre-fix this loop issued one execute per
        // changed row and blew past the change count.
        assert!(
            metrics.write_statements < changes.len(),
            "write statements {} did not stay below the {}-row change count — a \
             per-row write slipped back into apply_generation",
            metrics.write_statements,
            changes.len()
        );
        assert!(
            metrics.write_statements <= 16,
            "unexpected batched write fan-out: {}",
            metrics.write_statements
        );
        f.assert_clean_rebuild_parity()
    }

    #[test]
    fn unrelated_corpus_size_does_not_change_touched_work() -> Result<()> {
        fn run(unrelated: usize) -> Result<DeltaMetrics> {
            let mut f = BooleanDeltaFixture::new()?;
            f.seed_clean_unrelated(unrelated)?;
            let metrics = f.apply_generation(&[
                src("r", "target", "keep", 1),
                stable("target", "emit:value", 1),
            ])?;
            f.assert_clean_rebuild_parity()?;
            Ok(metrics)
        }
        let small = run(1_000)?;
        let large = run(10_000)?;
        assert_eq!(small, large);
        assert_eq!(small.delta_input_rows, 2);
        assert_eq!(small.candidate_rows, 3);
        assert_eq!(small.peak_queued_rows, 1);
        Ok(())
    }
}
