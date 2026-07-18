//! Analysis-derived relations: `agent_edit`/`agent_touch`, `dl_diag`,
//! `type_shape`, `type_lgg`.

use anyhow::Result;

use crate::ast::{RelDecl, Type, Value};
use crate::engine::Engine;
use crate::lower::txt_tbl;
use crate::typegraph;

use super::{col, RelKind};

// --- agent (analysis-derived) ------------------------------------------------

/// Agent-harness edit relations. `agent_edit(harness, session, idx, path)` is one
/// row per edit a coding agent made under `--root`; `agent_touch(harness, session,
/// path)` is the last-edited path per session. Read from `agent::agent_harnesses`.
pub struct AgentKind;

impl RelKind for AgentKind {
    fn rels(&self) -> &'static [&'static str] {
        &["agent_edit", "agent_touch", "skill_loaded"]
    }
    fn decls(&self) -> Vec<RelDecl> {
        vec![
            RelDecl { name: "agent_edit".into(), cols: vec![
                col("harness", Type::Text), col("session", Type::Text),
                col("idx", Type::Int), col("path", Type::Path)], group: "agent",
                doc: "every file edit in the latest agent turn, tagged harness+session+turn idx (from the at-rest harness store)", ..Default::default() },
            RelDecl { name: "agent_touch".into(), cols: vec![
                col("harness", Type::Text), col("session", Type::Text),
                col("path", Type::Path)], group: "agent",
                doc: "the latest agent turn's edited files (harness, session, path)", ..Default::default() },
            RelDecl { name: "skill_loaded".into(), cols: vec![
                col("harness", Type::Text), col("session", Type::Text),
                col("name", Type::Text)], group: "agent",
                doc: "skills loaded in the newest agent session (harness, session, name): explicit Skill tool calls + dl's own prior `dl --hook` injections — negate it for a declarative load-once guard", ..Default::default() },
        ]
    }
    fn reserved_msg(&self) -> &'static str {
        "a built-in agent-harness relation (agent_edit / agent_touch / skill_loaded)"
    }
    fn refresh(&self, eng: &Engine) -> Result<bool> {
        let root = std::fs::canonicalize(&eng.root).unwrap_or_else(|_| eng.root.clone());
        let mut edits: Vec<(String, String, i64, String)> = Vec::new(); // harness, session, idx, path
        let mut touch: Vec<(String, String, String)> = Vec::new();      // harness, session, path @ max idx
        let mut skills: Vec<(String, String, String)> = Vec::new();     // harness, session, skill name
        for h in crate::agent::agent_harnesses() {
            let hn = h.name().to_string();
            for sess in h.sessions_for(&root) {
                let maxidx = sess.edits.iter().map(|e| e.idx).max();
                for e in &sess.edits {
                    edits.push((hn.clone(), sess.id.clone(), e.idx, e.path.clone()));
                    if Some(e.idx) == maxidx {
                        touch.push((hn.clone(), sess.id.clone(), e.path.clone()));
                    }
                }
            }
            for (sid, name) in h.skill_loads(&root) {
                skills.push((hn.clone(), sid, name));
            }
        }
        edits.sort(); edits.dedup();
        touch.sort(); touch.dedup();
        skills.sort(); skills.dedup();
        // Early-out: nothing refreshes unless edits OR skill loads changed.
        let existing_edits: Vec<(String, String, i64, String)> = eng.db.query_rows(
            "agent_edit",
            &format!(
                "SELECT \"harness\", \"session\", \"idx\", \"path\" FROM {} ORDER BY 1,2,3,4",
                txt_tbl("agent_edit")
            ),
            &[],
            |r| Ok((
                r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, String>(3)?,
            )),
        )?;
        let existing_skills: Vec<(String, String, String)> = eng.db.query_rows(
            "skill_loaded",
            &format!("SELECT \"harness\", \"session\", \"name\" FROM {} ORDER BY 1,2,3", txt_tbl("skill_loaded")),
            &[],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)),
        )?;
        if existing_edits == edits && existing_skills == skills { return Ok(false); }
        let edit_rows: Vec<Vec<Value>> = edits.into_iter()
            .map(|(h, s, i, p)| vec![Value::Text(h), Value::Text(s), Value::Int(i), Value::Text(p)])
            .collect();
        eng.refresh_rel("agent_edit", &["harness", "session", "idx", "path"], &edit_rows)?;
        let touch_rows: Vec<Vec<Value>> = touch.into_iter()
            .map(|(h, s, p)| vec![Value::Text(h), Value::Text(s), Value::Text(p)])
            .collect();
        eng.refresh_rel("agent_touch", &["harness", "session", "path"], &touch_rows)?;
        let skill_rows: Vec<Vec<Value>> = skills.into_iter()
            .map(|(h, s, n)| vec![Value::Text(h), Value::Text(s), Value::Text(n)])
            .collect();
        eng.refresh_rel("skill_loaded", &["harness", "session", "name"], &skill_rows)?;
        Ok(true)
    }
}

// --- dl_diag (self-validation) -----------------------------------------------

/// `dl_diag(path, line, col, end_line, end_col, severity, code, msg)` — the
/// engine's own lexer/parser/typechecker run over every scanned `.dl` file (the
/// `file` rows whose path ends `.dl`), so dl lints dl the way rust-analyzer lints
/// Rust. Byte spans from `TypeDiag` map to 1-based line / 0-based byte col. A lex
/// or parse failure is one whole-file error row (code `lex`/`parse`); typecheck
/// diagnostics (brand-mismatch, unknown-anchor, type errors, stratification)
/// carry their real span. Same pass as `--check`, relocated into a relation.
///
/// Validated FILE-LOCALLY: a `use`-split program is checked per file, so a
/// relation defined in an *included* file is out of scope here (run `dl --check`
/// on the whole set for cross-file `use` resolution). Column names mirror the
/// `diag` sink so a rail forwards by name:
/// `diag(...) <- agent_changed(p), p =~ /\.dl$/, dl_diag(p, ...).`
pub struct DlDiagKind;

/// Byte offset -> (1-based line, 0-based byte col) within `content`.
fn offset_to_line_col(content: &str, off: u32) -> (i64, i64) {
    let off = (off as usize).min(content.len());
    let mut line = 1i64;
    let mut line_start = 0usize;
    for (i, b) in content.as_bytes().iter().enumerate() {
        if i >= off { break; }
        if *b == b'\n' { line += 1; line_start = i + 1; }
    }
    (line, (off - line_start) as i64)
}

type DlDiagRow = (String, i64, i64, i64, i64, String, String, String);

/// Lex+parse+typecheck one `.dl` source in isolation. A lex/parse failure is one
/// whole-file row; typecheck diags carry their byte span mapped to line/col.
fn validate_dl_source(content: &str, path: &str) -> Vec<DlDiagRow> {
    let toks = match crate::lex::lex(content) {
        Ok(t) => t,
        Err(e) => return vec![(path.into(), 1, 0, 1, 0, "error".into(), "lex".into(), e.to_string())],
    };
    let mut prog = match crate::parse::parse(toks) {
        Ok(p) => p,
        Err(e) => return vec![(path.into(), 1, 0, 1, 0, "error".into(), "parse".into(), e.to_string())],
    };
    let mut rows: Vec<DlDiagRow> = crate::typecheck::check_and_normalize(&mut prog, path).into_iter().map(|d| {
        let (l0, c0) = offset_to_line_col(content, d.span.0);
        let (l1, c1) = offset_to_line_col(content, d.span.1);
        (path.to_string(), l0, c0, l1, c1, d.severity.as_str().to_string(), d.code, d.msg)
    }).collect();
    // Unpinned closure query lint: `? reach(a, b)` on a closure head evaluates
    // the SQL reachability VIEW, which materializes the FULL relation (a LIMIT
    // does not short-circuit the UNION + recursive CTE) — unbounded on a dense
    // edge rel; the runtime guard (DL_CLOSURE_QUERY_MAX_EDGES) will refuse it
    // at tick time. Flag it at lint time so the author pins an endpoint first.
    let closure_heads: std::collections::HashSet<&str> = prog.items.iter()
        .filter_map(|i| match i {
            crate::ast::Item::Rule(r) if r.closure_edge().is_some() => Some(r.head.rel.as_str()),
            _ => None,
        }).collect();
    for item in &prog.items {
        let crate::ast::Item::Query(q) = item else { continue };
        if !closure_heads.contains(q.head.rel.as_str()) || q.head.terms.len() != 2 { continue; }
        let unpinned = q.head.terms.iter()
            .all(|t| matches!(t, crate::ast::Term::Var(_) | crate::ast::Term::Wild));
        if !unpinned { continue; }
        // Query nodes carry no span; locate the `? rel(` text directly.
        let off = content.find(&format!("? {}(", q.head.rel))
            .or_else(|| content.find(&format!("?{}(", q.head.rel))).unwrap_or(0) as u32;
        let (l, c) = offset_to_line_col(content, off);
        rows.push((path.to_string(), l, c, l, c, "warning".into(), "closure-unpinned".into(),
            format!("unpinned query on closure head `{0}` materializes the full reachability \
                     relation (unbounded on a dense graph); pin one endpoint, e.g. \
                     `? {0}(\"seed\", x)`, for the seeded fast path", q.head.rel)));
    }
    rows
}

impl RelKind for DlDiagKind {
    fn rels(&self) -> &'static [&'static str] { &["dl_diag"] }
    fn decls(&self) -> Vec<RelDecl> {
        vec![RelDecl { name: "dl_diag".into(), cols: vec![
            col("path", Type::Path), col("line", Type::Int), col("col", Type::Int),
            col("end_line", Type::Int), col("end_col", Type::Int),
            col("severity", Type::Text), col("code", Type::Text), col("msg", Type::Text)], group: "meta",
            doc: "parse/type diagnostics for each scanned `.dl` file (path, line, col, end_line, end_col, severity, code, msg); the engine's own lexer/parser/typechecker run over `file` rows ending in `.dl` — join agent_changed for lint-on-edit; line: 1-based; col: 0-based", ..Default::default() }]
    }
    fn reserved_msg(&self) -> &'static str {
        "the built-in dl self-diagnostics relation"
    }
    fn refresh(&self, eng: &Engine) -> Result<bool> {
        // Every scanned .dl path; the WORK text is read from disk (file.content is a
        // content hash, not the source). Validates the on-disk working copy — the
        // lint-on-edit target. A path that can't be read (a non-self repo root, a
        // git-only rev) is skipped, never a false diag.
        let paths: Vec<String> = eng.db.query_rows(
            "file",
            &format!(
                "SELECT DISTINCT \"path\" FROM {} WHERE \"path\" LIKE '%.dl' ORDER BY \"path\"",
                txt_tbl("file")
            ),
            &[],
            |r| Ok(r.get::<_, String>(0)?),
        )?;
        let mut rows: Vec<DlDiagRow> = Vec::new();
        for path in &paths {
            let Ok(content) = std::fs::read_to_string(eng.root.join(path)) else { continue };
            rows.extend(validate_dl_source(&content, path));
        }
        rows.sort();
        rows.dedup();
        let existing: Vec<DlDiagRow> = eng.db.query_rows(
            "dl_diag",
            &format!(
                "SELECT \"path\",\"line\",\"col\",\"end_line\",\"end_col\",\"severity\",\"code\",\"msg\" \
                 FROM {} ORDER BY 1,2,3,4,5,6,7,8", txt_tbl("dl_diag")
            ),
            &[],
            |r| Ok((
                r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?, r.get::<_, String>(5)?, r.get::<_, String>(6)?, r.get::<_, String>(7)?,
            )),
        )?;
        if existing == rows { return Ok(false); }
        let out: Vec<Vec<Value>> = rows.into_iter().map(|(p, l, c, el, ec, sev, code, msg)| vec![
            Value::Text(p), Value::Int(l), Value::Int(c), Value::Int(el), Value::Int(ec),
            Value::Text(sev), Value::Text(code), Value::Text(msg)]).collect();
        eng.refresh_rel("dl_diag",
            &["path", "line", "col", "end_line", "end_col", "severity", "code", "msg"], &out)?;
        Ok(true)
    }
}

// --- type_shape (analysis-derived) -------------------------------------------

/// `type_shape(name, hash)` — Merkle shape hash per type, from the current
/// `type_edge` rows via `typegraph::type_shape_hashes` (fixpoint). Reads the edge
/// set the type-rels refresh already populated, so it must run AFTER it (the
/// `rel_kinds()` loop sits after `refresh_type_rels` in both tick paths).
pub struct TypeShapeKind;

impl RelKind for TypeShapeKind {
    fn rels(&self) -> &'static [&'static str] {
        &["type_shape"]
    }
    fn decls(&self) -> Vec<RelDecl> {
        vec![RelDecl { name: "type_shape".into(),
            cols: vec![col("name", Type::Text), col("hash", Type::Text)], group: "type-shape",
            doc: "structural type-shape fingerprint per type (shape-iso experiment)", ..Default::default() }]
    }
    fn reserved_msg(&self) -> &'static str {
        "the built-in type-shape relation"
    }
    fn refresh(&self, eng: &Engine) -> Result<bool> {
        let edges: Vec<(String, String, String)> = eng.db.query_rows(
            "type_edge",
            &format!("SELECT \"from\",\"to\",\"kind\" FROM {}", txt_tbl("type_edge")),
            &[],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)),
        )?;
        let computed: Vec<(String, String)> = typegraph::type_shape_hashes(&edges);
        let stored: Vec<(String, String)> = eng.db.query_rows(
            "type_shape",
            &format!("SELECT \"name\",\"hash\" FROM {} ORDER BY \"name\",\"hash\"", txt_tbl("type_shape")),
            &[],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )?;
        if stored == computed { return Ok(false); }
        let rows: Vec<Vec<Value>> = computed.into_iter()
            .map(|(n, h)| vec![Value::Text(n), Value::Text(h)]).collect();
        eng.refresh_rel("type_shape", &["name", "hash"], &rows)?;
        Ok(true)
    }
}

// --- type_lgg (analysis-derived) ---------------------------------------------

/// `type_lgg(a, b, vars)` — least-general-generalization variable count per type
/// pair, from the resolved `type_link` graph via `typegraph::type_lgg_pairs`.
/// Uses `type_link` (SCIP-resolved syms), not `type_edge` (bare names), so the
/// LGG recurses into resolved local types. Runs after the type-rels refresh.
pub struct TypeLggKind;

impl RelKind for TypeLggKind {
    fn rels(&self) -> &'static [&'static str] {
        &["type_lgg"]
    }
    fn decls(&self) -> Vec<RelDecl> {
        vec![RelDecl { name: "type_lgg".into(),
            cols: vec![col("a", Type::Text), col("b", Type::Text), col("vars", Type::Int)], group: "type-shape",
            doc: "least-general generalization of two type shapes (shape-iso experiment)", ..Default::default() }]
    }
    fn reserved_msg(&self) -> &'static str {
        "the built-in type-lgg relation"
    }
    fn refresh(&self, eng: &Engine) -> Result<bool> {
        let edges: Vec<(String, String, String)> = eng.db.query_rows(
            "type_link",
            &format!("SELECT \"src\",\"dst\",\"kind\" FROM {}", txt_tbl("type_link")),
            &[],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)),
        )?;
        let computed: Vec<(String, String, i64)> = typegraph::type_lgg_pairs(&edges);
        let stored: Vec<(String, String, i64)> = eng.db.query_rows(
            "type_lgg",
            &format!("SELECT \"a\",\"b\",\"vars\" FROM {} ORDER BY \"a\",\"b\",\"vars\"", txt_tbl("type_lgg")),
            &[],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)),
        )?;
        if stored == computed { return Ok(false); }
        let rows: Vec<Vec<Value>> = computed.into_iter()
            .map(|(a, b, v)| vec![Value::Text(a), Value::Text(b), Value::Int(v)]).collect();
        eng.refresh_rel("type_lgg", &["a", "b", "vars"], &rows)?;
        Ok(true)
    }
}
