use super::*;
use crate::db::SqlVal;
use crate::lower::txt_tbl;

impl Engine {
    /// Located byte spans with their interned text, for the refactor sink:
    /// `_where_bytes ⋈ _strings`, sentinel skipped. Returns (path, lo, hi, text),
    /// where (lo, hi) is the rewrite coordinate in `path`'s WORK bytes and `text`
    /// is the contiguous source at that span. With a scan-only source program the
    /// only rows are import refs (no capture spans), so this is the `--move` feed.
    pub fn located_spans(&self) -> Result<Vec<(String, u32, u32, String)>> {
        self.db.query_rows(
            "_where_bytes",
            "SELECT w.path, w.lo, w.hi, s.content FROM _where_bytes w \
             JOIN _strings s ON s.id = w.string_id \
             WHERE w.id != '0' AND w.path != ''",
            &[],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)? as u32,
                    r.get::<_, i64>(2)? as u32,
                    r.get::<_, String>(3)?,
                ))
            },
        )
    }

    /// The WORK-content `FileId` for `path`, derived from the `_file` cache's
    /// stored blake3 the same way extraction derives it. Span queries filter
    /// `_where_bytes` by this id: a git-rev span shares the path attribution but
    /// its offsets index the old blob, so only rows whose file id matches the
    /// current WORK content are positionally valid for an editor cursor.
    pub(crate) fn work_file_id(&self, path: &str) -> Result<Option<spine::FileId>> {
        let row = self.db.query_opt(
            "_file",
            "SELECT hash, size FROM _file WHERE path = ?1 AND rev = ?2 LIMIT 1",
            &[SqlVal::from(path), SqlVal::from(self.self_rev_text())],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
        )?;
        Ok(row.and_then(|(hash, size)| {
            spine::FileId::from_content_address(&hash, size)
                .filter(|f| *f != spine::FileId::SYNTHETIC)
        }))
    }

    /// The innermost located WORK span containing `byte` in `path`, with its
    /// interned string: (string_id, text, lo, hi). Innermost so a cursor inside
    /// a brace-leaf import picks the leaf over the shared head span. None when
    /// the cursor is not on any located string. Drives the LSP
    /// definition/references handlers.
    pub fn span_at(&self, path: &str, byte: u32) -> Result<Option<(String, String, u32, u32)>> {
        let Some(fid) = self.work_file_id(path)? else {
            return Ok(None);
        };
        self.db.query_opt(
            "_where_bytes",
            "SELECT w.string_id, s.content, w.lo, w.hi FROM _where_bytes w \
             JOIN _strings s ON s.id = w.string_id \
             WHERE w.id != '0' AND w.path = ?1 AND w.file_id = ?2 \
               AND w.lo <= ?3 AND ?3 < w.hi \
             ORDER BY (w.hi - w.lo) ASC LIMIT 1",
            &[
                SqlVal::from(path),
                SqlVal::from(fid.to_string()),
                SqlVal::from(byte as i64),
            ],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?.to_string(),
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)? as u32,
                    r.get::<_, i64>(3)? as u32,
                ))
            },
        )
    }

    /// Every located WORK span of `string_id`, as (path, lo, hi). The
    /// file-id-matches-WORK-content filter runs in Rust per result path because
    /// the FileId derivation (blake3 prefix) is not expressible in SQL.
    /// `string_id` is the decimal-string form of `StringId::sqlite()` (as
    /// returned by `span_at`); parsed back to i64 to bind the INTEGER column.
    pub fn string_spans(&self, string_id: &str) -> Result<Vec<(String, u32, u32)>> {
        let sid: i64 = string_id.parse().unwrap_or(0);
        let candidates: Vec<(String, String, u32, u32)> = self.db.query_rows(
            "_where_bytes",
            "SELECT path, file_id, lo, hi FROM _where_bytes \
             WHERE string_id = ?1 AND id != '0' AND path != '' \
             ORDER BY path, lo",
            &[SqlVal::from(sid)],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)? as u32,
                    r.get::<_, i64>(3)? as u32,
                ))
            },
        )?;
        let mut work_ids: HashMap<String, Option<String>> = HashMap::new();
        let mut out = Vec::new();
        for (path, fid, lo, hi) in candidates {
            let want = match work_ids.get(&path) {
                Some(w) => w.clone(),
                None => {
                    let w = self.work_file_id(&path)?.map(|f| f.to_string());
                    work_ids.insert(path.clone(), w.clone());
                    w
                }
            };
            if want.as_deref() == Some(fid.as_str()) {
                out.push((path, lo, hi));
            }
        }
        Ok(out)
    }

    /// Definition targets for the located string `text` under a cursor in
    /// `file`. Two paths, tried in order:
    ///
    /// 1. **Phase E (rule-driven):** if the program declares a `def_target`
    ///    relation, query it by the cursor's text. The program writes rules
    ///    like `def_target(name, f, l, "fn") <- call_def(sym, _, f, l, _),
    ///    call_name(sym, name).` so go-to-def lands on real symbol definitions,
    ///    not just module edges. Returns `(file, line)` pairs.
    ///
    /// 2. **Fallback (module-edge):** the `module_edge(file, dst)` rows where a
    ///    segment of `text` names dst's module stem. Lands at line 0 (a module
    ///    edge is file-level; the spine carries no in-target symbol position).
    ///
    /// Empty when the span is not an import ref, no `def_target` match, and no
    /// module edge resolves. Drives the LSP `textDocument/definition` handler.
    pub fn definition_targets(&self, file: &str, text: &str) -> Result<Vec<(String, i64)>> {
        // Phase E: rule-driven def_target. Same pattern as `diags`: read by
        // column name so the program's column ordering/naming is flexible as
        // long as `name`, `file`, `line` are present.
        if let Some(meta) = self.rels.get("def_target") {
            let idx: HashMap<&str, usize> = meta
                .cols
                .iter()
                .enumerate()
                .map(|(i, c)| (c.name.as_str(), i))
                .collect();
            if let (Some(ni), Some(fi), Some(li)) =
                (idx.get("name"), idx.get("file"), idx.get("line"))
            {
                let cols: Vec<String> = meta
                    .cols
                    .iter()
                    .map(|c| format!("\"{}\"", c.name))
                    .collect();
                let sql = format!(
                    "SELECT DISTINCT \"{}\", \"{}\" FROM {} WHERE \"{}\" = ?1",
                    meta.cols[*fi].name,
                    meta.cols[*li].name,
                    txt_tbl("def_target"),
                    meta.cols[*ni].name
                );
                let out: Vec<(String, i64)> =
                    self.db
                        .query_rows("def_target", &sql, &[SqlVal::from(text)], |r| {
                            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
                        })?;
                let _ = cols; // (kept for symmetry with diags; unused today)
                if !out.is_empty() {
                    return Ok(out);
                }
            }
        }

        // Fallback: module_edge by specifier-segment match. Line 0 (file-level).
        let dsts: Vec<String> = self.db.query_rows(
            "module_edge",
            &format!(
                "SELECT DISTINCT \"dst\" FROM {} WHERE \"src\" = ?1",
                txt_tbl("module_edge")
            ),
            &[SqlVal::from(file)],
            |r| Ok(r.get::<_, String>(0)?),
        )?;
        let segs: HashSet<&str> = text
            .split(|c: char| c == ':' || c == '/')
            .filter(|s| !s.is_empty() && !matches!(*s, "crate" | "self" | "super" | "." | ".."))
            .collect();
        let matched: Vec<String> = dsts
            .iter()
            .filter(|d| segs.contains(module_stem(d)))
            .cloned()
            .collect();
        if matched.is_empty() && dsts.len() == 1 {
            return Ok(dsts.into_iter().map(|f| (f, 0)).collect());
        }
        Ok(matched.into_iter().map(|f| (f, 0)).collect())
    }

    /// Hover info for the located string `text` under a cursor. Auto-synthesizes
    /// a markdown summary by joining the type graph (`type_entity` by name) and
    /// the call graph (`call_name` -> `call_def`). No new builtin rel: the
    /// program opts in by referencing `type_entity` / `call_def` (the lazy
    /// indexers populate those tables when referenced). Returns None when no
    /// entity or callable shares the bare name.
    ///
    /// Drives the LSP `textDocument/hover` handler. One markdown block per
    /// match (a name may resolve to several entities/callables across modules).
    pub fn hover(&self, _file: &str, text: &str) -> Result<Option<String>> {
        // (kind, sym, file, line); deduped by sym so a callable that also has a
        // type_entity row (same sym shape) appears once.
        let mut entries: Vec<(String, String, String, i64)> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        // type_entity(_, name=text, kind, parent, file, line) -> one row per
        // entity whose bare name matches.
        let te = txt_tbl("type_entity");
        let te_rows: Vec<(String, String, String, i64)> = self.db.query_rows(
            "type_entity",
            &format!("SELECT \"sym\", \"kind\", \"file\", \"line\" FROM {te} WHERE \"name\" = ?1"),
            &[SqlVal::from(text)],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            },
        )?;
        for (sym, kind, file, line) in te_rows {
            if seen.insert(sym.clone()) {
                entries.push((kind, sym, file, line));
            }
        }

        // call_name(sym, text) -> call_def(sym, kind, file, line, _). The
        // intermediate join resolves the bare callee text to def syms; a bare
        // name may map to several defs (overloads, distinct modules).
        let cd = txt_tbl("call_def");
        let cn = txt_tbl("call_name");
        let cd_rows: Vec<(String, String, String, i64)> = self.db.query_rows(
            "call_def",
            &format!(
                "SELECT d.\"sym\", d.\"kind\", d.\"file\", d.\"line\" \
                 FROM {cd} d JOIN {cn} n ON n.\"sym\" = d.\"sym\" \
                 WHERE n.\"name\" = ?1"
            ),
            &[SqlVal::from(text)],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            },
        )?;
        for (sym, kind, file, line) in cd_rows {
            if seen.insert(sym.clone()) {
                entries.push((kind, sym, file, line));
            }
        }

        if entries.is_empty() {
            return Ok(None);
        }
        entries.sort();
        let mut md = String::new();
        for (i, (kind, sym, file, line)) in entries.into_iter().enumerate() {
            if i > 0 {
                md.push_str("\n\n---\n\n");
            }
            md.push_str(&format!("**{kind}** `{sym}`  \n{file}:{line}"));
            // Type-profile overlay for data types: Ca, Ce, fields, variants.
            // Callable kinds (function/method) don't have field/variant edges,
            // so the overlay is data-type-only. Cheap (4 COUNT queries on an
            // indexed column); no-op when type_edge isn't populated.
            if matches!(
                kind.as_str(),
                "struct" | "enum" | "trait" | "class" | "interface" | "alias"
            ) {
                if let Some(profile) = self.type_profile_overlay(&sym) {
                    md.push_str(&format!("  \n{profile}"));
                }
            }
        }
        Ok(Some(md))
    }

    /// User-headed markdown notes covering the point (`line`, `character`) in
    /// `path` — the `hover_note` sink (see `HOVER_RELS`). Positions are
    /// 0-based, `end_line`/`end_col` inclusive, the same convention as `diag`.
    /// Sorted by `md` so the merge order is stable across ticks. Tolerates a
    /// db where `hover_note` never derived (undeclared/empty table): returns
    /// an empty Vec rather than erroring, via `try_rows`.
    pub fn hover_notes_at(&self, path: &str, line: u32, character: u32) -> Result<Vec<String>> {
        let hn = txt_tbl("hover_note");
        Ok(self.try_rows(
            "hover_note",
            &format!(
                "SELECT \"md\" FROM {hn} WHERE \"path\" = ?1 \
                 AND (\"line\" < ?2 OR (\"line\" = ?2 AND \"col\" <= ?3)) \
                 AND (\"end_line\" > ?2 OR (\"end_line\" = ?2 AND \"end_col\" >= ?3)) \
                 ORDER BY \"md\""
            ),
            &[
                SqlVal::from(path),
                SqlVal::from(line),
                SqlVal::from(character),
            ],
            |r| r.get::<_, String>(0),
        ))
    }

    /// One-line type profile for `sym`: `Ca=N Ce=M fields=F variants=V impls=I`.
    /// `sym` is the type_entity sym; type_edge is name-keyed, so the trailing
    /// identifier of the sym (the bare name) is the join key. Returns None if
    /// type_edge is empty (program didn't opt into type rels) or all counts
    /// are zero (the type has no structural edges).
    pub(crate) fn type_profile_overlay(&self, sym: &str) -> Option<String> {
        let name = sym.rsplit("::").next()?;
        let edge = txt_tbl("type_edge");
        let q = |sql: String| -> Option<i64> {
            self.db
                .query_one("type_edge", &sql, &[SqlVal::from(name)], |r| {
                    Ok(r.get::<_, i64>(0)?)
                })
                .ok()
        };
        let ca = q(format!(
            "SELECT COUNT(DISTINCT \"from\") FROM {edge} WHERE \"to\" = ?1"
        ))?;
        let ce = q(format!(
            "SELECT COUNT(DISTINCT \"to\") FROM {edge} WHERE \"from\" = ?1"
        ))?;
        let fields = q(format!(
            "SELECT COUNT(DISTINCT \"to\") FROM {edge} WHERE \"from\" = ?1 AND \"kind\" = 'field'"
        ))?;
        let variants = q(format!(
            "SELECT COUNT(DISTINCT \"to\") FROM {edge} WHERE \"from\" = ?1 AND \"kind\" = 'variant'"))?;
        let impls = q(format!(
            "SELECT COUNT(DISTINCT \"to\") FROM {edge} WHERE \"from\" = ?1 AND \"kind\" = 'impl'"
        ))?;
        if ca == 0 && ce == 0 && fields == 0 && variants == 0 && impls == 0 {
            return None;
        }
        Some(format!(
            "Ca={ca} Ce={ce} fields={fields} variants={variants} impls={impls}"
        ))
    }

    /// Best-effort SELECT that tolerates a missing table (a builtin type/call/
    /// module rel the program never referenced, so its `rel_<name>` table was
    /// never created). Any prepare/query/row-map error yields an empty vec, so
    /// `refs_lens` degrades to whatever families ARE populated instead of
    /// erroring out. `rel` names the table chiefly read (N+1 counter key).
    pub(crate) fn try_rows<T, F>(&self, rel: &str, sql: &str, params: &[SqlVal], mut f: F) -> Vec<T>
    where
        F: FnMut(&crate::db::SqlRow) -> crate::db::SqlRowResult<T>,
    {
        self.db
            .query_rows(rel, sql, params, |r| Ok(f(r)?))
            .unwrap_or_default()
    }

    /// The repo slug a WORK file answers to, from the `_file` cache. Used to
    /// attribute a `module_import` hit (whose rel carries no repo column) to the
    /// right repo; falls back to the self slug when the path isn't cached.
    pub(crate) fn repo_for_path(&self, path: &str) -> String {
        self.try_rows(
            "_file",
            "SELECT repo FROM _file WHERE path = ?1 AND rev = ?2 LIMIT 1",
            &[SqlVal::from(path), SqlVal::from(self.self_rev_text())],
            |r| r.get::<_, String>(0),
        )
        .into_iter()
        .next()
        .unwrap_or_else(|| self.self_slug())
    }

    /// A resolved-tier `RefHit` off a rel row: 1-based `line` from the graph
    /// becomes a 0-based whole-token-start range (the type/call rels carry no
    /// column, so col is 0).
    pub(crate) fn rel_hit(
        repo: String,
        file: String,
        line: i64,
        role: &str,
        container: String,
    ) -> RefHit {
        let line0 = (line - 1).max(0) as u32;
        RefHit {
            repo,
            path: file,
            line: line0,
            col: 0,
            end_line: line0,
            end_col: 0,
            role: role.to_string(),
            container,
        }
    }

    /// Resolve a definition symbol (`file::kind::name`) to a located `RefHit`,
    /// trying `type_entity` (has a parent) then `call_def`. None when neither
    /// family knows the sym (so an edge to an un-indexed target is dropped rather
    /// than pointing nowhere).
    pub(crate) fn resolve_sym_hit(&self, sym: &str, role: &str) -> Option<RefHit> {
        let te = txt_tbl("type_entity");
        if let Some((repo, file, line, parent)) = self.try_rows(
            "type_entity",
            &format!("SELECT \"repo\", \"file\", \"line\", \"parent\" FROM {te} WHERE \"sym\" = ?1 LIMIT 1"),
            &[SqlVal::from(sym)],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, String>(3)?)),
        ).into_iter().next() {
            let container = if parent.is_empty() { sym.to_string() } else { parent };
            return Some(Self::rel_hit(repo, file, line, role, container));
        }
        let cd = txt_tbl("call_def");
        if let Some((repo, file, line)) = self
            .try_rows(
                "call_def",
                &format!(
                    "SELECT \"repo\", \"file\", \"line\" FROM {cd} WHERE \"sym\" = ?1 LIMIT 1"
                ),
                &[SqlVal::from(sym)],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, i64>(2)?,
                    ))
                },
            )
            .into_iter()
            .next()
        {
            return Some(Self::rel_hit(repo, file, line, role, sym.to_string()));
        }
        None
    }

    /// A compiler-tier `RefHit` off a `scip_occurrence` row. SCIP positions are
    /// already 0-based, so they map straight to a `RefHit` range (unlike the
    /// resolved-tier `rel_hit`, which decrements a 1-based graph line).
    pub(crate) fn scip_hit(
        repo: String,
        file: String,
        line: i64,
        col: i64,
        end_line: i64,
        end_col: i64,
        role: &str,
        container: String,
    ) -> RefHit {
        RefHit {
            repo,
            path: file,
            line: line.max(0) as u32,
            col: col.max(0) as u32,
            end_line: end_line.max(0) as u32,
            end_col: end_col.max(0) as u32,
            role: role.to_string(),
            container,
        }
    }

    /// Every `role='definition'` occurrence of `sym`, as compiler-tier hits with
    /// the given edge `role` (used to resolve a caller/callee/impl symbol back to
    /// its declaration site). Empty when the symbol has no definition occurrence
    /// in the loaded index (an out-of-workspace target).
    pub(crate) fn scip_def_hits(&self, sym: &str, role: &str) -> Vec<RefHit> {
        let so = txt_tbl("scip_occurrence");
        self.try_rows(
            "scip_occurrence",
            &format!(
                "SELECT \"file\", \"line\", \"col\", \"end_line\", \"end_col\", \"repo\" \
                      FROM {so} WHERE \"symbol\" = ?1 AND \"role\" = 'definition'"
            ),
            &[SqlVal::from(sym)],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, String>(5)?,
                ))
            },
        )
        .into_iter()
        .map(|(file, line, col, el, ec, repo)| {
            Self::scip_hit(repo, file, line, col, el, ec, role, String::new())
        })
        .collect()
    }

    /// tier "compiler": when a SCIP index is loaded for the request repo, resolve
    /// the cursor to its symbol via the `scip_occurrence` covering `(path, byte)`,
    /// then group every occurrence of that symbol: declarations from role
    /// `definition`, uses by their ACTUAL role (import/read/write/reference).
    /// Callers/callees are 1-hop over `scip_fn_edge`, containing types over
    /// `scip_impl` (the interfaces this symbol implements). `scip_binding`
    /// supplies a use's local alias name as its `container` when it differs from
    /// the symbol's descriptor name, so an aliased import shows its local
    /// spelling. Returns None when no index is loaded for the repo or no
    /// occurrence covers the cursor — the caller then falls through to the
    /// resolved/textual tiers WITHOUT degrading them.
    pub(crate) fn compiler_lens(
        &self,
        path: &str,
        byte: u32,
        text: &str,
    ) -> Result<Option<RefLens>> {
        let so = txt_tbl("scip_occurrence");
        let repo = self.repo_for_path(path);

        // Cursor (line, col) 0-based from the file's WORK content. SCIP columns
        // are UTF-16 units; for ASCII identifiers (the overwhelming majority)
        // char and UTF-16 offsets coincide. Containment (not exact col equality)
        // tolerates the rare wide-char line.
        let roots = self.repo_roots();
        let root = roots
            .get(&repo)
            .cloned()
            .unwrap_or_else(|| self.root.clone());
        let content = std::fs::read_to_string(root.join(path)).unwrap_or_default();
        let (cl, cc) = byte_to_lc0(&content, byte);
        let (cl, cc) = (cl as i64, cc as i64);

        // The innermost occurrence covering the cursor in this file/repo.
        let mut covering: Vec<(String, i64, i64, i64, i64)> = self.try_rows(
            "scip_occurrence",
            &format!(
                "SELECT \"symbol\", \"line\", \"col\", \"end_line\", \"end_col\" \
                      FROM {so} WHERE \"file\" = ?1 AND \"repo\" = ?2"
            ),
            &[SqlVal::from(path), SqlVal::from(&repo)],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?,
                ))
            },
        );
        covering.retain(|(_, sl, sc, el, ec)| {
            let after_start = *sl < cl || (*sl == cl && *sc <= cc);
            let before_end = *el > cl || (*el == cl && *ec > cc);
            after_start && before_end
        });
        // Innermost = fewest lines, then narrowest columns.
        covering.sort_by_key(|(_, sl, sc, el, ec)| (el - sl, (ec - sc).max(0)));
        let Some((symbol, _, _, _, _)) = covering.into_iter().next() else {
            return Ok(None);
        };

        // Local alias names for this symbol: (file, line, col) -> local_name.
        let sb = txt_tbl("scip_binding");
        let mut aliases: HashMap<(String, i64, i64), String> = HashMap::new();
        for (file, line, col, name) in self.try_rows(
            "scip_binding",
            &format!("SELECT \"file\", \"line\", \"col\", \"local_name\" FROM {sb} WHERE \"symbol\" = ?1"),
            &[SqlVal::from(&symbol)],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, String>(3)?)),
        ) {
            aliases.insert((file, line, col), name);
        }
        let canonical = scip_descriptor_name(&symbol).unwrap_or_else(|| text.to_string());

        // Declarations (role 'definition') and uses (every other role), grouped
        // straight off the occurrence table.
        let mut declarations: Vec<RefHit> = Vec::new();
        let mut uses: Vec<RefHit> = Vec::new();
        for (file, line, col, el, ec, role, orepo) in self.try_rows(
            "scip_occurrence",
            &format!("SELECT \"file\", \"line\", \"col\", \"end_line\", \"end_col\", \"role\", \"repo\" \
                      FROM {so} WHERE \"symbol\" = ?1"),
            &[SqlVal::from(&symbol)],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?, r.get::<_, i64>(4)?, r.get::<_, String>(5)?, r.get::<_, String>(6)?)),
        ) {
            // An aliased spelling at this site adds display value; the canonical
            // name is redundant with `symbol` so only a divergent local shows.
            let container = match aliases.get(&(file.clone(), line, col)) {
                Some(local) if *local != canonical => local.clone(),
                _ => String::new(),
            };
            let hit = Self::scip_hit(orepo, file, line, col, el, ec, &role, container);
            if role == "definition" {
                declarations.push(hit);
            } else {
                uses.push(hit);
            }
        }

        // Containing types: the interfaces this symbol implements (scip_impl),
        // resolved to their definition site.
        let si = txt_tbl("scip_impl");
        let mut containing_types: Vec<RefHit> = Vec::new();
        for iface in self.try_rows(
            "scip_impl",
            &format!("SELECT \"iface\" FROM {si} WHERE \"impl\" = ?1"),
            &[SqlVal::from(&symbol)],
            |r| r.get::<_, String>(0),
        ) {
            containing_types.extend(self.scip_def_hits(&iface, "impl"));
        }

        // Callers / callees: 1-hop over the function-level call graph.
        let sfe = txt_tbl("scip_fn_edge");
        let mut callers: Vec<RefHit> = Vec::new();
        let mut callees: Vec<RefHit> = Vec::new();
        for caller in self.try_rows(
            "scip_fn_edge",
            &format!("SELECT \"caller\" FROM {sfe} WHERE \"callee\" = ?1"),
            &[SqlVal::from(&symbol)],
            |r| r.get::<_, String>(0),
        ) {
            callers.extend(self.scip_def_hits(&caller, "caller"));
        }
        for callee in self.try_rows(
            "scip_fn_edge",
            &format!("SELECT \"callee\" FROM {sfe} WHERE \"caller\" = ?1"),
            &[SqlVal::from(&symbol)],
            |r| r.get::<_, String>(0),
        ) {
            callees.extend(self.scip_def_hits(&callee, "callee"));
        }

        dedup_hits(&mut declarations);
        dedup_hits(&mut uses);
        dedup_hits(&mut containing_types);
        dedup_hits(&mut callers);
        dedup_hits(&mut callees);

        Ok(Some(RefLens {
            tier: "compiler".to_string(),
            symbol,
            display_name: text.to_string(),
            declarations,
            uses,
            containing_types,
            callers,
            callees,
        }))
    }

    /// Grouped references for the identifier at (`path`, `byte`), for the LSP
    /// `dl/refs` request and `textDocument/references`. Tier "compiler" (SCIP
    /// occurrence covering the cursor) is tried first; on a miss (no index for
    /// the repo, or no occurrence covers the cursor) it falls through to tier
    /// "resolved", which joins the identifier text by name
    /// against the type graph (`type_entity`) and call graph (`call_def` via
    /// `call_name`) for declarations, then collects uses from `type_link`,
    /// `call_site`, and `module_import`, containing types from the declaration
    /// parents, and 1-hop callers/callees over `call_edge` (no closure — unpinned
    /// closure reads are refused). When the identifier resolves to zero syms it
    /// falls back to tier "textual": every same-string span from the ref spine.
    /// None when the cursor is not on a located string.
    pub fn refs_lens(&self, path: &str, byte: usize) -> Result<Option<RefLens>> {
        let Some((_string_id, text, _lo, _hi)) = self.span_at(path, byte as u32)? else {
            return Ok(None);
        };

        // --- tier "compiler": a SCIP occurrence covering the cursor wins ---
        if let Some(lens) = self.compiler_lens(path, byte as u32, &text)? {
            return Ok(Some(lens));
        }

        // --- tier "resolved": declarations by name ---
        let mut declarations: Vec<RefHit> = Vec::new();
        // (sym, repo, file) per declaration, for the ambiguity preference.
        let mut decl_meta: Vec<(String, String, String)> = Vec::new();
        let mut syms: Vec<String> = Vec::new();
        let mut parents: Vec<String> = Vec::new();

        let te = txt_tbl("type_entity");
        for (repo, sym, kind, parent, file, line) in self.try_rows(
            "type_entity",
            &format!(
                "SELECT \"repo\", \"sym\", \"kind\", \"parent\", \"file\", \"line\" \
                      FROM {te} WHERE \"name\" = ?1"
            ),
            &[SqlVal::from(&text)],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, i64>(5)?,
                ))
            },
        ) {
            declarations.push(Self::rel_hit(
                repo.clone(),
                file.clone(),
                line,
                &kind,
                parent.clone(),
            ));
            decl_meta.push((sym.clone(), repo, file));
            if !syms.contains(&sym) {
                syms.push(sym);
            }
            if !parent.is_empty() && !parents.contains(&parent) {
                parents.push(parent);
            }
        }

        let cd = txt_tbl("call_def");
        let cn = txt_tbl("call_name");
        for (repo, sym, kind, file, line) in self.try_rows(
            "call_def",
            &format!(
                "SELECT d.\"repo\", d.\"sym\", d.\"kind\", d.\"file\", d.\"line\" \
                      FROM {cd} d JOIN {cn} n ON n.\"sym\" = d.\"sym\" WHERE n.\"name\" = ?1"
            ),
            &[SqlVal::from(&text)],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)?,
                ))
            },
        ) {
            if !syms.contains(&sym) {
                declarations.push(Self::rel_hit(
                    repo.clone(),
                    file.clone(),
                    line,
                    &kind,
                    String::new(),
                ));
                decl_meta.push((sym.clone(), repo, file));
                syms.push(sym);
            }
        }

        if syms.is_empty() {
            return Ok(Some(self.textual_lens(&text)?));
        }

        // Ambiguity preference: same-repo-then-same-file wins the `symbol`
        // display slot, but every declaration stays in the list (the lens shows
        // the split rather than silently dropping).
        let path_repo = self.repo_for_path(path);
        let symbol = decl_meta
            .iter()
            .find(|(_, repo, file)| *repo == path_repo && file == path)
            .or_else(|| decl_meta.iter().find(|(_, repo, _)| *repo == path_repo))
            .map(|(sym, _, _)| sym.clone())
            .unwrap_or_else(|| decl_meta[0].0.clone());

        // --- uses ---
        let mut uses: Vec<RefHit> = Vec::new();
        // type_link(src, dst, kind): a use of this symbol as a type. Resolve the
        // using `src` sym back to its declaration site.
        let tl = txt_tbl("type_link");
        for sym in &syms {
            for (src, kind) in self.try_rows(
                "type_link",
                &format!("SELECT \"src\", \"kind\" FROM {tl} WHERE \"dst\" = ?1"),
                &[SqlVal::from(sym)],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            ) {
                if let Some(hit) = self.resolve_sym_hit(&src, &kind) {
                    uses.push(hit);
                }
            }
        }
        // call_site(repo, caller, callee, file, line): callee is the bare name.
        let cs = txt_tbl("call_site");
        for (repo, caller, file, line) in self.try_rows(
            "call_site",
            &format!(
                "SELECT \"repo\", \"caller\", \"file\", \"line\" FROM {cs} WHERE \"callee\" = ?1"
            ),
            &[SqlVal::from(&text)],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            },
        ) {
            uses.push(Self::rel_hit(repo, file, line, "call", caller));
        }
        // module_import(file, rev, specifier, kind, line): an import whose
        // specifier names this identifier as one of its path segments.
        let mi = txt_tbl("module_import");
        for (file, specifier, line) in self.try_rows(
            "module_import",
            &format!(
                "SELECT \"file\", \"specifier\", \"line\" FROM {mi} \
                      WHERE \"kind\" IN ('use', 'import') AND \"specifier\" LIKE ?1"
            ),
            &[SqlVal::from(format!("%{text}%"))],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            },
        ) {
            let names = specifier
                .split(|c: char| c == ':' || c == '/' || c == '.')
                .any(|seg| seg == text);
            if !names {
                continue;
            }
            let repo = self.repo_for_path(&file);
            uses.push(Self::rel_hit(repo, file, line, "import", specifier));
        }

        // --- containing types: each declaration's parent, resolved ---
        let mut containing_types: Vec<RefHit> = Vec::new();
        for parent in &parents {
            if let Some(hit) = self.resolve_sym_hit(parent, "container") {
                containing_types.push(hit);
            }
        }

        // --- callers / callees: 1-hop over call_edge(caller, callee, kind) ---
        let mut callers: Vec<RefHit> = Vec::new();
        let mut callees: Vec<RefHit> = Vec::new();
        let ce = txt_tbl("call_edge");
        for sym in &syms {
            for caller in self.try_rows(
                "call_edge",
                &format!("SELECT \"caller\" FROM {ce} WHERE \"callee\" = ?1"),
                &[SqlVal::from(sym)],
                |r| r.get::<_, String>(0),
            ) {
                if let Some(hit) = self.resolve_sym_hit(&caller, "caller") {
                    callers.push(hit);
                }
            }
            for callee in self.try_rows(
                "call_edge",
                &format!("SELECT \"callee\" FROM {ce} WHERE \"caller\" = ?1"),
                &[SqlVal::from(sym)],
                |r| r.get::<_, String>(0),
            ) {
                if let Some(hit) = self.resolve_sym_hit(&callee, "callee") {
                    callees.push(hit);
                }
            }
        }

        dedup_hits(&mut declarations);
        dedup_hits(&mut uses);
        dedup_hits(&mut containing_types);
        dedup_hits(&mut callers);
        dedup_hits(&mut callees);

        Ok(Some(RefLens {
            tier: "resolved".to_string(),
            symbol,
            display_name: text,
            declarations,
            uses,
            containing_types,
            callers,
            callees,
        }))
    }

    /// First `role='definition'` occurrence of `sym` in the loaded SCIP index,
    /// as a bare `(repo, file, line)` site. A single `LIMIT 1` query, not a
    /// collection — the point-query twin of `scip_def_hits`, used only to
    /// locate a symbol whose covering occurrence is itself a USE.
    pub(crate) fn scip_def_site(&self, sym: &str) -> Option<(String, String, i64)> {
        let so = txt_tbl("scip_occurrence");
        self.try_rows(
            "scip_occurrence",
            &format!(
                "SELECT \"repo\", \"file\", \"line\" FROM {so} \
                      WHERE \"symbol\" = ?1 AND \"role\" = 'definition' LIMIT 1"
            ),
            &[SqlVal::from(sym)],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            },
        )
        .into_iter()
        .next()
    }

    /// tier "compiler" for `locate`: the same covering-occurrence lookup
    /// `compiler_lens` uses (innermost SCIP occurrence over the cursor), but
    /// stops at the symbol + role instead of grouping every use/caller/callee.
    /// When the covering occurrence's own role is already `definition` its site
    /// IS the declaration; otherwise one `scip_def_site` lookup finds it (falls
    /// back to the occurrence's own site when the symbol has no definition
    /// occurrence in this index, e.g. an out-of-workspace target). None when no
    /// index is loaded for the repo or no occurrence covers the cursor.
    pub(crate) fn compiler_locate(
        &self,
        path: &str,
        byte: u32,
        text: &str,
    ) -> Result<Option<LocateHit>> {
        let so = txt_tbl("scip_occurrence");
        let repo = self.repo_for_path(path);
        let roots = self.repo_roots();
        let root = roots
            .get(&repo)
            .cloned()
            .unwrap_or_else(|| self.root.clone());
        let content = std::fs::read_to_string(root.join(path)).unwrap_or_default();
        let (cl, cc) = byte_to_lc0(&content, byte);
        let (cl, cc) = (cl as i64, cc as i64);

        let mut covering: Vec<(String, i64, i64, i64, i64, String)> = self.try_rows(
            "scip_occurrence",
            &format!(
                "SELECT \"symbol\", \"line\", \"col\", \"end_line\", \"end_col\", \"role\" \
                      FROM {so} WHERE \"file\" = ?1 AND \"repo\" = ?2"
            ),
            &[SqlVal::from(path), SqlVal::from(&repo)],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, String>(5)?,
                ))
            },
        );
        covering.retain(|(_, sl, sc, el, ec, _)| {
            let after_start = *sl < cl || (*sl == cl && *sc <= cc);
            let before_end = *el > cl || (*el == cl && *ec > cc);
            after_start && before_end
        });
        covering.sort_by_key(|(_, sl, sc, el, ec, _)| (el - sl, (ec - sc).max(0)));
        let Some((symbol, occ_line, _, _, _, role)) = covering.into_iter().next() else {
            return Ok(None);
        };

        let (def_repo, def_file, def_line) = if role == "definition" {
            (repo, path.to_string(), occ_line)
        } else {
            self.scip_def_site(&symbol)
                .unwrap_or((repo, path.to_string(), occ_line))
        };

        Ok(Some(LocateHit {
            tier: "compiler".to_string(),
            symbol,
            display_name: text.to_string(),
            role,
            repo: def_repo,
            file: def_file,
            line: def_line.max(0) as u32,
        }))
    }

    /// tier "resolved" for `locate`: the identifier text joined by name against
    /// `type_entity` then `call_def`/`call_name` (the same two lookups
    /// `refs_lens` uses to build its declaration list), keeping only the
    /// preferred declaration (same-repo-then-same-file, matching `refs_lens`'s
    /// ambiguity rule) instead of collecting every declaration and every use.
    /// None when the identifier resolves to no symbol (locate has no textual
    /// tier — a grep-grade hit would center the graph on nothing meaningful).
    pub(crate) fn resolved_locate(&self, path: &str, text: &str) -> Result<Option<LocateHit>> {
        let path_repo = self.repo_for_path(path);

        let te = txt_tbl("type_entity");
        let mut candidates: Vec<(String, String, String, String, i64)> = self.try_rows(
            "type_entity",
            &format!("SELECT \"repo\", \"sym\", \"kind\", \"file\", \"line\" FROM {te} WHERE \"name\" = ?1"),
            &[SqlVal::from(text)],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?, r.get::<_, i64>(4)?)),
        );
        if candidates.is_empty() {
            let cd = txt_tbl("call_def");
            let cn = txt_tbl("call_name");
            candidates = self.try_rows(
                "call_def",
                &format!(
                    "SELECT d.\"repo\", d.\"sym\", d.\"kind\", d.\"file\", d.\"line\" \
                          FROM {cd} d JOIN {cn} n ON n.\"sym\" = d.\"sym\" WHERE n.\"name\" = ?1"
                ),
                &[SqlVal::from(text)],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, i64>(4)?,
                    ))
                },
            );
        }
        if candidates.is_empty() {
            return Ok(None);
        }

        let pick = candidates
            .iter()
            .find(|(repo, _, _, file, _)| *repo == path_repo && file == path)
            .or_else(|| {
                candidates
                    .iter()
                    .find(|(repo, _, _, _, _)| *repo == path_repo)
            })
            .cloned()
            .unwrap_or_else(|| candidates[0].clone());
        let (repo, sym, kind, file, line) = pick;

        Ok(Some(LocateHit {
            tier: "resolved".to_string(),
            symbol: sym,
            display_name: text.to_string(),
            role: kind,
            repo,
            file,
            line: (line - 1).max(0) as u32,
        }))
    }

    /// Cheap point lookup for the "follow the user" navigation surface (Track B
    /// B4, the `dl/locate` LSP request): the cursor at (`path`, `byte`) resolves
    /// to the symbol it sits on and that symbol's declaration site, trying tier
    /// "compiler" (a loaded SCIP index) then tier "resolved" (the type/call
    /// graph by name) — the HEAD of the same ladder `refs_lens` walks, minus the
    /// uses/callers/callees collection and minus the textual tier. None when
    /// the cursor is not on a located string, or the identifier resolves to no
    /// symbol in either tier.
    pub fn locate(&self, path: &str, byte: usize) -> Result<Option<LocateHit>> {
        let Some((_string_id, text, _lo, _hi)) = self.span_at(path, byte as u32)? else {
            return Ok(None);
        };
        if let Some(hit) = self.compiler_locate(path, byte as u32, &text)? {
            return Ok(Some(hit));
        }
        if let Some(hit) = self.resolved_locate(path, &text)? {
            return Ok(Some(hit));
        }
        Ok(None)
    }

    /// tier "textual": the ref-spine same-string fallback. Every located WORK
    /// span whose interned `_strings.content` equals `text`, converted to a
    /// 0-based range by reading each file from its own repo root. Role "text"
    /// (grep-grade). Used when the identifier resolves to no symbol.
    pub(crate) fn textual_lens(&self, text: &str) -> Result<RefLens> {
        let candidates: Vec<(String, String, String, u32, u32)> = self.try_rows(
            "_where_bytes",
            "SELECT w.repo, w.path, w.file_id, w.lo, w.hi FROM _where_bytes w \
             JOIN _strings s ON s.id = w.string_id \
             WHERE s.content = ?1 AND w.id != '0' AND w.path != '' \
             ORDER BY w.path, w.lo",
            &[SqlVal::from(text)],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)? as u32,
                    r.get::<_, i64>(4)? as u32,
                ))
            },
        );
        let roots = self.repo_roots();
        let mut work_ids: HashMap<String, Option<String>> = HashMap::new();
        let mut contents: HashMap<(String, String), String> = HashMap::new();
        let mut uses: Vec<RefHit> = Vec::new();
        for (repo, path, fid, lo, hi) in candidates {
            let want = match work_ids.get(&path) {
                Some(w) => w.clone(),
                None => {
                    let w = self.work_file_id(&path)?.map(|f| f.to_string());
                    work_ids.insert(path.clone(), w.clone());
                    w
                }
            };
            if want.as_deref() != Some(fid.as_str()) {
                continue;
            }
            let key = (repo.clone(), path.clone());
            let content = contents.entry(key).or_insert_with(|| {
                let root = roots
                    .get(&repo)
                    .cloned()
                    .unwrap_or_else(|| self.root.clone());
                std::fs::read_to_string(root.join(&path)).unwrap_or_default()
            });
            let (sl, sc) = byte_to_lc0(content, lo);
            let (el, ec) = byte_to_lc0(content, hi);
            uses.push(RefHit {
                repo,
                path,
                line: sl,
                col: sc,
                end_line: el,
                end_col: ec,
                role: "text".to_string(),
                container: String::new(),
            });
        }
        Ok(RefLens {
            tier: "textual".to_string(),
            symbol: String::new(),
            display_name: text.to_string(),
            declarations: Vec::new(),
            uses,
            containing_types: Vec::new(),
            callers: Vec::new(),
            callees: Vec::new(),
        })
    }
}
