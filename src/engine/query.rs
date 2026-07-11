use super::*;

impl Engine {
    pub(crate) fn run_query(&self, q: &Query, closures: &HashMap<String, String>) -> Result<()> {
        // Seeded Rust path on a closure head: src pinned + dst free is a forward
        // walk (callees); dst pinned + src free is a reverse walk (callers).
        // Both-pinned, both-free, or anything else falls through to the SQL view.
        if let Some(edge) = closures.get(&q.head.rel) {
            if q.head.terms.len() == 2 {
                if let Some(cc) = self.closure_cache.get(edge) {
                    match (pinned_value(q, 0), pinned_value(q, 1)) {
                        (Some(seed), None) if matches!(q.head.terms[1], Term::Var(_)) =>
                            return self.run_reaches_point(q, cc, &seed, true),
                        (None, Some(seed)) if matches!(q.head.terms[0], Term::Var(_)) =>
                            return self.run_reaches_point(q, cc, &seed, false),
                        // Both pinned: an existence probe. The view pays the full
                        // materialization for it (constraints don't push into the
                        // recursive CTE), the condensation walk answers directly.
                        (Some(src), Some(dst)) => return self.run_reaches_pair(q, cc, &src, &dst),
                        _ => {}
                    }
                }
            }
            // Anything still here evaluates the closure VIEW = materializing full
            // reachability (a LIMIT does not short-circuit it). Refuse loudly on a
            // big edge rel instead of hanging the tick.
            let cap = closure_query_max_edges();
            if cap > 0 {
                let n: i64 = self.db.conn().query_row(
                    &format!("SELECT COUNT(*) FROM {}", tbl(edge)), [], |r| r.get(0))?;
                if n as usize > cap {
                    bail!("closure query `{0}` would evaluate the reachability view over edge \
                           rel `{edge}` ({n} rows > cap {cap}), which is unbounded on a graph \
                           this dense; pin an endpoint (e.g. `? {0}(\"seed\", x)`) for the \
                           seeded fast path, or raise/disable the guard with \
                           DL_CLOSURE_QUERY_MAX_EDGES=<n|0>", q.head.rel);
                }
            }
        }
        let res = self.query_one_sql(q)?;
        self.print_query_result(&res);
        Ok(())
    }

    fn print_query_result(&self, res: &QueryResult) {
        if self.query_json {
            emit_query_json(&res.rel, &res.columns, &res.rows);
        } else {
            println!("? {} => {}", res.rel, if res.columns.is_empty() { "(count)".into() } else { res.columns.join("\t") });
            for cells in &res.rows {
                println!("  {}", cells.iter().map(json_cell_tsv).collect::<Vec<_>>().join("\t"));
            }
            println!("  ({} rows)\n", res.rows.len());
        }
    }

    /// Run one `?` query through the SQL view path only (no closure-cache
    /// optimization). Used by the daemon RPC `query` path, which needs the
    /// rows without capturing stdout.
    fn query_one_sql(&self, q: &Query) -> Result<QueryResult> {
        let (sql, columns) = lower_query(q, &self.rels)?;
        let mut stmt = self.db.conn().prepare(&sql)?;
        let ncols = stmt.column_count();
        let mut rows = stmt.query([])?;
        let mut out: Vec<Vec<serde_json::Value>> = Vec::new();
        while let Some(row) = rows.next()? {
            let cells = (0..ncols)
                .map(|i| sqlite_to_json(row.get::<_, rusqlite::types::Value>(i).unwrap_or(rusqlite::types::Value::Null)))
                .collect();
            out.push(cells);
        }
        Ok(QueryResult { rel: q.head.rel.clone(), columns, rows: out })
    }

    /// Run every `?` query in `prog`, returning rows. Used by the daemon RPC
    /// `query` (the foreground path goes through `run_query` which prints). The
    /// closure-cache optimization is skipped; the SQL view is always used, so
    /// results are correct but a path query on a large graph pays the SQL cost.
    pub fn run_queries_capture(&self, prog: &Program) -> Result<Vec<QueryResult>> {
        let mut out = Vec::new();
        for item in &prog.items {
            if let Item::Query(q) = item { out.push(self.query_one_sql(q)?); }
        }
        Ok(out)
    }

    /// Run every `gen` item after the fixpoint: evaluate the body, render the
    /// templates, write the targets. A write is skipped when the target bytes
    /// Arm the verify-rollback journal (christmas #14): subsequent gen writes
    /// stash each target's pre-tick bytes so `rollback_writes` can undo them if a
    /// checker rejects the edit. Call before `tick`/`run`.
    pub fn begin_verify(&self) {
        *self.gen_journal.borrow_mut() = Some(Vec::new());
    }

    /// Restore every file a gen write touched since `begin_verify` to its
    /// pre-tick bytes (deleting files that did not exist before). Returns the
    /// number of paths restored and disarms the journal. Used when a verify
    /// checker fails: keep-if-pass means roll-back-if-fail.
    pub fn rollback_writes(&self) -> Result<usize> {
        let journal = self.gen_journal.borrow_mut().take().unwrap_or_default();
        let n = journal.len();
        for (path, original) in journal.into_iter().rev() {
            let full = self.root.join(&path);
            match original {
                Some(bytes) => std::fs::write(&full, &bytes)?,
                None => { let _ = std::fs::remove_file(&full); }
            }
        }
        Ok(n)
    }

    /// Disarm the journal without restoring (keep the writes). Returns how many
    /// paths were written under verify. Used when the checker passes.
    pub fn commit_writes(&self) -> usize {
        self.gen_journal.borrow_mut().take().map_or(0, |j| j.len())
    }

    /// `std::fs::write`, but when the verify journal is armed, stash the target's
    /// original bytes first (first write per path wins, so the stash is always
    /// the pre-tick state). All gen apply paths route writes through here.
    pub(crate) fn journaled_write(&self, full: &Path, rel: &str, bytes: &[u8]) -> Result<()> {
        if let Some(journal) = self.gen_journal.borrow_mut().as_mut() {
            if !journal.iter().any(|(p, _)| p == rel) {
                journal.push((rel.to_string(), std::fs::read(full).ok()));
            }
        }
        std::fs::write(full, bytes)?;
        Ok(())
    }
}

fn sqlite_to_json(v: rusqlite::types::Value) -> serde_json::Value {
    use rusqlite::types::Value as V;
    match v {
        V::Text(s) => serde_json::Value::String(s),
        V::Integer(n) => serde_json::Value::from(n),
        V::Real(f) => serde_json::Value::from(f),
        _ => serde_json::Value::Null,
    }
}

/// Render a JSON cell for the human TSV block: strings raw (no quotes), numbers
/// as their text, null empty — matching the pre-JSON behavior exactly.
fn json_cell_tsv(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// One JSON object per query (JSON-lines), so a program's multiple `?` queries
/// stream as independent records a tool can read line by line.
pub(crate) fn emit_query_json(rel: &str, columns: &[String], rows: &[Vec<serde_json::Value>]) {
    let obj = serde_json::json!({
        "query": rel,
        "columns": columns,
        "rows": rows,
        "count": rows.len(),
    });
    println!("{obj}");
}
