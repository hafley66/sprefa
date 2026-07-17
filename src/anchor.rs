//! Query-time anchor resolution + the `what`/`summary` read surface.
//!
//! An "anchor" is a user-supplied string that names a place in the code: a bare
//! symbol name (with `*` glob), a file path, or a `path:line` point. [`classify`]
//! sorts the string into an [`Anchor`], and [`resolve_name`] turns a name/glob
//! into typed [`AnchorHit`]s by reading ONLY the persisted `rel_<name>` tables
//! through the engine's SQLite handle. It never touches the extraction-time
//! `by_name` machinery in `engine::extract` — that resolver runs while facts are
//! minted; this one runs afterward against whatever the tables already hold.
//!
//! Every table read is tolerant: a family a program never extracted has no
//! backing table, so a missing table degrades to zero rows rather than erroring
//! (same posture as `Engine::try_rows`). Reads go through decoded relation views, so
//! no literal `FROM rel_<name>` string appears in this file (the magic-rel
//! audit only inspects hardcoded names; every relation named here is a
//! catalogued builtin regardless).

use std::path::PathBuf;

use anyhow::Result;

use crate::ast::Program;
use crate::engine::Engine;
use crate::lower::txt_tbl;

/// A classified anchor string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Anchor {
    /// A bare symbol name; `*` is a glob wildcard.
    Name(String),
    /// A file path (contains `/` or ends in a known source extension).
    Path(String),
    /// A `path:line` point.
    PathLine { path: String, line: i64 },
}

/// One resolved candidate for a name/glob anchor. `sym` is the join key for the
/// neighborhood detail; the remaining columns place the candidate. Fields a
/// given family does not carry are empty (`kind`/`file`) or zero (`line`).
#[derive(Debug, Clone)]
pub struct AnchorHit {
    pub family: &'static str,
    pub rel: &'static str,
    pub repo: String,
    pub sym: String,
    pub name: String,
    pub kind: String,
    pub file: String,
    pub line: i64,
}

/// The uniform shape both `what` and `summary` return: a header, rows, the
/// pre-paging total, and any advisory notes (e.g. the no-SCIP-index tip).
#[derive(Debug, Clone, Default)]
pub struct QueryOut {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub total: usize,
    pub notes: Vec<String>,
}

// LANG-JUNCTION(anchor-source-exts): the extension list that classifies a bare `dl what` anchor as a path (and the module's probe program's scan globs further down); a new language's extension belongs here for path anchors to classify
/// Known source extensions that make a bare (slash-free) string a [`Anchor::Path`].
const SOURCE_EXTS: &[&str] = &[
    "rs", "ts", "tsx", "js", "jsx", "mjs", "cjs", "kt", "kts", "py", "go",
    "java", "c", "cc", "cpp", "cxx", "h", "hh", "hpp", "cs", "rb", "php",
    "swift", "scala", "lua", "ex", "exs", "erl", "hs", "ml", "sh", "bash",
    "css", "scss", "html", "htm", "vue", "svelte", "json", "yaml", "yml",
    "toml", "md", "sql",
];

/// Sort an anchor string. A trailing `:<digits>` suffix is a `path:line` point;
/// a string with a `/` or a known source extension is a path; anything else is
/// a name (where `*` globs). The `:line` check runs first, so `src/x.rs:42`
/// lands as [`Anchor::PathLine`] rather than [`Anchor::Path`].
pub fn classify(input: &str) -> Anchor {
    if let Some(idx) = input.rfind(':') {
        let (head, tail) = input.split_at(idx);
        let digits = &tail[1..];
        if !head.is_empty() && !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
            if let Ok(line) = digits.parse::<i64>() {
                return Anchor::PathLine { path: head.to_string(), line };
            }
        }
    }
    let has_ext = input
        .rsplit_once('.')
        .map(|(_, ext)| SOURCE_EXTS.contains(&ext))
        .unwrap_or(false);
    if input.contains('/') || has_ext {
        return Anchor::Path(input.to_string());
    }
    Anchor::Name(input.to_string())
}

/// Strip the repo prefix from a repo-qualified sym once. `type_entity.sym` and
/// the `call_edge` endpoints are `{repo}::{file::kind::name}`; `type_edge`
/// from/to are bare names. Returns `(Some(repo), rest)` when a `::` is present,
/// else `(None, whole)`. Callers use this instead of re-deriving the split.
pub fn split_repo_sym(qualified: &str) -> (Option<&str>, &str) {
    match qualified.find("::") {
        Some(idx) => (Some(&qualified[..idx]), &qualified[idx + 2..]),
        None => (None, qualified),
    }
}

/// Turn a user glob (with `*`) into a SQL `LIKE` pattern: `*` becomes `%`, and a
/// literal `%`/`_`/`\` is backslash-escaped (paired with `ESCAPE '\'` in the
/// query). A pattern with no `*` becomes a plain literal, so `LIKE` acts as an
/// exact (case-folding) match.
fn like_pattern(glob: &str) -> String {
    let mut out = String::with_capacity(glob.len() + 4);
    for ch in glob.chars() {
        match ch {
            '*' => out.push('%'),
            '%' | '_' | '\\' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

/// Stringify one SQLite cell (Text as-is, Integer/Real rendered, Null empty).
fn cell(row: &rusqlite::Row, idx: usize) -> String {
    match row.get::<_, rusqlite::types::Value>(idx) {
        Ok(rusqlite::types::Value::Text(s)) => s,
        Ok(rusqlite::types::Value::Integer(n)) => n.to_string(),
        Ok(rusqlite::types::Value::Real(f)) => f.to_string(),
        _ => String::new(),
    }
}

/// Tolerant multi-row read: a prepare/query error (typically a missing table
/// for a never-extracted family) yields an empty vec instead of erroring.
fn rows_of(eng: &Engine, sql: &str, params: &[&dyn rusqlite::types::ToSql]) -> Vec<Vec<String>> {
    let conn = eng.db.conn();
    let Ok(mut stmt) = conn.prepare(sql) else { return Vec::new(); };
    let ncol = stmt.column_count();
    let mapped = stmt.query_map(params, |row| {
        Ok((0..ncol).map(|i| cell(row, i)).collect::<Vec<String>>())
    });
    let Ok(rows) = mapped else { return Vec::new(); };
    rows.filter_map(|r| r.ok()).collect()
}

/// Tolerant scalar count: a missing table or query error reads as 0.
fn count_of(eng: &Engine, sql: &str, params: &[&dyn rusqlite::types::ToSql]) -> i64 {
    eng.db
        .conn()
        .query_row(sql, params, |r| r.get::<_, i64>(0))
        .unwrap_or(0)
}

/// Resolve a name/glob `pattern` to candidates across every family that carries
/// a name-like column. Unions: `type_entity.name`, `call_def` via
/// `call_name.name`, `scip_name.name` (with `scip_def` for a file/repo), an
/// alias binding via `scip_binding.local_name`, and `df_node.var`. A missing
/// family table contributes nothing.
pub fn resolve_name(eng: &Engine, pattern: &str) -> Result<Vec<AnchorHit>> {
    let like = like_pattern(pattern);
    let p: &[&dyn rusqlite::types::ToSql] = &[&like];
    let mut hits: Vec<AnchorHit> = Vec::new();

    // type_entity(repo, sym, name, kind, parent, file, line)
    let sql = format!(
        "SELECT \"repo\", \"sym\", \"name\", \"kind\", \"file\", \"line\" FROM {} \
         WHERE \"name\" LIKE ?1 ESCAPE '\\'",
        txt_tbl("type_entity")
    );
    for r in rows_of(eng, &sql, p) {
        hits.push(AnchorHit {
            family: "type",
            rel: "type_entity",
            repo: r[0].clone(),
            sym: r[1].clone(),
            name: r[2].clone(),
            kind: r[3].clone(),
            file: r[4].clone(),
            line: r[5].parse().unwrap_or(0),
        });
    }

    // call_def(repo, sym, kind, file, line, end) joined to call_name(sym, name).
    let sql = format!(
        "SELECT d.\"repo\", d.\"sym\", n.\"name\", d.\"kind\", d.\"file\", d.\"line\" \
         FROM {} d JOIN {} n ON n.\"sym\" = d.\"sym\" \
         WHERE n.\"name\" LIKE ?1 ESCAPE '\\'",
        txt_tbl("call_def"),
        txt_tbl("call_name")
    );
    for r in rows_of(eng, &sql, p) {
        hits.push(AnchorHit {
            family: "call",
            rel: "call_def",
            repo: r[0].clone(),
            sym: r[1].clone(),
            name: r[2].clone(),
            kind: r[3].clone(),
            file: r[4].clone(),
            line: r[5].parse().unwrap_or(0),
        });
    }

    // scip_name(symbol, name) left-joined to scip_def(symbol, file, repo).
    let sql = format!(
        "SELECT DISTINCT n.\"symbol\", n.\"name\", COALESCE(d.\"file\", ''), COALESCE(d.\"repo\", '') \
         FROM {} n LEFT JOIN {} d ON d.\"symbol\" = n.\"symbol\" \
         WHERE n.\"name\" LIKE ?1 ESCAPE '\\'",
        txt_tbl("scip_name"),
        txt_tbl("scip_def")
    );
    for r in rows_of(eng, &sql, p) {
        hits.push(AnchorHit {
            family: "scip",
            rel: "scip_name",
            repo: r[3].clone(),
            sym: r[0].clone(),
            name: r[1].clone(),
            kind: String::new(),
            file: r[2].clone(),
            line: 0,
        });
    }

    // scip_binding(file, symbol, local_name, line, col, repo) — the local alias.
    let sql = format!(
        "SELECT \"symbol\", \"local_name\", \"file\", \"line\", \"repo\" FROM {} \
         WHERE \"local_name\" LIKE ?1 ESCAPE '\\'",
        txt_tbl("scip_binding")
    );
    for r in rows_of(eng, &sql, p) {
        hits.push(AnchorHit {
            family: "scip",
            rel: "scip_binding",
            repo: r[4].clone(),
            sym: r[0].clone(),
            name: r[1].clone(),
            kind: String::new(),
            file: r[2].clone(),
            line: r[3].parse().unwrap_or(0),
        });
    }

    // module_binding_resolved(file, local, source, dst) joined to type_entity — the
    // index-free equivalent of the scip_binding block above, for a repo with
    // no index.scip: `mb.local` is the alias in scope at `mb.file`, resolved
    // to the def it aliases at `mb.dst` (`te.name = mb.source`).
    let sql = format!(
        "SELECT te.\"repo\", te.\"sym\", mb.\"local\", te.\"kind\", te.\"file\", te.\"line\" \
         FROM {} mb JOIN {} te ON te.\"name\" = mb.\"source\" AND te.\"file\" = mb.\"dst\" \
         WHERE mb.\"local\" LIKE ?1 ESCAPE '\\'",
        txt_tbl("module_binding_resolved"),
        txt_tbl("type_entity")
    );
    for r in rows_of(eng, &sql, p) {
        hits.push(AnchorHit {
            family: "module",
            rel: "module_binding_resolved",
            repo: r[0].clone(),
            sym: r[1].clone(),
            name: r[2].clone(),
            kind: r[3].clone(),
            file: r[4].clone(),
            line: r[5].parse().unwrap_or(0),
        });
    }

    // df_node(id, kind, var, fn, file, line) — value flow nodes with a name.
    // df_node_repo(id, repo) carries the folder handle (df ids are path-keyed).
    let sql = format!(
        "SELECT dn.\"id\", dn.\"var\", dn.\"kind\", dn.\"file\", dn.\"line\", COALESCE(dr.\"repo\", '') \
         FROM {} dn LEFT JOIN {} dr ON dr.\"id\" = dn.\"id\" \
         WHERE dn.\"var\" LIKE ?1 ESCAPE '\\' AND dn.\"var\" <> ''",
        txt_tbl("df_node"),
        txt_tbl("df_node_repo")
    );
    for r in rows_of(eng, &sql, p) {
        hits.push(AnchorHit {
            family: "dataflow",
            rel: "df_node",
            repo: r[5].clone(),
            sym: r[0].clone(),
            name: r[1].clone(),
            kind: r[2].clone(),
            file: r[3].clone(),
            line: r[4].parse().unwrap_or(0),
        });
    }

    Ok(hits)
}

/// One-line neighborhood detail for a resolved sym: fan-in/out over `call_edge`
/// (repo-qualified endpoints, so a `call_def`/`type_entity` sym joins directly),
/// `type_link` in/out, whether a doc comment exists, and the SCIP occurrence
/// count. Every count is one set-based SQL statement — no per-row loop.
fn neighborhood(eng: &Engine, sym: &str) -> String {
    let s: &[&dyn rusqlite::types::ToSql] = &[&sym];
    let callers = count_of(
        eng,
        &format!("SELECT COUNT(DISTINCT \"caller\") FROM {} WHERE \"callee\" = ?1", txt_tbl("call_edge")),
        s,
    );
    let callees = count_of(
        eng,
        &format!("SELECT COUNT(DISTINCT \"callee\") FROM {} WHERE \"caller\" = ?1", txt_tbl("call_edge")),
        s,
    );
    let type_in = count_of(
        eng,
        &format!("SELECT COUNT(DISTINCT \"src\") FROM {} WHERE \"dst\" = ?1", txt_tbl("type_link")),
        s,
    );
    let type_out = count_of(
        eng,
        &format!("SELECT COUNT(DISTINCT \"dst\") FROM {} WHERE \"src\" = ?1", txt_tbl("type_link")),
        s,
    );
    let doc = count_of(
        eng,
        &format!("SELECT COUNT(*) FROM {} WHERE \"sym\" = ?1", txt_tbl("doc_comment")),
        s,
    ) > 0;
    let scip_occ = count_of(
        eng,
        &format!("SELECT COUNT(*) FROM {} WHERE \"symbol\" = ?1", txt_tbl("scip_occurrence")),
        s,
    );
    format!(
        "callers={callers} callees={callees} type_in={type_in} type_out={type_out} doc={} scip_occ={scip_occ}",
        if doc { "yes" } else { "no" }
    )
}

/// A `(col = exact OR col LIKE '%/'+path)` predicate + its two bound params, so
/// a user path matches an exact stored path or a basename/suffix of one.
fn path_predicate(col: &str, path: &str) -> (String, [String; 2]) {
    let suffix = format!("%/{}", like_pattern(path));
    let frag = format!("(\"{col}\" = ?1 OR \"{col}\" LIKE ?2 ESCAPE '\\')");
    (frag, [path.to_string(), suffix])
}

/// `dl what <anchor>`: resolve the anchor and describe its neighborhood. A
/// name/glob anchor lists each candidate with its neighborhood detail; a
/// `path:line` anchor lists the call sites / dataflow nodes / comments on that
/// line plus the enclosing definition. Paging is applied in memory over the
/// bounded row set; `total` is the pre-paging count.
pub fn what(eng: &Engine, anchor: &str, limit: Option<usize>, offset: Option<usize>) -> QueryOut {
    let columns = vec![
        "family".into(),
        "rel".into(),
        "match_kind".into(),
        "repo".into(),
        "file".into(),
        "line".into(),
        "detail".into(),
    ];
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut notes: Vec<String> = Vec::new();

    match classify(anchor) {
        Anchor::PathLine { path, line } => {
            let (pred, pp) = path_predicate("file", &path);
            // comment_node keys its file on the `path` column.
            let (cpred, _) = path_predicate("path", &path);
            let ln = line.to_string();
            let params: &[&dyn rusqlite::types::ToSql] = &[&pp[0], &pp[1], &ln];

            // call_site(repo, caller, callee, file, line)
            let sql = format!(
                "SELECT \"repo\", \"caller\", \"callee\", \"file\", \"line\" FROM {} \
                 WHERE {pred} AND \"line\" = ?3",
                txt_tbl("call_site")
            );
            for r in rows_of(eng, &sql, params) {
                rows.push(vec![
                    "call".into(),
                    "call_site".into(),
                    "call_site".into(),
                    r[0].clone(),
                    r[3].clone(),
                    r[4].clone(),
                    format!("caller={} callee={}", r[1], r[2]),
                ]);
            }

            // df_node(id, kind, var, fn, file, line)
            let sql = format!(
                "SELECT \"kind\", \"var\", \"fn\", \"file\", \"line\" FROM {} \
                 WHERE {pred} AND \"line\" = ?3",
                txt_tbl("df_node")
            );
            for r in rows_of(eng, &sql, params) {
                rows.push(vec![
                    "dataflow".into(),
                    "df_node".into(),
                    "df_node".into(),
                    String::new(),
                    r[3].clone(),
                    r[4].clone(),
                    format!("kind={} var={} fn={}", r[0], r[1], r[2]),
                ]);
            }

            // comment_node(path, line, col, end_line, end_col, text, kind)
            let sql = format!(
                "SELECT \"path\", \"line\", \"kind\", \"text\" FROM {} \
                 WHERE {cpred} AND \"line\" = ?3",
                txt_tbl("comment_node")
            );
            for r in rows_of(eng, &sql, params) {
                rows.push(vec![
                    "comment".into(),
                    "comment_node".into(),
                    "comment".into(),
                    String::new(),
                    r[0].clone(),
                    r[1].clone(),
                    format!("{}: {}", r[2], r[3]),
                ]);
            }

            // Enclosing definition: the smallest call_def span covering the line.
            let sql = format!(
                "SELECT \"repo\", \"sym\", \"kind\", \"file\", \"line\" FROM {} \
                 WHERE {pred} AND \"line\" <= ?3 AND \"end\" >= ?3 \
                 ORDER BY (\"end\" - \"line\") ASC LIMIT 1",
                txt_tbl("call_def")
            );
            for r in rows_of(eng, &sql, params) {
                rows.push(vec![
                    "call".into(),
                    "call_def".into(),
                    "enclosing_def".into(),
                    r[0].clone(),
                    r[3].clone(),
                    r[4].clone(),
                    format!("sym={} kind={}", r[1], r[2]),
                ]);
            }
        }
        Anchor::Path(path) => {
            // A bare path anchor with no line: list the definitions declared in
            // the file, each with its neighborhood.
            let (pred, pp) = path_predicate("file", &path);
            let params: &[&dyn rusqlite::types::ToSql] = &[&pp[0], &pp[1]];
            let sql = format!(
                "SELECT \"repo\", \"sym\", \"name\", \"kind\", \"file\", \"line\" FROM {} \
                 WHERE {pred} ORDER BY \"line\"",
                txt_tbl("type_entity")
            );
            for r in rows_of(eng, &sql, params) {
                let detail = neighborhood(eng, &r[1]);
                rows.push(vec![
                    "type".into(),
                    "type_entity".into(),
                    "path".into(),
                    r[0].clone(),
                    r[4].clone(),
                    r[5].clone(),
                    format!("{} {} :: {}", r[3], r[2], detail),
                ]);
            }
        }
        Anchor::Name(pattern) => {
            let match_kind = if pattern.contains('*') { "glob" } else { "exact" };
            let hits = resolve_name(eng, &pattern).unwrap_or_default();
            for h in hits {
                let detail = neighborhood(eng, &h.sym);
                rows.push(vec![
                    h.family.to_string(),
                    h.rel.to_string(),
                    match_kind.to_string(),
                    h.repo,
                    h.file,
                    h.line.to_string(),
                    format!("{} {} :: {}", h.kind, h.name, detail),
                ]);
            }
        }
    }

    // SCIP tier honesty.
    if count_of(eng, &format!("SELECT COUNT(*) FROM {}", txt_tbl("scip_def")), &[]) == 0 {
        notes.push("scip: no index (run dl index to upgrade)".into());
    }

    let total = rows.len();
    let rows = page(rows, limit, offset);
    QueryOut { columns, rows, total, notes }
}

/// `dl summary <path>`: a per-file report — declared entities, callables with
/// fan totals, imports out/in, unresolved imports, doc coverage, and comment /
/// dataflow node counts. Rows are `(section, name, detail)`.
pub fn summary(eng: &Engine, path: &str) -> QueryOut {
    let columns = vec!["section".into(), "name".into(), "detail".into()];
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut notes: Vec<String> = Vec::new();

    // Entities declared in the file.
    let (fpred, fp) = path_predicate("file", path);
    let fparams: &[&dyn rusqlite::types::ToSql] = &[&fp[0], &fp[1]];
    let sql = format!(
        "SELECT \"name\", \"kind\", \"line\" FROM {} WHERE {fpred} ORDER BY \"line\"",
        txt_tbl("type_entity")
    );
    for r in rows_of(eng, &sql, fparams) {
        rows.push(vec!["entity".into(), r[0].clone(), format!("{} L{}", r[1], r[2])]);
    }

    // Callables with fan totals (set-based correlated subqueries, one statement).
    let sql = format!(
        "SELECT d.\"sym\", d.\"kind\", d.\"line\", \
           (SELECT COUNT(DISTINCT \"caller\") FROM {ce} WHERE \"callee\" = d.\"sym\"), \
           (SELECT COUNT(DISTINCT \"callee\") FROM {ce} WHERE \"caller\" = d.\"sym\") \
         FROM {cd} d WHERE {fpred} ORDER BY d.\"line\"",
        ce = txt_tbl("call_edge"),
        cd = txt_tbl("call_def"),
    );
    for r in rows_of(eng, &sql, fparams) {
        rows.push(vec![
            "callable".into(),
            r[0].clone(),
            format!("fan_in={} fan_out={} L{}", r[3], r[4], r[2]),
        ]);
    }

    // Imports out (this file's edges) and in (edges landing here).
    let (spred, sp) = path_predicate("src", path);
    let sparams: &[&dyn rusqlite::types::ToSql] = &[&sp[0], &sp[1]];
    let sql = format!(
        "SELECT DISTINCT \"dst\" FROM {} WHERE {spred} ORDER BY \"dst\"",
        txt_tbl("module_edge")
    );
    for r in rows_of(eng, &sql, sparams) {
        rows.push(vec!["import_out".into(), r[0].clone(), String::new()]);
    }
    let (dpred, dp) = path_predicate("dst", path);
    let dparams: &[&dyn rusqlite::types::ToSql] = &[&dp[0], &dp[1]];
    let sql = format!(
        "SELECT DISTINCT \"src\" FROM {} WHERE {dpred} ORDER BY \"src\"",
        txt_tbl("module_edge")
    );
    for r in rows_of(eng, &sql, dparams) {
        rows.push(vec!["import_in".into(), r[0].clone(), String::new()]);
    }

    // Unresolved imports out of the file.
    let sql = format!(
        "SELECT \"specifier\", \"reason\", \"line\" FROM {} WHERE {fpred} ORDER BY \"line\"",
        txt_tbl("module_unresolved")
    );
    for r in rows_of(eng, &sql, fparams) {
        rows.push(vec![
            "unresolved".into(),
            r[0].clone(),
            format!("{} L{}", r[1], r[2]),
        ]);
    }

    // Doc coverage: documented entities / total, over the file's type_entity rows.
    let sql = format!(
        "SELECT COUNT(*), COUNT(dc.\"sym\") FROM {te} te \
         LEFT JOIN (SELECT DISTINCT \"sym\" FROM {doc}) dc ON dc.\"sym\" = te.\"sym\" \
         WHERE {fpred}",
        te = txt_tbl("type_entity"),
        doc = txt_tbl("doc_comment"),
    );
    if let Some(r) = rows_of(eng, &sql, fparams).into_iter().next() {
        let total: i64 = r[0].parse().unwrap_or(0);
        let documented: i64 = r[1].parse().unwrap_or(0);
        let pct = if total > 0 { documented * 100 / total } else { 0 };
        rows.push(vec![
            "metric".into(),
            "doc_coverage".into(),
            format!("{documented}/{total} ({pct}%)"),
        ]);
    }

    // Comment + dataflow node counts.
    let (cpred, cp) = path_predicate("path", path);
    let cparams: &[&dyn rusqlite::types::ToSql] = &[&cp[0], &cp[1]];
    let comments = count_of(
        eng,
        &format!("SELECT COUNT(*) FROM {} WHERE {cpred}", txt_tbl("comment_node")),
        cparams,
    );
    rows.push(vec!["metric".into(), "comment_nodes".into(), comments.to_string()]);
    let df_nodes = count_of(
        eng,
        &format!("SELECT COUNT(*) FROM {} WHERE {fpred}", txt_tbl("df_node")),
        fparams,
    );
    rows.push(vec!["metric".into(), "df_nodes".into(), df_nodes.to_string()]);

    if count_of(eng, &format!("SELECT COUNT(*) FROM {}", txt_tbl("scip_def")), &[]) == 0 {
        notes.push("scip: no index (run dl index to upgrade)".into());
    }

    let total = rows.len();
    QueryOut { columns, rows, total, notes }
}

/// In-memory paging over a bounded row set.
fn page(rows: Vec<Vec<String>>, limit: Option<usize>, offset: Option<usize>) -> Vec<Vec<String>> {
    let off = offset.unwrap_or(0);
    let mut out: Vec<Vec<String>> = rows.into_iter().skip(off).collect();
    if let Some(lim) = limit {
        out.truncate(lim);
    }
    out
}

/// The families the `what`/`summary` read surface consults refresh only when the
/// loaded program references one of their rels (`ExtractFamily::used`). For a
/// from-cold in-process answer there is no such program, so this synthetic probe
/// scans the WORK source + manifest corpus and names every family's rel in a `?`
/// query — the least invasive lever, since a query item flips each `used` gate
/// on without heading any relation or deriving any row. When the db is already
/// warm (the daemon case) the tables are read directly and this never runs.
pub fn probe_program() -> Result<Program> {
    const SRC: &str = r#"
rel _what_scan(path: file).
_what_scan(path) <- scan("WORK", "**/*.rs", path, rev).
_what_scan(path) <- scan("WORK", "**/*.{ts,tsx,js,jsx,mjs,cjs}", path, rev).
_what_scan(path) <- scan("WORK", "**/*.{kt,kts}", path, rev).
_what_scan(path) <- scan("WORK", "**/*.py", path, rev).
_what_scan(path) <- scan("WORK", "**/*.go", path, rev).
_what_scan(path) <- scan("WORK", "**/*.java", path, rev).
_what_scan(path) <- scan("WORK", "**/*.md", path, rev).
_what_scan(path) <- scan("WORK", "**/Cargo.toml", path, rev).
_what_scan(path) <- scan("WORK", "**/package.json", path, rev).
_what_scan(path) <- scan("WORK", "**/tsconfig*.json", path, rev).
_what_scan(path) <- scan("WORK", "**/go.mod", path, rev).
? type_entity(_, _, _, _, _, _, _).
? call_def(_, _, _, _, _, _).
? call_name(_, _).
? call_edge(_, _, _).
? call_site(_, _, _, _, _).
? type_link(_, _, _).
? df_node(_, _, _, _, _, _).
? df_node_repo(_, _).
? module_edge(_, _).
? module_unresolved(_, _, _, _).
? module_binding_resolved(_, _, _, _).
? doc_comment(_, _, _, _).
? comment_node(_, _, _, _, _, _, _).
? scip_def(_, _, _).
? scip_name(_, _).
? scip_binding(_, _, _, _, _, _).
? scip_occurrence(_, _, _, _, _, _, _, _).
? verb_catalog(_, _, _).
"#;
    let toks = crate::lex::lex(SRC)?;
    let mut prog = crate::parse::parse(toks)?;
    // Warnings (unused body vars, path/text coercions) are fine; the probe never
    // reads its own derived output, only the builtin tables the tick populates.
    let _ = crate::typecheck::check_and_normalize(&mut prog, "<what-probe>");
    Ok(prog)
}

/// Build a fresh in-process engine and tick [`probe_program`] so the code-graph
/// families (type / call / dataflow / module / comment / doc / scip + the verb
/// catalog) populate, then hand it back for a table read. This is the
/// from-cold, daemon-free path shared by `dl what` (src/cli/query.rs) and the
/// MCP `Local` tool pump: both need a warm engine that never disturbs any
/// served program. `db` reuses a cache file, else the db is in-memory.
pub fn warm_engine(root: PathBuf, db: Option<&str>) -> Result<Engine> {
    crate::watchdog::arm_wall_watchdog("warm_engine");
    let conn = crate::db::open(db)?;
    let mut eng = Engine::new(conn, root);
    eng.set_repos(crate::daemon::load_repos_eager());
    let prog = probe_program()?;
    eng.tick(&prog, true)?;
    Ok(eng)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_name() {
        assert_eq!(classify("Engine"), Anchor::Name("Engine".into()));
    }

    #[test]
    fn classify_glob() {
        assert_eq!(classify("type_*"), Anchor::Name("type_*".into()));
    }

    #[test]
    fn classify_path_by_slash() {
        assert_eq!(classify("src/engine/mod.rs"), Anchor::Path("src/engine/mod.rs".into()));
    }

    #[test]
    fn classify_path_by_ext() {
        assert_eq!(classify("model.ts"), Anchor::Path("model.ts".into()));
    }

    #[test]
    fn classify_path_line() {
        assert_eq!(
            classify("src/foo.rs:42"),
            Anchor::PathLine { path: "src/foo.rs".into(), line: 42 }
        );
    }

    #[test]
    fn classify_name_with_digit_suffix_is_not_a_line() {
        // No colon, so a trailing digit run stays part of the name.
        assert_eq!(classify("utf8"), Anchor::Name("utf8".into()));
        assert_eq!(classify("v2"), Anchor::Name("v2".into()));
    }

    #[test]
    fn classify_qualified_name_is_not_a_line() {
        // `Repo::method` has a `::` but the trailing segment is not digits.
        assert_eq!(classify("Repo::method"), Anchor::Name("Repo::method".into()));
    }

    #[test]
    fn like_translation_and_escaping() {
        assert_eq!(like_pattern("type_*"), "type\\_%");
        assert_eq!(like_pattern("a*b"), "a%b");
        assert_eq!(like_pattern("100%"), "100\\%");
        assert_eq!(like_pattern("plain"), "plain");
    }

    #[test]
    fn split_repo_sym_qualified() {
        assert_eq!(
            split_repo_sym("myrepo::src/x.rs::struct::Engine"),
            (Some("myrepo"), "src/x.rs::struct::Engine")
        );
    }

    #[test]
    fn split_repo_sym_bare() {
        assert_eq!(split_repo_sym("Engine"), (None, "Engine"));
    }

    #[test]
    fn probe_program_parses() {
        // The synthetic family-forcing program must lex + parse cleanly.
        assert!(probe_program().is_ok());
    }
}
