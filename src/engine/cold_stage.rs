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
//! Scope as of the D2/D4 landing: every used family runs WHOLESALE as one node
//! — `n_shards = 1`. Per-file sharding of an individual family (the plan's
//! Shape B) needs a family with NO corpus-global resolver barrier, since a
//! wrong slice would poison the digest skip and break the inline-equivalence
//! contract; `module`/`type`/`call`/`spine` keep `shardable_cold() == false`
//! for exactly that reason (a corpus-global name→def barrier or a
//! `_strings`/`_where_bytes` projection). `dataflow`/`comment`/`template`/
//! `unresolved` (2026-07-18 chunking arc) ARE barrier-free — every row is a
//! pure function of its own file's bytes — so each sets `shardable_cold() ==
//! true` and `cold_shard_count` partitions its OWN family-scoped file set (see
//! `cold_family_files`) into `cold_chunk_slices_of`'s byte-bounded runs. The
//! plan sanctions exactly this: "a wholesale family is just a family with
//! N_SHARDS=1".
//!
//! The `_cold_node` table lives in the corpus db next to `_reldigest` /
//! `_derived_complete` (engine FACT state: which slices are extracted); the
//! `ColdExtract` `_job` rows live in `jobs.sqlite` (CONTROL state). A node marked
//! `done` implies its rows are fully written (the mark runs only after the
//! wholesale refresh commits); a node still `pending` after a `kill -9` re-runs
//! its idempotent wholesale refresh — the crash-recovery unit is one family.

use anyhow::{bail, Result};

use crate::ast::{Program, Value};
use crate::db::SqlVal;

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

/// MB-bounded chunking (plan Addendum 2026-07-18). A shardable (barrier-free)
/// family partitions the corpus into contiguous byte-bounded runs of the
/// deterministically-ordered `extract_file_set()`: a chunk closes when its
/// summed file bytes reach `COLD_CHUNK_TARGET_BYTES` OR it holds
/// `COLD_CHUNK_MAX_FILES` files (the many-tiny-files cap). Bytes is the primary
/// axis — it tracks parse + emit + row-count cost (measured ~82 ms/MB parse,
/// dataflow ~1.3 s/MB total, release). A single file over the target is its own
/// chunk (a parse cannot split mid-file). Overridable for tests via
/// `DL_COLD_CHUNK_BYTES` / `DL_COLD_CHUNK_FILES`.
const COLD_CHUNK_TARGET_BYTES: i64 = 512 * 1024;
const COLD_CHUNK_MAX_FILES: usize = 64;

/// The scip pre-extract's own cold node (coordinator item 2): the SCIP index
/// load feeds the call/type resolution ladder, so it drains FIRST (highest
/// priority) and never hides inside another family's tick.
const COLD_SCIP_FAMILY: &str = "scip-index";

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

    /// The file set a shardable family partitions, keyed by family name: each
    /// barrier-free family's OWN scope, which can differ from the corpus-wide
    /// `extract_file_set()` — `comment-rels` also covers `.md` and the broader
    /// `AST_LANG_TABLE` grammars extract_file_set skips, and `template-rels`/
    /// `unresolved-rels` cover the TS/JS-family extensions only. Partitioning
    /// against the wrong (too-wide or too-narrow) set would silently miscount
    /// shards or drop a file from every chunk, so each family answers for
    /// itself; `dataflow-rels` (and any future family with no narrower scope)
    /// falls back to `extract_file_set()`.
    fn cold_family_files(&self, family: &str) -> Result<Vec<crate::engine::extract::ExtractFile>> {
        match family {
            "comment-rels" => self.comment_file_set(),
            "template-rels" => self.template_file_set(),
            "unresolved-rels" => self.unresolved_file_set(),
            _ => self.extract_file_set(),
        }
    }

    /// Deterministic closed partition of `files` into contiguous byte-bounded
    /// chunks (see `COLD_CHUNK_TARGET_BYTES`). `files` must already be in a
    /// stable canonical order (every `cold_family_files` query is `ORDER BY
    /// repo, path, rev`), so chunk `k` is the same slice at seed and at every
    /// crash resume (the corpus is frozen during cold start). A wholesale
    /// family maps to one chunk = its whole set.
    fn cold_chunk_slices_of(
        &self,
        files: Vec<crate::engine::extract::ExtractFile>,
    ) -> Result<Vec<Vec<crate::engine::extract::ExtractFile>>> {
        let target = std::env::var("DL_COLD_CHUNK_BYTES")
            .ok()
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(COLD_CHUNK_TARGET_BYTES);
        let cap = std::env::var("DL_COLD_CHUNK_FILES")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(COLD_CHUNK_MAX_FILES);
        // Per-file bytes from `_file.size`, keyed by (repo, path, rev).
        let sizes: std::collections::HashMap<(String, String, String), i64> = self
            .db
            .query_rows(
                "_file",
                "SELECT repo, path, rev, size FROM _file",
                &[],
                |r| {
                    Ok((
                        (r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?),
                        r.get::<_, i64>(3)?,
                    ))
                },
            )?
            .into_iter()
            .collect();
        let mut slices: Vec<Vec<crate::engine::extract::ExtractFile>> = Vec::new();
        let mut cur: Vec<crate::engine::extract::ExtractFile> = Vec::new();
        let mut cur_bytes: i64 = 0;
        for f in files {
            let bytes = sizes
                .get(&(f.0.clone(), f.1.clone(), f.2.clone()))
                .copied()
                .unwrap_or(0);
            tracing::trace!(
                repo = %f.0, path = %f.1, rev = %f.2, bytes, cur_bytes, slices = slices.len(),
                "[cold-chunk] file assigned to chunk"
            );
            cur.push(f);
            cur_bytes += bytes.max(0);
            if cur_bytes >= target || cur.len() >= cap {
                slices.push(std::mem::take(&mut cur));
                cur_bytes = 0;
            }
        }
        if !cur.is_empty() {
            slices.push(cur);
        }
        Ok(slices)
    }

    /// Chunk count for a family: the byte-partition size over ITS OWN file set
    /// (`cold_family_files`) for a shardable (barrier-free) family, else 1
    /// (wholesale single node).
    fn cold_shard_count(&self, family: &str, shardable: bool) -> Result<u32> {
        if !shardable {
            return Ok(1);
        }
        let files = self.cold_family_files(family)?;
        let n = self.cold_chunk_slices_of(files)?.len().max(1);
        Ok(n as u32)
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
            .query_one("_file", "SELECT COUNT(*) FROM _file", &[], |r| Ok(r.get(0)?))
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
            .query_one("_cold_node", "SELECT COUNT(*) FROM _cold_node", &[], |r| {
                Ok(r.get(0)?)
            })
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
            .query_one(
                "_cold_node",
                "SELECT COUNT(*) FROM _cold_node WHERE state != 'done'",
                &[],
                |r| Ok(r.get(0)?),
            )
            .unwrap_or(0);
        Ok(pending > 0)
    }

    /// Does the program use any `pre_extract` RelKind (scip)? Gates the
    /// `scip-index` cold node.
    fn cold_pre_extract_used(&self, prog: &Program) -> bool {
        crate::rels::rel_kinds()
            .iter()
            .any(|k| k.pre_extract() && k.used(prog))
    }

    /// Priority for a family at canonical index `idx` of `count` used families:
    /// module (idx 0) highest, spine-adjacent last. The `scip-index` node sits
    /// above every family (`count + 1`) so it drains first.
    fn cold_family_priority(&self, idx: usize, count: usize) -> i64 {
        (count - idx) as i64
    }

    /// Seed the cold-node set (one batched insert — N+1 law): one `scip-index`
    /// node (if scip is used), one node per WHOLESALE family, and one node per
    /// CHUNK of each shardable (barrier-free) family. Idempotent: PK
    /// `(family, shard)` + `INSERT OR IGNORE` make a re-seed a no-op.
    pub(crate) fn seed_cold_nodes(&self, prog: &Program) -> Result<Vec<ColdJob>> {
        self.ensure_cold_schema()?;
        let families = self.cold_families(prog);
        let count = families.len();
        let mut rows: Vec<Vec<Value>> = Vec::new();
        let mut jobs: Vec<ColdJob> = Vec::new();
        let mut push = |family: &str, shard: u32, n_shards: u32, priority: i64,
                        rows: &mut Vec<Vec<Value>>, jobs: &mut Vec<ColdJob>| {
            rows.push(vec![
                Value::Text(family.to_string()),
                Value::Int(shard as i64),
                Value::Int(n_shards as i64),
                Value::Text("pending".to_string()),
                Value::Null,
                Value::Null,
            ]);
            jobs.push(ColdJob { family: family.to_string(), shard, priority });
        };
        // scip first (feeds the call/type resolution ladder), above every family.
        if self.cold_pre_extract_used(prog) {
            push(COLD_SCIP_FAMILY, 0, 1, (count + 1) as i64, &mut rows, &mut jobs);
        }
        for (idx, fam) in families.iter().enumerate() {
            let n_shards = self.cold_shard_count(fam.name(), fam.shardable_cold())?;
            let priority = self.cold_family_priority(idx, count);
            for shard in 0..n_shards {
                push(fam.name(), shard, n_shards, priority, &mut rows, &mut jobs);
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
        let found = self.db.query_rows(
            "_cold_node",
            "SELECT family, shard FROM _cold_node WHERE state != 'done'",
            &[],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u32)),
        )?;
        for (family, shard) in found {
            // scip stays above every family (mirrors seed); a node whose family is
            // no longer used (program shrank mid-start) gets the lowest priority;
            // it will be swept at declare time.
            let priority = if family == COLD_SCIP_FAMILY {
                (count + 1) as i64
            } else {
                rank.get(family.as_str())
                    .map(|idx| self.cold_family_priority(*idx, count))
                    .unwrap_or(0)
            };
            jobs.push(ColdJob { family, shard, priority });
        }
        jobs.sort_by(|a, b| b.priority.cmp(&a.priority));
        Ok(jobs)
    }

    /// Every seeded node is `done`? Gate for the completion tick's first full
    /// derived rebuild.
    pub fn cold_nodes_complete(&self) -> Result<bool> {
        self.ensure_cold_schema()?;
        let (total, pending): (i64, i64) = self.db.query_one(
            "_cold_node",
            "SELECT COUNT(*), COALESCE(SUM(state != 'done'), 0) FROM _cold_node",
            &[],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok(total > 0 && pending == 0)
    }

    /// Run ONE cold node `(family, shard)`, then mark exactly that row `done`.
    /// Runs under the engine mutex like any tick. Idempotent — an already-`done`
    /// node is a no-op; a re-run after a crash re-does its work (wholesale refresh
    /// = wipe+repopulate; chunk append = `INSERT OR IGNORE`). The mark runs only
    /// AFTER the write commits, so a `done` node always has fully-written rows; a
    /// crash before the mark leaves the node `pending` for an idempotent redo.
    ///
    /// Dispatch:
    /// - `scip-index` → the `pre_extract` RelKind refresh (the SCIP index load).
    /// - a shardable (barrier-free) family → `refresh_<family>_slice(chunk)` over
    ///   the `shard`-th byte-bounded slice, an `INSERT OR IGNORE` append; when the
    ///   family's LAST shard lands, its `extract:` digest is saved so the
    ///   completion tick skips the already-written family.
    /// - else a wholesale family → `fam.refresh` (full corpus, saves its own digest).
    pub fn run_cold_node(&mut self, prog: &Program, family: &str, shard: u32) -> Result<()> {
        self.ensure_cold_schema()?;
        let already_done: i64 = self
            .db
            .query_one(
                "_cold_node",
                "SELECT COUNT(*) FROM _cold_node WHERE family=?1 AND shard=?2 AND state='done'",
                &[SqlVal::Text(family.into()), SqlVal::Int(shard as i64)],
                |r| Ok(r.get(0)?),
            )
            .unwrap_or(0);
        if already_done > 0 {
            return Ok(());
        }
        self.db.tick_begin();
        self.db.clear_write_ledger();
        if family == COLD_SCIP_FAMILY {
            // The SCIP index load (and any other pre_extract RelKind). Its rows
            // feed the call/type resolution the later family nodes run.
            for k in crate::rels::rel_kinds() {
                if k.pre_extract() && k.used(prog) {
                    k.refresh(self)?;
                }
            }
        } else {
            let Some(fam) = crate::rels::extract_families_pre_node()
                .iter()
                .copied()
                .find(|fam| fam.name() == family)
            else {
                bail!("cold-start: unknown extract family '{family}'");
            };
            // Re-prime the per-file analysis caches (idempotent — cached) so a
            // type/call node parses through the same bundle the inline path used.
            super::extract::prime_analysis_bundles(self, prog)?;
            if fam.shardable_cold() {
                let files = self.cold_family_files(family)?;
                let slices = self.cold_chunk_slices_of(files)?;
                let slice = slices.get(shard as usize).cloned().unwrap_or_default();
                tracing::debug!(
                    family, shard, slice_files = slice.len(), total_slices = slices.len(),
                    "[cold-chunk] running chunk"
                );
                match family {
                    "dataflow-rels" => self.refresh_dataflow_rels_slice(&slice)?,
                    "comment-rels" => self.refresh_comment_rels_slice(&slice)?,
                    "template-rels" => self.refresh_template_rels_slice(&slice)?,
                    "unresolved-rels" => self.refresh_unresolved_rel_slice(&slice)?,
                    other => bail!("cold-chunk: family '{other}' is shardable but has no slice refresh"),
                }
            } else {
                fam.refresh(self)?;
            }
        }
        let now = super::now_secs();
        self.db.exec_params(
            "_cold_node",
            "UPDATE _cold_node SET state='done', done_at=?3 WHERE family=?1 AND shard=?2",
            &[SqlVal::Text(family.into()), SqlVal::Int(shard as i64), SqlVal::Int(now)],
        )?;
        // Cold-start progress is always see-able: one verdict line per node
        // (stderr + perf.jsonl), N/M cumulative.
        {
            let (done, total): (i64, i64) = self.db.query_one(
                "_cold_node",
                "SELECT SUM(state='done'), COUNT(*) FROM _cold_node",
                &[],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            crate::verdict::verdict(
                "cold-progress",
                &format!("{done}/{total} nodes done"),
                &[("family", family), ("shard", &shard.to_string())],
            );
        }
        // Completion gate for a chunked family: once every shard is done, save its
        // `extract:` digest so the completion tick's wholesale refresh skips it.
        if family != COLD_SCIP_FAMILY {
            let pending: i64 = self
                .db
                .query_one(
                    "_cold_node",
                    "SELECT COUNT(*) FROM _cold_node WHERE family=?1 AND state != 'done'",
                    &[SqlVal::Text(family.into())],
                    |r| Ok(r.get(0)?),
                )
                .unwrap_or(0);
            if pending == 0 {
                match family {
                    "dataflow-rels" => self.save_dataflow_cold_digest()?,
                    "comment-rels" => self.save_comment_cold_digest()?,
                    "template-rels" => self.save_template_cold_digest()?,
                    "unresolved-rels" => self.save_unresolved_cold_digest()?,
                    _ => {}
                }
            }
        }
        self.last_n1 = self.db.tick_end();
        Ok(())
    }

    /// Declare-time sweep: drop `_cold_node` rows for families no longer used
    /// (mirrors the `_derived_complete` cleanup). Only runs once the table
    /// exists; a fresh db has nothing to sweep.
    pub(crate) fn sweep_cold_nodes(&self, prog: &Program) -> Result<()> {
        let exists: i64 = self
            .db
            .query_one(
                "sqlite_master",
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='_cold_node'",
                &[],
                |r| Ok(r.get(0)?),
            )
            .unwrap_or(0);
        if exists == 0 {
            return Ok(());
        }
        let mut used: std::collections::HashSet<&'static str> =
            self.cold_families(prog).iter().map(|fam| fam.name()).collect();
        // The scip-index node is not an extract family but is a valid cold node.
        if self.cold_pre_extract_used(prog) {
            used.insert(COLD_SCIP_FAMILY);
        }
        let stored: Vec<String> = self.db.query_rows(
            "_cold_node",
            "SELECT DISTINCT family FROM _cold_node",
            &[],
            |r| Ok(r.get::<_, String>(0)?),
        )?;
        for family in stored {
            if !used.contains(family.as_str()) {
                self.db.exec_params(
                    "_cold_node",
                    "DELETE FROM _cold_node WHERE family=?1",
                    &[SqlVal::Text(family)],
                )?;
            }
        }
        Ok(())
    }
}
