use super::*;

impl Engine {
    pub(crate) fn ensure_meta(&self) -> Result<()> {
        // Intern-key migration (2026-07-11): `_strings.id` / `_where_bytes.string_id`
        // move from TEXT (decimal StringId::Display) to INTEGER (StringId::sqlite,
        // the i64 bit-pattern lower.rs already compiles literals to). No row-level
        // data migration: an existing TEXT-typed table is DROPPED and recreated
        // empty, then the extraction digests are cleared so the very next tick
        // refills both tables from scratch (every extract:<family> digest folds
        // exe identity already, so a new binary re-extracts regardless).
        {
            let conn = self.db.conn();
            let strings_is_text = conn
                .prepare("SELECT type FROM pragma_table_info('_strings') WHERE name = 'id'")
                .and_then(|mut s| s.query_row([], |r| r.get::<_, String>(0)))
                .map(|t| t.eq_ignore_ascii_case("text"))
                .unwrap_or(false);
            if strings_is_text {
                conn.execute_batch(
                    "DROP TABLE IF EXISTS _strings;
                     DROP TABLE IF EXISTS _where_bytes;
                     DELETE FROM _reldigest WHERE key LIKE 'extract:%';",
                )?;
            }
        }
        self.db.conn().execute_batch(
            "CREATE TABLE IF NOT EXISTS _file (repo TEXT NOT NULL DEFAULT '', path TEXT, rev TEXT, hash TEXT,
                 mtime INTEGER DEFAULT 0, size INTEGER DEFAULT 0, lines INTEGER DEFAULT -1, PRIMARY KEY (repo, path, rev));
             CREATE TABLE IF NOT EXISTS _prov (rel TEXT, repo TEXT NOT NULL DEFAULT '', path TEXT, src TEXT, PRIMARY KEY (rel, repo, path, src));
             CREATE TABLE IF NOT EXISTS _reldigest (rel TEXT PRIMARY KEY, digest TEXT);
             -- P1 (2026-07-10 --check perf defect): one row per derived rel that
             -- has completed a `rebuild_derived` pass, regardless of the row
             -- count it ended with. `any_derived_empty`'s old COUNT(*)-per-rel
             -- probe treated a legitimately-empty derived rel (an inert rail, a
             -- diff view with nothing to report) the same as never derived,
             -- forcing a full rebuild of every derived rel on EVERY tick (154
             -- rels / ~2024 statements measured on a real db). This table lets
             -- `derived_incomplete_rels` tell the two cases apart with one
             -- query instead of N COUNT(*) round trips. Never migrated away on
             -- a rel rename/removal — a stale row for a since-deleted rel is
             -- simply never looked up again.
             CREATE TABLE IF NOT EXISTS _derived_complete (rel TEXT PRIMARY KEY);
             -- Persisted derived shapes (Phase 5): one row per (shape, column) the
             -- `type_decl_row` sink produced last tick. Read at the next tick's
             -- declare to resolve a `rel name: shape.` whose shape was computed,
             -- not written by hand. Digest-guarded full replace (see
             -- persist_type_decl_shapes); the one-tick phase delay.
             CREATE TABLE IF NOT EXISTS _shapes (shape TEXT, pos INTEGER, col TEXT, type TEXT, PRIMARY KEY (shape, pos));
             -- Wall ms of each derived rel's INSERT statements from its most
             -- recent rebuild (max across the rel's rules/passes). Written
             -- batched by rebuild_derived; projected by the perf built-in
             -- stmt_ms so a .dl rail can watch its own rule cost.
             CREATE TABLE IF NOT EXISTS _stmt_ms (rel TEXT PRIMARY KEY, ms INTEGER NOT NULL);
             -- CST node path attribution (not a public rel column): maps a
             -- node id to its source path so the delta refresh can prune one
             -- file's `node` rows. The `node.file` column is a content FileId
             -- shared by byte-identical files, so it cannot key the prune.
             CREATE TABLE IF NOT EXISTS _node_path (id TEXT PRIMARY KEY, path TEXT NOT NULL);
             CREATE INDEX IF NOT EXISTS _node_path_path_idx ON _node_path(path);
             -- id = StringId::sqlite() (the i64 bit-pattern of the content-derived
             -- u64 hash) as an INTEGER PRIMARY KEY / rowid alias — single-word int
             -- compares + smaller keys instead of TEXT memcmp on every join probe.
             CREATE TABLE IF NOT EXISTS _strings (
                 id INTEGER PRIMARY KEY,
                 content TEXT NOT NULL,
                 norm TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS _files (
                 id TEXT PRIMARY KEY,
                 content_hash TEXT NOT NULL,
                 path TEXT NOT NULL DEFAULT '',
                 size INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE IF NOT EXISTS _where_bytes (
                 id TEXT PRIMARY KEY,
                 string_id INTEGER NOT NULL,
                 file_id TEXT NOT NULL,
                 lo INTEGER NOT NULL,
                 hi INTEGER NOT NULL,
                 repo TEXT NOT NULL DEFAULT '0',
                 rev TEXT NOT NULL DEFAULT '0',
                 path TEXT NOT NULL DEFAULT ''
             );
             CREATE TABLE IF NOT EXISTS _program (
                 path TEXT PRIMARY KEY,
                 hash TEXT NOT NULL DEFAULT '',
                 mtime INTEGER NOT NULL DEFAULT 0,
                 loaded_at INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE IF NOT EXISTS _repo (
                 slug TEXT PRIMARY KEY,
                 root TEXT NOT NULL DEFAULT '',
                 url TEXT NOT NULL DEFAULT '',
                 registered_at INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE IF NOT EXISTS _ref (
                 repo TEXT NOT NULL,
                 name TEXT NOT NULL,
                 oid TEXT NOT NULL DEFAULT '',
                 observed_at INTEGER NOT NULL DEFAULT 0,
                 PRIMARY KEY (repo, name)
             );
             CREATE TABLE IF NOT EXISTS _rev_log (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 repo TEXT NOT NULL,
                 name TEXT NOT NULL,
                 old TEXT NOT NULL DEFAULT '',
                 new TEXT NOT NULL DEFAULT '',
                 at INTEGER NOT NULL DEFAULT 0
             );
             -- Content-addressed embeddings: one vector per (StringId, backend).
             -- `sid` joins `_strings.id`; `backend` namespaces the model so two
             -- backends coexist without cross-space cosine. `vec` is comma-joined
             -- f32 TEXT (the existing plural Value::Text insert path; the
             -- sqlite-vec ANN mirror is the scale follow-on).
             CREATE TABLE IF NOT EXISTS _embeddings (
                 sid TEXT NOT NULL,
                 backend TEXT NOT NULL,
                 dim INTEGER NOT NULL,
                 vec TEXT NOT NULL,
                 PRIMARY KEY (sid, backend)
             );
             CREATE INDEX IF NOT EXISTS _embeddings_backend_idx ON _embeddings(backend);
             -- Structural node embeddings (node2vec): one vector per node, keyed
             -- by `graph` = the edge rel name a `node2vec(edge)` rule consumed, so
             -- multiple graphs coexist (the `backend` analog for the text path).
             -- `node` is the node id verbatim (a sym / file / whatever the edge
             -- rel carries). `vec` is comma-joined f32 TEXT, same as _embeddings.
             -- `edge_digest` (W2) lets the last N distinct edge-digests of a
             -- graph coexist, so branch A<->B thrash is a cache hit both ways;
             -- `_node_emb_seen` is the per-graph LRU bookkeeping that bounds it.
             CREATE TABLE IF NOT EXISTS _node_embeddings (
                 node TEXT NOT NULL,
                 graph TEXT NOT NULL,
                 edge_digest TEXT NOT NULL DEFAULT '',
                 dim INTEGER NOT NULL,
                 vec TEXT NOT NULL,
                 PRIMARY KEY (node, graph, edge_digest)
             );
             CREATE INDEX IF NOT EXISTS _node_embeddings_graph_idx ON _node_embeddings(graph);
             CREATE INDEX IF NOT EXISTS _node_embeddings_gd_idx ON _node_embeddings(graph, edge_digest);
             CREATE TABLE IF NOT EXISTS _node_emb_seen (
                 graph TEXT NOT NULL,
                 digest TEXT NOT NULL,
                 last_tick INTEGER NOT NULL,
                 PRIMARY KEY (graph, digest)
             );
             CREATE INDEX IF NOT EXISTS _strings_norm_idx ON _strings(norm);
             CREATE INDEX IF NOT EXISTS _where_bytes_string_idx ON _where_bytes(string_id);
             CREATE INDEX IF NOT EXISTS _where_bytes_file_span_idx ON _where_bytes(file_id, lo, hi);
             CREATE INDEX IF NOT EXISTS _where_bytes_path_idx ON _where_bytes(path);
             INSERT OR IGNORE INTO _strings (id, content, norm) VALUES (0, '', '');
             INSERT OR IGNORE INTO _files (id, content_hash, path, size)
                 VALUES ('0', '0000000000000000000000000000000000000000000000000000000000000000', '', 0);
             INSERT OR IGNORE INTO _where_bytes (id, string_id, file_id, lo, hi, repo, rev, path)
                 VALUES ('0', 0, '0', 0, 0, '0', '0', '');
             -- The @next carry clock: one row, k='tx', the current generation.
             -- A @next rule reads carry_<rel> WHERE tx=current and stages rows at
             -- tx=current+1; the counter advances once per tick. See
             -- docs/research-reactive-effectful-datalog.md §8.
             CREATE TABLE IF NOT EXISTS _carry_meta (k TEXT PRIMARY KEY, tx INTEGER NOT NULL DEFAULT 0);
             INSERT OR IGNORE INTO _carry_meta (k, tx) VALUES ('tx', 0);
             -- @async effect queue: one row per outstanding request. `id` =
             -- digest(kind, args_json) so the same request emitted on two ticks
             -- before it runs does not double-fire. `kind` = the response rel
             -- name; `args_json` = the bound-var object; `done` flips to 1 once
             -- the executor has run and the response row is inserted. Off-tick
             -- `drain_effects` is the only writer of `done`. See §8.
             -- `kind` = the effect/`sh` template key (== head rel in the
             -- head-response form, the `sh` decl name in the explicit body-effect
             -- form). `head_rel` = the response rel the head is rebuilt into (they
             -- differ when `gh(..) -> (..)` lands into a differently-named rel).
             -- `full_json` (D-4) is the full body solution, the head-rebuild
             -- payload: the head may mix body vars NOT in the effect args with the
             -- response outs, so the digest keys on `args_json` (the hole map) but
             -- the head is reconstructed from `full_json` ∪ outs in `drain_effects`.
             CREATE TABLE IF NOT EXISTS pending_effect (
                 id TEXT PRIMARY KEY, kind TEXT NOT NULL,
                 head_rel TEXT NOT NULL DEFAULT '', args_json TEXT NOT NULL,
                 full_json TEXT NOT NULL DEFAULT '',
                 req_tx INTEGER NOT NULL, done INTEGER NOT NULL DEFAULT 0,
                 state TEXT NOT NULL DEFAULT 'queued', idem_key TEXT,
                 batch INTEGER NOT NULL DEFAULT 0);
             -- Server query history: one row per daemon `query`/`query_sql` RPC
             -- and LSP `dl/query` request, appended by `Engine::log_query` at the
             -- handler (src/daemon.rs, src/lsp.rs). No primary key: two requests
             -- with identical text within the same nanosecond are both real
             -- events and both land. Append-only by design, no retention/GC.
             -- Projected by the built-in `query_log` relation (src/rels/querylog.rs).
             CREATE TABLE IF NOT EXISTS _query_log (
                 ts TEXT NOT NULL,
                 source TEXT NOT NULL,
                 method TEXT NOT NULL,
                 body TEXT NOT NULL DEFAULT '',
                 params TEXT NOT NULL DEFAULT '[]'
             );"
        )?;
        // tolerate a pending_effect created before the body-effect columns existed.
        // The pre-migration default for head_rel is `kind` (head-response 1:1), set
        // on read in `drain_effects` via the empty-string fallback.
        let _ = self.db.conn().execute(
            "ALTER TABLE pending_effect ADD COLUMN head_rel TEXT NOT NULL DEFAULT ''", []);
        let _ = self.db.conn().execute(
            "ALTER TABLE pending_effect ADD COLUMN full_json TEXT NOT NULL DEFAULT ''", []);
        // Phase 3 job state machine: `state` (queued|running|done|failed) is the
        // reconcile axis; `idem_key` records the `sh!` exactly-once claim. Legacy
        // rows migrate with state derived from `done` below.
        let _ = self.db.conn().execute(
            "ALTER TABLE pending_effect ADD COLUMN state TEXT NOT NULL DEFAULT 'queued'", []);
        let _ = self.db.conn().execute(
            "ALTER TABLE pending_effect ADD COLUMN idem_key TEXT", []);
        // Phase 1b.2 `collect(x)`: a batch request gathers `x` across ALL body
        // solutions and fires ONE effect whose response fans back out (line per
        // entity). `batch=1` tells the drain to split the response into N head
        // rows (run_stream) like a stream, but one-shot (marked done).
        let _ = self.db.conn().execute(
            "ALTER TABLE pending_effect ADD COLUMN batch INTEGER NOT NULL DEFAULT 0", []);
        // A db whose rows predate `state` carry the column default 'queued' even
        // when already drained (done=1); reconcile their state from `done` once.
        let _ = self.db.conn().execute(
            "UPDATE pending_effect SET state = 'done' WHERE done = 1 AND state = 'queued'", []);
        // tolerate dbs created before mtime/size existed
        let _ = self.db.conn().execute("ALTER TABLE _file ADD COLUMN mtime INTEGER DEFAULT 0", []);
        let _ = self.db.conn().execute("ALTER TABLE _file ADD COLUMN size INTEGER DEFAULT 0", []);
        // tolerate dbs created before the line-count column existed; -1 = unknown,
        // reconcile_sources' fast path forces one read+count on the next tick.
        let _ = self.db.conn().execute("ALTER TABLE _file ADD COLUMN lines INTEGER DEFAULT -1", []);
        // tolerate _where_bytes created before the path attribution column existed
        let _ = self.db.conn().execute("ALTER TABLE _where_bytes ADD COLUMN path TEXT NOT NULL DEFAULT ''", []);
        // Re-key `_file` and `_prov` on (repo, ...) for dbs that predate the repo
        // coordinate. SQLite can't ALTER a PK, so rebuild: every old row is this
        // engine's own repo (the only one ever ingested before Phase 2), so stamp
        // its slug. The next reconcile wipes+rewrites `_file` anyway; stamping the
        // real slug keeps that tick's prev/current keys matching (no false churn).
        let slug = self.self_slug();
        if !self.column_exists("_file", "repo")? {
            self.db.conn().execute_batch(&format!(
                "ALTER TABLE _file RENAME TO _file_old;
                 CREATE TABLE _file (repo TEXT NOT NULL DEFAULT '', path TEXT, rev TEXT, hash TEXT,
                     mtime INTEGER DEFAULT 0, size INTEGER DEFAULT 0, lines INTEGER DEFAULT -1, PRIMARY KEY (repo, path, rev));
                 INSERT INTO _file (repo, path, rev, hash, mtime, size)
                     SELECT '{s}', path, rev, hash, mtime, size FROM _file_old;
                 DROP TABLE _file_old;",
                s = slug.replace('\'', "''"),
            ))?;
        }
        if !self.column_exists("_prov", "repo")? {
            self.db.conn().execute_batch(&format!(
                "ALTER TABLE _prov RENAME TO _prov_old;
                 CREATE TABLE _prov (rel TEXT, repo TEXT NOT NULL DEFAULT '', path TEXT, src TEXT,
                     PRIMARY KEY (rel, repo, path, src));
                 INSERT INTO _prov (rel, repo, path, src)
                     SELECT rel, '{s}', path, src FROM _prov_old;
                 DROP TABLE _prov_old;",
                s = slug.replace('\'', "''"),
            ))?;
        }
        // _node_embeddings gained an edge_digest column (W2 vector cache). It is
        // a pure derived cache (vectors re-embed on the next tick), so an old
        // single-digest table is dropped and rebuilt empty, not data-migrated.
        if !self.column_exists("_node_embeddings", "edge_digest")? {
            self.db.conn().execute_batch(
                "DROP TABLE IF EXISTS _node_embeddings;
                 CREATE TABLE _node_embeddings (
                     node TEXT NOT NULL, graph TEXT NOT NULL,
                     edge_digest TEXT NOT NULL DEFAULT '',
                     dim INTEGER NOT NULL, vec TEXT NOT NULL,
                     PRIMARY KEY (node, graph, edge_digest));
                 CREATE INDEX IF NOT EXISTS _node_embeddings_graph_idx ON _node_embeddings(graph);
                 CREATE INDEX IF NOT EXISTS _node_embeddings_gd_idx ON _node_embeddings(graph, edge_digest);")?;
        }
        Ok(())
    }

    /// Order-independent content digest of a relation: XOR-fold of the per-row
    /// `__src` hashes in `rel_<rel>`. The table is a set (PK on user cols), so
    /// each `__src` contributes once and XOR cannot cancel a duplicate; XOR is
    /// commutative + associative, so insert order does not matter. All-zero ⇒
    /// empty relation. Same row set ⇒ same digest; different rows ⇒ different
    /// (blake3). Lets a comment-only edit (bytes move, rows don't) skip rebuild.
    /// Does `table` already have a column named `col`? Used to gate one-shot
    /// schema migrations (a fresh db gets the new schema from `CREATE TABLE IF
    /// NOT EXISTS`; an old db keeps its columns and needs the rebuild).
    pub(crate) fn column_exists(&self, table: &str, col: &str) -> Result<bool> {
        let n: i64 = self.db.conn().query_row(
            &format!("SELECT COUNT(*) FROM pragma_table_info('{}') WHERE name = ?1", table.replace('\'', "''")),
            [col], |r| r.get(0))?;
        Ok(n > 0)
    }

    /// Load the persisted derived shapes from `_shapes` (Phase 5): shape name ->
    /// its columns in `pos` order. A `type` value that names a base type builds a plain
    /// column; anything else is a validated brand name (checked at persist time),
    /// so it lands a TEXT column carrying that brand. Read at the START of a tick
    /// (declare) to resolve a computed `rel name: shape.`.
    pub(crate) fn load_persisted_shapes(&self) -> Result<HashMap<String, Vec<Col>>> {
        let mut out: HashMap<String, Vec<Col>> = HashMap::new();
        let mut stmt = self.db.conn()
            .prepare("SELECT shape, col, type FROM _shapes ORDER BY shape, pos")?;
        let rows = stmt.query_map([], |r| Ok((
            r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
        for row in rows {
            let (shape, col, ty) = row?;
            let column = match Type::parse(&ty) {
                Some(base) => Col::plain(col, base),
                // A validated brand name (persist checked it): enum brands store
                // TEXT with the brand attached. A `<: int` brand base is not
                // reconstructed here (lands TEXT) — cosmetic, rules over the
                // derived rel type-checked at load when its cols were empty.
                None => Col::branded(&col, &ty),
            };
            out.entry(shape).or_default().push(column);
        }
        Ok(out)
    }

    /// Resolve every deferred `rel name: shape.` (shape_ref still set, cols empty)
    /// against the persisted derived shapes (Phase 5). Syntax `type name(...)`
    /// shapes already won at load (their refs are resolved before the engine sees
    /// the decl), so a ref still unresolved here is derived-only. Fills
    /// `self.rels[name]` via `declare` (which migrates a `rel_<name>` table on
    /// column drift and deletes its `_reldigest` row so it re-derives), or records
    /// a `shape-pending` info diag. A persisted shape that shares a name with a
    /// syntax shape records `shape-shadowed` (syntax won, the derived rows are
    /// ignored). Called at the top of a tick after the normal declare loop.
    pub(crate) fn resolve_derived_shapes(&mut self, prog: &Program) -> Result<()> {
        let deferred: Vec<(String, String)> = prog.items.iter().filter_map(|it| match it {
            Item::Rel(d) => d.shape_ref.as_ref().map(|s| (d.name.clone(), s.clone())),
            _ => None,
        }).collect();
        if deferred.is_empty() && !type_decl_row_used(prog) { return Ok(()); }
        let persisted = self.load_persisted_shapes()?;
        let builtins = builtin_rel_names();
        for (rel_name, shape) in &deferred {
            if builtins.contains(rel_name) { continue; } // exotic `rel diag: x` — leave the builtin alone
            match persisted.get(shape) {
                Some(cols) => {
                    let d = RelDecl { name: rel_name.clone(), cols: cols.clone(), ..Default::default() };
                    self.declare(&d)?;
                }
                None => self.shape_diags.push(DiagRow {
                    path: "(shapes)".into(), line: 1, col: 0, end_line: 1, end_col: 0,
                    severity: "info".into(), code: "shape-pending".into(),
                    msg: format!("rel `{rel_name}`: shape `{shape}` has no syntax `type {shape}(...)` \
                        decl and no derived rows yet — it derives from type_decl_row and becomes \
                        available on the next tick"),
                    hint: None,
                }),
            }
        }
        // A syntax `type X(...)` shadows a derived shape of the same name: syntax
        // won for any `rel _: X`, so the derived rows are unused. Warn once.
        let syntax_shapes: std::collections::HashSet<&str> = prog.items.iter()
            .filter_map(|it| if let Item::Shape(s) = it { Some(s.name.as_str()) } else { None })
            .collect();
        for shape in persisted.keys() {
            if syntax_shapes.contains(shape.as_str()) {
                self.shape_diags.push(DiagRow {
                    path: "(shapes)".into(), line: 1, col: 0, end_line: 1, end_col: 0,
                    severity: "warn".into(), code: "shape-shadowed".into(),
                    msg: format!("shape `{shape}` is declared both as a syntax `type {shape}(...)` \
                        and derived via type_decl_row; the syntax decl wins and the derived rows \
                        are ignored"),
                    hint: None,
                });
            }
        }
        Ok(())
    }

    /// Persist the `type_decl_row` sink's rows to `_shapes` (Phase 5), at the END
    /// of a tick (after the derived fixpoint filled `rel_type_decl_row`). Digest-
    /// guarded on the sink's content (a `shape:type_decl_row` key in `_reldigest`)
    /// so an unchanged sink does NOT re-persist or re-migrate every tick (the
    /// repo's recompute-guard rail). Each row's `type` must name a base type, an
    /// ambient builtin enum brand, or a program-declared brand; an unknown type
    /// records a `shape-unknown-type` warn and that whole shape is dropped from the
    /// persist (it stays pending). Full replace, batched (no per-row write).
    pub(crate) fn persist_type_decl_shapes(&mut self, prog: &Program) -> Result<()> {
        if !type_decl_row_used(prog) { return Ok(()); }

        // Valid ty vocabulary: base types + ambient builtin enum brands + program brands.
        let prog_brands: std::collections::HashSet<&str> = prog.items.iter()
            .filter_map(|it| if let Item::Brand(b) = it { Some(b.name.as_str()) } else { None })
            .collect();
        let ty_ok = |ty: &str| Type::parse(ty).is_some()
            || builtin_enum_variants(ty).is_some()
            || prog_brands.contains(ty);

        let mut stmt = self.db.conn()
            .prepare(&format!("SELECT shape, pos, col, type FROM {} ORDER BY shape, pos", tbl("type_decl_row")))?;
        let raw: Vec<(String, i64, String, String)> = stmt.query_map([], |r| Ok((
            r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .filter_map(|x| x.ok()).collect();
        drop(stmt);

        // Validate ty every tick (cheap, O(rows)) so the shape-unknown-type diag is
        // steady, not just on the tick the sink changed. Drop any shape carrying
        // an unknown type (loud diag), keep the rest.
        let mut bad_shapes: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (shape, _, _, ty) in &raw {
            if !ty_ok(ty) && bad_shapes.insert(shape.clone()) {
                self.shape_diags.push(DiagRow {
                    path: "(shapes)".into(), line: 1, col: 0, end_line: 1, end_col: 0,
                    severity: "warn".into(), code: "shape-unknown-type".into(),
                    msg: format!("derived shape `{shape}` names an unknown type `{ty}` — use a base \
                        type (text/int/path/file/dir/repo/rev) or a declared brand; the shape stays pending"),
                    hint: None,
                });
            }
        }
        // The WRITE is the recompute-guarded step: gate the DELETE + insert on the
        // sink's content digest so an unchanged sink does not re-migrate every tick
        // (the repo's recompute-guard rail).
        let digest = self.rel_content_digest("type_decl_row", &self.rels["type_decl_row"].clone())?;
        if self.load_rel_digest("shape:type_decl_row")? == Some(digest) { return Ok(()); }
        let rows: Vec<Vec<Value>> = raw.iter()
            .filter(|(shape, _, _, _)| !bad_shapes.contains(shape))
            .map(|(shape, pos, col, ty)| vec![
                Value::Text(shape.clone()), Value::Int(*pos),
                Value::Text(col.clone()), Value::Text(ty.clone())])
            .collect();
        self.db.conn().execute("DELETE FROM _shapes", [])?;
        self.db.insert_rows("_shapes", &["shape", "pos", "col", "type"], &rows)?;
        self.save_rel_digest("shape:type_decl_row", &digest)?;
        Ok(())
    }

    pub(crate) fn rel_digest(&self, rel: &str) -> Result<[u8; 32]> {
        let mut acc = [0u8; 32];
        let mut stmt = self.db.conn().prepare(&format!("SELECT __src FROM {}", tbl(rel)))?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let src: String = row.get(0).unwrap_or_default();
            if let Ok(bytes) = hex_to_32(&src) {
                for (a, b) in acc.iter_mut().zip(bytes.iter()) { *a ^= *b; }
            }
        }
        Ok(acc)
    }

    /// Order-independent content digest of a rel's LIVE table over its declared
    /// columns (not `__src`, which carry-loaded rows leave blank). Per-row blake3,
    /// XOR-folded so row order does not matter; relations are sets (PK-deduped) so
    /// no two rows are identical and the XOR never self-cancels. Used by
    /// `load_carry` to tell whether a carried rel actually moved this tick.
    pub(crate) fn rel_content_digest(&self, rel: &str, meta: &RelMeta) -> Result<[u8; 32]> {
        let sql = if meta.cols.is_empty() {
            format!("SELECT COUNT(*) FROM {}", tbl(rel))
        } else {
            let cl = meta.cols.iter().map(|c| format!("\"{}\"", c.name))
                .collect::<Vec<_>>().join(", ");
            format!("SELECT {cl} FROM {}", tbl(rel))
        };
        self.digest_of_query(&sql, [])
    }

    /// Whether the @next carry staged at `tx` differs from `rel`'s live rows —
    /// the non-destructive twin of `load_carry` (which applies the carry as its
    /// only mode). Used by the settle report to peek "will next tick move".
    pub(crate) fn carry_differs(&self, rel: &str, meta: &RelMeta, tx: i64) -> Result<bool> {
        let live = self.rel_content_digest(rel, meta)?;
        let cl = if meta.cols.is_empty() {
            "COUNT(*)".to_string()
        } else {
            meta.cols.iter().map(|c| format!("\"{}\"", c.name))
                .collect::<Vec<_>>().join(", ")
        };
        let sql = format!("SELECT {cl} FROM {} WHERE tx = ?1", carry_tbl(rel));
        let staged = self.digest_of_query(&sql, [tx])?;
        Ok(live != staged)
    }

    /// Order-independent (XOR-folded) content digest of a query's rows. Shared
    /// by `rel_content_digest` and `carry_differs`.
    pub(crate) fn digest_of_query(&self, sql: &str, params: impl rusqlite::Params) -> Result<[u8; 32]> {
        let mut acc = [0u8; 32];
        let mut stmt = self.db.conn().prepare(sql)?;
        let ncol = stmt.column_count();
        let mut rows = stmt.query(params)?;
        while let Some(row) = rows.next()? {
            let mut h = blake3::Hasher::new();
            for i in 0..ncol {
                match row.get::<_, rusqlite::types::Value>(i)? {
                    rusqlite::types::Value::Integer(n) => { h.update(b"i"); h.update(&n.to_le_bytes()); }
                    rusqlite::types::Value::Real(f) => { h.update(b"r"); h.update(&f.to_le_bytes()); }
                    rusqlite::types::Value::Text(s) => { h.update(b"t"); h.update(s.as_bytes()); }
                    rusqlite::types::Value::Blob(b) => { h.update(b"b"); h.update(&b); }
                    rusqlite::types::Value::Null => { h.update(b"n"); }
                }
                h.update(&[0]);
            }
            let d = h.finalize();
            for (a, b) in acc.iter_mut().zip(d.as_bytes().iter()) { *a ^= *b; }
        }
        Ok(acc)
    }

    pub(crate) fn load_rel_digest(&self, rel: &str) -> Result<Option<[u8; 32]>> {
        let hex: Option<String> = self.db.conn()
            .query_row("SELECT digest FROM _reldigest WHERE rel = ?1", [rel], |r| r.get(0))
            .ok();
        Ok(hex.and_then(|h| hex_to_32(&h).ok()))
    }

    pub(crate) fn save_rel_digest(&self, rel: &str, digest: &[u8; 32]) -> Result<()> {
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        self.db.conn().execute(
            "INSERT INTO _reldigest(rel, digest) VALUES (?1, ?2)
             ON CONFLICT(rel) DO UPDATE SET digest = excluded.digest",
            rusqlite::params![rel, hex])?;
        Ok(())
    }

    /// Digest each relation's source-rule TEXT (not row content): extraction
    /// rows are a function of (file content, rule), so an edited regex/glob/
    /// capture invalidates the per-file hash fast path in `reconcile_sources`
    /// even though no file moved. XOR-fold of per-rule blake3 over the Debug
    /// repr, so rule order within a relation is irrelevant. Stored in
    /// `_reldigest` under a `src:` key (distinct namespace from row-content
    /// digests). Returns (dirty rels, pending saves); the caller persists the
    /// saves only after re-extraction lands, so a failed tick retries.
    pub(crate) fn source_rule_digests(&self, source_rules: &[&Rule])
        -> Result<(HashSet<String>, Vec<(String, [u8; 32])>)>
    {
        let mut by_rel: HashMap<String, [u8; 32]> = HashMap::new();
        for r in source_rules {
            let h = blake3::hash(format!("{r:?}").as_bytes());
            let acc = by_rel.entry(r.head.rel.clone()).or_insert([0u8; 32]);
            for (a, b) in acc.iter_mut().zip(h.as_bytes()) { *a ^= b; }
        }
        let mut dirty = HashSet::new();
        let mut pending = Vec::new();
        for (rel, d) in by_rel {
            let key = format!("src:{rel}");
            if self.load_rel_digest(&key)? != Some(d) {
                dirty.insert(rel);
                pending.push((key, d));
            }
        }
        Ok((dirty, pending))
    }

    /// Drop from `changed` every relation whose freshly computed digest equals
    /// its stored digest (the file's bytes moved but the extracted rows did
    /// not). Records the new digest for the relations that really changed.
    /// This is v4's `Replay` short-circuit at relation granularity.
    pub(crate) fn prune_unchanged_by_digest(&self, changed: HashSet<String>) -> Result<HashSet<String>> {
        let mut out = HashSet::new();
        for rel in changed {
            let d_new = self.rel_digest(&rel)?;
            if self.load_rel_digest(&rel)? == Some(d_new) { continue; }
            self.save_rel_digest(&rel, &d_new)?;
            out.insert(rel);
        }
        Ok(out)
    }

    /// Seed `_reldigest` for every source relation, so the first delta after a
    /// cold run has a baseline to compare against. Returns the relations whose
    /// digest MOVED against the stored baseline (first-ever seeding counts as
    /// moved) — the full tick's per-rel change attribution, feeding the same
    /// `affected_derived` scoping `tick_paths` uses (perf gap B). An unchanged
    /// relation skips the save.
    pub(crate) fn seed_rel_digests(&self, source_rels: &[String]) -> Result<Vec<String>> {
        let mut moved = Vec::new();
        for rel in source_rels {
            let d = self.rel_digest(rel)?;
            if self.load_rel_digest(rel)? == Some(d) { continue; }
            self.save_rel_digest(rel, &d)?;
            moved.push(rel.clone());
        }
        Ok(moved)
    }

    /// P1 fix: which of `derived_rels` have NEVER completed a `rebuild_derived`
    /// pass (no `_derived_complete` marker) — the honest "must full-rebuild"
    /// signal. The old `any_derived_empty` asked "is this rel's table empty
    /// right now", which is also true for a rel a rule legitimately derived to
    /// zero rows (an inert rail, a diff view with nothing this tick), forcing
    /// a full rebuild of every derived rel on every subsequent tick. This is
    /// ONE query (load every completed marker into a set) instead of a
    /// `COUNT(*)` round trip per rel.
    pub(crate) fn derived_incomplete_rels(&self, derived_rels: &[String]) -> Result<Vec<String>> {
        if derived_rels.is_empty() { return Ok(Vec::new()); }
        let mut complete: HashSet<String> = HashSet::new();
        let mut stmt = self.db.conn().prepare("SELECT rel FROM _derived_complete")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            complete.insert(row.get::<_, String>(0)?);
        }
        Ok(derived_rels.iter().filter(|r| !complete.contains(r.as_str())).cloned().collect())
    }

    /// Mark every rel in `derived_rels` as having completed a rebuild pass —
    /// called once at the end of `rebuild_derived` for exactly the rels it was
    /// asked to rebuild (whatever row count they end with, including zero).
    /// `INSERT OR IGNORE` via the plural `insert_rows` seam, so this is one
    /// statement (chunked), never a per-rel write.
    pub(crate) fn mark_derived_complete(&self, derived_rels: &[String]) -> Result<()> {
        if derived_rels.is_empty() { return Ok(()); }
        let rows: Vec<Vec<Value>> = derived_rels.iter().map(|r| vec![Value::Text(r.clone())]).collect();
        self.db.insert_rows("_derived_complete", &["rel"], &rows)?;
        Ok(())
    }

    pub(crate) fn load_file_meta(&self) -> Result<FileMeta> {
        let mut stmt = self.db.conn().prepare("SELECT repo, path, rev, hash, mtime, size, lines FROM _file")?;
        let rows = stmt.query_map([], |r| Ok((
            (r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?),
            (r.get::<_, String>(3)?, r.get::<_, i64>(4)?, r.get::<_, i64>(5)?, r.get::<_, i64>(6)?),
        )))?;
        Ok(rows.filter_map(|x| x.ok()).collect())
    }

    /// Persist the `_file` cache DIFFERENTIALLY: delete keys that vanished or
    /// changed, insert keys that changed or are new. A warm no-change tick
    /// writes zero rows (the old shape rewrote the whole table every tick —
    /// O(total files) of churn per tick across a big repo config). The spine
    /// `_files` insert rides the same delta: content rows are INSERT-only and
    /// content-addressed, so unchanged keys need no re-touch.
    pub(crate) fn save_file_meta(&self, current: &FileMeta, prev: &FileMeta) -> Result<()> {
        let mut delta: FileMeta = HashMap::new();
        let mut stale: Vec<Vec<Value>> = Vec::new();
        for (k, v) in current {
            if prev.get(k) != Some(v) {
                delta.insert(k.clone(), v.clone());
                if prev.contains_key(k) {
                    stale.push(vec![Value::Text(k.0.clone()), Value::Text(k.1.clone()), Value::Text(k.2.clone())]);
                }
            }
        }
        for k in prev.keys() {
            if !current.contains_key(k) {
                stale.push(vec![Value::Text(k.0.clone()), Value::Text(k.1.clone()), Value::Text(k.2.clone())]);
            }
        }
        if !stale.is_empty() {
            self.db.exec("CREATE TEMP TABLE IF NOT EXISTS _stale_file(repo TEXT, path TEXT, rev TEXT, PRIMARY KEY (repo, path, rev))")?;
            self.db.exec("DELETE FROM _stale_file")?;
            self.db.insert_rows("_stale_file", &["repo", "path", "rev"], &stale)?;
            self.db.exec("DELETE FROM _file WHERE (repo, path, rev) IN (SELECT repo, path, rev FROM _stale_file)")?;
        }
        let rows: Vec<Vec<Value>> = delta.iter().map(|((repo, path, rev), (h, mt, sz, lines))| vec![
            Value::Text(repo.clone()),
            Value::Text(path.clone()),
            Value::Text(rev.clone()),
            Value::Text(h.clone()),
            Value::Int(*mt),
            Value::Int(*sz),
            Value::Int(*lines),
        ]).collect();
        self.db.insert_rows("_file", &["repo", "path", "rev", "hash", "mtime", "size", "lines"], &rows)?;
        self.insert_spine_files(&delta)?;
        Ok(())
    }

    pub(crate) fn insert_spine_files(&self, current: &FileMeta) -> Result<usize> {
        let mut by_id: BTreeMap<String, (String, String, i64)> = BTreeMap::new();
        for ((_repo, path, _rev), (hash, _mt, size, _lines)) in current {
            let Some(id) = spine::FileId::from_content_address(hash, *size) else { continue };
            if id == spine::FileId::SYNTHETIC { continue; }
            let entry = by_id.entry(id.to_string()).or_insert_with(|| (hash.clone(), path.clone(), *size));
            if path < &entry.1 {
                entry.1 = path.clone();
            }
        }
        let file_rows: Vec<Vec<Value>> = by_id.into_iter()
            .map(|(id, (content_hash, path, size))| {
                vec![Value::Text(id), Value::Text(content_hash), Value::Text(path), Value::Int(size)]
            })
            .collect();
        self.db.insert_rows("_files", &["id", "content_hash", "path", "size"], &file_rows)
    }

    pub(crate) fn current_tx(&self) -> Result<i64> {
        Ok(self.db.conn().query_row(
            "SELECT tx FROM _carry_meta WHERE k = 'tx'", [], |r| r.get(0))?)
    }

    /// Advance the carry clock to `tx` (called once per tick after staging).
    pub(crate) fn set_tx(&self, tx: i64) -> Result<()> {
        self.db.conn().execute("UPDATE _carry_meta SET tx = ?1 WHERE k = 'tx'", [tx])?;
        Ok(())
    }

    /// Create a carry buffer table mirroring the live rel's columns plus `tx`.
    /// PK is (all rel cols, tx) so a re-tick at the same generation is idempotent.
    pub(crate) fn ensure_carry_table(&self, rel: &str, meta: &RelMeta) -> Result<()> {
        let cols: Vec<String> = meta.cols.iter()
            .map(|c| format!("\"{}\" {}", c.name, c.ty.sql())).collect();
        let pk: Vec<String> = meta.cols.iter().map(|c| format!("\"{}\"", c.name)).collect();
        let sql = format!(
            "CREATE TABLE IF NOT EXISTS {} ({}, tx INTEGER NOT NULL, PRIMARY KEY ({}, tx))",
            carry_tbl(rel), cols.join(", "), pk.join(", "));
        self.db.conn().execute(&sql, [])?;
        Ok(())
    }

    /// Replace the live rel with the carry rows staged for generation `tx`.
    /// Load the carry rows staged for `tx` into the live rel table. Returns whether
    /// the loaded content DIFFERS from what the live table held before — a carry
    /// rel that advances is an EDB input change, so the caller must rebuild the
    /// derived rules that read it (a derived rule over a carried-in rel was
    /// otherwise frozen at its first value, since nothing flipped `changed`).
    pub(crate) fn load_carry(&self, rel: &str, meta: &RelMeta, tx: i64) -> Result<bool> {
        let before = self.rel_content_digest(rel, meta)?;
        let cl = meta.cols.iter().map(|c| format!("\"{}\"", c.name))
            .collect::<Vec<_>>().join(", ");
        self.db.conn().execute(&format!("DELETE FROM {}", tbl(rel)), [])?;
        self.db.conn().execute(
            &format!("INSERT OR IGNORE INTO {dst} ({cl}) SELECT {cl} FROM {src} WHERE tx = ?1",
                dst = tbl(rel), src = carry_tbl(rel)),
            [tx])?;
        let after = self.rel_content_digest(rel, meta)?;
        Ok(before != after)
    }

    /// Stage each @next rule's body (evaluated over the converged tick-T state)
    /// into its carry buffer at `cur + 1`. One pass: the body reads only relations
    /// that are already converged this tick (including the carried-in live rel),
    /// none of which change during staging, so no fixpoint is needed.
    pub(crate) fn rebuild_next(&self, next_rules: &[&Rule], next_rels: &[String], cur: i64) -> Result<()> {
        let nxt = cur + 1;
        for rel in next_rels {
            self.db.conn().execute(
                &format!("DELETE FROM {} WHERE tx = ?1", carry_tbl(rel)), [nxt])?;
        }
        for r in next_rules {
            let sql = crate::lower::lower_rule_to(
                r, &self.rels, &carry_tbl(&r.head.rel),
                &[("tx".to_string(), nxt.to_string())])?;
            self.db.conn().execute(&sql, [])?;
        }
        Ok(())
    }


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
    pub(crate) fn insert_spine_strings(&self, rows: &[(String, String, Vec<Value>)]) -> Result<usize> {
        let mut sink = spine::SymSink::new();
        for (_, _, row) in rows {
            for v in row {
                let Value::Text(s) = v else { continue };
                if s.is_empty() { continue; }
                sink.sym(s);
            }
        }
        self.db.flush_syms(&mut sink)
    }

    /// Batch located string occurrences into `_where_bytes`. Each row says
    /// "string S occupies bytes [lo, hi) in file F" — an INSERT-only index keyed
    /// by content-derived `WhereBytesId`, so duplicate occurrences (same string,
    /// same file, same span, reached via multiple binds) collapse to one row.
    /// `(repo, path)` is the source attribution `retract_paths` prunes by on
    /// reparse, and is folded into the row identity via `of_located` so two
    /// byte-identical files keep distinct rows — both within a repo (re-export
    /// stubs) and across two config repos sharing a path (otherwise the second
    /// row is lost on `INSERT OR IGNORE` and retraction misfires). The `repo`
    /// column holds the real slug (matching `_file`/`_prov`), not the vestigial
    /// `w.repo` u32.
    /// `text` (4th tuple slot) is the located source slice. When `Some`, it is
    /// interned into `_strings` under `StringId::of(text)` — the SAME id this
    /// WhereBytes already hashes — so EVERY located id round-trips through both
    /// `ref(id,_,_,lo,hi)` (the span) and `string(id,text,norm)` (the text).
    /// `None` is for callers that intern the text on a separate path (module
    /// spans, which call `insert_spine_strings` first).
    pub(crate) fn insert_spine_where_bytes(&self, wheres: &[(String, String, spine::WhereBytes, Option<String>)]) -> Result<usize> {
        if wheres.is_empty() { return Ok(0); }
        let mut by_id: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        let mut sink = spine::SymSink::new();
        for (repo, path, w, text) in wheres {
            let id = spine::WhereBytesId::of_located(*w, repo, path).to_string();
            by_id.entry(id.clone()).or_insert_with(|| vec![
                Value::Text(id),
                Value::Int(w.string.sqlite()),
                Value::Text(w.file.to_string()),
                Value::Int(w.lo as i64),
                Value::Int(w.hi as i64),
                Value::Text(repo.clone()),
                Value::Text(w.rev.to_string()),
                Value::Text(path.clone()),
            ]);
            if let Some(t) = text {
                if !t.is_empty() { sink.sym(t); }
            }
        }
        self.db.flush_syms(&mut sink)?;
        let rows: Vec<Vec<Value>> = by_id.into_values().collect();
        self.db.insert_rows("_where_bytes", &["id", "string_id", "file_id", "lo", "hi", "repo", "rev", "path"], &rows)
    }



}
