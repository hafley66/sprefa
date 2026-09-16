//! Term-form extraction (split out of `reconcile.rs` in the file-budget
//! decomp, 2026-07-18): the hybrid join+extract pass for `json`/`jsonp`/`sg`
//! over a BOUND STRING (a response body, a column) rather than a file. Runs
//! inside the source pass, after sources/responses are present and before the
//! derived fixpoint.

use super::*;

impl Engine {
    // ARCH {"url":"engine/20-reconcile","role":"term-extract"}
    /// Evaluate the TERM-form `json`/`jsonp` rules — the hybrid join+extract. A
    /// rule like `star(repo,n) <- page(repo,200,_,body), jsonp(body,"stars",n).`
    /// joins relations in SQL (binding the content var `body`), then runs the
    /// tree-sitter extractor over each joined row's bound string, fanning the
    /// extracted bindings into head rows. This is the only path that parses a
    /// value held in a relation (a response body, a column) rather than a file.
    /// Runs after sources/responses are present and before the derived fixpoint
    /// (so derived rules see the output). Returns whether any head rel changed,
    /// which the caller ORs into the rebuild gate.
    ///
    /// @recompute unguarded: re-runs each tick — its inputs (response/source rels)
    /// move off the file-source-digest path, so a digest skip here would miss a
    /// freshly-drained body. The join is bounded by the read relations (the
    /// response/page set), not the repo; the downstream rebuild is gated on the
    /// returned changed flag, so a steady state does not re-run the fixpoint.
    pub(crate) fn eval_extract_rules(&self, extract_rules: &[&Rule]) -> Result<bool> {
        if extract_rules.is_empty() {
            return Ok(false);
        }
        let mut heads: Vec<String> = Vec::new();
        for r in extract_rules {
            if !heads.contains(&r.head.rel) {
                heads.push(r.head.rel.clone());
            }
        }
        let mut any_changed = false;
        // Per-head-rel wall time (parse/extract + before/after row-set diff +
        // the DELETE/insert flush) into `_stmt_ms` under `extract:<rel>` — this
        // hybrid join+extract pass (term-form json/jsonp/sg) ran untimed before,
        // part of the derived-phase attribution gap.
        let mut stmt_ms: HashMap<String, (i64, i64)> = HashMap::new();
        for head_rel in &heads {
            let t = std::time::Instant::now();
            let cols: Vec<String> = {
                let meta = self.rels.get(head_rel).ok_or_else(|| {
                    anyhow::anyhow!("term-extract head rel `{head_rel}` is not declared")
                })?;
                meta.cols.iter().map(|c| c.name.clone()).collect()
            };
            let mut rows: Vec<Vec<Value>> = Vec::new();
            for r in extract_rules.iter().filter(|r| &r.head.rel == head_rel) {
                self.extract_rule_rows(r, &mut rows)?;
            }
            let col_refs: Vec<&str> = cols.iter().map(|s| s.as_str()).collect();
            let encoded_rows = self.encode_rel_rows(head_rel, &col_refs, &rows)?;
            // Changed iff the head row SET differs from what is stored (sorted
            // compare): only then does the downstream fixpoint need to re-run.
            // `Value` is not Ord/Eq; compare the row SETS via a string projection.
            let key = |row: &[Value]| -> Vec<String> {
                row.iter()
                    .map(|v| match v {
                        Value::Int(n) => format!("i{n}"),
                        Value::Text(s) => format!("t{s}"),
                        Value::Null => "n".to_string(),
                    })
                    .collect()
            };
            let mut before: Vec<Vec<String>> = {
                // Project the DECLARED columns only: `SELECT *` also returns the
                // `__src` bookkeeping column every rel table carries, which the
                // extracted rows below never have — the arity mismatch made this
                // set compare report "changed" on every tick a head had rows,
                // and the caller then forced a whole-program derived rebuild
                // (the 2026-07-18 271-rel full-wipe-per-tick storm).
                let quoted: Vec<String> = cols.iter().map(|c| format!("\"{c}\"")).collect();
                let sql = format!("SELECT {} FROM {}", quoted.join(", "), tbl(head_rel));
                self.db
                    .query_values(&tbl(head_rel), &sql, &[])?
                    .into_iter()
                    .map(|row| {
                        row.into_iter()
                            .map(|cell| match cell {
                                crate::db::SqlVal::Int(x) => format!("i{x}"),
                                crate::db::SqlVal::Text(s) => format!("t{s}"),
                                // Must match `key`'s Value::Null arm ("n"); the
                                // old "t" encoding made any NULL cell read as a
                                // permanent before/after diff.
                                crate::db::SqlVal::Null => "n".to_string(),
                                other => format!("{other:?}"),
                            })
                            .collect()
                    })
                    .collect()
            };
            let mut after: Vec<Vec<String>> = encoded_rows.iter().map(|r| key(r)).collect();
            before.sort();
            after.sort();
            if before != after {
                any_changed = true;
            }
            self.db
                .exec_on(&tbl(head_rel), &format!("DELETE FROM {}", tbl(head_rel)))?;
            self.db
                .insert_rows(&tbl(head_rel), &col_refs, &encoded_rows)?;
            stmt_ms.insert(
                format!("extract:{head_rel}"),
                (t.elapsed().as_millis() as i64, 1),
            );
        }
        self.save_stmt_ms(&stmt_ms)?;
        Ok(any_changed)
    }

    /// One term-extract rule: project the relational join to bind the content var,
    /// then fan the extractor (`run_data` for jsonp, `run_pattern` for json) over
    /// each joined row's bound string into head rows. Cmps over both join vars AND
    /// the extracted vars are post-filtered with `eval_cmp`.
    pub(crate) fn extract_rule_rows(&self, r: &Rule, out_rows: &mut Vec<Vec<Value>>) -> Result<()> {
        let extracts: Vec<&BodyItem> = r
            .body
            .iter()
            .filter(|b| {
                matches!(
                    b,
                    BodyItem::JsonP { rev: None, .. }
                        | BodyItem::Json { rev: None, .. }
                        | BodyItem::Sg { rev: None, .. }
                )
            })
            .collect();
        if extracts.len() != 1 {
            bail!(
                "rule `{}`: a term-form json/jsonp/sg rule must have exactly one extract op \
                   (split a multi-extract rule into chained rules)",
                r.head.rel
            );
        }
        let cmps: Vec<&Constraint> = r
            .body
            .iter()
            .filter_map(|b| {
                if let BodyItem::Cmp(c) = b {
                    Some(c)
                } else {
                    None
                }
            })
            .collect();
        // The relational join binds the content var (and the head's join vars).
        let vars = async_bound_vars(r);
        if vars.is_empty() {
            bail!(
                "rule `{}`: a term-extract rule needs a positive atom binding the content var",
                r.head.rel
            );
        }
        let mut body = r.body.clone();
        crate::lower::resolve_work_alias_body(&mut body, &self.rels, &self.self_rev_text());
        let sql = crate::lower::lower_body_projection(&body, &self.rels, &vars)?;
        let join_rows: Vec<Bind> = {
            let rel = crate::lower::tbl(&r.head.rel);
            self.db
                .query_values(&rel, &sql, &[])?
                .into_iter()
                .map(|row| {
                    let mut b: Bind = HashMap::new();
                    for (i, v) in vars.iter().enumerate() {
                        let val = match row.get(i) {
                            Some(crate::db::SqlVal::Int(x)) => Value::Int(*x),
                            Some(crate::db::SqlVal::Text(s)) => Value::Text(s.clone()),
                            Some(crate::db::SqlVal::Null) => Value::Text(String::new()),
                            other => Value::Text(format!("{other:?}")),
                        };
                        b.insert(v.clone(), val);
                    }
                    b
                })
                .collect()
        };
        // A term source has no extension to dispatch on (response bodies are
        // json); the synthetic name routes `run_data`/`run_pattern` to the json
        // walker. yaml/toml-in-a-string is not supported (v1).
        let synth = "_.json";
        let emit = |env: &Bind, out: &mut Vec<Vec<Value>>| -> Result<()> {
            for c in &cmps {
                if !eval_cmp(c, env)? {
                    return Ok(());
                }
            }
            let mut row = Vec::with_capacity(r.head.terms.len());
            for t in &r.head.terms {
                row.push(val_of(t, env)?);
            }
            out.push(row);
            Ok(())
        };
        match extracts[0] {
            BodyItem::JsonP {
                src,
                jpath,
                out,
                id,
                ..
            } => {
                let srcvar = var_of(src)?;
                let outvar = var_of(out)?;
                if id.is_some() {
                    bail!(
                        "rule `{}`: a term-form jsonp has no file to locate — drop the `id` arg",
                        r.head.rel
                    );
                }
                for jr in &join_rows {
                    let content = match jr.get(&srcvar) {
                        Some(Value::Text(s)) => s.clone(),
                        Some(Value::Int(n)) => n.to_string(),
                        _ => continue,
                    };
                    for (v, _lo, _hi) in crate::datapath::run_data(synth, &content, jpath) {
                        let mut env = jr.clone();
                        env.insert(outvar.clone(), Value::Text(v));
                        emit(&env, out_rows)?;
                    }
                }
            }
            BodyItem::Json { src, pat, .. } => {
                let srcvar = var_of(src)?;
                let (steps, _) = crate::datapath::parse_pattern(pat)
                    .map_err(|e| anyhow::anyhow!("json pattern error: {e}"))?;
                for jr in &join_rows {
                    let content = match jr.get(&srcvar) {
                        Some(Value::Text(s)) => s.clone(),
                        Some(Value::Int(n)) => n.to_string(),
                        _ => continue,
                    };
                    for m in crate::datapath::run_pattern(synth, &content, &steps) {
                        let mut env = jr.clone();
                        for (cap, text, _lo, _hi) in m {
                            env.insert(cap, Value::Text(text));
                        }
                        emit(&env, out_rows)?;
                    }
                }
            }
            // Term-form `sg(:lang, src, "pat", line, col, end_line, end_col)`:
            // run the ast-grep pattern over each joined row's bound string. Metavar
            // captures bind by name (like the file form); the span outputs bind the
            // match's line/col RELATIVE to the bound string (byte 0 = start of the
            // value). No file, no located id — the caller adds the enclosing
            // region's own line to reach file coordinates.
            BodyItem::Sg {
                src,
                lang,
                pattern,
                line,
                col,
                end_line,
                end_col,
                ..
            } => {
                let srcvar = var_of(src)?;
                let slv = opt_var(line)?;
                let clv = opt_var(col)?;
                let ellv = opt_var(end_line)?;
                let eclv = opt_var(end_col)?;
                for jr in &join_rows {
                    let content = match jr.get(&srcvar) {
                        Some(Value::Text(s)) => s.clone(),
                        Some(Value::Int(n)) => n.to_string(),
                        _ => continue,
                    };
                    for (ln, c, eln, ec, _mlo, _mhi, caps) in
                        crate::sg::run_sg(&content, lang, pattern)?
                    {
                        let mut env = jr.clone();
                        if let Some(v) = &slv {
                            env.insert(v.clone(), Value::Int(ln));
                        }
                        if let Some(v) = &clv {
                            env.insert(v.clone(), Value::Int(c));
                        }
                        if let Some(v) = &ellv {
                            env.insert(v.clone(), Value::Int(eln));
                        }
                        if let Some(v) = &eclv {
                            env.insert(v.clone(), Value::Int(ec));
                        }
                        for (name, text, _lo, _hi) in caps {
                            env.insert(name, Value::Text(text));
                        }
                        emit(&env, out_rows)?;
                    }
                }
            }
            _ => unreachable!("extracts filtered to JsonP/Json/Sg"),
        }
        Ok(())
    }
}
