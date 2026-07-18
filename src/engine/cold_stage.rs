//! Cold-start staging: extract one extract-family node per tick instead of the
//! whole corpus in one lock-held pass (plan `2026-07-17-cold-start-staging.md`).
//!
//! On a blank-slate db the daemon poll loop drives `tick_report`, which normally
//! runs every used extraction family (module/type/call/dataflow/doc/...) over the
//! whole corpus inline, then the full derived rebuild — one long lock-held tick.
//! Staging defers that fan-out: the first blank-slate tick reconciles sources +
//! loads the scip index + primes the analysis bundles (the INPUTS each family
//! reads), seeds one `_cold_node` row per used family, and returns. Each node
//! then runs its wholesale family refresh in its own budget-throttled
//! `ColdExtract` job; when the last node lands, the completion tick does the
//! single blank-slate derived rebuild over the now-complete fact base.
//!
//! Scope of THIS arc (D2/D4 as implemented): every used family runs WHOLESALE as
//! one node — `n_shards = 1`. Per-file sharding of an individual family (the
//! plan's Shape B) is deferred: the type/call/dataflow resolvers run a
//! corpus-global name→def barrier and the `extract:<family>` skip digest is
//! per-rev, not per-shard, so a per-file slice cannot be made digest-consistent
//! without new infra — and a wrong slice would poison the digest skip and break
//! the inline-equivalence contract. `cold_shard_count` carries the D2 formula
//! (`ceil(files/200)` capped at 16) behind a `shardable()` gate that no family
//! sets this arc, so the node table and seam are ready for Shape B. The plan
//! sanctions exactly this: "a wholesale family is just a family with N_SHARDS=1".
//!
//! The `_cold_node` table lives in the corpus db next to `_reldigest` /
//! `_derived_complete` (engine FACT state: which slices are extracted); the
//! `ColdExtract` `_job` rows live in `jobs.sqlite` (CONTROL state). A node marked
//! `done` implies its rows are fully written (the mark runs only after the
//! wholesale refresh commits); a node still `pending` after a `kill -9` re-runs
//! its idempotent wholesale refresh — the crash-recovery unit is one family.

use anyhow::{bail, Result};

use crate::ast::{Program, Value};

use super::Engine;

const COLD_SCHEMA: &str = "\
CREATE TABLE IF NOT EXISTS _cold_node(
  family      TEXT NOT NULL,
  shard       INTEGER NOT NULL,
  n_shards    INTEGER NOT NULL,
  state       TEXT NOT NULL DEFAULT 'pending',
  input_digest BLOB,
  done_at     INTEGER,
  PRIMARY KEY (family, shard)
) WITHOUT ROWID;";

/// D2 target files per shard; a family with more files splits into
/// `ceil(files / this)` shards, capped at `COLD_MAX_SHARDS`. Overridable for
/// tests via `DL_COLD_SHARD_FILES`.
const COLD_TARGET_FILES_PER_SHARD: i64 = 200;
const COLD_MAX_SHARDS: u32 = 16;

/// One seeded `(family, shard)` node the tick hands back to the caller (the
/// daemon shell) to enqueue as a `ColdExtract` job. `priority` is the canonical
/// extraction rank (module highest) so a single-flight worker runs them in the
/// same order the inline tick would.
#[derive(Clone, Debug)]
pub struct ColdJob {
    pub family: String,
    pub shard: u32,
    pub priority: i64,
}

impl Engine {
    fn ensure_cold_schema(&self) -> Result<()> {
        self.db.execute_batch(COLD_SCHEMA)
    }

    /// D2 shard count for a family over `file_count` files. Wholesale (1) unless
    /// the family opts into sharding; no family does this arc (see module doc).
    fn cold_shard_count(&self, shardable: bool, file_count: i64) -> u32 {
        if !shardable || file_count <= 0 {
            return 1;
        }
        let target = std::env::var("DL_COLD_SHARD_FILES")
            .ok()
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(COLD_TARGET_FILES_PER_SHARD);
        let n = ((file_count + target - 1) / target).max(1);
        (n as u32).min(COLD_MAX_SHARDS)
    }

    /// The used extract families in canonical (inline-tick) order — the cold-node
    /// set. `spine` (post-node projection) and the hand-dispatched `node` (CST)
    /// walk are excluded: they lack a pre-walk skip digest, so they re-run on the
    /// completion tick anyway, and staging them would need node→spine ordering.
    fn cold_families(&self, prog: &Program) -> Vec<&'static dyn crate::rels::ExtractFamily> {
        crate::rels::extract_families_pre_node()
            .iter()
            .copied()
            .filter(|fam| fam.used(prog))
            .collect()
    }

    fn cold_corpus_file_count(&self) -> i64 {
        self.db
            .conn()
            .query_row("SELECT COUNT(*) FROM _file", [], |r| r.get(0))
            .unwrap_or(0)
    }

    /// Should this (daemon, blank-slate) tick START cold staging? True exactly
    /// once on a genuinely cold db: staging enabled, corpus non-empty, at least
    /// one extract family used, no `_cold_node` rows yet, and the derived layer
    /// still blank (`derived:program` unset). `--no-daemon`/one-shot keeps the
    /// inline cold tick (D1).
    pub(crate) fn cold_start_should_seed(&self, prog: &Program) -> Result<bool> {
        if !self.poll_loop {
            return Ok(false);
        }
        if std::env::var_os("DL_NO_COLD_STAGE").is_some() {
            return Ok(false);
        }
        self.ensure_cold_schema()?;
        let already: i64 = self
            .db
            .conn()
            .query_row("SELECT COUNT(*) FROM _cold_node", [], |r| r.get(0))
            .unwrap_or(0);
        if already > 0 {
            return Ok(false);
        }
        if self.cold_corpus_file_count() <= 0 {
            return Ok(false);
        }
        // A db with an already-built derived layer is not a blank slate: a warm
        // program edit / new family goes through the normal digest-driven path,
        // not staging.
        if self.load_rel_digest("derived:program")?.is_some() {
            return Ok(false);
        }
        Ok(!self.cold_families(prog).is_empty())
    }

    /// Is a cold start in progress (some `_cold_node` still `pending`)? While
    /// true the tick returns early after (re-)enqueuing the pending nodes — the
    /// resume path after a `kill -9` (done nodes stay done, only pending re-run).
    pub(crate) fn cold_start_in_progress(&self) -> Result<bool> {
        self.ensure_cold_schema()?;
        let pending: i64 = self
            .db
            .conn()
            .query_row("SELECT COUNT(*) FROM _cold_node WHERE state != 'done'", [], |r| r.get(0))
            .unwrap_or(0);
        Ok(pending > 0)
    }

    /// Seed one `_cold_node` row per used family (one batched insert — N+1 law),
    /// returning the `ColdJob`s for the caller to enqueue. Idempotent: the PK
    /// `(family, shard)` + `INSERT OR IGNORE` make a re-seed a no-op.
    pub(crate) fn seed_cold_nodes(&self, prog: &Program) -> Result<Vec<ColdJob>> {
        self.ensure_cold_schema()?;
        let families = self.cold_families(prog);
        let count = families.len();
        let file_count = self.cold_corpus_file_count();
        let mut rows: Vec<Vec<Value>> = Vec::new();
        let mut jobs: Vec<ColdJob> = Vec::new();
        for (idx, fam) in families.iter().enumerate() {
            let n_shards = self.cold_shard_count(fam.shardable_cold(), file_count);
            // Priority = reverse canonical rank so module (idx 0) claims first.
            let priority = (count - idx) as i64;
            for shard in 0..n_shards {
                rows.push(vec![
                    Value::Text(fam.name().to_string()),
                    Value::Int(shard as i64),
                    Value::Int(n_shards as i64),
                    Value::Text("pending".to_string()),
                    Value::Null,
                    Value::Null,
                ]);
                jobs.push(ColdJob { family: fam.name().to_string(), shard, priority });
            }
        }
        self.db.insert_rows(
            "_cold_node",
            &["family", "shard", "n_shards", "state", "input_digest", "done_at"],
            &rows,
        )?;
        Ok(jobs)
    }

    /// The still-`pending` nodes as `ColdJob`s in canonical priority order — the
    /// resume set re-enqueued while `cold_start_in_progress`.
    pub(crate) fn pending_cold_jobs(&self, prog: &Program) -> Result<Vec<ColdJob>> {
        self.ensure_cold_schema()?;
        let families = self.cold_families(prog);
        let count = families.len();
        let mut rank: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for (idx, fam) in families.iter().enumerate() {
            rank.insert(fam.name(), idx);
        }
        let mut jobs: Vec<ColdJob> = Vec::new();
        let conn = self.db.conn();
        let mut stmt =
            conn.prepare("SELECT family, shard FROM _cold_node WHERE state != 'done'")?;
        let found = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u32))
        })?;
        for row in found {
            let (family, shard) = row?;
            // A node whose family is no longer used (program shrank mid-start)
            // gets the lowest priority; it will be swept at declare time.
            let priority = rank
                .get(family.as_str())
                .map(|idx| (count - idx) as i64)
                .unwrap_or(0);
            jobs.push(ColdJob { family, shard, priority });
        }
        jobs.sort_by(|a, b| b.priority.cmp(&a.priority));
        Ok(jobs)
    }

    /// Every seeded node is `done`? Gate for the completion tick's first full
    /// derived rebuild.
    pub fn cold_nodes_complete(&self) -> Result<bool> {
        self.ensure_cold_schema()?;
        let (total, pending): (i64, i64) = self.db.conn().query_row(
            "SELECT COUNT(*), COALESCE(SUM(state != 'done'), 0) FROM _cold_node",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok(total > 0 && pending == 0)
    }

    /// Run ONE cold node: dispatch to the family's wholesale refresh, then mark
    /// the row `done`. Runs under the engine mutex like any tick. Idempotent — an
    /// already-`done` node is a no-op; a re-run after a crash re-does the family's
    /// wholesale wipe+repopulate. The mark runs only AFTER `refresh` commits, so a
    /// `done` node always has fully-written rows; a crash before the mark leaves
    /// the node `pending` for an idempotent redo.
    pub fn run_cold_node(&mut self, prog: &Program, family: &str, _shard: u32) -> Result<()> {
        self.ensure_cold_schema()?;
        let done: i64 = self
            .db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM _cold_node WHERE family=?1 AND state='done'",
                [family],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if done > 0 {
            return Ok(());
        }
        let Some(fam) = crate::rels::extract_families_pre_node()
            .iter()
            .copied()
            .find(|fam| fam.name() == family)
        else {
            bail!("cold-start: unknown extract family '{family}'");
        };
        self.db.tick_begin();
        self.db.clear_write_ledger();
        // The scip pre-extract already loaded on the seed tick; re-prime the
        // per-file analysis caches (idempotent — cached) so a type/call node
        // parses through the same bundle the inline path used.
        super::extract::prime_analysis_bundles(self, prog)?;
        fam.refresh(self)?;
        let now = super::now_secs();
        self.db.conn().execute(
            "UPDATE _cold_node SET state='done', done_at=?2 WHERE family=?1",
            rusqlite::params![family, now],
        )?;
        self.last_n1 = self.db.tick_end();
        Ok(())
    }

    /// Declare-time sweep: drop `_cold_node` rows for families no longer used
    /// (mirrors the `_derived_complete` cleanup). Only runs once the table
    /// exists; a fresh db has nothing to sweep.
    pub(crate) fn sweep_cold_nodes(&self, prog: &Program) -> Result<()> {
        let exists: i64 = self
            .db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='_cold_node'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if exists == 0 {
            return Ok(());
        }
        let used: std::collections::HashSet<&'static str> =
            self.cold_families(prog).iter().map(|fam| fam.name()).collect();
        let stored: Vec<String> = {
            let conn = self.db.conn();
            let mut stmt = conn.prepare("SELECT DISTINCT family FROM _cold_node")?;
            let found = stmt.query_map([], |r| r.get::<_, String>(0))?;
            found.filter_map(|x| x.ok()).collect()
        };
        for family in stored {
            if !used.contains(family.as_str()) {
                self.db
                    .conn()
                    .execute("DELETE FROM _cold_node WHERE family=?1", [&family])?;
            }
        }
        Ok(())
    }
}
