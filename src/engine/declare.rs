use super::*;

/// Below this row count no auto-index is demanded (a scan of a handful of
/// pages, or SQLite's automatic transient index inside a join loop, beats a
/// persistent B-tree). Snapshot receipt: 349 of 771 standing idx_ indexes sat
/// on sub-1024-row rels and held 2.2MB combined; timing showed sub-ms deltas.
pub(crate) const TINY_REL_FLOOR: i64 = 1024;

/// Cache row for one (rel, col) auto-index demand probe: the digest
/// fingerprint the stats were measured at, and the verdict.
pub(crate) struct IdxDemandProbe {
    pub(crate) fingerprint: [u8; 32],
    pub(crate) demand: bool,
}

/// First PRIMARY KEY column of a rel's table, mirroring the DDL in
/// `Engine::declare`: an explicit `key(...)` names the PK; the default PK is
/// the full row in declared column order. On a rowid table, a single-column
/// auto-index on this column duplicates the leftmost prefix of the PK's
/// `sqlite_autoindex_*`.
fn pk_prefix_col(meta: &RelMeta) -> Option<&str> {
    match &meta.key {
        Some(key) => key.first().map(|name| name.as_str()),
        None => meta.cols.first().map(|c| c.name.as_str()),
    }
}

/// XOR-fold of every `_reldigest` namespace that tracks this rel's rows:
/// `<rel>` (source-rule seed), `rows:<rel>` (builtin whole-rel refresh),
/// `drv:<rel>` (derived digest-before-write). Motion in any one moves the
/// fold; a rel with no digest row in any namespace returns `None` (cold).
fn rel_digest_fingerprint(
    digests: &HashMap<String, [u8; 32]>,
    rel: &str,
) -> Option<[u8; 32]> {
    let keys = [rel.to_string(), format!("rows:{rel}"), format!("drv:{rel}")];
    let mut fold: Option<[u8; 32]> = None;
    for key in &keys {
        if let Some(digest) = digests.get(key) {
            let acc = fold.get_or_insert([0u8; 32]);
            for (acc_byte, digest_byte) in acc.iter_mut().zip(digest.iter()) {
                *acc_byte ^= digest_byte;
            }
        }
    }
    fold
}

/// Edge relations reached by a graph traversal: the edge argument of
/// `closure(edge)` (from `closures`, the head->edge map `closure_map`
/// already built upstream in `tick.rs`), plus any relation read by a
/// SELF-REFERENTIAL recursive rule — `head(seed, to) <- head(seed, from),
/// edge(from, to).`, the hand-rolled walk shape `port_of_reach_rec`,
/// `port_out_reach`, `mark_reach`, `member_from_new`, and `trace_step_raw`
/// (`.dl/flow-panel.dl`, `.dl/mark-lens.dl`) all use.
///
/// A traversal walks EITHER direction of a 2-ary edge: `closure()` seeds a
/// blast-radius walk from either endpoint (`ClosureSeed.forward`, strata.rs
/// — src pinned walks out, dst pinned walks in), and a recursive rule's own
/// next-hop step is symmetric to a find-refs query walking the same edge
/// backward. Both are invisible to `auto_indexes()`'s raw co-occurrence:
/// `BodyItem::Closure`/`Scc`/`Node2vec` atoms carry no `Term::Var`
/// occurrence at all (that loop's `_ => continue` arm skips them), and the
/// canonical recursive shape above binds `to` exactly once per rule body
/// (only `from`, shared with the recursive atom, co-occurs) — so raw
/// co-occurrence proposes the forward column at best, never the reverse
/// one. Returns the edge's leading two columns (position 0 = src, position
/// 1 = dst — the shape `df_edge`/`flow_edge`/`map_edge`/`bom_edge`/
/// `port_edge` all share) as raw candidates; the caller applies the SAME
/// PK-prefix/tiny-rel/constant-column filters to them as to every other
/// candidate.
///
/// `node2vec(edge)` (the third walk-family operator, random walks +
/// skip-gram) is NOT reachable from here: its rules are excluded from
/// `derived_rules` before `create_auto_indexes` ever runs (tick.rs
/// `all_derived` filter drops any rule with `node2vec_edge().is_some()`)
/// and its edge name never reaches `closures` (`closure_map` only reads
/// `closure_edge()`). Covering it needs `create_auto_indexes` to take an
/// extra `node2vec_rules` argument threaded from its two tick.rs call
/// sites — out of scope for a declare.rs-only change; named here so it is
/// not silently assumed covered.
fn traversal_edge_cols(
    derived_rules: &[&Rule],
    closures: &HashMap<String, String>,
    rels: &Rels,
) -> Vec<(String, String)> {
    let mut edges: HashSet<String> = closures.values().cloned().collect();
    for rule in derived_rules {
        let head = rule.head.rel.as_str();
        let self_referential = rule
            .body
            .iter()
            .any(|item| matches!(item, BodyItem::Pos(a) if a.rel == head));
        if !self_referential {
            continue;
        }
        for item in &rule.body {
            let atom = match item {
                BodyItem::Pos(a) | BodyItem::Neg(a) => a,
                _ => continue,
            };
            if atom.rel != head {
                edges.insert(atom.rel.clone());
            }
        }
    }
    let mut out = Vec::new();
    for edge in &edges {
        if let Some(meta) = rels.get(edge) {
            if meta.cols.len() >= 2 {
                out.push((edge.clone(), meta.cols[0].name.clone()));
                out.push((edge.clone(), meta.cols[1].name.clone()));
            }
        }
    }
    out
}

impl Engine {
    pub(crate) fn create_rel_view(&self, rel: &str, meta: &RelMeta) -> Result<()> {
        let view = crate::lower::txt_tbl(rel);
        let columns: Vec<String> = meta.cols.iter().map(|col| {
            if col.coord {
                // A df-coordinate id: reconstruct `file:line:col:kind` from
                // `_df_node_dict` (keyed by the dense surrogate; the coordinate
                // text is not in `_strings`).
                format!(
                    "(SELECT (SELECT content FROM _strings WHERE _strings.id = dnd.\"file\") || ':' || \
                     dnd.\"line\" || ':' || dnd.\"col\" || ':' || \
                     (SELECT content FROM _strings WHERE _strings.id = dnd.\"kind\") \
                     FROM _df_node_dict dnd WHERE dnd.\"id\" = rel_{rel}.\"{}\" LIMIT 1) AS \"{}\"",
                    col.name, col.name
                )
            } else if col.interned() {
                // COALESCE fallback: an ordinary `text` column carrying a df
                // coordinate id (ecosystem flow rels) reconstructs from
                // `_df_node_dict` when its value is absent from `_strings`.
                // Short-circuits for a normal interned string (the _strings hit).
                format!(
                    "COALESCE((SELECT content FROM _strings WHERE _strings.id = rel_{rel}.\"{name}\"), \
                     (SELECT (SELECT content FROM _strings WHERE _strings.id = dnd.\"file\") || ':' || \
                     dnd.\"line\" || ':' || dnd.\"col\" || ':' || \
                     (SELECT content FROM _strings WHERE _strings.id = dnd.\"kind\") \
                     FROM _df_node_dict dnd WHERE dnd.\"id\" = rel_{rel}.\"{name}\" LIMIT 1)) AS \"{name}\"",
                    name = col.name
                )
            } else {
                format!("\"{}\"", col.name)
            }
        }).collect();
        let select = if columns.is_empty() {
            "*".to_string()
        } else {
            columns.join(", ")
        };
        let create = format!("CREATE VIEW {view} AS SELECT {select} FROM {}", tbl(rel));
        // Unchanged view: skip the DROP+CREATE. Schema DDL rewrites
        // sqlite_master pages into the WAL — measured ~2.5MB per tick across
        // ~120 rel views on a quiet tick (2026-07-18, the dominant slice of
        // the clock-program write churn after the derived digest skip).
        // sqlite_master stores the CREATE statement verbatim, so string
        // equality is an exact match on the view's definition.
        let existing = self.db.query_opt(
            rel,
            "SELECT sql FROM sqlite_master WHERE type = 'view' AND name = ?1",
            &[view.as_str().into()],
            |row| Ok(row.get::<_, String>(0)?),
        )?;
        if existing.as_deref() == Some(create.as_str()) {
            return Ok(());
        }
        self.db
            .exec_on(rel, &format!("DROP VIEW IF EXISTS {view}"))?;
        self.db.exec_on(rel, &create)?;
        Ok(())
    }
}

/// Storage-diet step 4a (plans/2026-07-18-storage-diet.md Direction 4a,
/// storage-key-audit decision 2): classifies a rel decl as a `WITHOUT ROWID`
/// candidate — a "pure junction/set" relation. SQLite backs a plain rowid
/// table's full-row `PRIMARY KEY` with a UNIQUE autoindex that duplicates the
/// entire row (measured: rel_flow_edge 8.6MB table + 8.6MB autoindex,
/// rel_df_edge_src_kind 8.3+8.3); `WITHOUT ROWID` makes the table its own PK
/// B-tree, storing the row once.
///
/// Four conditions, each critical:
/// - `d.pk_never_null`: an explicit per-decl vouch (see the field doc on
///   `RelDecl`) that no row will ever carry NULL in a PK column. This is the
///   FIRST check, not a formality: `WITHOUT ROWID` requires every PK column
///   NOT NULL, but a plain rowid table's composite `PRIMARY KEY` tolerates
///   NULL (SQLite treats it as always-distinct there), and the `.dl`
///   surface's named-arg partial-head padding (`person(name: "z").` leaves
///   `age` NULL) can put a NULL in any column of any parsed decl. Without
///   this gate, `wants_without_rowid` judged purely by shape and made a
///   `.dl`-declared 2-column no-`key()` rel silently DROP its NULL-padded
///   row under `INSERT OR IGNORE` — caught by tests/it/named_args.rs
///   `named_args_in_a_rule_head_resolve` (docs/failure-modes.md, 2026-07-18,
///   step-4a NULL-in-PK incident). Default `false`, so every `.dl`-parsed
///   decl is excluded unconditionally; only specific Rust-authored decls
///   with an audited, tuple-sourced insert path opt in.
/// - `d.key.is_none()`: no `key(...)` narrowing, so the full row already IS
///   the uniqueness contract (Soufflé APLAS'21 set semantics per the audit's
///   decision 2) — there is no separate narrow identity a secondary index
///   would want to reference instead.
/// - every column is SQLite INTEGER (`Col::sql() == "INTEGER"`, true for
///   `Type::Int` and any interned text column): no raw TEXT payload sitting
///   in the clustered PK leaf, keeping the composite key compact and
///   comparison-cheap.
/// - column count in 2..=4: the plan's own scope ("flow_edge, df_edge, the
///   many two-and-three-column edge rels"). This is the guard against the
///   locator-growth risk the plan flags: a WITHOUT ROWID table's secondary
///   indexes carry the FULL pk as locator instead of an 8-byte rowid, so this
///   lever only wins where the PK is narrow or the table has few secondary
///   indexes. A wide entity/occurrence table with several join-key indexes
///   (df_node at 6 cols + 6 indexes, scip_occurrence at 8 cols) fails this
///   cap and is deferred to Direction 1/4b (dense single-column node_id,
///   step 5/6), which narrows the PK before applying WITHOUT ROWID to it —
///   never widens a WITHOUT ROWID table's secondary-index locators.
///
/// Revertable by construction: flip this fn's answer for a rel (or the
/// `pk_never_null` vouch) and the drift check in `declare` (rowid-mode
/// mismatch) drops and recreates the table on the next tick, same mechanism
/// a column-set or key-shape change already uses. No separate migration
/// path needed.
pub(crate) fn wants_without_rowid(d: &RelDecl) -> bool {
    d.pk_never_null
        && d.key.is_none()
        && (2..=4).contains(&d.cols.len())
        && d.cols.iter().all(|c| c.sql() == "INTEGER")
}

impl Engine {
    pub(crate) fn declare(&mut self, d: &RelDecl) -> Result<()> {
        // Port envelope check: a `@in(class)`/`@out(class)` rel must carry the
        // class's contract columns BY NAME (order-free, extra columns rejected
        // for now — the drain reads the envelope, nothing else). The class is
        // the contract, never a transport; binding happens at the CLI (--mcp).
        if let Some(p) = &d.port {
            let dir = match p.dir {
                crate::ast::PortDir::In => "@in",
                crate::ast::PortDir::Out => "@out",
            };
            let Some(env) = crate::ast::Port::envelope(&p.class, p.dir) else {
                bail!(
                    "rel {}: unknown port class {dir}({}); `rpc` is the only class today \
                       (stream/duplex are reserved)",
                    d.name,
                    p.class
                );
            };
            for (cname, cty) in env {
                match d.cols.iter().find(|c| c.name == *cname) {
                    Some(c) if c.ty == *cty => {}
                    Some(c) => bail!(
                        "rel {}: {dir}({}) needs column {cname}: {}, found {cname}: {}",
                        d.name,
                        p.class,
                        cty.name(),
                        c.ty.name()
                    ),
                    None => bail!(
                        "rel {}: {dir}({}) needs column {cname}: {}",
                        d.name,
                        p.class,
                        cty.name()
                    ),
                }
            }
            if d.cols.len() != env.len() {
                let extra: Vec<&str> = d
                    .cols
                    .iter()
                    .filter(|c| !env.iter().any(|(n, _)| c.name == *n))
                    .map(|c| c.name.as_str())
                    .collect();
                bail!(
                    "rel {}: {dir}({}) allows only the envelope columns ({}); extra: {}",
                    d.name,
                    p.class,
                    env.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", "),
                    extra.join(", ")
                );
            }
        }
        // View-backed rel (see `RelDecl::view_body`): `rel_<name>` is a VIEW over
        // its `_rev` twin, not a base table. Skips every base-table concern below
        // (migration, PK, WITHOUT ROWID, autoindex) and the write path entirely.
        if let Some(body) = &d.view_body {
            return self.declare_view_backed(d, body);
        }
        // Migrate a stale cached table whose column set OR primary key no longer
        // matches the decl. Two triggers:
        //   1. Column-set drift (e.g. a release added a leading column) — the
        //      next `refresh_rel` insert would fail "no column named ...".
        //   2. Key-set drift — a hot-reload that ADDS/REMOVES/CHANGES a
        //      `key(...)` qualifier leaves the old PRIMARY KEY in place (a bare
        //      `CREATE TABLE IF NOT EXISTS` keeps the cached shape). The lattice
        //      upsert then targets `ON CONFLICT(key)` against a full-row PK that
        //      does not match, and every subsequent tick fails "ON CONFLICT
        //      clause does not match any PRIMARY KEY or UNIQUE constraint",
        //      wedging the daemon. A merge-fn-only change (MaxBy(a) -> MaxBy(b))
        //      keeps the same PK column set, so it does NOT drop here — the
        //      upsert SQL is regenerated by `lower_rule` every tick.
        // Rel tables are derived (or source rows reconciled every tick), so
        // dropping loses nothing — reconcile / rebuild_derived refills.
        if !d.cols.is_empty() {
            let table = tbl(&d.name);
            let want: Vec<String> = d.cols.iter().map(|c| c.name.clone()).collect();
            // The PK the decl wants: the `key(...)` subset, else the full row.
            let want_pk: Vec<String> = match &d.key {
                Some(k) => k.clone(),
                None => want.clone(),
            };
            // Step 4a: WITHOUT ROWID mismatch is a drift trigger too — the
            // classifier (`wants_without_rowid`) is pure code, so a decl edit
            // that moves a rel across the 2..=4 column boundary or adds/drops
            // its `key(...)` must drop+recreate the table the same way a
            // column-set or key-shape edit already does, or the cached table
            // shape silently outlives the decl.
            let want_without_rowid = wants_without_rowid(d);
            let have_without_rowid: bool = self
                .db
                .query_rows(
                    &d.name,
                    "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    &[table.as_str().into()],
                    |r| Ok(r.get::<_, String>(0)?),
                )
                .ok()
                .and_then(|rows| rows.into_iter().next())
                .map(|sql| sql.to_uppercase().ends_with("WITHOUT ROWID"))
                .unwrap_or(false);
            let (have, have_pk): (Vec<String>, Vec<String>) = {
                let mut have = Vec::new();
                // (pk_position, column) for columns in the existing PRIMARY KEY.
                let mut pk_pos: Vec<(i64, String)> = Vec::new();
                let rows = self.db.query_rows(
                    &d.name,
                    &format!("PRAGMA table_info({table})"),
                    &[],
                    |r| Ok((r.get::<_, String>(1)?, r.get::<_, i64>(5)?)),
                );
                if let Ok(rows) = rows {
                    for (name, pk) in rows {
                        if name == "__src" {
                            continue;
                        }
                        if pk > 0 {
                            pk_pos.push((pk, name.clone()));
                        }
                        have.push(name);
                    }
                }
                pk_pos.sort_by_key(|(p, _)| *p);
                (have, pk_pos.into_iter().map(|(_, c)| c).collect())
            };
            // ON CONFLICT matches a constraint by column SET, order-free — so
            // compare PK as sorted sets (a pure column reorder is not a drift).
            let pk_set = |mut v: Vec<String>| {
                v.sort();
                v
            };
            let key_drift = pk_set(have_pk.clone()) != pk_set(want_pk.clone());
            let rowid_drift = have_without_rowid != want_without_rowid;
            if !have.is_empty() && (have != want || key_drift || rowid_drift) {
                self.db.exec_on(
                    &d.name,
                    &format!("DROP VIEW IF EXISTS {}", crate::lower::txt_tbl(&d.name)),
                )?;
                self.db
                    .exec_on(&d.name, &format!("DROP TABLE IF EXISTS {table}"))?;
                self.db.exec_params(
                    "_reldigest",
                    "DELETE FROM _reldigest WHERE rel = ?1",
                    &[d.name.as_str().into()],
                )?;
                // P1 interaction: before the completion-marker fix, a dropped
                // derived table read back as 0 rows, and `any_derived_empty`
                // treated that as "must full-rebuild" — the (accidental) thing
                // that actually refilled `d.name` after this migration. Now
                // that a legitimately-empty derived rel does NOT force a full
                // rebuild, the stale completion marker must be invalidated
                // here explicitly, or a rel that was already marked complete
                // before the drop would read as "done" against its freshly
                // empty, never-refilled table. `_derived_complete` may have no
                // row for `d.name` (a source rel, or a derived rel that never
                // completed a pass yet) — the DELETE is then simply a no-op.
                self.db.exec_params(
                    "_derived_complete",
                    "DELETE FROM _derived_complete WHERE rel = ?1",
                    &[d.name.as_str().into()],
                )?;
            }
        }
        let sql = if d.cols.is_empty() {
            // Zero-column relation (the built-in `true()` singleton): one row,
            // no user columns. SQLite needs at least one column, so the table
            // carries only the universal `__src` sentinel.
            format!(
                "CREATE TABLE IF NOT EXISTS {} (__src TEXT DEFAULT '')",
                tbl(&d.name)
            )
        } else {
            let cols: Vec<String> = d
                .cols
                .iter()
                .map(|c| format!("\"{}\" {}", c.name, c.sql()))
                .collect();
            // The PRIMARY KEY drives dedup. The default (no `key(...)`) is the
            // full row, so identical rows collapse (set semantics). A `key(...)`
            // qualifier narrows the PK to that column subset = a functional
            // dependency / choice-domain (Soufflé APLAS'21): one row per key,
            // first-wins under `INSERT OR IGNORE`, or lattice-merged under a
            // `merge(...)`. Validate the key/merge columns exist and that the
            // merge col is NOT a key col (it ranks rows within a key).
            let pk: Vec<String> = if let Some(key) = &d.key {
                for k in key {
                    if !d.cols.iter().any(|c| &c.name == k) {
                        bail!("key column {k} not in rel {}", d.name);
                    }
                }
                key.iter().map(|c| format!("\"{c}\"")).collect()
            } else {
                d.cols.iter().map(|c| format!("\"{}\"", c.name)).collect()
            };
            if let Some(merge) = &d.merge {
                let (mc, _) = merge.col_and_cmp();
                let key = d.key.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("rel {} has merge(...) without key(...)", d.name)
                })?;
                if !d.cols.iter().any(|c| c.name == mc) {
                    bail!("merge column {mc} not in rel {}", d.name);
                }
                if key.iter().any(|k| k == mc) {
                    bail!("rel {}: merge column {mc} is also a key column; the merge ranks rows WITHIN a key", d.name);
                }
            }
            // Step 4a (storage-diet Direction 4a): a pure junction/set rel
            // (see `wants_without_rowid`) gets `WITHOUT ROWID` — the table
            // becomes its own PK B-tree instead of a rowid table plus a
            // duplicate full-row autoindex. `__src` rides along as an
            // ordinary non-key column in the WITHOUT ROWID leaf; SQLite does
            // not require the PK to cover every column.
            let without_rowid = if wants_without_rowid(d) { " WITHOUT ROWID" } else { "" };
            format!(
                "CREATE TABLE IF NOT EXISTS {} ({}, __src TEXT DEFAULT '', PRIMARY KEY ({})){}",
                tbl(&d.name),
                cols.join(", "),
                pk.join(", "),
                without_rowid
            )
        };
        // Revert guard: a rel that was view-backed on a prior run (see
        // `declare_view_backed`) and is now a base table leaves a stale VIEW of
        // this name. The migration drift check above reads PRAGMA table_info,
        // which reports a view's columns too, so it never fires — and
        // `CREATE TABLE IF NOT EXISTS` sees the view in the shared name space and
        // silently no-ops, leaving the view in place. Drop it (and its `_txt`
        // view) first. `DROP VIEW IF EXISTS` on a real table would error, so this
        // is gated on the object actually being a view.
        let stale_view = self
            .db
            .query_rows(
                &d.name,
                "SELECT 1 FROM sqlite_master WHERE type = 'view' AND name = ?1",
                &[tbl(&d.name).as_str().into()],
                |_row| Ok(()),
            )
            .map(|rows| !rows.is_empty())
            .unwrap_or(false);
        if stale_view {
            self.db.exec_on(
                &d.name,
                &format!("DROP VIEW IF EXISTS {}", crate::lower::txt_tbl(&d.name)),
            )?;
            self.db
                .exec_on(&d.name, &format!("DROP VIEW IF EXISTS {}", tbl(&d.name)))?;
        }
        self.db.exec_on(&d.name, &sql)?;
        self.rels.insert(
            d.name.clone(),
            RelMeta {
                cols: d.cols.clone(),
                key: d.key.clone(),
                merge: d.merge.clone(),
                port: d.port.clone(),
                view_backed: false,
            },
        );
        let meta = self.rels.get(&d.name).cloned().unwrap_or_default();
        self.create_rel_view(&d.name, &meta)?;
        Ok(())
    }

    /// Declare a view-backed rel: `rel_<name>` becomes a `CREATE VIEW` over the
    /// authored `body` (a `SELECT DISTINCT <non-rev cols> FROM rel_<name>_rev`
    /// for the twins this ships for), never a base table. No `__src`, no
    /// autoindex, no rows, no `_reldigest`/`_derived_complete` row of its own.
    /// Queries and `_txt` decode resolve it exactly like a table-backed rel.
    fn declare_view_backed(&mut self, d: &RelDecl, body: &str) -> Result<()> {
        let table = tbl(&d.name);
        let txt = crate::lower::txt_tbl(&d.name);
        // A prior run may have left `rel_<name>` as a base TABLE (pre-conversion)
        // or as this VIEW already; the `_txt` decode view rides on top of it and
        // must drop first. `DROP TABLE IF EXISTS` on an object that is a VIEW (or
        // vice versa) ERRORS in SQLite even with IF EXISTS — the object exists,
        // just as the wrong type — so pick the drop verb from the actual type.
        self.db.exec_on(&d.name, &format!("DROP VIEW IF EXISTS {txt}"))?;
        let existing_type: Option<String> = self
            .db
            .query_rows(
                &d.name,
                "SELECT type FROM sqlite_master WHERE name = ?1",
                &[table.as_str().into()],
                |row| Ok(row.get::<_, String>(0)?),
            )
            .ok()
            .and_then(|rows| rows.into_iter().next());
        match existing_type.as_deref() {
            Some("table") => {
                self.db.exec_on(&d.name, &format!("DROP TABLE {table}"))?;
            }
            Some("view") => {
                self.db.exec_on(&d.name, &format!("DROP VIEW {table}"))?;
            }
            _ => {}
        }
        self.db
            .exec_on(&d.name, &format!("CREATE VIEW {table} AS {body}"))?;
        // This rel never owns rows/digest/completion. Clear any tracking a prior
        // base-table incarnation wrote so a stale row cannot outlive the table.
        for key in [d.name.clone(), format!("rows:{}", d.name), format!("drv:{}", d.name)] {
            self.db.exec_params(
                "_reldigest",
                "DELETE FROM _reldigest WHERE rel = ?1",
                &[key.into()],
            )?;
        }
        self.db.exec_params(
            "_derived_complete",
            "DELETE FROM _derived_complete WHERE rel = ?1",
            &[d.name.as_str().into()],
        )?;
        self.rels.insert(
            d.name.clone(),
            RelMeta {
                cols: d.cols.clone(),
                key: d.key.clone(),
                merge: d.merge.clone(),
                port: d.port.clone(),
                view_backed: true,
            },
        );
        let meta = self.rels.get(&d.name).cloned().unwrap_or_default();
        // The `_txt` decode view reads `FROM rel_<name>` (a view now); SQLite
        // resolves a view over a view fine. create_rel_view rebuilds it.
        self.create_rel_view(&d.name, &meta)?;
        Ok(())
    }

    /// Create the join-key indexes derived rules need (see auto_indexes), and
    /// drop any previously auto-created index that no rule of the currently
    /// served program needs anymore. Skips closure heads, which are views.
    ///
    /// Demand-aware reconcile (storage-diet plan, Direction 5 / step 2, index
    /// audit): `CREATE INDEX IF NOT EXISTS` alone is monotonic-add-only — a
    /// `.dl` program dropped from discovery left its join-key indexes behind
    /// forever, which is how the sprefa root accumulated 758 indexes across
    /// every program ever served (329.6MB, more than the entire autoindex
    /// pool). This function is a full reconcile of the `idx_<rel>_<col>`
    /// naming convention against the currently-wanted set every tick: every
    /// other engine-created index uses a different naming scheme (grep
    /// receipt: `node_file_span_idx`, `_write_ledger_tick_idx`, etc., never
    /// the `idx_` prefix), so the sweep only ever touches indexes this
    /// function itself created.
    /// Planner-honesty (storage-diet Direction 5, index audit residual): raw
    /// join-key co-occurrence over-demands — the sprefa root still carried 771
    /// idx_ indexes (357.6MB) of which EXPLAIN QUERY PLAN receipts over the
    /// served program set showed only ~half ever chosen. Demand is filtered
    /// three ways before the reconcile, each backed by before/after EQP +
    /// timing receipts on the live-root snapshot (see the landing commit):
    ///
    ///   #1 PK-prefix on a ROWID table: the PK's `sqlite_autoindex_*` already
    ///      serves the seek, so `idx_<rel>_<col>` on the PK's first column is
    ///      a duplicate prefix. Timing: every affected shape flat (max
    ///      +0.02ms). NOT applied to WITHOUT ROWID tables: dropping
    ///      idx_df_edge_from (PK (from,to), junction) flipped a 209k-row
    ///      fixpoint join's order (scan side <-> seek side) for a measured
    ///      155ms -> 345ms — without `sqlite_stat1` the planner's cost model
    ///      leans on the narrow secondary index to pick the cheap order, so
    ///      on WR tables the "duplicate" B-tree stays.
    ///   #2 tiny rel / #3 constant column: `demand_auto_index` below.
    ///
    /// A broader low-selectivity filter (drop when ndistinct <= rows/1024,
    /// e.g. `kind` enums) was measured and REJECTED: value skew defeats
    /// distinct-count reasoning — the served shapes probe RARE kinds, so
    /// idx_df_node_kind (23 distinct over 280k rows) degraded 21 shapes by
    /// +5..+110ms and idx_df_arg_pos (13 distinct) one by +80ms when dropped.
    ///
    /// Reverse-traversal residual (verified 2026-07-20 against the live
    /// root's `rel_map_edge`, and independently against `rel_df_edge` by the
    /// `graph-libs:sqlite-native:opus-redo` research arc): a recursive CTE
    /// walking an edge relation BACKWARD, with no index on the reverse
    /// column, makes SQLite build a throwaway `AUTOMATIC COVERING INDEX`
    /// per query — `tests/it/reverse_traversal_plan.rs` pins the query-plan
    /// text and reports the timing gap (advisory; see that file for why the
    /// ratio itself is not asserted in CI). `auto_indexes()`'s raw
    /// co-occurrence NEVER proposes this reverse column: `BodyItem::
    /// Closure`/`Scc`/`Node2vec` atoms carry no `Term::Var` occurrences (the
    /// loop skips them outright), and a hand-rolled recursive rule's
    /// canonical shape — `head(seed, to) <- head(seed, from), edge(from,
    /// to).` — binds `to` exactly once per rule body, so only `from`
    /// (shared with the recursive atom) ever co-occurs. `traversal_edge_cols`
    /// below adds BOTH of a traversed edge's leading columns as raw
    /// candidates before the three filters above run, so the SAME PK-prefix/
    /// tiny-rel/constant-column reasoning decides whether either survives —
    /// no parallel demand mechanism.
    pub(crate) fn create_auto_indexes(
        &self,
        derived_rules: &[&Rule],
        closures: &HashMap<String, String>,
    ) -> Result<()> {
        let mut raw: Vec<(String, String)> = auto_indexes(derived_rules, &self.rels);
        raw.extend(traversal_edge_cols(derived_rules, closures, &self.rels));
        // Skip closure heads (views) and view-backed rels (see
        // `RelDecl::view_body`): a `CREATE INDEX` on a view errors, and a
        // view-backed edge (e.g. `closure(type_edge)`) would otherwise reach
        // here via `traversal_edge_cols`.
        raw.retain(|(rel, _)| {
            !closures.contains_key(rel) && !self.rels.get(rel).is_some_and(|meta| meta.view_backed)
        });
        raw.sort();
        raw.dedup();
        // Filter #1. One bulk sqlite_master read for rowid-mode (never a
        // per-candidate point read); `tbl()` names only, so non-rel tables
        // never match a candidate.
        let without_rowid: HashSet<String> = self
            .db
            .query_rows(
                "sqlite_master",
                "SELECT name, sql FROM sqlite_master WHERE type = 'table' AND name LIKE 'rel\\_%' ESCAPE '\\'",
                &[],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            )?
            .into_iter()
            .filter(|(_, sql)| {
                sql.as_deref()
                    .is_some_and(|s| s.to_uppercase().trim_end().ends_with("WITHOUT ROWID"))
            })
            .map(|(name, _)| name)
            .collect();
        let candidates: Vec<(String, String)> = raw
            .into_iter()
            .filter(|(rel, col)| {
                without_rowid.contains(&tbl(rel))
                    || self
                        .rels
                        .get(rel)
                        .and_then(pk_prefix_col)
                        .is_none_or(|pk_first| pk_first != col)
            })
            .collect();
        // Filters #2/#3. One bulk read of the whole digest table; each
        // candidate rel's fingerprint folds every digest namespace that
        // tracks its rows (`<rel>` for source rules, `rows:<rel>` for builtin
        // refreshes, `drv:<rel>` for derived digest-before-write), so motion
        // in any of them reprobes the stats.
        let digests = self.load_rel_digests_prefix("")?;
        let mut wanted: Vec<(String, String)> = Vec::new();
        for (rel, col) in candidates {
            let fingerprint = rel_digest_fingerprint(&digests, &rel);
            if self.demand_auto_index(&rel, &col, fingerprint)? {
                wanted.push((rel, col));
            }
        }
        let wanted_names: HashSet<String> = wanted
            .iter()
            .map(|(rel, col)| format!("idx_{rel}_{col}"))
            .collect();

        let existing: Vec<String> = self.db.query_rows(
            "sqlite_master",
            "SELECT name FROM sqlite_master WHERE type = 'index' AND name LIKE 'idx\\_%' ESCAPE '\\'",
            &[],
            |row| Ok(row.get::<_, String>(0)?),
        )?;
        let stale: Vec<&String> = existing.iter().filter(|n| !wanted_names.contains(*n)).collect();
        if !stale.is_empty() {
            tracing::debug!(
                pruned = stale.len(),
                "create_auto_indexes: dropping join-key indexes no longer needed by the served program"
            );
        }
        for name in stale {
            self.db.exec(&format!("DROP INDEX \"{name}\""))?;
        }

        for (rel, col) in &wanted {
            let ix = format!("idx_{rel}_{col}");
            self.db.exec_on(
                rel,
                &format!(
                    "CREATE INDEX IF NOT EXISTS \"{ix}\" ON {}(\"{col}\")",
                    tbl(rel)
                ),
            )?;
        }
        Ok(())
    }

    /// Planner-honesty filters #2 and #3: should a join-key candidate index
    /// actually be demanded, given the rel's measured shape?
    ///
    ///   - #2 tiny rel (`1 <= rows < TINY_REL_FLOOR`): a scan of a few pages —
    ///     or SQLite's automatic transient index inside a join loop — beats
    ///     maintaining a persistent B-tree. A rel with NO rows yet stays
    ///     demanded: that is the cold-build tick (indexes are created before
    ///     extraction fills the tables), an empty index costs nothing, and it
    ///     protects the first fixpoint from the O(F*C) nested-scan trap.
    ///   - #3 constant column (exactly one distinct value over a floor-sized
    ///     rel): an equality seek either matches every row (= the scan) or
    ///     nothing, so the B-tree cannot discriminate. Snapshot receipt: 11
    ///     such indexes, 18.3MB, topped by idx_df_node_repo_rev_repo (10.2MB
    ///     over a repo column with ONE value) and idx_df_node_repo_repo
    ///     (6.2MB, same). Columns with >= 2 distinct values always stay:
    ///     value skew makes distinct-count reasoning unsafe past this point
    ///     (measured: rare-`kind` probes regressed up to +110ms when a
    ///     23-distinct column lost its index).
    ///
    /// Probes are digest-gated (recompute-guard law): the counts re-run only
    /// when the rel's digest fingerprint moves, so a quiet tick re-reads only
    /// `_reldigest`. `fingerprint == None` (no digest row at all) is a cold
    /// table: demand, and cache nothing so the first real fill reprobes.
    fn demand_auto_index(
        &self,
        rel: &str,
        col: &str,
        fingerprint: Option<[u8; 32]>,
    ) -> Result<bool> {
        let Some(fingerprint) = fingerprint else {
            return Ok(true);
        };
        let key = (rel.to_string(), col.to_string());
        if let Some(probe) = self.idx_demand_cache.borrow().get(&key) {
            if probe.fingerprint == fingerprint {
                return Ok(probe.demand);
            }
        }
        let table = tbl(rel);
        let total_rows: i64 = self.db.query_one(
            rel,
            &format!("SELECT count(*) FROM {table}"),
            &[],
            |row| Ok(row.get(0)?),
        )?;
        let demand = if total_rows == 0 {
            true // cold build: probes run pre-extraction; an empty index is free
        } else if total_rows < TINY_REL_FLOOR {
            false
        } else {
            // Bounded: stops at the second distinct value, so any
            // discriminating column costs a short prefix scan. Only a truly
            // constant column pays a full column scan, and the digest gate
            // keeps that off every subsequent unchanged tick.
            let distinct_seen: i64 = self.db.query_one(
                rel,
                &format!(
                    "SELECT count(*) FROM (SELECT DISTINCT \"{col}\" FROM {table} LIMIT 2)"
                ),
                &[],
                |row| Ok(row.get(0)?),
            )?;
            distinct_seen > 1
        };
        if !demand {
            tracing::debug!(
                rel,
                col,
                total_rows,
                "auto-index demand dropped: planner-honesty probe (tiny rel or constant column)"
            );
        }
        self.idx_demand_cache
            .borrow_mut()
            .insert(key, IdxDemandProbe { fingerprint, demand });
        Ok(demand)
    }

    /// Declare every relation: closure heads become a VIEW over the condensation,
    /// everything else a base table.
    pub(crate) fn declare_all(
        &mut self,
        prog: &Program,
        closures: &HashMap<String, String>,
    ) -> Result<()> {
        // Discovery (`.dl/*.dl`) merges several files into one program, so the
        // same relation may be declared in more than one file. An identical
        // re-declaration is a no-op; a conflicting shape is an error.
        let mut seen: HashMap<String, Vec<crate::ast::Col>> = HashMap::new();
        for item in &prog.items {
            if let Item::Rel(d) = item {
                // A deferred `rel name: shape.` (shape_ref still set, cols empty)
                // is a computed shape: don't declare a zero-column table here.
                // `resolve_derived_shapes` (after builtins) fills it from _shapes
                // or records shape-pending. (Frontend already resolved syntax
                // shapes; a ref surviving to here is derived-only.)
                if d.shape_ref.is_some() {
                    continue;
                }
                if let Some(prev) = seen.get(&d.name) {
                    if *prev == d.cols {
                        continue;
                    }
                    bail!("rel {} declared twice with different columns", d.name);
                }
                seen.insert(d.name.clone(), d.cols.clone());
                if BUILTIN_RELS.contains(&d.name.as_str()) {
                    bail!(
                        "{} is a built-in relation (true/repo/rev/content/file); pick another name",
                        d.name
                    );
                }
                if MODULE_RELS.contains(&d.name.as_str()) {
                    bail!(
                        "{} is a built-in module-graph relation; pick another name",
                        d.name
                    );
                }
                if TYPE_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in type-graph relation (type_edge / type_edge_rev / type_entity / type_entity_rev / type_sig / type_link / type_link_rev); pick another name", d.name);
                }
                if DOC_TEXT_RELS.contains(&d.name.as_str()) {
                    bail!(
                        "{} is a built-in doc relation (doc_comment / doc_tag); pick another name",
                        d.name
                    );
                }
                if CONST_VALUE_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in const-value relation (const_value / const_value_rev); pick another name", d.name);
                }
                if COMMENT_RELS.contains(&d.name.as_str()) {
                    bail!(
                        "{} is a built-in comment relation (comment_node); pick another name",
                        d.name
                    );
                }
                if TEMPLATE_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in template-literal relation (template_parts); pick another name", d.name);
                }
                if UNRESOLVED_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in unresolved-marker relation (unresolved); pick another name", d.name);
                }
                if CALL_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in call-graph relation (call_def / call_def_rev / call_site / call_edge / call_edge_rev / call_name / call_kind); pick another name", d.name);
                }
                if DATAFLOW_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in dataflow relation (df_node / df_node_rev / df_node_repo / df_node_repo_rev / df_edge / loop_over / allocates / nest / df_param / df_arg / df_arg_rev / df_field / df_field_rev / df_lit / df_lit_rev); pick another name", d.name);
                }
                if DOC_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in document relation (doc_node / doc_ref); pick another name", d.name);
                }
                if SPINE_RELS.contains(&d.name.as_str()) {
                    bail!(
                        "{} is a built-in ref-spine relation (string / ref); pick another name",
                        d.name
                    );
                }
                if NODE_RELS.contains(&d.name.as_str()) {
                    bail!(
                        "{} is a built-in CST relation (node / child); pick another name",
                        d.name
                    );
                }
                for k in crate::rels::rel_kinds() {
                    if k.rels().contains(&d.name.as_str()) {
                        bail!("{} is {}; pick another name", d.name, k.reserved_msg());
                    }
                }
                if DAEMON_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in daemon-state relation (program / head / rev_advanced); pick another name", d.name);
                }
                if EVERY_RELS.contains(&d.name.as_str()) {
                    bail!(
                        "{} is the built-in clock relation (every); pick another name",
                        d.name
                    );
                }
                if CLOCK_RELS.contains(&d.name.as_str()) {
                    bail!(
                        "{} is the built-in clock relation (clock); pick another name",
                        d.name
                    );
                }
                if HOOK_RELS.contains(&d.name.as_str()) {
                    bail!(
                        "{} is the built-in harness-hook event log (hook_event); pick another name",
                        d.name
                    );
                }
                if DIAG_RELS.contains(&d.name.as_str()) {
                    bail!("diag is the built-in diagnostic sink (fixed schema: path, line, col, end_line, end_col, severity, code, msg, hint); drop the `rel diag(...)` decl and write it directly — name only the columns you use, e.g. `diag(path: p, line: l, msg: m) <- ...`");
                }
                if DIAG_STAGE_RELS.contains(&d.name.as_str()) {
                    bail!("diag_stage is the built-in diag-routing sink (fixed schema: code, stage); drop the `rel diag_stage(...)` decl and head it directly from a rule, like diag — e.g. `diag_stage(\"my-code\", \"agent-turn\") <- ...` (stage in live | commit | agent-turn | agent-session)");
                }
                if HOVER_RELS.contains(&d.name.as_str()) {
                    bail!("hover_note is the built-in hover-note sink (fixed schema: path, line, col, end_line, end_col, md); drop the `rel hover_note(...)` decl and head it directly from a rule, like diag — name only the columns you use, e.g. `hover_note(path: p, line: l, end_line: l, end_col: c, md: text) <- ...`");
                }
                if GRAPH_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in drawable-graph sink (graph_node(id, label, kind[, file, line, parent]) / graph_edge(src, dst, kind)); drop the `rel {}(...)` decl and head it directly from a rule, like diag — name only the columns you use", d.name, d.name);
                }
                if MUTE_RELS.contains(&d.name.as_str()) {
                    bail!("diag_mute is the built-in diagnostic-mute set (code); it is written only by the LSP toggle command, never a rule — pick another name");
                }
                if DEMAND_RELS.contains(&d.name.as_str()) {
                    bail!("{} is a built-in demand sink (scip_want / rev_cmp_want / def_target / effect_cmd / checkout) — drop the `rel {}(...)` decl and head it directly from a rule, like diag/repo", d.name, d.name);
                }
                if CHECKOUT_OUT_RELS.contains(&d.name.as_str()) {
                    bail!("checkout_done is the built-in checkout-sweep outcome (repo, branch, action, ok, detail); it is written by the `checkout` sink, so READ it — do not `rel`-declare or head it");
                }
                if TYPE_DECL_RELS.contains(&d.name.as_str()) {
                    bail!("type_decl_row is the built-in derived-shape sink (shape, pos, col, type); drop the `rel type_decl_row(...)` decl and head it directly from a derived rule, like diag/graph_node");
                }
                match closures.get(&d.name) {
                    Some(edge) => self.declare_closure(d, edge)?,
                    None => self.declare(d)?,
                }
            }
        }
        self.declare_builtins()?;
        // Phase 5: resolve any computed `rel name: shape.` against the shapes the
        // `type_decl_row` sink persisted on a prior tick. Runs after builtins so a
        // shape rel can be declared alongside them; the one-tick phase delay makes
        // the derive -> persist -> resolve loop invisible under the daemon.
        self.resolve_derived_shapes(prog)?;
        Ok(())
    }

    /// Register and create the built-in relation tables (repo/rev/content/file).
    /// Reuses `declare`, so they get the same `rel_<name>` table shape and a
    /// `self.rels` entry, which is what lets `lower_rule` join body atoms against
    /// them. Populated by `refresh_builtin_rels`, not by source rules.
    pub(crate) fn declare_builtins(&mut self) -> Result<()> {
        for d in builtin_rel_decls() {
            self.declare(&d)?;
        }
        for d in module_rel_decls() {
            self.declare(&d)?;
        }
        for d in type_rel_decls() {
            self.declare(&d)?;
        }
        for d in doc_text_rel_decls() {
            self.declare(&d)?;
        }
        for d in const_value_rel_decls() {
            self.declare(&d)?;
        }
        for d in comment_rel_decls() {
            self.declare(&d)?;
        }
        for d in template_rel_decls() {
            self.declare(&d)?;
        }
        for d in unresolved_rel_decls() {
            self.declare(&d)?;
        }
        for d in call_rel_decls() {
            self.declare(&d)?;
        }
        for d in dataflow_rel_decls() {
            self.declare(&d)?;
        }
        for d in doc_rel_decls() {
            self.declare(&d)?;
        }
        for d in spine_rel_decls() {
            self.declare(&d)?;
        }
        for d in node_rel_decls() {
            self.declare(&d)?;
        }
        // Optional point/containment index: "innermost CST node covering byte C
        // in file F" is `node(_, _, F, lo, hi, _), lo <= C, C < hi` — a range
        // scan on (file, lo, hi) instead of a full `node` table scan. Mirrors
        // `_where_bytes_file_span_idx`. The closure(child) path is still the
        // pick for full-ancestry materialization (measured); this just makes
        // the LSP-common point query first-class. Idempotent.
        self.db.exec_on(
            "node",
            &format!(
                "CREATE INDEX IF NOT EXISTS node_file_span_idx ON {}(\"file\", \"lo\", \"hi\")",
                tbl("node")
            ),
        )?;
        for d in crate::rels::rel_kind_decls() {
            self.declare(&d)?;
        }
        for d in daemon_rel_decls() {
            self.declare(&d)?;
        }
        for d in every_rel_decls() {
            self.declare(&d)?;
        }
        for d in clock_rel_decls() {
            self.declare(&d)?;
        }
        for d in effect_rel_decls() {
            self.declare(&d)?;
        }
        for d in hook_rel_decls() {
            self.declare(&d)?;
        }
        for d in diag_rel_decls() {
            self.declare(&d)?;
        }
        for d in diag_stage_rel_decls() {
            self.declare(&d)?;
        }
        for d in hover_note_rel_decls() {
            self.declare(&d)?;
        }
        for d in graph_rel_decls() {
            self.declare(&d)?;
        }
        for d in diag_mute_rel_decls() {
            self.declare(&d)?;
        }
        for d in demand_rel_decls() {
            self.declare(&d)?;
        }
        for d in checkout_out_rel_decls() {
            self.declare(&d)?;
        }
        for d in type_decl_rel_decls() {
            self.declare(&d)?;
        }
        Ok(())
    }

    /// Rebuild the built-in relations from the `_file` cache. Wholesale wipe +
    /// repopulate (bounded by repo size, one row per tracked file). Stage 1:
    /// repo = one row from `--root`; rev.id/file.rev = the raw rev string;
    /// content.id = the content hash. No interning (Stage 2).
    #[tracing::instrument(skip_all, level = "debug")]
    pub(crate) fn refresh_builtin_rels(&self) -> Result<()> {
        let files: Vec<(String, String, String, String)> = self.db.query_rows(
            "_file",
            "SELECT repo, path, rev, hash FROM _file",
            &[],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
        let t = |s: &str| Value::Text(s.to_string());
        // slug -> on-disk root for the repos we can name (self + config). Fills
        // the `repo` relation's root column; an ingested-by-path repo not in
        // either set gets ''.
        let mut root_of: HashMap<String, String> = HashMap::new();
        root_of.insert(self.self_slug(), self.root.to_string_lossy().to_string());
        for rc in &self.repos {
            root_of.insert(rc.slug.clone(), rc.root.to_string_lossy().to_string());
        }
        // `rev`/`content` dedup by their own key (id / hash). `rev.id` is the rev
        // string, so a `WORK` shared across repos folds to one row (the `repo`
        // column then names just the first); committed revs are unique shas.
        let mut revs: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        let mut contents: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        let mut file_rows: Vec<Vec<Value>> = Vec::new();
        let mut repo_slugs: BTreeSet<String> = BTreeSet::new();
        for (repo, path, rev, hash) in files {
            revs.entry(rev.clone())
                .or_insert_with(|| vec![t(&rev), t(&repo), t(&rev), Value::Int(0)]);
            contents
                .entry(hash.clone())
                .or_insert_with(|| vec![t(&hash), t(&hash)]);
            file_rows.push(vec![t(&repo), t(&rev), t(&path), t(&hash)]);
            repo_slugs.insert(repo);
        }
        // `repo` lists the configured repos when a config is loaded; otherwise
        // every repo actually ingested into `_file`, or `--root` if nothing has
        // been scanned yet. A configured repo whose root is missing is omitted
        // (the `allow_missing` flag keeps the engine alive past it; the absence
        // here is what lets a program write `!repo(S, _, _)` to surface misses).
        let repo_rows: Vec<Vec<Value>> = if !self.repos.is_empty() {
            self.repos
                .iter()
                .filter(|r| r.root.exists())
                .map(|r| {
                    vec![
                        t(&r.slug),
                        t(&r.root.to_string_lossy()),
                        t(&r.url.clone().unwrap_or_default()),
                    ]
                })
                .collect()
        } else {
            if repo_slugs.is_empty() {
                repo_slugs.insert(self.self_slug());
            }
            repo_slugs
                .iter()
                .map(|slug| {
                    let root = root_of.get(slug).cloned().unwrap_or_default();
                    vec![t(slug), t(&root), t("")]
                })
                .collect()
        };
        let revs: Vec<Vec<Value>> = revs.into_values().collect();
        let contents: Vec<Vec<Value>> = contents.into_values().collect();
        self.refresh_rel("repo", &["slug", "root", "url"], &repo_rows)?;
        self.refresh_rel("rev", &["id", "repo", "oid", "ts"], &revs)?;
        self.refresh_rel("content", &["id", "hash"], &contents)?;
        self.refresh_rel("file", &["repo", "rev", "path", "content"], &file_rows)?;
        // The `true()` singleton: always exactly one zero-column row. The range
        // anchor for negation-only rules (`diag(...) <- true(), !rel(_,_,_).`).
        self.db.exec(&format!("DELETE FROM {}", tbl("true")))?;
        self.db.exec(&format!(
            "INSERT OR IGNORE INTO {} DEFAULT VALUES",
            tbl("true")
        ))?;
        Ok(())
    }

    /// Populate the `every` clock relation for this tick. For each interval `N`
    /// the program names, `secs=N` lands a row IFF the current wall-second is in a
    /// different `N`-bucket than the last time `N` fired (so it fires on the first
    /// tick and once per boundary crossing thereafter, exact regardless of tick
    /// cadence). The last-fired bucket per `N` is stored in `_carry_meta` under
    /// `every:N`. Wholesale wipe: the rel is ephemeral, not derived.
    /// Returns true if the rel's content changed this tick (a row landed, or rows
    /// cleared after the previous tick had some) — the incremental path uses it to
    /// re-derive rules that join `every`.
    pub(crate) fn refresh_every(&self, intervals: &[i64]) -> Result<bool> {
        let before: i64 = self.db.query_one(
            "every",
            &format!("SELECT COUNT(*) FROM {}", tbl("every")),
            &[],
            |r| Ok(r.get(0)?),
        )?;
        self.db.exec_on("every", &format!("DELETE FROM {}", tbl("every")))?;
        let now = now_secs();
        let mut rows: Vec<Vec<Value>> = Vec::new();
        for &n in intervals {
            if n <= 0 {
                continue;
            }
            let bucket = now / n;
            let key = format!("every:{n}");
            let prev: Option<i64> = self.db.query_opt(
                "_carry_meta",
                "SELECT tx FROM _carry_meta WHERE k = ?1",
                &[key.clone().into()],
                |r| Ok(r.get::<_, i64>(0)?),
            )?;
            if prev != Some(bucket) {
                rows.push(vec![Value::Int(n)]);
                self.db.exec_params(
                    "_carry_meta",
                    "INSERT INTO _carry_meta (k, tx) VALUES (?1, ?2) \
                     ON CONFLICT(k) DO UPDATE SET tx = ?2",
                    &[key.into(), bucket.into()],
                )?;
            }
        }
        let landed = rows.len();
        self.db.insert_rows(&tbl("every"), &["secs"], &rows)?;
        Ok(landed > 0 || before > 0)
    }

    /// Populate `clock(secs, bucket)` with the CURRENT bucket `now / secs` for each
    /// named period — one persistent row per period, every tick (unlike `every`'s
    /// edge-trigger). Skips the write when no bucket moved, returning whether the
    /// content changed so the incremental path re-derives rules that join it.
    pub(crate) fn refresh_clock(&self, periods: &[i64]) -> Result<bool> {
        let now = now_secs();
        let mut want: Vec<(i64, i64)> = periods
            .iter()
            .filter(|&&n| n > 0)
            .map(|&n| (n, now / n))
            .collect();
        want.sort();
        want.dedup();
        let have: Vec<(i64, i64)> = self.db.query_rows(
            "clock",
            &format!(
                "SELECT \"secs\", \"bucket\" FROM {} ORDER BY \"secs\", \"bucket\"",
                tbl("clock")
            ),
            &[],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )?;
        if have == want {
            return Ok(false);
        }
        let rows: Vec<Vec<Value>> = want
            .into_iter()
            .map(|(s, b)| vec![Value::Int(s), Value::Int(b)])
            .collect();
        self.refresh_rel("clock", &["secs", "bucket"], &rows)?;
        Ok(true)
    }

    /// A closure head `rel_<head>` is a recursive-CTE view over the condensation
    /// tables of its edge relation. The view yields cross-component reach plus
    /// same-cyclic-component pairs (so a node on a cycle reaches itself).
    pub(crate) fn declare_closure(&mut self, d: &RelDecl, edge: &str) -> Result<()> {
        if d.cols.len() != 2 {
            bail!("closure head {} must have 2 columns", d.name);
        }
        self.rels.insert(
            d.name.clone(),
            RelMeta {
                cols: d.cols.clone(),
                ..Default::default()
            },
        );
        let (nt, et, v) = (scc_node_tbl(edge), scc_edge_tbl(edge), tbl(&d.name));
        self.db.execute_batch_on(
            &d.name,
            &format!(
                "CREATE TABLE IF NOT EXISTS {nt} (name TEXT PRIMARY KEY, comp INTEGER, cyclic INTEGER);
                 CREATE TABLE IF NOT EXISTS {et} (comp_src INTEGER, comp_dst INTEGER, PRIMARY KEY(comp_src, comp_dst));"
            ),
        )?;
        // a prior run may have left rel_<head> as a view or a real table; clear both.
        self.db
            .exec_on(&d.name, &format!("DROP VIEW IF EXISTS {v}"))?;
        self.db
            .exec_on(&d.name, &format!("DROP TABLE IF EXISTS {v}"))?;
        let (c0, c1) = (&d.cols[0].name, &d.cols[1].name);
        // An int-keyed head column (interned text OR a df-coordinate `node`)
        // carries its STORED id verbatim in `scc_node.name` (the closure edge was
        // read raw, `load_edges`), so recovering it is a plain `CAST(name AS
        // INTEGER)` — no re-hash. This replaces the old `sprf_sym(name)`, which
        // rebuilt the id from its text (correct only while the id WAS a hash of
        // that text; false for the dense `_df_node_dict` surrogate, identity
        // normalization 2026-07-20). A plain (non-interned) column stores its
        // text directly and passes through unchanged.
        let output_expr = |side: &str, column: &str| {
            if d.cols
                .iter()
                .find(|c| c.name == column)
                .is_some_and(|c| c.interned() || c.coord)
            {
                format!("CAST({side}.name AS INTEGER)")
            } else {
                format!("{side}.name")
            }
        };
        let output_name = |column: &str| output_expr("na", column);
        let output_name_b = |column: &str| output_expr("nb", column);
        let first_a = output_name(c0);
        let first_b = output_name_b(c1);
        let second_a = output_name(c0);
        let second_b = output_name_b(c1);
        self.db.execute_batch_on(
            &d.name,
            &format!(
                "CREATE VIEW {v} AS
                 WITH RECURSIVE cr(a, b) AS (
                   SELECT comp_src, comp_dst FROM {et}
                   UNION
                   SELECT cr.a, e.comp_dst FROM cr JOIN {et} e ON e.comp_src = cr.b
                 )
                 SELECT {first_a} AS \"{c0}\", {first_b} AS \"{c1}\"
                   FROM cr JOIN {nt} na ON na.comp = cr.a JOIN {nt} nb ON nb.comp = cr.b
                 UNION
                 SELECT {second_a} AS \"{c0}\", {second_b} AS \"{c1}\"
                   FROM {nt} na JOIN {nt} nb ON na.comp = nb.comp AND na.cyclic = 1;"
            ),
        )?;
        if d.cols[0].interned() && d.cols[1].interned() {
            // Decode a closure head column for `_txt`: a df-coordinate (`node`)
            // column reconstructs `file:line:col:kind` from rel_df_node (its text
            // is no longer interned); a plain interned column decodes through
            // `_strings`.
            let decode_head = |col: &Col| -> String {
                if col.coord {
                    format!(
                        "(SELECT (SELECT content FROM _strings WHERE _strings.id = dnd.\"file\") || ':' || \
                         dnd.\"line\" || ':' || dnd.\"col\" || ':' || \
                         (SELECT content FROM _strings WHERE _strings.id = dnd.\"kind\") \
                         FROM _df_node_dict dnd WHERE dnd.\"id\" = rel_{}.\"{}\" LIMIT 1)",
                        d.name, col.name
                    )
                } else {
                    // COALESCE fallback (see create_rel_view): a `text` closure
                    // head carrying df ids reconstructs from `_df_node_dict`.
                    format!(
                        "COALESCE((SELECT content FROM _strings WHERE _strings.id = rel_{name}.\"{col}\"), \
                         (SELECT (SELECT content FROM _strings WHERE _strings.id = dnd.\"file\") || ':' || \
                         dnd.\"line\" || ':' || dnd.\"col\" || ':' || \
                         (SELECT content FROM _strings WHERE _strings.id = dnd.\"kind\") \
                         FROM _df_node_dict dnd WHERE dnd.\"id\" = rel_{name}.\"{col}\" LIMIT 1))",
                        name = d.name, col = col.name
                    )
                }
            };
            self.db.exec_on(
                &d.name,
                &format!("DROP VIEW IF EXISTS {}", crate::lower::txt_tbl(&d.name)),
            )?;
            self.db.exec_on(
                &d.name,
                &format!(
                    "CREATE VIEW {} AS SELECT {} AS \"{}\", {} AS \"{}\" FROM {}",
                    crate::lower::txt_tbl(&d.name),
                    decode_head(&d.cols[0]),
                    c0,
                    decode_head(&d.cols[1]),
                    c1,
                    tbl(&d.name),
                ),
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod without_rowid_classifier_tests {
    // Storage-diet step 4a (plans/2026-07-18-storage-diet.md): `wants_without_
    // rowid` is a pure fn of `RelDecl` shape, so its boundary cases (raw text
    // payload column, column count above/below the 2..=4 scope) are cheaper
    // and more precise to pin here than through a `.dl` fixture — the DSL
    // surface has no way to declare a raw-text column on a user rel (only
    // built-ins like `df_lit` use `Col::raw`), and the column-count boundary
    // needs exact 1/2/4/5-column decls, not a plausible-looking program.
    // End-to-end DDL wiring (the CREATE TABLE actually carries WITHOUT ROWID,
    // an eligible table has no PK autoindex, a byte receipt) is pinned by
    // tests/it/storage_diet_without_rowid.rs through the real `dl` binary.
    use super::wants_without_rowid;
    use crate::ast::{Col, RelDecl, Type};

    // `pk_never_null` defaults to `false` via `..Default::default()` — this
    // helper's default arm is deliberately the SAME shape any `.dl`-parsed
    // decl gets, so a test that wants the vouch must ask for it explicitly.
    fn decl(cols: Vec<Col>, key: Option<Vec<String>>, pk_never_null: bool) -> RelDecl {
        RelDecl { name: "t".into(), cols, key, pk_never_null, ..Default::default() }
    }

    fn int_col(name: &str) -> Col {
        Col::plain(name.to_string(), Type::Int)
    }

    #[test]
    fn two_to_four_column_all_integer_no_key_vouched_rels_qualify() {
        for n in 2..=4 {
            let cols: Vec<Col> = (0..n).map(|i| int_col(&format!("c{i}"))).collect();
            assert!(wants_without_rowid(&decl(cols, None, true)), "{n}-column all-INTEGER no-key vouched rel should qualify");
        }
    }

    /// The regression this gate exists to prevent: docs/failure-modes.md,
    /// 2026-07-18 step-4a NULL-in-PK incident. A `.dl`-parsed decl (the
    /// `person(name: text, age: int)` shape from tests/it/named_args.rs
    /// `named_args_in_a_rule_head_resolve`) is a 2-column, all-INTEGER,
    /// no-`key()` rel — otherwise a perfect classifier match — but the
    /// parser never sets `pk_never_null`, so it must NOT qualify: named-arg
    /// partial-head padding can leave `age` NULL, and `WITHOUT ROWID`
    /// silently drops such a row under `INSERT OR IGNORE` where a plain
    /// rowid table's composite PK would not.
    #[test]
    fn unvouched_rel_does_not_qualify_even_when_shape_matches() {
        let cols = vec![Col::plain("name".into(), Type::Text), Col::plain("age".into(), Type::Int)];
        assert!(
            !wants_without_rowid(&decl(cols, None, false)),
            "pk_never_null defaults false; an un-vouched decl must never get WITHOUT ROWID regardless of shape"
        );
    }

    #[test]
    fn single_column_rel_does_not_qualify() {
        assert!(!wants_without_rowid(&decl(vec![int_col("a")], None, true)));
    }

    #[test]
    fn five_column_rel_does_not_qualify() {
        let cols: Vec<Col> = (0..5).map(|i| int_col(&format!("c{i}"))).collect();
        assert!(!wants_without_rowid(&decl(cols, None, true)), "wide rels ride Direction 1/4b, not step 4a");
    }

    #[test]
    fn key_narrowed_rel_does_not_qualify() {
        let cols = vec![int_col("a"), int_col("b"), int_col("c")];
        assert!(!wants_without_rowid(&decl(cols, Some(vec!["a".to_string()]), true)), "key(...) means the full row is not the PK — not a pure junction");
    }

    #[test]
    fn raw_text_payload_column_does_not_qualify() {
        let cols = vec![int_col("a"), Col::raw("text", Type::Text)];
        assert!(!wants_without_rowid(&decl(cols, None, true)), "a raw TEXT payload column sitting in the clustered PK leaf is not the narrow-int shape this step targets");
    }

    #[test]
    fn interned_text_columns_count_as_integer() {
        // Type::Text columns intern to _strings and store as SQLite INTEGER
        // (Col::interned() / Col::sql()) — this is the df_edge shape
        // (`from: text, to: text`), one of the plan's own named examples.
        let cols = vec![Col::plain("from".into(), Type::Text), Col::plain("to".into(), Type::Text)];
        assert!(wants_without_rowid(&decl(cols, None, true)));
    }
}
