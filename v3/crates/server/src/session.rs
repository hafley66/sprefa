//! Per-URI DocSession. Owns a source buffer, latest ParsedSource, and
//! the parse errors that came with it. Reparses on source change.
//!
//! Uses `pipeline::op_languages::language_of` as the injection resolver
//! so diagnostics from inside pattern-op paren bodies (`glob(...)`,
//! `re(...)`) surface alongside host-tree errors.
//!
//! Hover dispatch: `hover_at(offset)` walks the host CST to the
//! innermost `op_invocation`, looks up the op in the shared
//! [`pipeline::registry::Registry`], and returns the op's markdown
//! doc. Hover inside an injected pattern body is the next step (spec
//! §14.6); the registry + op trait surface is in place for it.

use std::cell::OnceCell;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pipeline::binding_graph::{analyze_pipe, BindingDiagnostic};
use pipeline::pattern_op::PatternDiagnostic;
use pipeline::registry::Registry;
use pipeline::Op;
use sprefa_parse::{host_parse_with_injections, ParseError, ParsedSource, Pipe};
use tree_sitter::Node;

use crate::config::{self, Config};

/// One op in a pipe, lowered lazily from the host CST. Shared by the
/// LSP hover render path and the pipeline run path. The same
/// `Arc<dyn Op>` flows into `Pipeline::Op` and into the hover snapshot
/// — there is no second lowering pass.
#[derive(Clone)]
pub struct LoweredOp {
    pub name: Arc<str>,
    pub op:   Option<Arc<dyn Op>>,
    pub diagnostics: Vec<PatternDiagnostic>,
    /// Host-source byte range of the full op invocation.
    pub byte_range: std::ops::Range<usize>,
    /// Host-source byte offset of the first character inside the paren
    /// body, or 0 when the op has no paren slot.
    pub paren_origin: usize,
}

pub struct DocSession {
    file: Arc<Path>,
    source: String,
    parsed: ParsedSource,
    errors: Vec<ParseError>,
    binding_diags: Vec<BindingDiagnostic>,
    registry: Registry,
    config: Config,
    config_source: ConfigSource,
    /// Lazily computed on first call to [`DocSession::lowered_pipes`].
    /// Cleared on every source change. One inner `Vec<LoweredOp>` per
    /// host-level pipe, in source order.
    lowered: OnceCell<Vec<Vec<LoweredOp>>>,
}

/// Where the resolved [`Config`] came from. Surfaced on hover so the
/// author can tell whether a seed came from an auto-discovered
/// `.sprefa.toml` or the cwd fallback.
#[derive(Debug, Clone)]
pub enum ConfigSource {
    File(PathBuf),
    CwdFallback,
}

impl DocSession {
    pub fn new(file: Arc<Path>, source: String, registry: Registry) -> Self {
        let (parsed, errors) = host_parse_with_injections(
            &source,
            file.clone(),
            &pipeline::op_languages::language_of,
        );
        let binding_diags = collect_binding_diags(&parsed, source.as_bytes());
        let (config, config_source) = discover_config(&file);
        Self {
            file, source, parsed, errors, binding_diags, registry,
            config, config_source,
            lowered: OnceCell::new(),
        }
    }

    /// Lazily lower every op in every top-level pipe. Computed once per
    /// source revision; subsequent calls return the cached slice. Both
    /// the LSP hover path and the pipeline run path read from this —
    /// the synth-reparse round-trip in earlier code is gone.
    pub fn lowered_pipes(&self) -> &[Vec<LoweredOp>] {
        self.lowered.get_or_init(|| lower_all_pipes(&self.parsed, self.source.as_bytes(), &self.registry))
    }

    pub fn config(&self) -> &Config { &self.config }
    pub fn config_source(&self) -> &ConfigSource { &self.config_source }
    pub fn registry(&self) -> &Registry { &self.registry }

    pub fn on_source_change(&mut self, new_source: String) {
        self.source = new_source;
        let (parsed, errors) = host_parse_with_injections(
            &self.source,
            self.file.clone(),
            &pipeline::op_languages::language_of,
        );
        self.binding_diags = collect_binding_diags(&parsed, self.source.as_bytes());
        self.parsed = parsed;
        self.errors = errors;
        self.lowered = OnceCell::new();
    }

    pub fn source(&self) -> &str { &self.source }
    pub fn parsed(&self) -> &ParsedSource { &self.parsed }
    pub fn parse_errors(&self) -> &[ParseError] { &self.errors }
    pub fn binding_diagnostics(&self) -> &[BindingDiagnostic] { &self.binding_diags }

    /// Hover markdown at a byte offset. Returns `None` when the offset
    /// is outside any op_invocation or the op is not registered.
    ///
    /// Walks the host CST to find the innermost descendant at `offset`,
    /// then climbs to the enclosing `op_invocation` (or matches
    /// directly when the cursor sits on the op's name identifier).
    /// Registry lookup by op name produces the hover body.
    /// Locate the pipe + op index at `offset` for a hover run. Returns
    /// a slice of every op invocation from the pipe's head up to and
    /// including the hovered op, plus the op's name. Pipes nested
    /// inside rule bodies are skipped (top-level pipes only for now).
    pub fn hover_plan(&self, offset: usize) -> Option<HoverPlan> {
        for (pipe_idx, pipe) in self.parsed.pipes.iter().enumerate() {
            for (idx, inv) in pipe.ops.iter().enumerate() {
                let rng = &inv.parse_site.byte_range;
                if rng.contains(&offset) || offset == rng.end {
                    return Some(HoverPlan {
                        pipe_idx,
                        focus_name: inv.name.to_string(),
                        focus_step: idx,
                        pipe_len: pipe.ops.len(),
                        hover_offset: offset,
                    });
                }
            }
        }
        None
    }

    pub fn hover_at(&self, offset: usize) -> Option<String> {
        let root = self.parsed.tree.root_node();
        let node = root.descendant_for_byte_range(offset, offset)?;
        let inv = ascend_to_op_invocation(node)?;
        let name_node = inv.child_by_field_name("name")?;
        let name = &self.source[name_node.start_byte()..name_node.end_byte()];
        self.registry.doc(name).map(|d| d.to_string())
    }

    /// Completion suggestions at `offset`. Two contexts, picked
    /// textually so mid-edit (broken-parse) states still fire:
    ///
    /// - **Pipe-head** — cursor follows `>`, `;`, `{`, or is the start
    ///   of the file (modulo whitespace and `#`-comments). Suggests
    ///   every registered op name, filtered by the identifier prefix
    ///   typed so far; hover doc rides along as `documentation`.
    /// - **$-capture** — cursor sits inside an `op_invocation` paren
    ///   body immediately after `$`. Suggests the union of `$NAME`
    ///   bindings from upstream steps of the enclosing pipe so repeat
    ///   references to `$R`, `$V`, `$P` across a pipe don't require
    ///   retyping.
    ///
    /// Rule-name and injected-body completion (glob/regex) deferred.
    pub fn completions_at(&self, offset: usize) -> Vec<CompletionSuggestion> {
        let src = self.source.as_bytes();
        if offset > src.len() {
            return Vec::new();
        }

        if let Some(items) = self.capture_completions(offset) {
            return items;
        }

        let prefix = identifier_prefix(src, offset);
        let head_start = offset - prefix.len();
        if !is_pipe_head(src, head_start) {
            return Vec::new();
        }

        let mut names = self.registry.names();
        names.sort();
        names
            .into_iter()
            .filter(|n| n.starts_with(prefix))
            .map(|n| CompletionSuggestion {
                label: n.to_string(),
                detail: Some("op".to_string()),
                documentation: self.registry.doc(n).map(|d| d.to_string()),
                kind: SuggestionKind::Op,
            })
            .collect()
    }

    fn capture_completions(&self, offset: usize) -> Option<Vec<CompletionSuggestion>> {
        let src = self.source.as_bytes();

        let prefix = identifier_prefix(src, offset);
        let dollar_pos = offset.checked_sub(prefix.len())?.checked_sub(1)?;
        if src.get(dollar_pos).copied() != Some(b'$') {
            return None;
        }

        // Textual inside-paren detection: mid-edit sources like
        // `repo($R) > fs($|` don't parse a paren_slot yet, so walk
        // backward counting bracket depth.
        if !inside_unclosed_paren(src, dollar_pos) {
            return None;
        }

        // Upstream scope: start of this pipe. Walk backward from
        // dollar_pos to the nearest `;` or `{` at the same or lower
        // brace-depth, or to 0.
        let scope_start = pipe_scope_start(src, dollar_pos);
        let scope = std::str::from_utf8(&src[scope_start..dollar_pos]).unwrap_or("");

        let mut names: Vec<String> = Vec::new();
        scan_dollar_idents(scope, &mut names);
        names.sort();
        names.dedup();

        Some(
            names
                .into_iter()
                .filter(|n| n.starts_with(prefix))
                .map(|n| CompletionSuggestion {
                    label: n.clone(),
                    detail: Some("capture".to_string()),
                    documentation: None,
                    kind: SuggestionKind::Capture,
                })
                .collect(),
        )
    }
}

/// Plan for an enriched hover. Carries indices into the session's
/// lazily-lowered pipe cache; the renderer reads the actual
/// `Arc<dyn Op>` slice from there, so there is no hover-specific
/// lowering pathway.
#[derive(Debug, Clone)]
pub struct HoverPlan {
    /// Index into `DocSession::lowered_pipes()`.
    pub pipe_idx:    usize,
    pub focus_name:  String,
    pub focus_step:  usize,
    pub pipe_len:    usize,
    /// Absolute hover offset into the host source. The renderer pairs
    /// it with the focus op's `term_positions` to identify a focused
    /// capture inside opaque paren bodies (json/ast/str), and to
    /// surface a capture-targeted hover.
    pub hover_offset: usize,
}

/// Single lowering pathway shared by hover, run, future HTTP/CLI
/// drivers. For each top-level pipe, walks its op invocations and calls
/// `registry.build_from_node` against the original host CST node — no
/// synth re-parse, no string round-trip. `Op::term_positions()` therefore
/// comes back in host coordinates directly.
///
/// On lower failure, the slot keeps `op = None` and stashes the
/// diagnostics. Callers (hover render, future LSP diag publisher) can
/// surface those instead of guessing from a missing `Pipeline`.
fn lower_all_pipes(
    parsed:   &ParsedSource,
    src:      &[u8],
    registry: &Registry,
) -> Vec<Vec<LoweredOp>> {
    parsed
        .pipes
        .iter()
        .map(|pipe| {
            pipe.ops
                .iter()
                .map(|inv| {
                    let mut diags = Vec::new();
                    let lookup_name: Arc<str> = if inv.predicate {
                        Arc::from(format!("{}?", inv.name))
                    } else {
                        inv.name.clone()
                    };
                    let built = registry.build_from_node(&lookup_name, inv.node(), src, &mut diags);
                    let (op, mut diagnostics) = match built {
                        Some(Ok(arc)) => (Some(arc), diags),
                        Some(Err(errs)) => (None, errs),
                        None            => (None, diags),
                    };
                    diagnostics.shrink_to_fit();
                    let node = inv.node();
                    let paren_origin = node
                        .child_by_field_name("paren")
                        .map(|p| p.start_byte() + 1)
                        .unwrap_or(0);
                    LoweredOp {
                        name:         inv.name.clone(),
                        op,
                        diagnostics,
                        byte_range:   inv.parse_site.byte_range.clone(),
                        paren_origin,
                    }
                })
                .collect()
        })
        .collect()
}

/// Walk from the .sprf file's directory up to the filesystem root,
/// picking up the first `.sprefa.toml` we find. Fall back to a single
/// cwd-rooted HEAD seed.
fn collect_binding_diags(parsed: &ParsedSource, src: &[u8]) -> Vec<BindingDiagnostic> {
    let mut out = Vec::new();
    let root = parsed.tree.root_node();
    let mut walker = root.walk();
    for child in root.named_children(&mut walker) {
        if child.kind() != "pipe" { continue; }
        let report = analyze_pipe(child, src);
        out.extend(report.diagnostics);
    }
    out
}

fn discover_config(file: &Path) -> (Config, ConfigSource) {
    let start = file.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    });
    let mut dir = start.as_path();
    loop {
        let candidate = dir.join(".sprefa.toml");
        if candidate.is_file() {
            if let Ok(cfg) = config::from_path(&candidate) {
                return (cfg, ConfigSource::File(candidate));
            }
        }
        let Some(parent) = dir.parent() else { break };
        if parent == dir { break; }
        dir = parent;
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    (config::from_cli(cwd, "HEAD".into()), ConfigSource::CwdFallback)
}

#[allow(dead_code)]
fn _pipe_trait_witness(_: &Pipe) {}

/// Completion suggestion. Backend adapts this into `CompletionItem`.
#[derive(Debug, Clone)]
pub struct CompletionSuggestion {
    pub label: String,
    pub detail: Option<String>,
    pub documentation: Option<String>,
    pub kind: SuggestionKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionKind {
    Op,
    Capture,
}

/// Climb from `node` toward the root until we reach an `op_invocation`.
/// Returns the node itself when it is already one.
fn ascend_to_op_invocation(node: Node<'_>) -> Option<Node<'_>> {
    let mut cur = node;
    loop {
        if cur.kind() == "op_invocation" {
            return Some(cur);
        }
        cur = cur.parent()?;
    }
}

fn identifier_prefix(src: &[u8], offset: usize) -> &str {
    let mut start = offset;
    while start > 0 {
        let b = src[start - 1];
        if b.is_ascii_alphanumeric() || b == b'_' {
            start -= 1;
        } else {
            break;
        }
    }
    std::str::from_utf8(&src[start..offset]).unwrap_or("")
}

/// Is `offset` a pipe-head position? Scan backward past whitespace
/// and `#`-prefixed line comments. Accept if the previous non-blank
/// character is `>`, `;`, `{`, or start of file.
fn is_pipe_head(src: &[u8], offset: usize) -> bool {
    let mut i = offset;
    while i > 0 {
        let b = src[i - 1];
        if b == b' ' || b == b'\t' {
            i -= 1;
            continue;
        }
        if b == b'\n' || b == b'\r' {
            i -= 1;
            let line_end = i;
            let mut line_start = line_end;
            while line_start > 0 && src[line_start - 1] != b'\n' {
                line_start -= 1;
            }
            let line = &src[line_start..line_end];
            let trimmed_start = line
                .iter()
                .position(|c| *c != b' ' && *c != b'\t')
                .unwrap_or(line.len());
            if line.get(trimmed_start).copied() == Some(b'#') {
                i = line_start;
                continue;
            }
            if line.iter().all(|c| *c == b' ' || *c == b'\t') {
                i = line_start;
                continue;
            }
            let last = line.iter().rposition(|c| *c != b' ' && *c != b'\t');
            return matches!(last.map(|p| line[p]), Some(b'>') | Some(b';') | Some(b'{'));
        }
        return matches!(b, b'>' | b';' | b'{');
    }
    true
}

/// Scan every `$IDENT` token in `s`. Handles `$NAME` and `${NAME}`.
fn scan_dollar_idents(s: &str, out: &mut Vec<String>) {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'$' {
            let mut j = i + 1;
            if j < b.len() && b[j] == b'{' {
                j += 1;
            }
            let start = j;
            while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                j += 1;
            }
            if j > start {
                if let Ok(tok) = std::str::from_utf8(&b[start..j]) {
                    out.push(tok.to_string());
                }
            }
            i = j;
        } else {
            i += 1;
        }
    }
}

/// True if `pos` is inside an unclosed `(` when scanning source
/// textually. Tracks `"`/`'` string state and skips over balanced
/// `[...]`/`{...}`. Robust against mid-edit broken parses.
fn inside_unclosed_paren(src: &[u8], pos: usize) -> bool {
    let mut paren_depth: i32 = 0;
    let mut bracket_depth: i32 = 0;
    let mut brace_depth: i32 = 0;
    let mut in_str: Option<u8> = None;
    let end = pos.min(src.len());
    let mut i = 0;
    while i < end {
        let b = src[i];
        match in_str {
            Some(q) => {
                if b == b'\\' { i += 2; continue; }
                if b == q { in_str = None; }
            }
            None => match b {
                b'"' | b'\'' => in_str = Some(b),
                b'(' => paren_depth += 1,
                b')' => paren_depth -= 1,
                b'[' => bracket_depth += 1,
                b']' => bracket_depth -= 1,
                b'{' => brace_depth += 1,
                b'}' => brace_depth -= 1,
                _ => {}
            },
        }
        i += 1;
    }
    paren_depth > 0 && bracket_depth == 0 && in_str.is_none()
        // Rule/brace bodies nest — we're inside paren at any brace depth.
        && brace_depth >= 0
}

/// Start of the current pipe, scanning backward from `pos`. Stops at
/// `;`, `{`, or start-of-file, ignoring those inside strings or deeper
/// paren/bracket/brace depth relative to `pos`. Good enough to scope
/// upstream `$IDENT` collection.
fn pipe_scope_start(src: &[u8], pos: usize) -> usize {
    let mut i = pos.min(src.len());
    let mut paren_depth = 0i32;
    let mut bracket_depth = 0i32;
    let mut brace_depth = 0i32;
    while i > 0 {
        i -= 1;
        let b = src[i];
        match b {
            b')' => paren_depth += 1,
            b'(' => {
                if paren_depth == 0 {
                    // Started at inside-paren; crossing the opening
                    // means we reached the op invocation at the head.
                    // Keep going so we capture identifiers before it.
                } else {
                    paren_depth -= 1;
                }
            }
            b']' => bracket_depth += 1,
            b'[' => bracket_depth = (bracket_depth - 1).max(0),
            b'}' => brace_depth += 1,
            b'{' => {
                if brace_depth == 0 {
                    return i + 1;
                }
                brace_depth -= 1;
            }
            b';' if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                return i + 1;
            }
            _ => {}
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn doc(src: &str) -> DocSession {
        let p: Arc<Path> = Arc::from(PathBuf::from("test.sprf").as_path());
        DocSession::new(p, src.to_string(), Registry::with_stdlib())
    }

    #[test]
    fn parse_and_hover_scenarios() {
        // Clean source yields zero errors and reparse-on-change flips
        // the flag when syntax breaks.
        let mut s = doc("foo > bar");
        assert!(s.parse_errors().is_empty());
        assert_eq!(s.parsed().pipes.len(), 1);

        s.on_source_change("foo > > bar".into());
        assert!(
            !s.parse_errors().is_empty(),
            "`foo > > bar` is a syntax error"
        );

        // Hover inside `repo(...)` returns the registered doc.
        let s = doc("repo(myorg/*) > fs(**/*.rs)");
        let offset = s.source().find("repo").unwrap() + 1;
        let hover = s.hover_at(offset).expect("hover on repo");
        assert!(hover.contains("**repo**"), "doc starts with op name");
        assert!(hover.contains("`cursor.repo`"), "explains what it filters");

        // Hover on `fs` picks up the fs doc independently.
        let offset = s.source().find("fs").unwrap() + 1;
        let hover = s.hover_at(offset).expect("hover on fs");
        assert!(hover.contains("**fs**"));

        // Offset outside any op invocation returns None.
        assert!(s.hover_at(0).is_none() || s.hover_at(s.source().len()).is_none());
    }

    // A plain source with one unregistered op: hover must return None
    // (no crash, no stale doc from a different name).
    #[test]
    fn completion_pipe_head_and_capture_scenarios() {
        // Start of file: all op names offered.
        let s = doc("");
        let items = s.completions_at(0);
        assert!(items.iter().any(|i| i.label == "repo"));
        assert!(items.iter().any(|i| i.label == "fs"));
        assert!(items.iter().all(|i| i.kind == SuggestionKind::Op));

        // After `>` with prefix `f`: only ops starting with `f`.
        let src = "repo(r) > f";
        let s = doc(src);
        let items = s.completions_at(src.len());
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, vec!["fs"], "only `fs` starts with `f`");

        // Comment-preceded head position still qualifies.
        let src = "# comment\n";
        let s = doc(src);
        assert!(!s.completions_at(src.len()).is_empty());

        // Inside `fs($|` — capture completion picks up upstream `$R`.
        let src = "repo($R) > fs($";
        let s = doc(src);
        let items = s.completions_at(src.len());
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, vec!["R"]);
        assert_eq!(items[0].kind, SuggestionKind::Capture);

        // Mid-paren non-`$` context: no head completion.
        let src = "repo(";
        let s = doc(src);
        assert!(s.completions_at(src.len()).is_empty());
    }

    #[test]
    fn hover_plan_slices_pipe_up_to_focused_op() {
        let s = doc("repo($R) > rev(HEAD) > fs(**/*.rs)");
        // Hover inside `fs` (step 2, 0-indexed).
        let offset = s.source().find("fs").unwrap() + 1;
        let plan = s.hover_plan(offset).expect("fs is inside a pipe");
        assert_eq!(plan.focus_name, "fs");
        assert_eq!(plan.focus_step, 2);
        assert_eq!(plan.pipe_len, 3);
        assert_eq!(plan.pipe_idx, 0);
        // The lowering cache resolves the same slice the renderer reads.
        let slice = &s.lowered_pipes()[plan.pipe_idx][..=plan.focus_step];
        let names: Vec<_> = slice.iter().map(|o| o.name.as_ref()).collect();
        assert_eq!(names, vec!["repo", "rev", "fs"]);
        assert!(slice.iter().all(|o| o.op.is_some()), "every step lowered");
    }

    #[test]
    fn hover_on_unregistered_op_returns_none() {
        let s = DocSession::new(
            Arc::from(PathBuf::from("t.sprf").as_path()),
            "nope(x) > repo(r)".into(),
            Registry::with_stdlib(),
        );
        let offset = s.source().find("nope").unwrap() + 1;
        assert!(s.hover_at(offset).is_none());
    }
}
