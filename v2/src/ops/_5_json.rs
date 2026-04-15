//! json — walk structured data files (JSON / YAML / TOML) using brace patterns.
//!
//! Usage:
//!   fs(**/package.json) > json({ name: $N, version: $V })
//!   repo(r) > rev(main) > json({ ** : { image: $I } })   ← self-enumerates

use std::sync::Arc;

use bytes::Bytes;
use futures_core::stream::BoxStream;
use futures_util::stream::StreamExt;

use rustc_hash::FxHashMap;

use crate::_0_types::{Capture, Cursor, FilePath, ParseSite, Tri};
use crate::_1_diagnostic::{Diagnostic, Renderer};
use crate::_16_pattern::CompiledPattern;
use crate::_5_op::{
    hover_render_grouped, BraceMode, CompletionItem, GrammarRef, Op, OpCtx, OpInvocation,
    Operator, Pipeline, ProgramCtx,
};
use crate::data::{parse_by_ext, DataKind, DataNode, AnyDataNode};
use crate::jq_path;
use crate::walk::_2_compile::compile_steps;
use crate::walk::_1_compiled::CompiledStep;
use crate::walk::_3_walker::walk;
use crate::walk::_4_brace_parse::{parse_body, ScanAnnotation};

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

pub struct JsonFactory;

impl Operator for JsonFactory {
    fn name(&self) -> &'static str { "json" }
    fn paren_grammar(&self) -> GrammarRef { GrammarRef(Arc::from("json-none")) }
    fn brace_mode(&self) -> BraceMode { BraceMode::WalkerPattern }

    fn completion_item(&self) -> CompletionItem {
        CompletionItem {
            label:  "json".to_string(),
            detail: "json({ key: $CAP })".to_string(),
            doc:    "# json\n\nWalk JSON/YAML/TOML files with a brace pattern. Binds captures from matching keys.".to_string(),
        }
    }

    fn parse(&self, inv: &OpInvocation, _pctx: &mut ProgramCtx)
        -> Result<Pipeline, Vec<Box<dyn Diagnostic>>>
    {
        // json({ pattern }) — body lives in the paren slot since {..} is inside (..),
        // so paren_src.src = "{ version: $VER }".
        let paren = inv.paren_src.as_ref().ok_or_else(|| {
            vec![Box::new(JsonDiag::ParseBody {
                site: (*inv.parse_site).clone(),
                msg:  Arc::from("json requires a pattern argument (e.g. json({ key: $V }))"),
            }) as Box<dyn Diagnostic>]
        })?;

        let src = paren.src.trim();
        let (steps, annotations) = parse_body(src).map_err(|e| {
            let msg = e.to_string();
            let code = if msg.contains("requires a bare capture var") {
                "json/annotation-requires-capture"
            } else {
                "json/parse-body"
            };
            vec![Box::new(JsonDiag::ParseBodyCode {
                site: (*inv.parse_site).clone(),
                msg:  Arc::from(msg.as_str()),
                code: Arc::from(code),
            }) as Box<dyn Diagnostic>]
        })?;

        let compiled = compile_steps(&steps).map_err(|e| {
            vec![Box::new(JsonDiag::ParseBody {
                site: (*inv.parse_site).clone(),
                msg:  Arc::from(e.to_string().as_str()),
            }) as Box<dyn Diagnostic>]
        })?;

        Ok(Pipeline::Op(Arc::new(JsonOp {
            compiled:    Arc::from(compiled.into_boxed_slice()),
            annotations: Arc::from(annotations.into_boxed_slice()),
            parse_site:  inv.parse_site.clone(),
        }).into()))
    }
}

// ---------------------------------------------------------------------------
// Op
// ---------------------------------------------------------------------------

pub struct JsonOp {
    compiled:    Arc<[CompiledStep]>,
    annotations: Arc<[ScanAnnotation]>,
    parse_site:  Arc<ParseSite>,
}

const JSON_EXTS: &[&str] = &["json", "yaml", "yml", "toml"];

fn file_ext(fp: &FilePath) -> Option<String> {
    fp.0.extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
}

impl Op for JsonOp {
    fn name(&self) -> &'static str { "json" }
    fn step(&self) -> u16 { 0 }
    fn parse_site(&self) -> &Arc<ParseSite> { &self.parse_site }

    fn witness(&self, c: &Cursor) -> Option<Arc<str>> {
        c.fs.as_ref().map(|fp| Arc::from(fp.0.to_string_lossy().as_ref()))
    }

    fn hover_self(&self) -> String {
        "# json\n\nWalk JSON/YAML/TOML files with a brace pattern. Binds captures from matching keys.".to_string()
    }

    fn hover_capture(&self, cap: &str, cursors: &[Cursor]) -> Option<String> {
        let site = self.parse_site.as_ref();
        let header = format!("**`${cap}`** values:");
        let entries: Vec<(Option<String>, String, String)> = cursors.iter()
            .filter(|c| c.evidence.iter().any(|ev|
                ev.op_name == "json" && ev.parse_site.as_ref() == site
            ))
            .filter_map(|c| {
                c.captures.get(cap).map(|capture| (
                    c.fs.as_ref().map(|fp| fp.0.to_string_lossy().into_owned()),
                    c.rev.to_string(),
                    capture.value.to_string(),
                ))
            })
            .collect();
        hover_render_grouped(&header, &entries)
    }

    fn hover_match(&self, site: &crate::_0_types::ParseSite, cursors: &[Cursor]) -> Option<String> {
        let header = "**json** pattern site";
        let entries: Vec<(Option<String>, String, String)> = cursors.iter()
            .flat_map(|c| {
                c.evidence.iter()
                    .filter(|ev| ev.op_name == "json" && ev.parse_site.as_ref() == site)
                    .map(move |ev| (
                        c.fs.as_ref().map(|fp| fp.0.to_string_lossy().into_owned()),
                        c.rev.to_string(),
                        ev.matched.to_string(),
                    ))
                    .collect::<Vec<_>>()
            })
            .collect();
        if entries.is_empty() {
            return Some(format!("{header}\n\n(no matches yet)"));
        }
        hover_render_grouped(header, &entries)
    }

    fn pipe(&self, input: BoxStream<'static, Cursor>, ctx: OpCtx)
        -> BoxStream<'static, Cursor>
    {
        let compiled    = self.compiled.clone();
        let annotations = self.annotations.clone();
        let parse_site  = self.parse_site.clone();
        let reader      = ctx.reader.clone();
        let diags       = ctx.diags.clone();

        // Content-side stamping map: capture var -> sigil. Used to stamp
        // walker-extracted Captures with scan_pointer + Tri::Claimed. This
        // is the scout half of the claim/verify pair; a separate checker
        // pass downgrades unverified claims.
        // TODO: when line()/ast-grep land, hoist this stamping into a shared
        // walker-driver helper so it isn't re-implemented per op.
        let stamp_map: Arc<FxHashMap<Arc<str>, Arc<str>>> = {
            let mut m: FxHashMap<Arc<str>, Arc<str>> = FxHashMap::default();
            for ann in annotations.iter() {
                m.insert(Arc::<str>::from(ann.var.as_str()), ann.sigil.clone());
            }
            Arc::new(m)
        };

        input.then(move |c| {
            let compiled    = compiled.clone();
            let parse_site  = parse_site.clone();
            let reader      = reader.clone();
            let diags       = diags.clone();
            let stamp_map   = stamp_map.clone();
            async move {
                // Collect (FilePath, Bytes) candidates
                let candidates: Vec<(FilePath, Bytes)> = match &c.fs {
                    Some(fp) => {
                        // Single-file branch: skip if extension not in accepted set
                        match file_ext(fp) {
                            Some(ref e) if JSON_EXTS.contains(&e.as_str()) => {}
                            _ => return vec![],
                        }
                        let mut s = reader.bytes(&c.repo, &c.rev, fp);
                        let raw = s.next().await.unwrap_or_default();
                        vec![(fp.clone(), raw)]
                    }
                    None => {
                        // Self-enumerate via reader.files("**"), filter by ext
                        let all = CompiledPattern::compile("**")
                            .expect("'**' is a valid glob");
                        let mut s = reader.files(&c.repo, &c.rev, &all);
                        let files = s.next().await.unwrap_or_default();
                        let mut out = vec![];
                        for fp in files {
                            match file_ext(&fp) {
                                Some(ref e) if JSON_EXTS.contains(&e.as_str()) => {}
                                _ => continue,
                            }
                            let mut s2 = reader.bytes(&c.repo, &c.rev, &fp);
                            let raw = s2.next().await.unwrap_or_default();
                            out.push((fp, raw));
                        }
                        out
                    }
                };

                // json/no-candidates: self-enum mode with zero json/yaml/yml/toml files
                if c.fs.is_none() && candidates.is_empty() {
                    diags.0(Box::new(JsonDiag::NoCandidates {
                        site: (*parse_site).clone(),
                        msg:  Arc::from(format!(
                            "json() found no .json/.yaml/.yml/.toml files in {}@{}",
                            c.repo, c.rev
                        )),
                    }));
                }

                let mut out_cursors: Vec<Cursor> = vec![];

                for (fp, raw) in candidates {
                    let ext = match file_ext(&fp) {
                        Some(e) => e,
                        None => continue,
                    };
                    let arc_bytes = Arc::new(raw.clone());
                    let tree = match parse_by_ext(&ext, arc_bytes) {
                        Ok(t) => t,
                        Err(e) => {
                            diags.0(Box::new(JsonDiag::ParseError {
                                site: (*parse_site).clone(),
                                msg:  Arc::from(e.to_string().as_str()),
                            }));
                            continue;
                        }
                    };

                    let outcome = walk(&tree, &compiled);
                    let rows = outcome.rows;

                    // json/no-match: file walked but zero rows returned
                    if rows.is_empty() {
                        let dump = tree_dump(&tree, 4, 200);
                        diags.0(Box::new(JsonDiag::NoMatch {
                            site: (*parse_site).clone(),
                            file: Arc::from(fp.0.to_string_lossy().as_ref()),
                            dump: Arc::from(dump.as_str()),
                        }));
                        continue;
                    }

                    if !outcome.missed_keys.is_empty() {
                        diags.0(Box::new(JsonDiag::PartialMatch {
                            site:   (*parse_site).clone(),
                            file:   Arc::from(fp.0.to_string_lossy().as_ref()),
                            missed: outcome.missed_keys.clone(),
                        }));
                    }

                    for row in rows {
                        let mut c2 = c.clone();
                        c2.fs = Some(fp.clone());
                        // Merge walk captures into cursor captures.
                        // If the capture var has a scan annotation, stamp
                        // scan_pointer + Tri::Claimed (content-side claim).
                        for (name, wc) in row.captures {
                            let cap = match stamp_map.get(&name) {
                                Some(sigil) => Capture::new(wc.text.clone())
                                    .with_scan(sigil.clone(), Tri::Claimed),
                                None => Capture::new(wc.text.clone()),
                            };
                            c2.captures.insert(name, cap);
                        }
                        out_cursors.push(c2);
                    }
                }

                out_cursors
            }
        })
        .flat_map(|v| futures_util::stream::iter(v))
        .boxed()
    }
}

// ---------------------------------------------------------------------------
// Tree dump helper (file-private)
// ---------------------------------------------------------------------------

/// Render a depth-limited jq-path listing of `node` up to `max_depth` levels.
/// Each line is one leaf or container header. Truncated at `max_lines` with
/// "...(more)" appended if the tree exceeds the cap.
fn tree_dump(node: &AnyDataNode, max_depth: u32, max_lines: usize) -> String {
    let mut lines: Vec<String> = Vec::new();
    dump_node(node, &mut String::new(), 0, max_depth, &mut lines, max_lines);
    if lines.len() > max_lines {
        lines.truncate(max_lines);
        lines.push("...(more)".to_owned());
    }
    lines.join("\n")
}

fn dump_node(
    node:      &AnyDataNode,
    path:      &mut String,
    depth:     u32,
    max_depth: u32,
    lines:     &mut Vec<String>,
    max_lines: usize,
) {
    if lines.len() >= max_lines { return; }
    match node.kind() {
        DataKind::Object => {
            let label = if path.is_empty() { ".".to_owned() } else { path.clone() };
            lines.push(format!("{label}: {{}}"));
            if depth < max_depth {
                for (k, v) in node.entries() {
                    if lines.len() >= max_lines { return; }
                    let key_text = k.as_scalar_text()
                        .map(|s| s.into_owned())
                        .unwrap_or_default();
                    let mut child_path = path.clone();
                    jq_path::push_key(&mut child_path, &key_text);
                    dump_node(&v, &mut child_path, depth + 1, max_depth, lines, max_lines);
                }
            }
        }
        DataKind::Array => {
            let label = if path.is_empty() { ".".to_owned() } else { path.clone() };
            lines.push(format!("{label}: []"));
            if depth < max_depth {
                for (i, item) in node.items().enumerate() {
                    if lines.len() >= max_lines { return; }
                    let mut child_path = path.clone();
                    jq_path::push_index(&mut child_path, i as u32);
                    dump_node(&item, &mut child_path, depth + 1, max_depth, lines, max_lines);
                }
            }
        }
        DataKind::Scalar => {
            let val = node.as_scalar_text()
                .map(|s| s.into_owned())
                .unwrap_or_default();
            let label = if path.is_empty() { ".".to_owned() } else { path.clone() };
            // Truncate long scalar values for readability
            let display = if val.len() > 60 { format!("{}...", &val[..60]) } else { val };
            lines.push(format!("{label}: {display}"));
        }
        DataKind::Null => {
            let label = if path.is_empty() { ".".to_owned() } else { path.clone() };
            lines.push(format!("{label}: null"));
        }
    }
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum JsonDiag {
    ParseBody {
        site: crate::_0_types::ParseSite,
        msg:  Arc<str>,
    },
    ParseBodyCode {
        site: crate::_0_types::ParseSite,
        msg:  Arc<str>,
        code: Arc<str>,
    },
    ParseError {
        site: crate::_0_types::ParseSite,
        msg:  Arc<str>,
    },
    NoCandidates {
        site: crate::_0_types::ParseSite,
        msg:  Arc<str>,
    },
    NoMatch {
        site: crate::_0_types::ParseSite,
        file: Arc<str>,
        dump: Arc<str>,
    },
    PartialMatch {
        site:   crate::_0_types::ParseSite,
        file:   Arc<str>,
        missed: Vec<Arc<str>>,
    },
}

impl Diagnostic for JsonDiag {
    fn code(&self) -> &str {
        match self {
            JsonDiag::ParseBody      { .. }       => "json/parse-body",
            JsonDiag::ParseBodyCode  { code, .. } => code,
            JsonDiag::ParseError     { .. }       => "json/parse-error",
            JsonDiag::NoCandidates   { .. }       => "json/no-candidates",
            JsonDiag::NoMatch        { .. }       => "json/no-match",
            JsonDiag::PartialMatch   { .. }       => "json/partial-match",
        }
    }
    fn severity(&self) -> crate::_0_types::Severity {
        match self {
            JsonDiag::ParseBody     { .. }
            | JsonDiag::ParseBodyCode { .. }
            | JsonDiag::ParseError  { .. } => crate::_0_types::Severity::Error,
            JsonDiag::NoCandidates  { .. } => crate::_0_types::Severity::Warn,
            JsonDiag::NoMatch       { .. } => crate::_0_types::Severity::Hint,
            JsonDiag::PartialMatch  { .. } => crate::_0_types::Severity::Hint,
        }
    }
    fn primary(&self) -> &crate::_0_types::ParseSite {
        match self {
            JsonDiag::ParseBody      { site, .. } => site,
            JsonDiag::ParseBodyCode  { site, .. } => site,
            JsonDiag::ParseError     { site, .. } => site,
            JsonDiag::NoCandidates   { site, .. } => site,
            JsonDiag::NoMatch        { site, .. } => site,
            JsonDiag::PartialMatch   { site, .. } => site,
        }
    }
    fn render(&self, out: &mut dyn Renderer) {
        match self {
            JsonDiag::ParseBody { site, msg } => {
                out.header(self.code(), self.severity(), msg);
                out.primary(site);
            }
            JsonDiag::ParseBodyCode { site, msg, .. } => {
                out.header(self.code(), self.severity(), msg);
                out.primary(site);
            }
            JsonDiag::ParseError { site, msg } => {
                out.header(self.code(), self.severity(), msg);
                out.primary(site);
            }
            JsonDiag::NoCandidates { site, msg } => {
                out.header(self.code(), self.severity(), msg);
                out.primary(site);
            }
            JsonDiag::NoMatch { site, file, dump } => {
                let msg = format!("pattern found no matches in {file} (file parsed OK; keys/shape differ)");
                out.header(self.code(), self.severity(), &msg);
                out.primary(site);
                out.note(dump);
            }
            JsonDiag::PartialMatch { site, file, missed } => {
                let joined = missed.iter().map(|k| k.as_ref()).collect::<Vec<_>>().join(", ");
                let msg = format!("pattern partially matched in {file}; missing keys: {joined}");
                out.header(self.code(), self.severity(), &msg);
                out.primary(site);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use crate::_0_types::{Capture, FilePath, OpEvidence, RunId, SprfPath};

    fn dummy_site() -> Arc<ParseSite> {
        Arc::new(ParseSite {
            file:       Arc::from(Path::new("test.sprf")),
            path:       Arc::from(vec![].into_boxed_slice()),
            byte_range: 0..1,
        })
    }

    fn base_cursor(rev: &str, fs: Option<&str>) -> Cursor {
        Cursor {
            run_id:   RunId(0),
            repo:     Arc::from("org/repo"),
            rev:      Arc::from(rev),
            fs:       fs.map(|p| FilePath(Arc::from(Path::new(p)))),
            captures: Default::default(),
            fks:      Default::default(),
            path:     SprfPath(Arc::from(vec![].into_boxed_slice())),
            evidence: vec![],
            content:  None,
        }
    }

    fn make_op(site: &Arc<ParseSite>) -> JsonOp {
        use crate::walk::_4_brace_parse::parse_body;
        use crate::walk::_2_compile::compile_steps;
        let (steps, anns) = parse_body("{ name: $N }").unwrap();
        let compiled = compile_steps(&steps).unwrap();
        JsonOp {
            compiled:    Arc::from(compiled.into_boxed_slice()),
            annotations: Arc::from(anns.into_boxed_slice()),
            parse_site:  site.clone(),
        }
    }

    #[test]
    fn hover_capture_groups_by_file_rev() {
        let site = dummy_site();
        let op = make_op(&site);

        let mut c1 = base_cursor("main", Some("crates/a/Cargo.toml"));
        c1.evidence.push(OpEvidence {
            op_name:    "json",
            parse_site: site.clone(),
            matched:    Arc::from("alpha"),
            capture:    None,
        });
        c1.captures.insert(Arc::from("N"), Capture::new(Arc::from("alpha")));

        let mut c2 = base_cursor("main", Some("crates/b/Cargo.toml"));
        c2.evidence.push(OpEvidence {
            op_name:    "json",
            parse_site: site.clone(),
            matched:    Arc::from("beta"),
            capture:    None,
        });
        c2.captures.insert(Arc::from("N"), Capture::new(Arc::from("beta")));

        let mut c3 = base_cursor("v2", Some("crates/a/Cargo.toml"));
        c3.evidence.push(OpEvidence {
            op_name:    "json",
            parse_site: site.clone(),
            matched:    Arc::from("gamma"),
            capture:    None,
        });
        c3.captures.insert(Arc::from("N"), Capture::new(Arc::from("gamma")));

        let md = op.hover_capture("N", &[c1, c2, c3]).unwrap();

        assert!(md.contains("### `crates/a/Cargo.toml`"), "missing a/ heading: {md}");
        assert!(md.contains("- `alpha`"), "missing alpha: {md}");
        assert!(md.contains("### `crates/b/Cargo.toml`"), "missing b/ heading: {md}");
        assert!(md.contains("- `beta`"), "missing beta: {md}");
        assert!(md.contains("- `gamma`"), "missing gamma: {md}");
    }

    #[test]
    fn hover_match_groups_by_file_rev() {
        let site = dummy_site();
        let op = make_op(&site);

        let mut c1 = base_cursor("main", Some("package.json"));
        c1.evidence.push(OpEvidence {
            op_name:    "json",
            parse_site: site.clone(),
            matched:    Arc::from("foo"),
            capture:    None,
        });
        let mut c2 = base_cursor("main", Some("sub/package.json"));
        c2.evidence.push(OpEvidence {
            op_name:    "json",
            parse_site: site.clone(),
            matched:    Arc::from("bar"),
            capture:    None,
        });

        let md = op.hover_match(site.as_ref(), &[c1, c2]).unwrap();

        assert!(md.contains("### `package.json`"), "missing package.json heading: {md}");
        assert!(md.contains("- `foo`"), "missing foo: {md}");
        assert!(md.contains("### `sub/package.json`"), "missing sub/package.json heading: {md}");
        assert!(md.contains("- `bar`"), "missing bar: {md}");
    }
}
