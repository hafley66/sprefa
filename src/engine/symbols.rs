use super::*;
use crate::lower::txt_tbl;

impl Engine {
    /// Same-string spans of the identifier at (`path`, `byte`) restricted to
    /// `path` itself, for `textDocument/documentHighlight`. Reuses `span_at` (the
    /// innermost located string under the cursor) and `string_spans` (every WORK
    /// span of that interned string), then keeps only the cursor's own file so the
    /// highlight stays file-scoped and fast. Returns byte ranges `(lo, hi)`; empty
    /// when the cursor is not on a located string.
    pub fn document_highlights(&self, path: &str, byte: u32) -> Result<Vec<(u32, u32)>> {
        let Some((string_id, _text, _lo, _hi)) = self.span_at(path, byte)? else {
            return Ok(Vec::new());
        };
        Ok(self
            .string_spans(&string_id)?
            .into_iter()
            .filter(|(span_path, _, _)| span_path == path)
            .map(|(_, lo, hi)| (lo, hi))
            .collect())
    }

    /// Workspace-wide symbol search for `workspace/symbol`: every `type_entity`
    /// plus every callable (`call_def` joined `call_name`) whose name contains
    /// `query` (case-insensitive substring). Prefix matches sort first, then by
    /// name; the result is capped at `limit`. Each row carries its OWN repo so the
    /// LSP maps the slug to that repo's on-disk root. Tolerates a missing type or
    /// call family (returns whatever IS populated).
    pub fn workspace_symbols(&self, query: &str, limit: usize) -> Result<Vec<SymbolRow>> {
        let like = like_contains(query);
        let te = txt_tbl("type_entity");
        let mut rows: Vec<SymbolRow> = self.try_rows(
            &format!(
                "SELECT \"repo\", \"sym\", \"name\", \"kind\", \"parent\", \"file\", \"line\" \
                      FROM {te} WHERE \"name\" LIKE ?1 ESCAPE '\\'"
            ),
            &[&like],
            |r| {
                Ok(SymbolRow {
                    repo: r.get::<_, String>(0)?,
                    sym: r.get::<_, String>(1)?,
                    name: r.get::<_, String>(2)?,
                    kind: r.get::<_, String>(3)?,
                    parent: r.get::<_, String>(4)?,
                    file: r.get::<_, String>(5)?,
                    line: r.get::<_, i64>(6)?,
                    container: String::new(),
                })
            },
        );
        // A type_entity's `parent` sym is its enclosing symbol, the flat list's
        // container qualifier.
        for row in &mut rows {
            row.container = row.parent.clone();
        }
        let cd = txt_tbl("call_def");
        let cn = txt_tbl("call_name");
        let calls: Vec<SymbolRow> = self.try_rows(
            &format!(
                "SELECT d.\"repo\", d.\"sym\", n.\"name\", d.\"kind\", d.\"file\", d.\"line\" \
                      FROM {cd} d JOIN {cn} n ON n.\"sym\" = d.\"sym\" \
                      WHERE n.\"name\" LIKE ?1 ESCAPE '\\'"
            ),
            &[&like],
            |r| {
                Ok(SymbolRow {
                    repo: r.get::<_, String>(0)?,
                    sym: r.get::<_, String>(1)?,
                    name: r.get::<_, String>(2)?,
                    kind: r.get::<_, String>(3)?,
                    file: r.get::<_, String>(4)?,
                    line: r.get::<_, i64>(5)?,
                    parent: String::new(),
                    container: String::new(),
                })
            },
        );
        // Merge, dropping a callable already present as a type_entity sym (a
        // method is declared in both families).
        let mut seen: HashSet<String> = rows.iter().map(|s| s.sym.clone()).collect();
        for row in calls {
            if seen.insert(row.sym.clone()) {
                rows.push(row);
            }
        }
        // Prefix matches first, then by name; case-insensitive to match the LIKE.
        let needle = query.to_lowercase();
        rows.sort_by(|left, right| {
            let left_prefix = left.name.to_lowercase().starts_with(&needle);
            let right_prefix = right.name.to_lowercase().starts_with(&needle);
            right_prefix
                .cmp(&left_prefix)
                .then_with(|| left.name.cmp(&right.name))
        });
        rows.truncate(limit);
        Ok(rows)
    }

    /// Every `type_entity` declared in `path`, ordered by line, for
    /// `textDocument/documentSymbol`. Returns the flat rows; the LSP handler nests
    /// them into a `DocumentSymbol` tree by the `parent` sym (a member whose parent
    /// is not in the same file attaches at the top level). Tolerates a missing type
    /// family (empty). `line` is 1-based as stored.
    pub fn document_symbols(&self, path: &str) -> Result<Vec<SymbolRow>> {
        let te = txt_tbl("type_entity");
        Ok(self.try_rows(
            &format!(
                "SELECT \"repo\", \"sym\", \"name\", \"kind\", \"parent\", \"line\" \
                      FROM {te} WHERE \"file\" = ?1 ORDER BY \"line\""
            ),
            &[&path],
            |r| {
                Ok(SymbolRow {
                    repo: r.get::<_, String>(0)?,
                    sym: r.get::<_, String>(1)?,
                    name: r.get::<_, String>(2)?,
                    kind: r.get::<_, String>(3)?,
                    parent: r.get::<_, String>(4)?,
                    line: r.get::<_, i64>(5)?,
                    file: path.to_string(),
                    container: String::new(),
                })
            },
        ))
    }

    // ---------- call hierarchy (Track B B5) ----------

    /// `textDocument/prepareCallHierarchy`: the cursor at (`path`, `byte`)
    /// resolves to a callable `HierarchyItem`, trying tier "compiler" (a
    /// loaded SCIP index, restricted to a symbol that actually participates
    /// in `scip_fn_edge` — a struct or field moniker under the cursor would
    /// otherwise produce an item with an empty incoming/outgoing list) then
    /// tier "resolved" (`call_def`/`call_name` by identifier text ONLY, not
    /// `type_entity` — a type name that collides with a function name must
    /// not win the ambiguity pick here). None when the cursor is not on a
    /// located string or the identifier is not a callable in either tier.
    pub fn call_hierarchy_prepare(&self, path: &str, byte: usize) -> Result<Option<HierarchyItem>> {
        let Some((_string_id, text, _lo, _hi)) = self.span_at(path, byte as u32)? else {
            return Ok(None);
        };
        if let Some(item) = self.compiler_call_prepare(path, byte as u32, &text)? {
            return Ok(Some(item));
        }
        self.resolved_call_prepare(path, &text)
    }

    /// tier "compiler" prepare: the same covering-occurrence resolution
    /// `compiler_locate` uses, kept only when the symbol is a participant in
    /// `scip_fn_edge` (as caller or callee). `scip_fn_edge`/`scip_occurrence`
    /// carry no entity kind, so `kind` defaults to `"function"` — every
    /// `scip_fn_edge` participant IS a function/method by construction (the
    /// extractor's caller is always "the innermost enclosing fn def").
    fn compiler_call_prepare(
        &self,
        path: &str,
        byte: u32,
        text: &str,
    ) -> Result<Option<HierarchyItem>> {
        let Some(hit) = self.compiler_locate(path, byte, text)? else {
            return Ok(None);
        };
        let sfe = txt_tbl("scip_fn_edge");
        let participates = !self
            .try_rows(
                &format!("SELECT 1 FROM {sfe} WHERE \"caller\" = ?1 OR \"callee\" = ?1 LIMIT 1"),
                &[&hit.symbol],
                |r| r.get::<_, i64>(0),
            )
            .is_empty();
        if !participates {
            return Ok(None);
        }
        Ok(Some(HierarchyItem {
            tier: "compiler".to_string(),
            sym: String::new(),
            scip_symbol: hit.symbol,
            name: hit.display_name,
            kind: "function".to_string(),
            repo: hit.repo,
            file: hit.file,
            line: hit.line,
            end_line: hit.line,
        }))
    }

    /// tier "resolved" prepare: the identifier text joined against
    /// `call_def`/`call_name`, keeping the same same-repo-then-same-file
    /// ambiguity pick as `resolved_locate`.
    fn resolved_call_prepare(&self, path: &str, text: &str) -> Result<Option<HierarchyItem>> {
        let path_repo = self.repo_for_path(path);
        let cd = txt_tbl("call_def");
        let cn = txt_tbl("call_name");
        let candidates: Vec<(String, String, String, String, i64, i64)> = self.try_rows(
            &format!(
                "SELECT d.\"repo\", d.\"sym\", d.\"kind\", d.\"file\", d.\"line\", d.\"end\" \
                      FROM {cd} d JOIN {cn} n ON n.\"sym\" = d.\"sym\" WHERE n.\"name\" = ?1"
            ),
            &[&text],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, i64>(5)?,
                ))
            },
        );
        if candidates.is_empty() {
            return Ok(None);
        }
        let pick = candidates
            .iter()
            .find(|(repo, _, _, file, _, _)| *repo == path_repo && file == path)
            .or_else(|| candidates.iter().find(|(repo, ..)| *repo == path_repo))
            .cloned()
            .unwrap_or_else(|| candidates[0].clone());
        let (repo, sym, kind, file, line, end) = pick;
        Ok(Some(HierarchyItem {
            tier: "resolved".to_string(),
            sym,
            scip_symbol: String::new(),
            name: text.to_string(),
            kind,
            repo,
            file,
            line: (line - 1).max(0) as u32,
            end_line: (end.max(line) - 1).max(0) as u32,
        }))
    }

    /// `callHierarchy/incomingCalls`: every 1-hop caller of `item`. Tier-matched
    /// to how `item` resolved — no closure (an unpinned closure read over
    /// `call_edge`/`scip_fn_edge` is refused; the editor recurses by re-calling
    /// this on each returned item for a deeper tree).
    pub fn call_hierarchy_incoming(&self, item: &HierarchyItem) -> Result<Vec<HierarchyCallEdge>> {
        if item.tier == "compiler" {
            return self.compiler_incoming(item);
        }
        self.resolved_incoming(item)
    }

    /// `callHierarchy/outgoingCalls`: every 1-hop callee of `item`, same tier
    /// matching and no-closure discipline as `call_hierarchy_incoming`.
    pub fn call_hierarchy_outgoing(&self, item: &HierarchyItem) -> Result<Vec<HierarchyCallEdge>> {
        if item.tier == "compiler" {
            return self.compiler_outgoing(item);
        }
        self.resolved_outgoing(item)
    }

    /// `call_def`/`call_name` row for one sym -> a resolved-tier `HierarchyItem`,
    /// the by-sym twin of `resolved_call_prepare` (which resolves by name).
    fn resolved_call_item(&self, sym: &str) -> Option<HierarchyItem> {
        let cd = txt_tbl("call_def");
        let cn = txt_tbl("call_name");
        self.try_rows(
            &format!("SELECT d.\"repo\", d.\"kind\", d.\"file\", d.\"line\", d.\"end\", n.\"name\" \
                      FROM {cd} d JOIN {cn} n ON n.\"sym\" = d.\"sym\" WHERE d.\"sym\" = ?1 LIMIT 1"),
            &[&sym],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?, r.get::<_, i64>(4)?, r.get::<_, String>(5)?)),
        )
        .into_iter()
        .next()
        .map(|(repo, kind, file, line, end, name)| HierarchyItem {
            tier: "resolved".to_string(), sym: sym.to_string(), scip_symbol: String::new(),
            name, kind, repo, file,
            line: (line - 1).max(0) as u32,
            end_line: (end.max(line) - 1).max(0) as u32,
        })
    }

    /// Call-site line(s) of every `call_site` row matching (`caller_sym`,
    /// `callee_name`) — the resolved-tier `from_ranges` source, shared by both
    /// `resolved_incoming` (caller = the neighbor, callee = `item`'s name) and
    /// `resolved_outgoing` (caller = `item`'s sym, callee = the neighbor's name).
    fn resolved_call_site_lines(&self, caller_sym: &str, callee_name: &str) -> Vec<u32> {
        let cs = txt_tbl("call_site");
        self.try_rows(
            &format!("SELECT \"line\" FROM {cs} WHERE \"caller\" = ?1 AND \"callee\" = ?2"),
            &[&caller_sym, &callee_name],
            |r| r.get::<_, i64>(0),
        )
        .into_iter()
        .map(|line| (line - 1).max(0) as u32)
        .collect()
    }

    fn resolved_incoming(&self, item: &HierarchyItem) -> Result<Vec<HierarchyCallEdge>> {
        let ce = txt_tbl("call_edge");
        let callers: Vec<String> = self.try_rows(
            &format!("SELECT \"caller\" FROM {ce} WHERE \"callee\" = ?1"),
            &[&item.sym],
            |r| r.get::<_, String>(0),
        );
        let mut out = Vec::new();
        for caller_sym in callers {
            let Some(caller_item) = self.resolved_call_item(&caller_sym) else {
                continue;
            };
            let from_lines = self.resolved_call_site_lines(&caller_sym, &item.name);
            out.push(HierarchyCallEdge {
                item: caller_item,
                from_lines,
            });
        }
        Ok(out)
    }

    fn resolved_outgoing(&self, item: &HierarchyItem) -> Result<Vec<HierarchyCallEdge>> {
        let ce = txt_tbl("call_edge");
        let callees: Vec<String> = self.try_rows(
            &format!("SELECT \"callee\" FROM {ce} WHERE \"caller\" = ?1"),
            &[&item.sym],
            |r| r.get::<_, String>(0),
        );
        let mut out = Vec::new();
        for callee_sym in callees {
            let Some(callee_item) = self.resolved_call_item(&callee_sym) else {
                continue;
            };
            let from_lines = self.resolved_call_site_lines(&item.sym, &callee_item.name);
            out.push(HierarchyCallEdge {
                item: callee_item,
                from_lines,
            });
        }
        Ok(out)
    }

    /// Every non-definition occurrence of `sym` in `file` — the compiler-tier
    /// `from_ranges` approximation (`scip_fn_edge` pairs a caller/callee fn by
    /// symbol only, not the individual call occurrence's own position, so every
    /// non-definition occurrence of the target inside the caller's file stands
    /// in for "the call site(s)").
    fn scip_call_site_lines(&self, sym: &str, file: &str) -> Vec<u32> {
        let so = txt_tbl("scip_occurrence");
        self.try_rows(
            &format!("SELECT \"line\" FROM {so} WHERE \"symbol\" = ?1 AND \"file\" = ?2 AND \"role\" != 'definition'"),
            &[&sym, &file],
            |r| r.get::<_, i64>(0),
        )
        .into_iter()
        .map(|line| line.max(0) as u32)
        .collect()
    }

    fn compiler_incoming(&self, item: &HierarchyItem) -> Result<Vec<HierarchyCallEdge>> {
        let sfe = txt_tbl("scip_fn_edge");
        let callers: Vec<String> = self.try_rows(
            &format!("SELECT \"caller\" FROM {sfe} WHERE \"callee\" = ?1"),
            &[&item.scip_symbol],
            |r| r.get::<_, String>(0),
        );
        let mut out = Vec::new();
        for caller_sym in callers {
            let Some((repo, file, line)) = self.scip_def_site(&caller_sym) else {
                continue;
            };
            let from_lines = self.scip_call_site_lines(&item.scip_symbol, &file);
            let caller_item = HierarchyItem {
                tier: "compiler".to_string(),
                sym: String::new(),
                scip_symbol: caller_sym.clone(),
                name: scip_descriptor_name(&caller_sym).unwrap_or_default(),
                kind: "function".to_string(),
                repo,
                file,
                line: line.max(0) as u32,
                end_line: line.max(0) as u32,
            };
            out.push(HierarchyCallEdge {
                item: caller_item,
                from_lines,
            });
        }
        Ok(out)
    }

    fn compiler_outgoing(&self, item: &HierarchyItem) -> Result<Vec<HierarchyCallEdge>> {
        let sfe = txt_tbl("scip_fn_edge");
        let callees: Vec<String> = self.try_rows(
            &format!("SELECT \"callee\" FROM {sfe} WHERE \"caller\" = ?1"),
            &[&item.scip_symbol],
            |r| r.get::<_, String>(0),
        );
        let mut out = Vec::new();
        for callee_sym in callees {
            let Some((repo, file, line)) = self.scip_def_site(&callee_sym) else {
                continue;
            };
            let from_lines = self.scip_call_site_lines(&callee_sym, &item.file);
            let callee_item = HierarchyItem {
                tier: "compiler".to_string(),
                sym: String::new(),
                scip_symbol: callee_sym.clone(),
                name: scip_descriptor_name(&callee_sym).unwrap_or_default(),
                kind: "function".to_string(),
                repo,
                file,
                line: line.max(0) as u32,
                end_line: line.max(0) as u32,
            };
            out.push(HierarchyCallEdge {
                item: callee_item,
                from_lines,
            });
        }
        Ok(out)
    }

    // ---------- type hierarchy (Track B B5) ----------

    /// `textDocument/prepareTypeHierarchy`: the cursor at (`path`, `byte`)
    /// resolves to a type-shaped `HierarchyItem`, trying tier "compiler" (a
    /// `scip_impl` participant) then tier "resolved" (`type_entity` restricted
    /// to struct/enum/trait/class/interface — a function/alias/const under the
    /// cursor has no super/subtype relationship). None when neither tier
    /// resolves.
    pub fn type_hierarchy_prepare(&self, path: &str, byte: usize) -> Result<Option<HierarchyItem>> {
        let Some((_string_id, text, _lo, _hi)) = self.span_at(path, byte as u32)? else {
            return Ok(None);
        };
        if let Some(item) = self.compiler_type_prepare(path, byte as u32, &text)? {
            return Ok(Some(item));
        }
        self.resolved_type_prepare(path, &text)
    }

    /// tier "compiler" prepare: the `compiler_locate` covering-occurrence
    /// resolution, kept only when the symbol participates in `scip_impl`
    /// (as the implementing type or the interface/supertype). `kind` is a
    /// heuristic (`scip_impl` carries no entity kind): a symbol that only ever
    /// appears as `iface` is `"interface"`, otherwise `"class"`.
    fn compiler_type_prepare(
        &self,
        path: &str,
        byte: u32,
        text: &str,
    ) -> Result<Option<HierarchyItem>> {
        let Some(hit) = self.compiler_locate(path, byte, text)? else {
            return Ok(None);
        };
        let si = txt_tbl("scip_impl");
        let is_impl = !self
            .try_rows(
                &format!("SELECT 1 FROM {si} WHERE \"impl\" = ?1 LIMIT 1"),
                &[&hit.symbol],
                |r| r.get::<_, i64>(0),
            )
            .is_empty();
        let is_iface = !self
            .try_rows(
                &format!("SELECT 1 FROM {si} WHERE \"iface\" = ?1 LIMIT 1"),
                &[&hit.symbol],
                |r| r.get::<_, i64>(0),
            )
            .is_empty();
        if !is_impl && !is_iface {
            return Ok(None);
        }
        let kind = if is_iface && !is_impl {
            "interface"
        } else {
            "class"
        };
        Ok(Some(HierarchyItem {
            tier: "compiler".to_string(),
            sym: String::new(),
            scip_symbol: hit.symbol,
            name: hit.display_name,
            kind: kind.to_string(),
            repo: hit.repo,
            file: hit.file,
            line: hit.line,
            end_line: hit.line,
        }))
    }

    /// tier "resolved" prepare: the identifier text joined against
    /// `type_entity`, restricted to the type-shaped kinds (a function/method
    /// entity of the same name must not win the ambiguity pick — it has no
    /// place in a type hierarchy).
    fn resolved_type_prepare(&self, path: &str, text: &str) -> Result<Option<HierarchyItem>> {
        let path_repo = self.repo_for_path(path);
        let te = txt_tbl("type_entity");
        let candidates: Vec<(String, String, String, String, i64)> = self.try_rows(
            &format!("SELECT \"repo\", \"sym\", \"kind\", \"file\", \"line\" FROM {te} \
                      WHERE \"name\" = ?1 AND \"kind\" IN ('struct', 'enum', 'trait', 'class', 'interface')"),
            &[&text],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?, r.get::<_, i64>(4)?)),
        );
        if candidates.is_empty() {
            return Ok(None);
        }
        let pick = candidates
            .iter()
            .find(|(repo, _, _, file, _)| *repo == path_repo && file == path)
            .or_else(|| candidates.iter().find(|(repo, ..)| *repo == path_repo))
            .cloned()
            .unwrap_or_else(|| candidates[0].clone());
        let (repo, sym, kind, file, line) = pick;
        let line0 = (line - 1).max(0) as u32;
        Ok(Some(HierarchyItem {
            tier: "resolved".to_string(),
            sym,
            scip_symbol: String::new(),
            name: text.to_string(),
            kind,
            repo,
            file,
            line: line0,
            end_line: line0,
        }))
    }

    /// `typeHierarchy/supertypes`: the interfaces/superclasses `item` implements
    /// or extends, one hop. `type_link` kind `"impl"` (class/object implements/
    /// extends) always counts; kind `"generic"` counts ONLY when `item`'s own
    /// entity kind is `interface`/`trait` (an interface's `extends` are also
    /// emitted as `"generic"` — see typegraph.rs; a struct/fn's `"generic"`
    /// bound edges are a type-parameter constraint, not a supertype, and are
    /// excluded by this same owner-kind filter).
    pub fn type_hierarchy_supertypes(&self, item: &HierarchyItem) -> Result<Vec<HierarchyItem>> {
        if item.tier == "compiler" {
            return self.compiler_supertypes(item);
        }
        self.resolved_supertypes(item)
    }

    /// `typeHierarchy/subtypes`: the types that implement/extend `item`, one
    /// hop, same kind filter as `type_hierarchy_supertypes`.
    pub fn type_hierarchy_subtypes(&self, item: &HierarchyItem) -> Result<Vec<HierarchyItem>> {
        if item.tier == "compiler" {
            return self.compiler_subtypes(item);
        }
        self.resolved_subtypes(item)
    }

    fn resolved_supertypes(&self, item: &HierarchyItem) -> Result<Vec<HierarchyItem>> {
        let tl = txt_tbl("type_link");
        let te = txt_tbl("type_entity");
        let dsts: Vec<String> = self.try_rows(
            &format!("SELECT tl.\"dst\" FROM {tl} tl JOIN {te} te ON te.\"sym\" = tl.\"src\" \
                      WHERE tl.\"src\" = ?1 \
                      AND (tl.\"kind\" = 'impl' OR (tl.\"kind\" = 'generic' AND te.\"kind\" IN ('interface', 'trait')))"),
            &[&item.sym], |r| r.get::<_, String>(0),
        );
        Ok(dsts
            .into_iter()
            .filter_map(|sym| self.resolved_type_item(&sym))
            .collect())
    }

    fn resolved_subtypes(&self, item: &HierarchyItem) -> Result<Vec<HierarchyItem>> {
        let tl = txt_tbl("type_link");
        let te = txt_tbl("type_entity");
        let srcs: Vec<String> = self.try_rows(
            &format!("SELECT tl.\"src\" FROM {tl} tl JOIN {te} te ON te.\"sym\" = tl.\"src\" \
                      WHERE tl.\"dst\" = ?1 \
                      AND (tl.\"kind\" = 'impl' OR (tl.\"kind\" = 'generic' AND te.\"kind\" IN ('interface', 'trait')))"),
            &[&item.sym], |r| r.get::<_, String>(0),
        );
        Ok(srcs
            .into_iter()
            .filter_map(|sym| self.resolved_type_item(&sym))
            .collect())
    }

    /// `type_entity` row for one sym -> a resolved-tier `HierarchyItem`, the
    /// by-sym twin of `resolved_type_prepare` (which resolves by name).
    fn resolved_type_item(&self, sym: &str) -> Option<HierarchyItem> {
        let te = txt_tbl("type_entity");
        self.try_rows(
            &format!("SELECT \"repo\", \"name\", \"kind\", \"file\", \"line\" FROM {te} WHERE \"sym\" = ?1 LIMIT 1"),
            &[&sym],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?, r.get::<_, i64>(4)?)),
        )
        .into_iter()
        .next()
        .map(|(repo, name, kind, file, line)| {
            let line0 = (line - 1).max(0) as u32;
            HierarchyItem { tier: "resolved".to_string(), sym: sym.to_string(), scip_symbol: String::new(),
                name, kind, repo, file, line: line0, end_line: line0 }
        })
    }

    fn compiler_supertypes(&self, item: &HierarchyItem) -> Result<Vec<HierarchyItem>> {
        let si = txt_tbl("scip_impl");
        let ifaces: Vec<String> = self.try_rows(
            &format!("SELECT \"iface\" FROM {si} WHERE \"impl\" = ?1"),
            &[&item.scip_symbol],
            |r| r.get::<_, String>(0),
        );
        Ok(ifaces
            .into_iter()
            .filter_map(|iface| {
                let (repo, file, line) = self.scip_def_site(&iface)?;
                Some(HierarchyItem {
                    tier: "compiler".to_string(),
                    sym: String::new(),
                    scip_symbol: iface.clone(),
                    name: scip_descriptor_name(&iface).unwrap_or_default(),
                    kind: "interface".to_string(),
                    repo,
                    file,
                    line: line.max(0) as u32,
                    end_line: line.max(0) as u32,
                })
            })
            .collect())
    }

    fn compiler_subtypes(&self, item: &HierarchyItem) -> Result<Vec<HierarchyItem>> {
        let si = txt_tbl("scip_impl");
        let impls: Vec<String> = self.try_rows(
            &format!("SELECT \"impl\" FROM {si} WHERE \"iface\" = ?1"),
            &[&item.scip_symbol],
            |r| r.get::<_, String>(0),
        );
        Ok(impls
            .into_iter()
            .filter_map(|imp| {
                let (repo, file, line) = self.scip_def_site(&imp)?;
                Some(HierarchyItem {
                    tier: "compiler".to_string(),
                    sym: String::new(),
                    scip_symbol: imp.clone(),
                    name: scip_descriptor_name(&imp).unwrap_or_default(),
                    kind: "class".to_string(),
                    repo,
                    file,
                    line: line.max(0) as u32,
                    end_line: line.max(0) as u32,
                })
            })
            .collect())
    }
}
