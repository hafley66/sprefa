use super::*;

impl Engine {
    /// Intra-procedural dataflow lift over the corpus. Each `.rs/.kt/.kts/.ts/.tsx`
    /// file in `_file` is parsed once by the matching front-end's
    /// `extract_dataflow`; nodes and edges are corpus-deduped by id (the
    /// `file:line:col` start span is already unique across files). No resolution
    /// pass is needed — node ids and the enclosing `fn` sym are self-contained,
    /// so this is a straight extract + bulk write.
    /// Change-reporting contract mirrors `refresh_type_rels`.
    pub(crate) fn refresh_dataflow_rels(&self) -> Result<bool> {
        let files = self.extract_file_set()?;
        // Perf gap A, per rev: no resolution pass here, so each rev's digest
        // folds its corpus rows only (no scip term). Empty `moved` = skip. The
        // df rels gain `_rev` twins in D5.4; today they stay whole-table
        // `refresh_rel` writes (single-rev daemon sees today's behavior).
        let moved = self.moved_extract_revs("dataflow", &files, false)?;
        if moved.is_empty() { return Ok(false); }

        let root = self.root.clone();
        // Read each file from its OWN repo root (same as type/call), so a config
        // repo's WORK content lifts too; reading everything from `self.root`
        // stranded config-repo files at self-root/path (missing -> empty -> zero
        // df rows) or a git blob, never their working tree. The derived repo id
        // (nearest `.git` basename, like type/call) rides each fact so df_node_repo
        // can attribute every node to the folder it lives in.
        let roots = self.repo_roots();
        let facts: Vec<(String, String, String, Arc<typegraph::DataflowFacts>)> =
            cached_facts_profiled(&self.df_facts_cache, &files, &self.extract_files_parsed, "dataflow", |repo, path, rev| {
                let lang = typegraph::type_langs().iter().find(|l| l.matches(path))?;
                let froot = roots.get(repo).map(|p| p.as_path()).unwrap_or(&root);
                let content = read_content(froot, rev, path).unwrap_or_default();
                let rid = repo_id_of(froot, path, repo);
                Some((rid, lang.extract_dataflow(path, &content)))
            });

        let t = |s: &str| Value::Text(s.to_string());
        let i = |n: u32| Value::Int(n as i64);
        // Opaque id-handle columns (df_node.id and every column that carries one:
        // df_edge.from/to, df_arg.call/arg, df_field.id/value, df_param.id,
        // df_lit.id, df_node_repo.id, and the rev-salted twins) intern through
        // one `SymSink` so joins on them are int compares. The text these ids
        // are BUILT from (fn_sym, var, kind, file, ...) stays plain text — only
        // the opaque handle itself interns. `Db::flush_syms` drains the sink
        // into ONE batched `_strings` insert below, so `sym(id)` decodes.
        let mut sink = spine::SymSink::new();
        let mut sym = |s: &str| Value::Int(sink.sym(s).cell());
        let mut node_rows: Vec<Vec<Value>> = Vec::new();
        let mut node_repo_rows: Vec<Vec<Value>> = Vec::new();
        let mut edge_rows: Vec<Vec<Value>> = Vec::new();
        let mut loop_rows: Vec<Vec<Value>> = Vec::new();
        let mut alloc_rows: Vec<Vec<Value>> = Vec::new();
        let mut nest_rows: Vec<Vec<Value>> = Vec::new();
        let mut param_rows: Vec<Vec<Value>> = Vec::new();
        let mut arg_rows: Vec<Vec<Value>> = Vec::new();
        let mut field_rows: Vec<Vec<Value>> = Vec::new();
        let mut lit_rows: Vec<Vec<Value>> = Vec::new();
        // Rev-carrying twins (D5.4). Every id-valued column is salted by rev so a
        // file byte-identical at two revs emits DISJOINT ids per rev (the raw ids
        // collide and would cross-wire base into head). Legacy rows above keep raw
        // ids. Twin dedup keys carry rev, so one file at two revs emits its twin
        // rows once PER rev.
        let mut node_rev_rows: Vec<Vec<Value>> = Vec::new();
        let mut node_repo_rev_rows: Vec<Vec<Value>> = Vec::new();
        let mut arg_rev_rows: Vec<Vec<Value>> = Vec::new();
        let mut field_rev_rows: Vec<Vec<Value>> = Vec::new();
        let mut lit_rev_rows: Vec<Vec<Value>> = Vec::new();
        let mut seen_param: HashSet<&str> = HashSet::new();
        let mut seen_arg: HashSet<(&str, i64, &str)> = HashSet::new();
        let mut seen_field: HashSet<(&str, &str, &str)> = HashSet::new();
        let mut seen_lit: HashSet<&str> = HashSet::new();
        let mut seen_node: HashSet<&str> = HashSet::new();
        let mut seen_node_repo: HashSet<(&str, &str)> = HashSet::new();
        let mut seen_edge: HashSet<(&str, &str)> = HashSet::new();
        let mut seen_loop: HashSet<(&str, u32)> = HashSet::new();
        let mut seen_nest: HashSet<(&str, &str)> = HashSet::new();
        let mut seen_node_rev: HashSet<(&str, &str)> = HashSet::new();
        let mut seen_node_repo_rev: HashSet<(&str, &str, &str)> = HashSet::new();
        let mut seen_arg_rev: HashSet<(&str, i64, &str, &str)> = HashSet::new();
        let mut seen_field_rev: HashSet<(&str, &str, &str, &str)> = HashSet::new();
        let mut seen_lit_rev: HashSet<(&str, &str)> = HashSet::new();
        for (repo, _, rev, f) in &facts {
            for n in &f.nodes {
                if seen_node.insert(n.id.as_str()) {
                    node_rows.push(vec![sym(&n.id), t(&n.kind), t(&n.var), t(&n.fn_sym), t(&n.file), i(n.line)]);
                }
                if seen_node_rev.insert((n.id.as_str(), rev.as_str())) {
                    let salted = Self::salt_rev(&n.id, rev);
                    node_rev_rows.push(vec![
                        sym(&salted), t(&n.kind), t(&n.var),
                        t(&n.fn_sym), t(&n.file), i(n.line), t(rev),
                    ]);
                }
                // df_node id is `file:line:col` (path only, no repo). Attribute
                // each node to EVERY repo it appears in so a downstream join
                // (member_node field/fill in flow-panel.dl) can scope a fill to
                // its own repo instead of fanning across every repo that shares
                // the constructed type's NAME. Emitted per (id, repo) OUTSIDE the
                // node dedup: two repos with a byte-identical file share one
                // df_node row but get TWO df_node_repo rows, so the join mints the
                // field for BOTH (they both really have it). A fill unique to one
                // repo (a working-tree-only edit) yields a single (id, repo) row,
                // so it stays a per-repo fact — the property the worktree-pair diff
                // needs.
                if seen_node_repo.insert((n.id.as_str(), repo.as_str())) {
                    node_repo_rows.push(vec![sym(&n.id), t(repo)]);
                }
                if seen_node_repo_rev.insert((n.id.as_str(), repo.as_str(), rev.as_str())) {
                    let salted = Self::salt_rev(&n.id, rev);
                    node_repo_rev_rows.push(vec![sym(&salted), t(repo), t(rev)]);
                }
            }
            for e in &f.edges {
                if seen_edge.insert((e.from.as_str(), e.to.as_str())) {
                    edge_rows.push(vec![sym(&e.from), sym(&e.to)]);
                }
            }
            for l in &f.loops {
                if seen_loop.insert((l.file.as_str(), l.start)) {
                    loop_rows.push(vec![t(&l.file), i(l.start), i(l.end), t(&l.var), t(&l.collection), t(&l.fn_sym)]);
                }
            }
            for fn_sym in &f.allocators {
                alloc_rows.push(vec![t(fn_sym)]);
            }
            for ns in &f.nests {
                if seen_nest.insert((ns.call_id.as_str(), ns.loop_id.as_str())) {
                    nest_rows.push(vec![t(&ns.call_id), t(&ns.loop_id), i(ns.depth), t(&ns.collection)]);
                }
            }
            for (id, pos) in &f.param_pos {
                if seen_param.insert(id.as_str()) {
                    param_rows.push(vec![sym(id), i(*pos)]);
                }
            }
            for (call, pos, arg) in &f.args {
                if seen_arg.insert((call.as_str(), *pos, arg.as_str())) {
                    arg_rows.push(vec![sym(call), Value::Int(*pos), sym(arg)]);
                }
                // both id columns salted so the arg->node join stays intra-rev
                if seen_arg_rev.insert((call.as_str(), *pos, arg.as_str(), rev.as_str())) {
                    let scall = Self::salt_rev(call, rev);
                    let sarg = Self::salt_rev(arg, rev);
                    arg_rev_rows.push(vec![
                        sym(&scall), Value::Int(*pos),
                        sym(&sarg), t(rev),
                    ]);
                }
            }
            for (id, field, value) in &f.fields {
                if seen_field.insert((id.as_str(), field.as_str(), value.as_str())) {
                    field_rows.push(vec![sym(id), t(field), sym(value)]);
                }
                // value is always a value df_node id (never a literal), so it
                // salts like id; the field name is a plain string, unsalted
                if seen_field_rev.insert((id.as_str(), field.as_str(), value.as_str(), rev.as_str())) {
                    let sid = Self::salt_rev(id, rev);
                    let svalue = Self::salt_rev(value, rev);
                    field_rev_rows.push(vec![
                        sym(&sid), t(field),
                        sym(&svalue), t(rev),
                    ]);
                }
            }
            for (id, text, kind) in &f.lits {
                if seen_lit.insert(id.as_str()) {
                    lit_rows.push(vec![sym(id), t(text), t(kind)]);
                }
                // id salted like df_node_rev.id (D5 pattern, same shape as df_field_rev).
                if seen_lit_rev.insert((id.as_str(), rev.as_str())) {
                    let sid = Self::salt_rev(id, rev);
                    lit_rev_rows.push(vec![sym(&sid), t(text), t(kind), t(rev)]);
                }
            }
        }

        // One batched `_strings` flush for every df id-handle hashed above (raw
        // AND rev-salted), so `sym(df_node.id)` decodes back to the id text in
        // any text context (hover, panel, hand queries). Collect-then-flush —
        // the N+1 law applies to this intern just like any other.
        self.db.flush_syms(&mut sink)?;

        self.refresh_rel("df_node", &["id", "kind", "var", "fn", "file", "line"], &node_rows)?;
        self.refresh_rel("df_node_repo", &["id", "repo"], &node_repo_rows)?;
        self.refresh_rel("df_edge", &["from", "to"], &edge_rows)?;
        self.refresh_rel("loop_over", &["file", "start", "end", "var", "collection", "fn"], &loop_rows)?;
        self.refresh_rel("allocates", &["fn"], &alloc_rows)?;
        self.refresh_rel("nest", &["call_id", "loop_id", "depth", "collection"], &nest_rows)?;
        self.refresh_rel("df_param", &["id", "pos"], &param_rows)?;
        self.refresh_rel("df_arg", &["call", "pos", "arg"], &arg_rows)?;
        self.refresh_rel("df_field", &["id", "field", "value"], &field_rows)?;
        self.refresh_rel("df_lit", &["id", "text", "kind"], &lit_rows)?;
        // Rev-carrying twins: same delete scope as type/call — wipe every corpus
        // rev and reinsert the whole-corpus salted rows (the emit above is
        // whole-corpus; a rev absent from the corpus is D5.5's retraction sweep,
        // not this path). Legacy df rels above stay raw-id, no rebuild needed
        // (the salt is not cleanly reversible in SQL and the raw rows are in hand).
        let all_revs = Self::corpus_revs(&files);
        let all_rev_refs: Vec<&str> = all_revs.iter().map(|s| s.as_str()).collect();
        self.refresh_rel_for_revs("df_node_rev", &["id", "kind", "var", "fn", "file", "line", "rev"], &node_rev_rows, &all_rev_refs)?;
        self.refresh_rel_for_revs("df_node_repo_rev", &["id", "repo", "rev"], &node_repo_rev_rows, &all_rev_refs)?;
        self.refresh_rel_for_revs("df_arg_rev", &["call", "pos", "arg", "rev"], &arg_rev_rows, &all_rev_refs)?;
        self.refresh_rel_for_revs("df_field_rev", &["id", "field", "value", "rev"], &field_rev_rows, &all_rev_refs)?;
        self.refresh_rel_for_revs("df_lit_rev", &["id", "text", "kind", "rev"], &lit_rev_rows, &all_rev_refs)?;
        // Persisted only after the writes land, so a failed refresh retries.
        for (rev, d) in &moved { self.save_rel_digest(&format!("extract:dataflow:{rev}"), d)?; }
        Ok(true)
    }
}
