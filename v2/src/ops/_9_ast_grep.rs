//! ast — AST pattern matching via ast-grep-core.
//!
//! Examples:
//!   ast[rust](fn $NAME($$$ARGS) { $$$BODY })
//!   ast[typescript](let $X: ${TY}Error = $V)
//!
//! `$VAR` / `$$$VAR` are ast-grep native metavars and pass through. `${VAR}`
//! is sprefa sugar: the surrounding token run collapses to a synthetic
//! `$SPRFSLOTN` metavar and a named regex runs over the slot's matched text to
//! extract the sub-token capture.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use futures_core::stream::BoxStream;
use futures_util::stream::StreamExt;
use futures::future::join_all;

use crate::_17_parallel::rayon_map;

use ast_grep_core::{AstGrep, Language, Pattern, source::StrDoc};
use ast_grep_language::SupportLang;
use regex::Regex;

use crate::_3_reader::ParserKind;
use crate::readers::RsAstPayload;

use crate::_0_types::{Capture, Cursor, FilePath, OpEvidence, ParseSite, Severity};
use crate::_1_diagnostic::{Diagnostic, Renderer};
use crate::_5_op::{
    hover_render_grouped, BraceMode, CompletionItem, GrammarRef, Op, OpCtx, OpInvocation,
    Operator, Pipeline, ProgramCtx,
};

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

pub struct AstGrepFactory;

impl Operator for AstGrepFactory {
    fn name(&self) -> &'static str { "ast" }
    fn paren_grammar(&self) -> GrammarRef { GrammarRef(Arc::from("ast-grep-pattern")) }
    fn brace_mode(&self) -> BraceMode { BraceMode::DefaultFork }

    fn completion_item(&self) -> CompletionItem {
        CompletionItem {
            label:  "ast".to_string(),
            detail: "ast[lang](pattern)".to_string(),
            doc:    "# ast\n\nMatch an ast-grep pattern against cursor content. `lang` is `rust` or `typescript`. `$VAR` / `$$$VAR` are native ast-grep metavars. `${VAR}` is sprefa sugar for sub-token capture.".to_string(),
        }
    }

    fn parse(&self, inv: &OpInvocation, _pctx: &mut ProgramCtx)
        -> Result<Pipeline, Vec<Box<dyn Diagnostic>>>
    {
        let bracket = inv.brackets.get(0).ok_or_else(|| {
            vec![Box::new(AstDiag::MissingLang { site: (*inv.parse_site).clone() })
                as Box<dyn Diagnostic>]
        })?;
        let lang_name = bracket.src.trim();
        let lang = parse_lang(lang_name).ok_or_else(|| {
            vec![Box::new(AstDiag::UnknownLang {
                site: (*inv.parse_site).clone(),
                got:  Arc::from(lang_name),
            }) as Box<dyn Diagnostic>]
        })?;
        let paren = inv.paren_src.as_ref().ok_or_else(|| {
            vec![Box::new(AstDiag::BadPattern {
                site: (*inv.parse_site).clone(),
                got:  Arc::from(""),
                msg:  Arc::from("missing pattern body"),
            }) as Box<dyn Diagnostic>]
        })?;
        let raw = paren.src.trim();
        if raw.is_empty() {
            return Err(vec![Box::new(AstDiag::BadPattern {
                site: (*inv.parse_site).clone(),
                got:  Arc::from(raw),
                msg:  Arc::from("empty pattern"),
            }) as Box<dyn Diagnostic>]);
        }

        let (rewritten, slot_res, sugar_caps) = lower_sugar(raw);
        let native_caps = scan_metavars(raw);

        let pattern = Pattern::try_new(&rewritten, lang).map_err(|e| {
            vec![Box::new(AstDiag::BadPattern {
                site: (*inv.parse_site).clone(),
                got:  Arc::from(raw),
                msg:  Arc::from(e.to_string().as_str()),
            }) as Box<dyn Diagnostic>]
        })?;

        let mut bound: Vec<Arc<str>> = Vec::new();
        for n in native_caps.into_iter().chain(sugar_caps.into_iter()) {
            if !bound.iter().any(|s| s.as_ref() == n.as_ref()) {
                bound.push(n);
            }
        }

        Ok(Pipeline::Op(Arc::new(AstGrepOp {
            lang,
            pattern:        Arc::new(pattern),
            slot_regexes:   slot_res,
            bound_captures: bound,
            src:            Arc::from(raw),
            parse_site:     inv.parse_site.clone(),
        }).into()))
    }
}

fn parse_lang(s: &str) -> Option<SupportLang> {
    match s {
        "rust" | "rs"          => Some(SupportLang::Rust),
        "typescript" | "ts"    => Some(SupportLang::TypeScript),
        _ => None,
    }
}

// Scan a pattern string for native ast-grep metavars (`$NAME` / `$$$NAME`),
// skipping `${…}` sugar tokens. First-seen order, no duplicates.
fn scan_metavars(pat: &str) -> Vec<Arc<str>> {
    let b = pat.as_bytes();
    let mut out: Vec<Arc<str>> = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'$' {
            if i + 1 < b.len() && b[i + 1] == b'{' {
                i += 2;
                while i < b.len() && b[i] != b'}' { i += 1; }
                if i < b.len() { i += 1; }
                continue;
            }
            let mut j = i + 1;
            if j + 1 < b.len() && b[j] == b'$' && b[j + 1] == b'$' { j += 2; }
            let start = j;
            while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') { j += 1; }
            if j > start {
                let name = &pat[start..j];
                let a: Arc<str> = Arc::from(name);
                if !out.iter().any(|s| s.as_ref() == a.as_ref()) { out.push(a); }
            }
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

// Rewrite each `${VAR}` / `${$$$VAR}` plus its surrounding identifier-like
// token run into `$SPRFSLOTN`, emitting a named regex that recovers VAR from
// the slot's matched text.
fn lower_sugar(src: &str) -> (String, Vec<(String, Regex)>, Vec<Arc<str>>) {
    let bytes = src.as_bytes();
    let mut out = String::new();
    let mut regexes: Vec<(String, Regex)> = Vec::new();
    let mut captures: Vec<Arc<str>> = Vec::new();
    let mut slot_idx = 0usize;
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            let inner_start = i + 2;
            let mut j = inner_start;
            while j < bytes.len() && bytes[j] != b'}' { j += 1; }
            if j >= bytes.len() {
                out.push('$');
                i += 1;
                continue;
            }
            let inner = &src[inner_start..j];
            let inner_trimmed = inner.trim();
            let (multi, name) = if let Some(rest) = inner_trimmed.strip_prefix("$$$") {
                (true, rest.trim())
            } else {
                (false, inner_trimmed)
            };
            if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                out.push_str(&src[i..j + 1]);
                i = j + 1;
                continue;
            }

            // Pull adjacent token-char run from the already-written output as prefix.
            let mut prefix_bytes = 0usize;
            {
                let ob = out.as_bytes();
                let mut k = ob.len();
                while k > 0 {
                    let c = ob[k - 1];
                    if c.is_ascii_alphanumeric() || c == b'_' {
                        k -= 1;
                        prefix_bytes += 1;
                    } else {
                        break;
                    }
                }
            }
            let prefix: String = out[out.len() - prefix_bytes..].to_string();
            out.truncate(out.len() - prefix_bytes);

            let after_brace = j + 1;
            let mut k = after_brace;
            while k < bytes.len() && (bytes[k].is_ascii_alphanumeric() || bytes[k] == b'_') {
                k += 1;
            }
            let suffix: String = src[after_brace..k].to_string();

            let slot_name = format!("SPRFSLOT{}", slot_idx);
            slot_idx += 1;
            out.push('$');
            out.push_str(&slot_name);

            let inner_re = if multi { ".+" } else { "\\S+" };
            let re_src = format!(
                "^{}(?P<{}>{}){}$",
                regex::escape(&prefix),
                name,
                inner_re,
                regex::escape(&suffix),
            );
            if let Ok(re) = Regex::new(&re_src) {
                regexes.push((slot_name, re));
            }
            let cap_arc: Arc<str> = Arc::from(name);
            if !captures.iter().any(|s| s.as_ref() == cap_arc.as_ref()) {
                captures.push(cap_arc);
            }
            i = k;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }

    (out, regexes, captures)
}

// ---------------------------------------------------------------------------
// Op
// ---------------------------------------------------------------------------

pub struct AstGrepOp {
    lang:           SupportLang,
    pattern:        Arc<Pattern<SupportLang>>,
    slot_regexes:   Vec<(String, Regex)>,
    bound_captures: Vec<Arc<str>>,
    src:            Arc<str>,
    parse_site:     Arc<ParseSite>,
}

impl Op for AstGrepOp {
    fn name(&self) -> &'static str { "ast" }
    fn step(&self) -> u16 { 0 }
    fn parse_site(&self) -> &Arc<ParseSite> { &self.parse_site }

    fn binds_captures(&self) -> Vec<Arc<str>> { self.bound_captures.clone() }

    fn hover_capture(&self, cap: &str, cursors: &[Cursor]) -> Option<String> {
        if !self.bound_captures.iter().any(|n| n.as_ref() == cap) { return None; }
        let entries: Vec<(Option<String>, String, String)> = cursors.iter()
            .filter_map(|c| c.captures.get(cap).map(|cv| (
                c.fs.as_ref().map(|fp| fp.0.to_string_lossy().into_owned()),
                c.rev.to_string(),
                cv.value.to_string(),
            )))
            .collect();
        let header = format!("`${}` (from ast)", cap);
        hover_render_grouped(&header, &entries)
    }

    fn pipe(&self, input: BoxStream<'static, Arc<[Cursor]>>, ctx: OpCtx)
        -> BoxStream<'static, Arc<[Cursor]>>
    {
        // Captured per-op state. All Send+Sync — cross into rayon below.
        let lang     = self.lang;
        let pattern  = self.pattern.clone();
        let slot_res = Arc::new(self.slot_regexes.clone());
        let bound    = Arc::new(self.bound_captures.clone());
        let src      = self.src.clone();
        let site     = self.parse_site.clone();
        let reader   = ctx.reader.clone();
        let diags    = ctx.diags.clone();

        // Per batch:
        //   [03] async prefetch — join_all the reader.bytes() fan-out
        //   [06] CPU work       — rayon::spawn + par_iter, oneshot back into async
        input.then(move |batch| {
            let pattern  = pattern.clone();
            let slot_res = slot_res.clone();
            let bound    = bound.clone();
            let src      = src.clone();
            let site     = site.clone();
            let reader   = reader.clone();
            let diags    = diags.clone();

            async move {
                // [03] Phase 1 — async I/O fan-out. PATH B (content in cursor)
                // is pure sync; PATH C (reader fallback) awaits. join_all drives
                // all reads concurrently on the tokio runtime.
                let preps: Vec<Prep> = join_all(
                    batch.iter().cloned().map(|c| {
                        let reader = reader.clone();
                        async move { prep_cursor(c, lang, &*reader).await }
                    })
                ).await.into_iter().flatten().collect();

                // [06] Phase 2 — CPU work on rayon pool via rayon_map helper.
                // Pattern/regex/metadata are Arc'd and cheap to clone into the
                // rayon closure.
                let site_r = site.clone();
                let results: Vec<PerFile> = rayon_map(preps, move |p| {
                    process_one(p, lang, &pattern, &slot_res, &bound, &site_r)
                }).await;

                // Diags re-enter the async runtime — DiagSink routes onto the
                // per-expr collector.
                let mut out_cursors: Vec<Cursor> = Vec::new();
                for r in results {
                    match r {
                        PerFile::Matches(v) => out_cursors.extend(v),
                        PerFile::NoMatch { path } => {
                            (diags.0)(Box::new(AstDiag::NoMatch {
                                site:    (*site).clone(),
                                path,
                                pattern: src.to_string(),
                            }));
                        }
                        PerFile::ReadFailed { path } => {
                            (diags.0)(Box::new(AstDiag::ReadFailed {
                                site: (*site).clone(),
                                path,
                            }));
                        }
                    }
                }

                Arc::<[Cursor]>::from(out_cursors.into_boxed_slice())
            }
        }).boxed()
    }
}

// ---------------------------------------------------------------------------
// Per-cursor prep + rayon worker — isolated so rayon's Send bounds are local
// ---------------------------------------------------------------------------

struct Prep {
    cursor:       Cursor,
    content:      Arc<bytes::Bytes>,
    base_offset:  usize,
    fp:           FilePath,
    /// True when PATH C read returned nothing — signals ReadFailed downstream.
    read_failed:  bool,
    /// Byte range inside `content` to feed into the parser. Equal to
    /// `cursor.byte_range` on PATH B, `0..content.len()` on PATH C.
    slice_start:  usize,
    slice_end:    usize,
    /// Cached full-file ast-grep tree. `Some` only on PATH C whole-file
    /// reads via `reader.parsed()` — rayon workers skip the local parse
    /// and `find_all` directly on the cached tree.
    cached_grep:  Option<Arc<RsAstPayload>>,
}

enum PerFile {
    Matches(Vec<Cursor>),
    NoMatch   { path: String },
    ReadFailed { path: String },
}

async fn prep_cursor(
    c:      Cursor,
    lang:   SupportLang,
    reader: &dyn crate::_3_reader::Reader,
) -> Option<Prep> {
    // PATH B — content already in cursor. Parse locally in rayon; the
    // slice may be a partial view of the file and the cache keys on full
    // files only.
    if let Some(content) = c.content.as_ref() {
        let base = c.byte_range.as_ref().map(|r| r.start).unwrap_or(0);
        let fp = c.fs.clone().unwrap_or_else(|| {
            FilePath(Arc::from(Path::new("<rebased>")))
        });
        let (start, end) = match &c.byte_range {
            Some(r) => (r.start, r.end),
            None    => (0, content.len()),
        };
        return Some(Prep {
            content:     content.clone(),
            base_offset: base,
            fp,
            read_failed: false,
            slice_start: start,
            slice_end:   end,
            cursor:      c,
            cached_grep: None,
        });
    }
    // PATH C — reader.parsed() for whole-file reads. ParseCacheReader
    // dedups across ops in the same run; process_one skips reparse.
    let fp = c.fs.clone()?;
    let kind = lang_to_kind(lang);
    let mut s = reader.parsed(&c.repo, &c.rev, &fp, kind);
    match s.next().await {
        Some(tree) => {
            let len = tree.bytes.len();
            let cached: Option<Arc<RsAstPayload>> = tree
                .payload
                .clone()
                .downcast::<RsAstPayload>()
                .ok();
            let read_failed = len == 0 && cached.is_none();
            Some(Prep {
                content:     Arc::new(tree.bytes.clone()),
                base_offset: 0,
                fp,
                read_failed,
                slice_start: 0,
                slice_end:   len,
                cursor:      c,
                cached_grep: cached,
            })
        }
        None => Some(Prep {
            content:     Arc::new(bytes::Bytes::new()),
            base_offset: 0,
            fp,
            read_failed: true,
            slice_start: 0,
            slice_end:   0,
            cursor:      c,
            cached_grep: None,
        }),
    }
}

fn lang_to_kind(lang: SupportLang) -> ParserKind {
    match lang {
        SupportLang::Rust       => ParserKind::RsAst,
        SupportLang::TypeScript => ParserKind::TsAst,
        _                       => ParserKind::RsAst, // unreachable; ast op gates on Rust/TS.
    }
}

fn process_one(
    p:        Prep,
    lang:     SupportLang,
    pattern:  &Arc<Pattern<SupportLang>>,
    slot_res: &[(String, Regex)],
    bound:    &[Arc<str>],
    site:     &Arc<ParseSite>,
) -> PerFile {
    if p.read_failed {
        return PerFile::ReadFailed { path: p.fp.0.to_string_lossy().into_owned() };
    }
    let sliced: &[u8] = &p.content[p.slice_start..p.slice_end];
    let src_str = match std::str::from_utf8(sliced) {
        Ok(s)  => s,
        Err(_) => return PerFile::NoMatch { path: p.fp.0.to_string_lossy().into_owned() },
    };

    // PATH C whole-file reads land here with a cached tree — find_all on
    // it directly. PATH B (slice) always reparses locally.
    let local_grep: Option<AstGrep<StrDoc<SupportLang>>> = if p.cached_grep.is_some() {
        None
    } else {
        let t0 = std::time::Instant::now();
        let g = lang.ast_grep(src_str);
        crate::readers::_4_parse_cache::record_parse(src_str.len(), t0);
        Some(g)
    };
    let grep_ref: &AstGrep<StrDoc<SupportLang>> = match (&p.cached_grep, &local_grep) {
        (Some(c), _)      => c.as_ref(),
        (None,    Some(g)) => g,
        _ => unreachable!(),
    };
    let mut per_cursor: Vec<Cursor> = Vec::new();

    for nm in grep_ref.root().find_all(&**pattern) {
        let env = nm.get_env();
        let mut cap_values: HashMap<Arc<str>, Arc<str>> = HashMap::new();

        for name in bound.iter() {
            if let Some(node) = env.get_match(name.as_ref()) {
                cap_values.insert(name.clone(), Arc::from(node.text().as_ref()));
            } else {
                let nodes = env.get_multiple_matches(name.as_ref());
                if !nodes.is_empty() {
                    let joined: String = nodes.iter()
                        .map(|n| n.text().to_string())
                        .collect::<Vec<_>>()
                        .join(" ");
                    cap_values.insert(name.clone(), Arc::from(joined.as_str()));
                }
            }
        }

        let mut slot_ok = true;
        for (slot_name, re) in slot_res.iter() {
            let node = match env.get_match(slot_name) {
                Some(n) => n,
                None    => { slot_ok = false; break; }
            };
            let text = node.text();
            let caps = match re.captures(text.as_ref()) {
                Some(c) => c,
                None    => { slot_ok = false; break; }
            };
            for cap_name in re.capture_names().flatten() {
                if let Some(v) = caps.name(cap_name) {
                    cap_values.insert(Arc::from(cap_name), Arc::from(v.as_str()));
                }
            }
        }
        if !slot_ok { continue; }

        let range = nm.range();
        let matched_text: Arc<str> = Arc::from(&src_str[range.clone()]);
        let mut c2 = p.cursor.clone();
        c2.content    = Some(p.content.clone());
        c2.fs         = Some(p.fp.clone());
        c2.byte_range = Some(
            (p.base_offset + range.start)..(p.base_offset + range.end)
        );
        for (k, v) in cap_values {
            c2.captures.insert(k, Capture::new(v));
        }
        c2.evidence.push(OpEvidence {
            op_name:    "ast",
            parse_site: site.clone(),
            matched:    matched_text,
            capture:    None,
        });
        per_cursor.push(c2);
    }

    if per_cursor.is_empty() {
        PerFile::NoMatch { path: p.fp.0.to_string_lossy().into_owned() }
    } else {
        PerFile::Matches(per_cursor)
    }
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum AstDiag {
    MissingLang { site: ParseSite },
    UnknownLang { site: ParseSite, got: Arc<str> },
    BadPattern  { site: ParseSite, got: Arc<str>, msg: Arc<str> },
    ReadFailed  { site: ParseSite, path: String },
    NoMatch     { site: ParseSite, path: String, pattern: String },
}

impl Diagnostic for AstDiag {
    fn code(&self) -> &str {
        match self {
            AstDiag::MissingLang { .. } => "ast/missing-lang",
            AstDiag::UnknownLang { .. } => "ast/unknown-lang",
            AstDiag::BadPattern  { .. } => "ast/bad-pattern",
            AstDiag::ReadFailed  { .. } => "ast/read-failed",
            AstDiag::NoMatch     { .. } => "ast/no-match",
        }
    }
    fn severity(&self) -> Severity { Severity::Error }
    fn primary(&self) -> &ParseSite {
        match self {
            AstDiag::MissingLang { site }     => site,
            AstDiag::UnknownLang { site, .. } => site,
            AstDiag::BadPattern  { site, .. } => site,
            AstDiag::ReadFailed  { site, .. } => site,
            AstDiag::NoMatch     { site, .. } => site,
        }
    }
    fn render(&self, out: &mut dyn Renderer) {
        match self {
            AstDiag::MissingLang { site } => {
                out.header(self.code(), self.severity(),
                    "ast requires a language bracket, e.g. ast[rust](fn $N() {})");
                out.primary(site);
            }
            AstDiag::UnknownLang { site, got } => {
                out.header(self.code(), self.severity(),
                    &format!("ast language `{got}` unknown; supported: rust, typescript"));
                out.primary(site);
            }
            AstDiag::BadPattern { site, got, msg } => {
                out.header(self.code(), self.severity(),
                    &format!("ast pattern `{got}` is invalid: {msg}"));
                out.primary(site);
            }
            AstDiag::ReadFailed { site, path } => {
                out.header(self.code(), self.severity(),
                    &format!("ast failed to read `{path}`"));
                out.primary(site);
            }
            AstDiag::NoMatch { site, path, pattern } => {
                out.header(self.code(), self.severity(),
                    &format!("ast(`{pattern}`) matched nothing in `{path}`"));
                out.primary(site);
            }
        }
    }
}
