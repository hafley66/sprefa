use super::*;
use crate::lower::txt_tbl;

impl Engine {
    /// Distinct WORK source paths from the `_file` cache. Feeds crate-root
    /// discovery (`rspath::crate_roots`) for the `--move` rewriter, so a crate
    /// whose root is `rust/kernel/lib.rs` (no `src/`) still yields module paths.
    pub fn source_paths(&self) -> Result<Vec<String>> {
        let conn = self.db.conn();
        let mut s = conn.prepare("SELECT DISTINCT path FROM _file WHERE rev = 'WORK'")?;
        let rows = s.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(|x| x.ok()).collect())
    }

    /// Row count of a relation's backing table (`rel_<name>`). Test/bench
    /// instrumentation; returns 0 when the table is empty, errors if absent.
    pub fn count_rows(&self, rel: &str) -> Result<i64> {
        let conn = self.db.conn();
        Ok(conn.query_row(&format!("SELECT COUNT(*) FROM {}", txt_tbl(rel)), [], |r| r.get(0))?)
    }

    pub fn query_sql(&self, sql: &str, params: &[serde_json::Value]) -> Result<Vec<Vec<serde_json::Value>>> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(sql)?;
        let col_count = stmt.column_count();
        let param_vals: Vec<rusqlite::types::Value> = params.iter().map(|v| match v {
            serde_json::Value::String(s) => rusqlite::types::Value::Text(s.clone()),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() { rusqlite::types::Value::Integer(i) }
                else if let Some(f) = n.as_f64() { rusqlite::types::Value::Real(f) }
                else { rusqlite::types::Value::Null }
            }
            serde_json::Value::Null => rusqlite::types::Value::Null,
            _ => rusqlite::types::Value::Text(v.to_string()),
        }).collect();
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = param_vals.iter()
            .map(|v| v as &dyn rusqlite::types::ToSql).collect();
        let rows_iter = stmt.query_map(param_refs.as_slice(), |row| {
            let mut vals: Vec<serde_json::Value> = Vec::new();
            for i in 0..col_count {
                let v: rusqlite::types::Value = row.get_unwrap(i);
                vals.push(match v {
                    rusqlite::types::Value::Null => serde_json::Value::Null,
                    rusqlite::types::Value::Integer(n) => serde_json::json!(n),
                    rusqlite::types::Value::Real(f) => serde_json::json!(f),
                    rusqlite::types::Value::Text(s) => serde_json::json!(s),
                    rusqlite::types::Value::Blob(b) => serde_json::json!(format!("<blob {}B>", b.len())),
                });
            }
            Ok(vals)
        })?;
        Ok(rows_iter.filter_map(|r| r.ok()).collect())
    }

    /// (file, specifier) for every `use`/`import` row in `module_import`. The
    /// specifier is the resolver's synthesized full path (brace leaves expanded),
    /// which the refactor sink uses to detect imports it cannot yet splice (a
    /// brace leaf's located span covers the leaf name, not the full path).
    pub fn module_imports(&self) -> Result<Vec<(String, String)>> {
        let conn = self.db.conn();
        let mut s = conn.prepare(&format!(
            "SELECT \"file\", \"specifier\" FROM {} WHERE \"kind\" IN ('use', 'import')",
            txt_tbl("module_import")))?;
        let rows = s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        Ok(rows.filter_map(|x| x.ok()).collect())
    }

    /// (file, decl-name) for every Kotlin same-package implicit ref. These have
    /// no import text to rewrite, so `--move` can only count them loudly.
    pub fn same_package_uses(&self) -> Result<Vec<(String, String)>> {
        let conn = self.db.conn();
        let mut s = conn.prepare(&format!(
            "SELECT \"file\", \"specifier\" FROM {} WHERE \"kind\" = 'same-package'",
            txt_tbl("module_import")))?;
        let rows = s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        Ok(rows.filter_map(|x| x.ok()).collect())
    }

    /// Read the `diag` relation, if declared, as normalized DiagRows. Maps each
    /// row by column NAME (recognized: path, line, col, end_line, end_col,
    /// severity, msg); missing optional columns take defaults. Returns empty if
    /// the program declares no `diag` relation. Drives LSP publishDiagnostics.
    /// `only` filters to one path (the changed file) when Some.
    pub fn diags(&self, only: Option<&str>) -> Result<Vec<DiagRow>> {
        // `diag` is a fixed-schema built-in (declare_builtins), so the columns
        // and their positions are known. A rule that names only some of them
        // (via head named args) leaves the rest NULL — read NULL-tolerant and
        // apply the same defaults the old by-name reader did (severity "warn",
        // end_line = line, ints 0, empty hint = None).
        let Some(_meta) = self.rels.get("diag") else { return Ok(Vec::new()); };
        let mut sql = format!(
            "SELECT \"path\", \"line\", \"col\", \"end_line\", \"end_col\", \
             \"severity\", \"code\", \"msg\", \"hint\" FROM {}", txt_tbl("diag"));
        if only.is_some() { sql.push_str(" WHERE \"path\" = ?1"); }
        let mut stmt = self.db.conn().prepare(&sql)?;
        let map_row = |row: &rusqlite::Row| -> rusqlite::Result<DiagRow> {
            let text = |i: usize| row.get::<_, rusqlite::types::Value>(i)
                .map(|v| match v {
                    rusqlite::types::Value::Text(s) => s,
                    rusqlite::types::Value::Integer(n) => n.to_string(),
                    _ => String::new(),
                }).unwrap_or_default();
            // NULL (unnamed column) -> None, so a default can fill it.
            let int_opt = |i: usize| row.get::<_, Option<i64>>(i).ok().flatten();
            let line = int_opt(1).unwrap_or(0);
            let sev = text(5);
            Ok(DiagRow {
                path: text(0),
                line,
                col: int_opt(2).unwrap_or(0),
                end_line: int_opt(3).unwrap_or(line),
                end_col: int_opt(4).unwrap_or(0),
                severity: if sev.is_empty() { "warn".into() } else { sev },
                code: text(6),
                msg: text(7),
                hint: { let h = text(8); if h.is_empty() { None } else { Some(h) } },
            })
        };
        let mut out = Vec::new();
        let mut rows = match only {
            Some(p) => stmt.query(rusqlite::params![p])?,
            None => stmt.query([])?,
        };
        while let Some(row) = rows.next()? { out.push(map_row(row)?); }
        // Engine-structural shape diagnostics (Phase 5): not `diag`-rel rows, so
        // append them here — the single read seam --check / --lsp / the daemon
        // schema RPC all go through. Respect the `only` path filter.
        for d in &self.shape_diags {
            if only.map(|p| p == d.path).unwrap_or(true) { out.push(d.clone()); }
        }
        Ok(out)
    }

    /// The extraction type-drop diagnostics collected during the last tick (one
    /// per file+relation that lost rows). The LSP publish path merges these with
    /// the `diag` relation rows so a file whose rows were dropped shows a squiggle.
    /// File-level, line 1 (a row type-failure has no byte span). Cleared at the
    /// start of each tick.
    pub fn extraction_drops(&self) -> &[DiagRow] { &self.extraction_drops }

    /// Push a file-level drop diagnostic for `n` rows lost extracting `rel` from
    /// `path`. `path` is repo-relative (matches `DiagRow.path` and how publish
    /// joins it onto root). Collected, flushed once after the tick.
    pub(crate) fn record_extraction_drop(&mut self, path: &str, rel: &str, n: usize) {
        self.extraction_drops.push(DiagRow {
            path: path.to_string(),
            line: 1, col: 0, end_line: 1, end_col: 0,
            severity: "warn".into(),
            code: "checked-type".into(),
            msg: format!("{n} row(s) failing file/dir/path checks dropped from `{rel}`"),
            hint: None,
        });
    }
    /// Append one row to the server-request history (`_query_log`), the meta
    /// table the built-in `query_log` relation projects (`src/rels/querylog.rs`).
    /// Called once per request from the daemon's `query`/`query_sql` RPC
    /// handlers and the LSP's `dl/query` handler — a single-row insert through
    /// the plural `Db::insert_rows` seam (same shape as the `pending_effect`
    /// job queue), never a raw per-row `conn()` write. `source` is which server
    /// ("daemon"/"lsp"); `method` is the RPC/request method name; `body` is the
    /// SQL text (empty for the plain `query` RPC, which carries no SQL param);
    /// `params` is the JSON array text of bound parameters ("[]" when none).
    /// Append-only, no retention: a polling reader (the flow panel's
    /// auto-refresh) querying `query_log` logs its own read as a new row too —
    /// intentional self-noise, not a bug.


    pub fn log_query(&self, source: &str, method: &str, body: &str, params: &str) -> Result<()> {
        let row = vec![vec![
            Value::Text(iso8601_utc_now()),
            Value::Text(source.to_string()),
            Value::Text(method.to_string()),
            Value::Text(body.to_string()),
            Value::Text(params.to_string()),
        ]];
        self.db.insert_rows("_query_log", &["ts", "source", "method", "body", "params"], &row)?;
        Ok(())
    }

    /// Persist the loaded `.dl` program file set into `_program` (wipe + insert,
    /// plural seam). Each row is (path, content hash, mtime); `loaded_at` stamps
    /// the flush. Diffable on restart against the new file set. The daemon calls
    /// this on cold tick and after a hot reload.
    pub fn save_program_meta(&self, files: &[PathBuf]) -> Result<()> {
        let now = unix_secs();
        let mut rows: Vec<Vec<Value>> = Vec::with_capacity(files.len());
        for f in files {
            let (hash, mtime) = match std::fs::read(f) {
                Ok(bytes) => {
                    let mt = std::fs::metadata(f).ok().map(|m| mtime_secs(&m)).unwrap_or(0);
                    (blake3::hash(&bytes).to_hex().to_string(), mt)
                }
                Err(_) => (String::new(), 0),
            };
            rows.push(vec![
                Value::Text(f.to_string_lossy().into_owned()),
                Value::Text(hash),
                Value::Int(mtime),
                Value::Int(now),
            ]);
        }
        self.db.exec("DELETE FROM _program")?;
        self.db.insert_rows("_program", &["path", "hash", "mtime", "loaded_at"], &rows)?;
        Ok(())
    }

    /// Inject one inbound rpc request into an `@in(rpc)` port rel (the serving
    /// loop's pre-tick write). `id` is the raw JSON serialization of the
    /// request id (int or string), so it round-trips exactly. The rel must
    /// already be declared (the priming tick declares every program rel), so
    /// injection never races the schema.
    pub fn inject_rpc(&mut self, rel: &str, id: &str, method: &str, params: &str) -> Result<()> {
        if !self.rels.contains_key(rel) {
            bail!("@in(rpc) rel {rel} is not declared; run a tick before injecting");
        }
        self.insert_rel_rows(rel, &["id", "method", "params"],
            &[vec![Value::Text(id.into()), Value::Text(method.into()), Value::Text(params.into())]])?;
        Ok(())
    }

    /// Append one harness-hook event to the built-in `hook_event` rel. Rows
    /// accumulate (facts in the db, no retention sweep); the tick's content
    /// digest (`hook:hook_event`) re-derives dependents on a new row. The rel
    /// must already be declared (a priming tick declares every built-in), so the
    /// insert never races the schema.
    pub fn insert_hook_event(&mut self, kind: &str, session: &str, seq: i64, json: &str) -> Result<()> {
        if !self.rels.contains_key("hook_event") {
            bail!("hook_event rel is not declared; run a tick before feeding an event");
        }
        self.insert_rel_rows("hook_event", &["kind", "session", "seq", "json"],
            &[vec![Value::Text(kind.into()), Value::Text(session.into()),
                   Value::Int(seq), Value::Text(json.into())]])?;
        Ok(())
    }

    /// Toggle a diagnostic code in the built-in `diag_mute` set: insert the row
    /// if absent (returns `true` = now muted), delete it if present (returns
    /// `false` = now unmuted). Persisted in the db, so a mute survives a daemon
    /// restart. Written out-of-tick, never by a refresh; the LSP publish seam
    /// reads the set to drop muted `diag` rows. `--check` never consults it.
    pub fn toggle_diag_mute(&mut self, code: &str) -> Result<bool> {
        if !self.rels.contains_key("diag_mute") {
            bail!("diag_mute rel is not declared; run a tick before toggling a mute");
        }
        let already: i64 = self.db.conn().query_row(
            &format!("SELECT COUNT(*) FROM {} WHERE \"code\" = ?1", txt_tbl("diag_mute")),
            rusqlite::params![code], |r| r.get(0))?;
        if already > 0 {
            self.db.conn().execute(
                &format!("DELETE FROM {} WHERE \"code\" = sprf_sym(?1)", tbl("diag_mute")),
                rusqlite::params![code])?;
            Ok(false)
        } else {
            self.insert_rel_rows("diag_mute", &["code"], &[vec![Value::Text(code.into())]])?;
            Ok(true)
        }
    }

    /// The set of currently-muted diagnostic codes (the `diag_mute` rows). The
    /// LSP publish path filters `diag` rows against this before sending them.
    pub fn muted_codes(&self) -> Result<std::collections::HashSet<String>> {
        if !self.rels.contains_key("diag_mute") {
            return Ok(std::collections::HashSet::new());
        }
        let conn = self.db.conn();
        let mut stmt = conn.prepare(&format!("SELECT \"code\" FROM {}", txt_tbl("diag_mute")))?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(|x| x.ok()).collect())
    }

    /// Every distinct diagnostic code currently in the `diag` relation, paired
    /// with whether it is muted. Powers the editor quick-pick behind
    /// `dl.listDiagCodes`. Codes that appear only in the mute set (muted but no
    /// live finding) are included too, so a user can un-mute a code with no
    /// current occurrences.
    pub fn diag_code_states(&self) -> Result<Vec<(String, bool)>> {
        let muted = self.muted_codes()?;
        let mut codes: std::collections::BTreeSet<String> = muted.iter().cloned().collect();
        if self.rels.contains_key("diag") {
            let conn = self.db.conn();
            let mut stmt = conn.prepare(
                &format!("SELECT DISTINCT \"code\" FROM {} WHERE \"code\" IS NOT NULL AND \"code\" != ''", txt_tbl("diag")))?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            for c in rows.filter_map(|x| x.ok()) { codes.insert(c); }
        }
        Ok(codes.into_iter().map(|c| { let m = muted.contains(&c); (c, m) }).collect())
    }

    /// Drain an `@out(rpc)` port rel: return its rows, clear the table, and
    /// retire the answered rows from the paired `@in(rpc)` rel (drain law 1:
    /// every answered request row is consumed). Rows are produced by the
    /// fixpoint, pushed to the transport, deleted. The NEXT request's rebuild
    /// no longer rides "the out rel is empty" (P1 retired that signal —
    /// `derived_incomplete_rels` marks a legitimately-empty derived rel as
    /// complete, not "never derived"); instead `inject_rpc`'s write to the
    /// `@in(rpc)` rel is itself content-digested (a `port:` key in
    /// `_reldigest`, tick.rs) like `async:`/`hook:`, so the fresh request row
    /// is what re-derives the out rel's dependents.
    pub fn drain_rpc(&mut self, out_rel: &str, in_rel: &str) -> Result<Vec<(String, String)>> {
        if !self.rels.contains_key(out_rel) { return Ok(Vec::new()); }
        let rows: Vec<(String, String)> = {
            let conn = self.db.conn();
            let mut s = conn.prepare(&format!("SELECT id, result FROM {}", txt_tbl(out_rel)))?;
            let rows = s.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .filter_map(|x| x.ok()).collect();
            conn.execute(&format!("DELETE FROM {}", tbl(out_rel)), [])?;
            rows
        };
        let ids: Vec<String> = rows.iter().map(|(id, _)| id.clone()).collect();
        self.retire_rpc(in_rel, &ids)?;
        Ok(rows)
    }

    /// Delete the given request ids from an `@in(rpc)` rel (answered, or given
    /// up on). One batched DELETE, not per-row.
    pub fn retire_rpc(&mut self, in_rel: &str, ids: &[String]) -> Result<()> {
        if ids.is_empty() || !self.rels.contains_key(in_rel) { return Ok(()); }
        let interned_id = self.rels.get(in_rel)
            .and_then(|meta| meta.cols.first())
            .is_some_and(|col| col.interned());
        let ph = (1..=ids.len()).map(|i| {
            if interned_id { format!("sprf_sym(?{i})") } else { format!("?{i}") }
        }).collect::<Vec<_>>().join(",");
        self.db.conn().execute(
            &format!("DELETE FROM {} WHERE id IN ({ph})", tbl(in_rel)),
            rusqlite::params_from_iter(ids))?;
        Ok(())
    }

    // (cell_as_string lives at module scope, just above this impl block, so
    // every generic row reader — rel_rows, load_edges, edge_content_digest —
    // shares one stringify path across TEXT and INTEGER (sym) columns.)

    /// Read a relation's table as positional String rows (test/diagnostic).
    /// Returns empty if the relation isn't declared.
    pub fn rel_rows(&self, rel: &str, ncols: usize) -> Vec<Vec<String>> {
        if !self.rels.contains_key(rel) { return Vec::new(); }
        let conn = self.db.conn();
        let mut s = match conn.prepare(&format!("SELECT * FROM {}", txt_tbl(rel))) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = s.query_map([], |r| {
            let mut v = Vec::with_capacity(ncols);
            for i in 0..ncols {
                // Stringify whatever the column holds: an int column read as
                // String is a rusqlite type error, which would silently drop
                // the whole row from a diagnostic read.
                v.push(cell_as_string(r, i)?);
            }
            Ok(v)
        });
        rows.map(|iter| iter.filter_map(|x| x.ok()).collect()).unwrap_or_default()
    }

    /// The query-facing `repo` relation (slug, root, url) as it stood after the
    /// last tick's `refresh_builtin_rels` — the union of config and dynamically
    /// pulled repos whose root exists. Diagnostics/tests.
    pub fn repo_relation(&self) -> Vec<(String, String, String)> {
        let conn = self.db.conn();
        let mut s = match conn.prepare(&format!("SELECT slug, root, url FROM {} ORDER BY slug", txt_tbl("repo"))) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = s.query_map([], |r|
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)));
        rows.ok()
            .map(|iter| iter.filter_map(|x| x.ok()).collect())
            .unwrap_or_default()
    }

    /// Drain the NETWORK/MUTATING sinks (`repo` pulls + `checkout` sweeps) AFTER
    /// the read-only fixpoint + query + gens ran in `tick_report`. This is the
    /// half of the tick that hits the network and rewrites checkouts, so it is
    /// split out from `tick_report` to keep read paths (`?` queries, `--check`,
    /// LSP, MCP) pure: a query must never trigger a 90s destructive sweep.
    ///
    /// The daemon's poll loop calls this off-tick on its cadence; one-shot CLI
    /// runs opt in via `--apply` / `DL_APPLY_SINKS=1` (so `dl prog.dl` on a
    /// gh-checkout program is a read by default and surfaces nothing new
    /// unless the operator opted in). `DL_CHECKOUT_DRY_RUN=1` previews the
    /// checkout sweep as `checkout_plan` rows without mutating anything.
    /// Returns the number of sink rows that landed (repos pulled + checkout
    /// outcomes), so a settle loop knows whether to re-tick to derive from them.
    pub fn drain_external_sinks(&mut self, prog: &Program) -> Result<usize> {
        use crate::ast::Item;
        let rules: Vec<&Rule> = prog.items.iter().filter_map(|i| match i { Item::Rule(r) => Some(r), _ => None }).collect();
        let repo_sinks: Vec<&Rule> = rules.iter().copied().filter(|r| r.is_repo_sink()).collect();
        // Validate repo-sink shape here (not in tick_report) so a read-only tick
        // does not pay it, and a malformed sink only bails when something would
        // actually try to drain it.
        for r in &repo_sinks {
            if r.is_source() {
                bail!("repo-sink rule must be derived-style (no scan/match/ast/...); \
                       its body is compiled as a SELECT over already-derived relations");
            }
        }
        // Repo pulls first: a pull clones + registers into self.repos; the new
        // repo is scannable / appears in the `repo` builtin on the NEXT tick
        // (mid-tick registration would shift the repo set under derived rules).
        let repos_before = self.repos.len();
        self.run_repo_pulls(&repo_sinks)?;
        let repos_pulled = self.repos.len().saturating_sub(repos_before);
        // Checkout sweeps after the pull: this tick's derived
        // `checkout(repo, branch, pr_heads)` rows keep each named repo's
        // checkout current (fetch + non-destructive fast-forward).
        let mut sink_rows = repos_pulled;
        if rules.iter().any(|r| r.is_checkout_sink()) {
            let outcomes_before = self.checkout_outcome_count()?;
            self.run_checkout_sweeps()?;
            sink_rows += self.checkout_outcome_count()?.saturating_sub(outcomes_before);
        }
        Ok(sink_rows)
    }

}
