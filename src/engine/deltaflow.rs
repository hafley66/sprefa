//! Test-only positive-acyclic Boolean incremental-view fixture.
//!
//! This module deliberately uses a private in-memory SQLite schema. It does not
//! participate in production execution; it proves candidate revalidation and
//! public-presence transitions before the runtime is wired to a typed plan.

use rusqlite::{params, Connection, OptionalExtension, Result};
use std::collections::BTreeSet;

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
    db: Connection,
    generation: u64,
    executed_mutations: Vec<&'static str>,
}

impl BooleanDeltaFixture {
    pub(crate) fn new() -> Result<Self> {
        let db = Connection::open_in_memory()?;
        db.execute_batch(
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
        })
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn apply_generation(&mut self, changes: &[InputChange]) -> Result<DeltaMetrics> {
        let mut metrics = DeltaMetrics::default();
        let mut changed_src_syms = BTreeSet::new();
        let mut changed_stable_syms = BTreeSet::new();
        self.db.execute_batch("BEGIN IMMEDIATE")?;

        for change in changes {
            match change {
                InputChange::Src {
                    repo,
                    sym,
                    kind,
                    diff: 1,
                } => {
                    let changed = self.db.execute(
                        "INSERT OR IGNORE INTO src(repo,sym,kind) VALUES(?1,?2,?3)",
                        params![repo, sym, kind],
                    )?;
                    if changed != 0 {
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
                    let changed = self.db.execute(
                        "DELETE FROM src WHERE repo=?1 AND sym=?2 AND kind=?3",
                        params![repo, sym, kind],
                    )?;
                    if changed != 0 {
                        metrics.delta_input_rows += 1;
                        changed_src_syms.insert(sym.clone());
                    }
                }
                InputChange::Stable {
                    sym,
                    payload,
                    diff: 1,
                } => {
                    let changed = self.db.execute(
                        "INSERT OR IGNORE INTO stable(sym,payload) VALUES(?1,?2)",
                        params![sym, payload],
                    )?;
                    if changed != 0 {
                        metrics.delta_input_rows += 1;
                        changed_stable_syms.insert(sym.clone());
                    }
                }
                InputChange::Stable {
                    sym,
                    payload,
                    diff: -1,
                } => {
                    let changed = self.db.execute(
                        "DELETE FROM stable WHERE sym=?1 AND payload=?2",
                        params![sym, payload],
                    )?;
                    if changed != 0 {
                        metrics.delta_input_rows += 1;
                        changed_stable_syms.insert(sym.clone());
                    }
                }
                _ => return Err(rusqlite::Error::InvalidQuery),
            }
        }

        let projected_candidates: Vec<String> = changed_src_syms.into_iter().collect();
        metrics.candidate_rows += projected_candidates.len();
        metrics.observe_queue(
            projected_candidates.len(),
            string_bytes(&projected_candidates),
        );
        let mut projected_transitions = BTreeSet::new();
        if !projected_candidates.is_empty() {
            metrics.rules_run += 1;
            for sym in projected_candidates {
                let want = self.db.query_row(
                    "SELECT EXISTS(SELECT 1 FROM src WHERE sym=?1 AND kind='keep')",
                    [&sym],
                    |row| row.get::<_, bool>(0),
                )?;
                let have = self
                    .db
                    .query_row("SELECT 1 FROM projected WHERE sym=?1", [&sym], |_| Ok(true))
                    .optional()?
                    .unwrap_or(false);
                if want != have {
                    if want {
                        self.db
                            .execute("INSERT INTO projected(sym) VALUES(?1)", [&sym])?;
                    } else {
                        const SQL: &str = "DELETE FROM projected WHERE sym=?1";
                        self.executed_mutations.push(SQL);
                        self.db.execute(SQL, [&sym])?;
                    }
                    metrics.transition(want);
                    projected_transitions.insert(sym);
                }
            }
        }

        let joined_syms: BTreeSet<String> = projected_transitions
            .into_iter()
            .chain(changed_stable_syms)
            .collect();
        let mut joined_candidates = BTreeSet::new();
        for sym in joined_syms {
            let mut old = self.db.prepare("SELECT payload FROM joined WHERE sym=?1")?;
            let rows = old.query_map([&sym], |row| row.get::<_, String>(0))?;
            for payload in rows {
                joined_candidates.insert((sym.clone(), payload?));
            }
            let mut current = self.db.prepare("SELECT payload FROM stable WHERE sym=?1")?;
            let rows = current.query_map([&sym], |row| row.get::<_, String>(0))?;
            for payload in rows {
                joined_candidates.insert((sym.clone(), payload?));
            }
        }
        metrics.candidate_rows += joined_candidates.len();
        metrics.observe_queue(joined_candidates.len(), pair_bytes(&joined_candidates));
        let mut joined_transition_syms = BTreeSet::new();
        if !joined_candidates.is_empty() {
            metrics.rules_run += 1;
            for (sym, payload) in joined_candidates {
                let want = self.db.query_row(
                    "SELECT EXISTS(SELECT 1 FROM projected WHERE sym=?1) AND EXISTS(SELECT 1 FROM stable WHERE sym=?1 AND payload=?2)",
                    params![sym, payload], |row| row.get::<_, bool>(0),
                )?;
                let have = self
                    .db
                    .query_row(
                        "SELECT 1 FROM joined WHERE sym=?1 AND payload=?2",
                        params![sym, payload],
                        |_| Ok(true),
                    )
                    .optional()?
                    .unwrap_or(false);
                if want != have {
                    if want {
                        self.db.execute(
                            "INSERT INTO joined(sym,payload) VALUES(?1,?2)",
                            params![sym, payload],
                        )?;
                    } else {
                        const SQL: &str = "DELETE FROM joined WHERE sym=?1 AND payload=?2";
                        self.executed_mutations.push(SQL);
                        self.db.execute(SQL, params![sym, payload])?;
                    }
                    metrics.transition(want);
                    joined_transition_syms.insert(sym);
                }
            }
        }

        let out_candidates: Vec<String> = joined_transition_syms.into_iter().collect();
        metrics.candidate_rows += out_candidates.len();
        metrics.observe_queue(out_candidates.len(), string_bytes(&out_candidates));
        if !out_candidates.is_empty() {
            metrics.rules_run += 1;
            for sym in out_candidates {
                let want = self.db.query_row(
                    "SELECT EXISTS(SELECT 1 FROM joined WHERE sym=?1 AND payload LIKE 'emit:%')",
                    [&sym],
                    |row| row.get::<_, bool>(0),
                )?;
                let have = self
                    .db
                    .query_row("SELECT 1 FROM out WHERE sym=?1", [&sym], |_| Ok(true))
                    .optional()?
                    .unwrap_or(false);
                if want != have {
                    if want {
                        self.db.execute("INSERT INTO out(sym) VALUES(?1)", [&sym])?;
                    } else {
                        const SQL: &str = "DELETE FROM out WHERE sym=?1";
                        self.executed_mutations.push(SQL);
                        self.db.execute(SQL, [&sym])?;
                    }
                    metrics.transition(want);
                }
            }
        }

        self.db.execute_batch("COMMIT")?;
        self.generation += 1;
        Ok(metrics)
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
            "SELECT DISTINCT sym FROM src WHERE kind='keep' ORDER BY sym",
        )?;
        let actual_projected = strings(&self.db, "SELECT sym FROM projected ORDER BY sym")?;
        assert_eq!(
            actual_projected, expected_projected,
            "projected differs from clean rebuild"
        );
        let expected_joined = pairs(&self.db,
            "SELECT DISTINCT p.sym,s.payload FROM (SELECT DISTINCT sym FROM src WHERE kind='keep') p JOIN stable s USING(sym) ORDER BY p.sym,s.payload")?;
        let actual_joined = pairs(
            &self.db,
            "SELECT sym,payload FROM joined ORDER BY sym,payload",
        )?;
        assert_eq!(
            actual_joined, expected_joined,
            "joined differs from clean rebuild"
        );
        let expected_out = strings(&self.db,
            "SELECT DISTINCT p.sym FROM (SELECT DISTINCT sym FROM src WHERE kind='keep') p JOIN stable s USING(sym) WHERE s.payload LIKE 'emit:%' ORDER BY p.sym")?;
        let actual_out = strings(&self.db, "SELECT sym FROM out ORDER BY sym")?;
        assert_eq!(actual_out, expected_out, "out differs from clean rebuild");
        Ok(())
    }

    fn seed_clean_unrelated(&mut self, count: usize) -> Result<()> {
        self.db.execute_batch(&format!(
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

fn strings(db: &Connection, sql: &str) -> Result<Vec<String>> {
    let mut stmt = db.prepare(sql)?;
    let rows = stmt.query_map([], |row| row.get(0))?.collect();
    rows
}

fn pairs(db: &Connection, sql: &str) -> Result<Vec<(String, String)>> {
    let mut stmt = db.prepare(sql)?;
    let rows = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect();
    rows
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
        assert!(strings(&f.db, "SELECT sym FROM out")?.is_empty());
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
