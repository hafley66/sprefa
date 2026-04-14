//! DocSession — Layer 3: interactive analysis state.
//!
//! Sits between framework+ops (Layer 1+2) and the LSP bin (Layer 4).
//! Zero LSP crates. Positions are plain byte offsets into `source`.
//!
//! ## Concerns
//!
//! - `source` (Bytes): the .sprf rule file text being edited.
//! - `reader` (BufferOverlay): the *data* reader for repo/file content
//!   that rule pipelines consume. This is completely separate from `source`.
//!   The .sprf source lives here; BufferOverlay is for repo data files.
//! - `last_run` (RunReport): cached pipeline results, invalidated on any change.
//!
//! ## Dry-run note
//!
//! Pipeline::run produces no side effects in v2 (flush phase not yet implemented).
//! Running the pipeline is already read-only. No special flag needed; just run.

use std::collections::HashMap;
use std::sync::Arc;

use futures::executor::block_on;
use futures_util::stream::StreamExt;

use crate::_0_types::{Cursor, OpId, ParseSite, RunId};
use crate::_1_diagnostic::Diagnostic;
use crate::_5_op::{
    CompletionItem, DiagSink, EventSink, Op, OpCtx, Pipeline, ProgramCtx,
};
use crate::_5_op::OpInvocation;
use crate::_8_parse::{host_parse, host_parse_arm_brace, parse_capture, parse_cross_ref, Pipe};
use crate::_10_registry::{lower_rules, OperatorRegistry};
use crate::readers::BufferOverlay;
use crate::writers::MemWriter;
use crate::{Config, RuntimeConfig};

// ---------------------------------------------------------------------------
// SpanIndex
// ---------------------------------------------------------------------------

/// A single entry in the source byte-range → semantic kind index.
#[derive(Debug, Clone)]
pub struct SpanEntry {
    /// Byte range in `DocSession.source`.
    pub range: std::ops::Range<usize>,
    /// Which top-level rule (pipeline index) this span belongs to.
    pub rule:  Arc<str>,
    pub kind:  SpanKind,
}

/// What semantic thing a source span represents.
#[derive(Debug, Clone)]
pub enum SpanKind {
    /// The op-name identifier token (e.g. `repo` in `repo($R)`).
    /// `op_path` is a sequence of indices to walk the Pipeline tree:
    ///   [] = this is the root Op,
    ///   [i, ...] = descend into Seq child i, then follow ...
    OpName { op_path: Box<[usize]> },

    /// A `$CAP` token inside an op's paren_src.
    Capture { op_path: Box<[usize]>, var: Arc<str> },

    /// A `${rule.$VAR}` cross-ref token inside an op's paren_src.
    CrossRef { op_path: Box<[usize]>, target_rule: Arc<str>, var: Arc<str> },

    /// A literal-match token (non-capture paren arg) in an op's paren_src.
    MatchSite { op_path: Box<[usize]>, parse_site: Arc<ParseSite> },
}

// ---------------------------------------------------------------------------
// RunReport
// ---------------------------------------------------------------------------

pub struct RunReport {
    pub cursors_by_rule: HashMap<Arc<str>, Vec<Cursor>>,
    pub diags:           Vec<Box<dyn Diagnostic>>,
}

// ---------------------------------------------------------------------------
// DocSession
// ---------------------------------------------------------------------------

pub struct DocSession {
    // --- source ---
    source:   String,
    /// rule_name → Pipeline
    pipelines:    Vec<(Arc<str>, Pipeline)>,
    span_ix:      Vec<SpanEntry>,
    parse_diags:  Vec<Box<dyn Diagnostic>>,

    // --- runtime ---
    reader:   Arc<BufferOverlay>,
    factory_registry: Arc<OperatorRegistry>,

    // --- report cache ---
    last_run: Option<RunReport>,
    stale:    bool,
}

impl DocSession {
    pub fn new(
        source:   String,
        reader:   Arc<BufferOverlay>,
        registry: Arc<OperatorRegistry>,
    ) -> Self {
        let mut session = Self {
            source:           String::new(),
            pipelines:        Vec::new(),
            span_ix:          Vec::new(),
            parse_diags:      Vec::new(),
            reader,
            factory_registry: registry,
            last_run:         None,
            stale:            true,
        };
        session.reparse(source);
        session
    }

    // -----------------------------------------------------------------------
    // Mutation
    // -----------------------------------------------------------------------

    /// Replace the .sprf source text. Reparses and marks stale.
    pub fn on_source_change(&mut self, new_source: String) {
        self.reparse(new_source);
    }

    /// Update a data buffer in the reader overlay and mark stale.
    pub fn on_buffer_change(&mut self, key: crate::readers::BufferKey, bytes: bytes::Bytes) {
        self.reader.set_buffer(key, Arc::new(bytes));
        self.stale = true;
        self.last_run = None;
    }

    // -----------------------------------------------------------------------
    // Ensure run
    // -----------------------------------------------------------------------

    /// If stale, run all pipelines under the scan-check demand loop
    /// (read-only; no flush phase in v2). Results cached in `last_run`.
    ///
    /// The loop re-runs every pipeline against a single shared `ResultStore`
    /// and grows `Config.repos`/`Config.revs` from content-side scan-pointer
    /// claims until the config stops growing or `DEFAULT_DEPTH` passes run.
    /// Only the final pass's cursors + diags land in `RunReport`.
    pub fn ensure_run(&mut self) {
        if !self.stale { return; }

        let initial_config = Arc::new(Config {
            repos:        vec![],
            revs:         vec![],
            fs_exclude:   vec![],
            sprf_files:   vec![],
            shell_allow:  vec![],
            runtime: RuntimeConfig {
                worker_threads:    1,
                buffer_size:       256,
                flush_interval_ms: 100,
                collect_witnesses: true,
            xref_cartesian_limit: 10_000,
            },
            content_hash: 0,
        });

        let writer = Arc::new(MemWriter::new());
        let store  = Arc::new(crate::_12_result_store::ResultStore::new());
        // Scratch cursor map rewritten each pass; final pass wins.
        let cursors_scratch: Arc<std::sync::Mutex<HashMap<Arc<str>, Vec<Cursor>>>> =
            Arc::new(std::sync::Mutex::new(HashMap::new()));

        let pipelines = &self.pipelines;
        let reader    = self.reader.clone();
        let writer_ref = writer.clone();
        let cs        = cursors_scratch.clone();

        let result = crate::_14_scan_loop::run_scan_loop(
            store.clone(),
            initial_config,
            crate::_14_scan_loop::DEFAULT_DEPTH,
            |config, shared_store| {
                let mut pipe_diags: Vec<Box<dyn Diagnostic>> = Vec::new();
                let mut local: HashMap<Arc<str>, Vec<Cursor>> = HashMap::new();

                for (rule_name, pipeline) in pipelines {
                    let captured_diags: Arc<std::sync::Mutex<Vec<Box<dyn Diagnostic>>>> =
                        Arc::new(std::sync::Mutex::new(Vec::new()));
                    let diags_ref = captured_diags.clone();

                    let (_fresh_store, xref_seen) = OpCtx::fresh_xref_state();
                    let _ = _fresh_store; // shared_store supersedes; keep xref_seen fresh per pass.

                    let ctx = OpCtx {
                        run_id: RunId(1),
                        op_id:  OpId(0),
                        reader: reader.clone(),
                        writer: writer_ref.clone(),
                        config: config.clone(),
                        diags:  DiagSink(Arc::new(move |d| {
                            diags_ref.lock().unwrap().push(d);
                        })),
                        events: EventSink(Arc::new(|_| {})),
                        result_store: shared_store.clone(),
                        xref_seen,
                    };

                    let empty: futures_core::stream::BoxStream<'static, Cursor> =
                        futures_util::stream::iter(Vec::<Cursor>::new()).boxed();

                    let cursors: Vec<Cursor> = block_on(pipeline.run(empty, ctx).collect());
                    local.insert(rule_name.clone(), cursors);

                    let mut rd = captured_diags.lock().unwrap();
                    pipe_diags.append(&mut rd);
                }

                *cs.lock().unwrap() = local;
                pipe_diags
            },
        );

        let cursors_by_rule = std::mem::take(&mut *cursors_scratch.lock().unwrap());
        self.last_run = Some(RunReport { cursors_by_rule, diags: result.diags });
        self.stale = false;
    }

    // -----------------------------------------------------------------------
    // LSP surface
    // -----------------------------------------------------------------------

    /// Markdown string for hovering at `pos` (byte offset into source).
    pub fn hover_at(&mut self, pos: usize) -> Option<String> {
        self.ensure_run();

        // Find the smallest span containing pos.
        let entry = self.span_ix.iter()
            .filter(|e| e.range.contains(&pos))
            .min_by_key(|e| e.range.end - e.range.start)?
            .clone();

        let cursors_for = |rule: &Arc<str>| -> &[Cursor] {
            self.last_run.as_ref()
                .and_then(|r| r.cursors_by_rule.get(rule))
                .map(|v| v.as_slice())
                .unwrap_or(&[])
        };

        match &entry.kind {
            SpanKind::OpName { op_path } => {
                let op = resolve_op_in_rule(&entry.rule, &self.pipelines, op_path)?;
                Some(op.hover_self())
            }
            SpanKind::Capture { op_path, var } => {
                let cursors: Vec<Cursor> = cursors_for(&entry.rule).to_vec();
                let op = resolve_op_in_rule(&entry.rule, &self.pipelines, op_path)?;
                op.hover_capture(var, &cursors)
            }
            SpanKind::CrossRef { target_rule, var, op_path: _ } => {
                let target_rule = target_rule.clone();
                let var = var.clone();
                let cursors: Vec<Cursor> = cursors_for(&target_rule).to_vec();
                // Delegate to the first op of target rule.
                let op = resolve_first_op_of_rule(&target_rule, &self.pipelines)?;
                op.hover_capture(&var, &cursors)
            }
            SpanKind::MatchSite { op_path, parse_site } => {
                let cursors: Vec<Cursor> = cursors_for(&entry.rule).to_vec();
                eprintln!("HOVER_DBG MatchSite op_path={:?} cursors={}", op_path, cursors.len());
                let op = resolve_op_in_rule(&entry.rule, &self.pipelines, op_path);
                eprintln!("HOVER_DBG resolve_op_in_rule -> is_some={}", op.is_some());
                let op = op?;
                eprintln!("HOVER_DBG op.name={}", op.name());
                let r = op.hover_match(parse_site, &cursors);
                eprintln!("HOVER_DBG hover_match -> is_some={}", r.is_some());
                r
            }
        }
    }

    /// Completion items for the operator at `pos`. Returns all registered ops
    /// when pos is at/near the start of a new pipeline segment.
    pub fn completions_at(&self, _pos: usize) -> Vec<CompletionItem> {
        // Walk all registered operators and collect their completion items.
        // Position-sensitivity (e.g. inside a paren arg vs. op name position)
        // is a Layer 4 concern; here we always return the full op list.
        let registry = &self.factory_registry;
        // OperatorRegistry exposes only `resolve` by name; iterate known names.
        let op_names = ["rule", "repo", "rev", "fs", "read", "json"];
        op_names.iter()
            .filter_map(|name| registry.resolve(name))
            .map(|op| op.completion_item())
            .collect()
    }

    /// Diagnostics from the last run plus any parse-time diagnostics.
    pub fn diagnostics(&self) -> &[Box<dyn Diagnostic>] {
        if let Some(report) = &self.last_run {
            // parse_diags + run diags — run diags are more common query
            // Return run diags; parse diags surfaced separately via parse_diags().
            &report.diags
        } else {
            &self.parse_diags
        }
    }

    /// Parse-time diagnostics (available even before ensure_run).
    pub fn parse_diagnostics(&self) -> &[Box<dyn Diagnostic>] {
        &self.parse_diags
    }

    // -----------------------------------------------------------------------
    // Internal: reparse
    // -----------------------------------------------------------------------

    fn reparse(&mut self, source: String) {
        self.source = source;
        self.pipelines.clear();
        self.span_ix.clear();
        self.parse_diags.clear();
        self.last_run = None;
        self.stale = true;

        let file: Arc<std::path::Path> = Arc::from(std::path::Path::new("<lsp>"));
        let file_for_xref = file.clone();
        let pipes: Vec<Pipe> = match host_parse(&self.source, file.clone()) {
            Ok(p) => p,
            Err(errs) => {
                for e in errs {
                    self.parse_diags.push(Box::new(ParseDiagWrapper {
                        code:    Arc::from("parse/syntax"),
                        message: e.message.clone(),
                        offset:  e.offset,
                    }));
                }
                return;
            }
        };

        let config = Arc::new(Config {
            repos:        vec![],
            revs:         vec![],
            fs_exclude:   vec![],
            sprf_files:   vec![],
            shell_allow:  vec![],
            runtime: RuntimeConfig {
                worker_threads:    1,
                buffer_size:       256,
                flush_interval_ms: 100,
                collect_witnesses: true,
            xref_cartesian_limit: 10_000,
            },
            content_hash: 0,
        });

        // Extract rule names from paren_src before lowering.
        let rule_names: Vec<Arc<str>> = pipes.iter().enumerate().map(|(i, pipe)| {
            pipe.ops.first()
                .and_then(|op| op.paren_src.as_ref())
                .map(|ps| Arc::<str>::from(ps.src.trim()))
                .unwrap_or_else(|| Arc::from(format!("rule_{i}").as_str()))
        }).collect();

        let pctx = ProgramCtx {
            rules:     Default::default(),
            constants: Default::default(),
            config:    config.clone(),
            registry:  self.factory_registry.clone(),
            diags:     Vec::new(),
        };

        let outcome = lower_rules(pipes.clone(), pctx);
        eprintln!("REPARSE_DBG pipes={} outcome_pipelines={} diags={}",
            pipes.len(), outcome.pipelines.len(), outcome.diags.len());
        for (i, p) in outcome.pipelines.iter().enumerate() {
            let k = match p {
                Pipeline::Op(lop) => format!("Op({})", lop.op.name()),
                Pipeline::Seq(c) => format!("Seq({})", c.len()),
                Pipeline::Fork(a) => format!("Fork({})", a.len()),
                Pipeline::Switch{..} => "Switch".into(),
            };
            eprintln!("  [{}] {}", i, k);
        }
        for d in &outcome.diags {
            eprintln!("  DIAG {}: {:?}", d.code(), d.primary().byte_range);
        }
        for d in outcome.diags {
            self.parse_diags.push(d);
        }

        // Rule DAG: Kahn topo-sort over RuleHandle.depends_on. On cycle, emit
        // `xref/cycle` anchored at the first rule in the recovered loop.
        if let crate::_11_dag::RuleDag::Cycle { path } =
            crate::_11_dag::build(&outcome.pctx.rules)
        {
            let first = path.first().cloned();
            let site  = first
                .as_ref()
                .and_then(|n| outcome.pctx.rules.get(n))
                .map(|h| (*h.parse_site).clone())
                .unwrap_or_else(|| ParseSite {
                    file:       file_for_xref.clone(),
                    path:       Arc::from(Vec::new().into_boxed_slice()),
                    byte_range: 0..0,
                });
            self.parse_diags.push(Box::new(XRefDiag::Cycle { site, path }));
        }

        // Build pipelines and span index.
        // Path encoding (must match between resolve_op_in_rule and span_ix):
        //   [i, 0]               → rule op i (top-level Pipeline::Op(RuleOp))
        //   [i, 0, fork_idx, j]  → inner op j in fork arm fork_idx of rule i
        for (i, (pipeline, rule_name)) in outcome.pipelines.into_iter().zip(rule_names.iter()).enumerate() {
            let pipe = &pipes[i];
            self.build_spans_for_pipe(pipe, rule_name, &[i], 0);
            self.pipelines.push((rule_name.clone(), pipeline));
        }

        self.validate_cross_refs(file_for_xref);
    }

    /// After span_ix is fully built, walk CrossRef entries and emit diagnostics
    /// for unknown target rules or unknown target captures. Target rule captures
    /// are derived from SpanKind::Capture entries indexed by rule name.
    fn validate_cross_refs(&mut self, file: Arc<std::path::Path>) {
        use std::collections::{HashMap, HashSet};
        let mut caps_by_rule: HashMap<Arc<str>, HashSet<Arc<str>>> = HashMap::new();
        for e in &self.span_ix {
            if let SpanKind::Capture { var, .. } = &e.kind {
                caps_by_rule
                    .entry(e.rule.clone())
                    .or_default()
                    .insert(var.clone());
            }
        }
        let known_rules: HashSet<Arc<str>> =
            self.pipelines.iter().map(|(n, _)| n.clone()).collect();
        let mut out: Vec<Box<dyn Diagnostic>> = Vec::new();
        for e in &self.span_ix {
            let SpanKind::CrossRef { target_rule, var, .. } = &e.kind else { continue; };
            let site = ParseSite {
                file:       file.clone(),
                path:       Arc::from(vec![].into_boxed_slice()),
                byte_range: e.range.clone(),
            };
            if !known_rules.contains(target_rule) {
                out.push(Box::new(XRefDiag::UnknownRule {
                    site,
                    target: target_rule.clone(),
                    known:  known_rules.iter().cloned().collect(),
                }));
                continue;
            }
            let empty: HashSet<Arc<str>> = HashSet::new();
            let caps = caps_by_rule.get(target_rule).unwrap_or(&empty);
            if !caps.contains(var) {
                out.push(Box::new(XRefDiag::UnknownCapture {
                    site,
                    target: target_rule.clone(),
                    var:    var.clone(),
                    known:  caps.iter().cloned().collect(),
                }));
            }
        }
        self.parse_diags.extend(out);
    }

    /// Walk `pipe.ops` and emit SpanEntry records.
    /// `source_offset`: the absolute byte position in `self.source` corresponding
    /// to position 0 in the coordinate space these invocations were parsed in.
    /// For top-level `host_parse`, `source_offset = 0`.
    /// For brace-body ops (from `host_parse_brace`), `source_offset = brace.byte_range.start`.
    fn build_spans_for_pipe(
        &mut self,
        pipe: &Pipe,
        rule: &Arc<str>,
        path_prefix: &[usize],
        source_offset: usize,
    ) {
        self.build_spans_for_invs(&pipe.ops, rule, path_prefix, source_offset);
    }

    fn build_spans_for_invs(
        &mut self,
        invs:          &[OpInvocation],
        rule:          &Arc<str>,
        path_prefix:   &[usize],
        source_offset: usize,
    ) {
        for (op_idx, inv) in invs.iter().enumerate() {
            let mut op_path: Vec<usize> = path_prefix.to_vec();
            op_path.push(op_idx);
            let op_path_box: Box<[usize]> = op_path.into_boxed_slice();

            // OpName span: source_offset + inv's local byte start to end of name.
            let abs_name_start = source_offset + inv.parse_site.byte_range.start;
            let abs_name_end   = abs_name_start + inv.name.len();
            self.span_ix.push(SpanEntry {
                range: abs_name_start..abs_name_end,
                rule:  rule.clone(),
                kind:  SpanKind::OpName { op_path: op_path_box.clone() },
            });

            // Paren slot: scan for $CAP and ${rule.$VAR} tokens.
            // Canonicalize parse_site byte_range to absolute so it compares
            // equal to runtime-lowered OpEvidence.parse_site (which is built
            // via host_parse_arm_brace_abs).
            let abs_site = Arc::new(ParseSite {
                file:       inv.parse_site.file.clone(),
                path:       inv.parse_site.path.clone(),
                byte_range: (source_offset + inv.parse_site.byte_range.start)
                    ..(source_offset + inv.parse_site.byte_range.end),
            });
            if let Some(paren) = &inv.paren_src {
                let abs_paren_base = source_offset + paren.byte_range.start;
                self.scan_paren_tokens(
                    &paren.src,
                    abs_paren_base,
                    rule,
                    &op_path_box,
                    &abs_site,
                );
            }

            // Brace slot: recurse into brace body with adjusted offset.
            // op_path_box encodes the path to this outer op (e.g. the rule op).
            // Inner ops get paths [rule_idx, fork_idx, inner_op_idx].
            if let Some(brace) = &inv.brace_src {
                let abs_brace_base = source_offset + brace.byte_range.start;
                let file: Arc<std::path::Path> = Arc::from(std::path::Path::new("<lsp>"));
                // Prefer structural recursion (sprefa-syntax braces like rule bodies or
                // fork-arm braces). Fall back to token scan for walker-DSL braces
                // (json/md/marker) where host_parse_brace fails — tokens there still
                // need Capture spans pointing at the enclosing op.
                let parsed = host_parse_arm_brace(&brace.src, file, &inv.parse_site.path);
                match parsed {
                    Ok(inner_pipes) => {
                        for (fork_idx, inner_pipe) in inner_pipes.iter().enumerate() {
                            let mut fork_path = op_path_box.to_vec();
                            fork_path.push(fork_idx);
                            self.build_spans_for_invs(
                                &inner_pipe.ops,
                                rule,
                                &fork_path,
                                abs_brace_base,
                            );
                        }
                    }
                    Err(_) => {
                        self.scan_paren_tokens(
                            &brace.src,
                            abs_brace_base,
                            rule,
                            &op_path_box,
                            &Arc::new((*inv.parse_site).clone()),
                        );
                    }
                }
            }
        }
    }

    /// Lex `text` (contents of a paren slot) for `$CAP` and `${rule.$VAR}` tokens.
    /// `base` is the byte offset of the first char of `text` in `source`.
    fn scan_paren_tokens(
        &mut self,
        text:      &str,
        base:      usize,
        rule:      &Arc<str>,
        op_path:   &Box<[usize]>,
        parse_site: &Arc<ParseSite>,
    ) {
        let bytes = text.as_bytes();

        // If there are no `$` tokens, the whole paren content is a literal.
        // Emit a single MatchSite covering the full range and return early.
        if !bytes.contains(&b'$') {
            if !text.is_empty() {
                self.span_ix.push(SpanEntry {
                    range: base..(base + bytes.len()),
                    rule:  rule.clone(),
                    kind:  SpanKind::MatchSite {
                        op_path:    op_path.clone(),
                        parse_site: parse_site.clone(),
                    },
                });
            }
            return;
        }

        let mut i = 0usize;

        while i < bytes.len() {
            if bytes[i] != b'$' {
                i += 1;
                continue;
            }

            // Try ${rule.$VAR} cross-ref first (consumes more).
            if i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                // Find matching }
                let start = i;
                let mut j = i + 2;
                while j < bytes.len() && bytes[j] != b'}' { j += 1; }
                if j < bytes.len() {
                    let tok = &text[start..=j];
                    if let Some(xref) = parse_cross_ref(tok) {
                        self.span_ix.push(SpanEntry {
                            range: (base + start)..(base + j + 1),
                            rule:  rule.clone(),
                            kind:  SpanKind::CrossRef {
                                op_path:     op_path.clone(),
                                target_rule: xref.rule,
                                var:         xref.var,
                            },
                        });
                    }
                    i = j + 1;
                    continue;
                }
                i += 1;
                continue;
            }

            // Skip $$ provenance
            if i + 1 < bytes.len() && bytes[i + 1] == b'$' {
                i += 2;
                continue;
            }

            // Try $CAP
            let tok_start = i;
            let cap_start = i + 1;
            if cap_start < bytes.len()
                && (bytes[cap_start].is_ascii_alphabetic() || bytes[cap_start] == b'_')
            {
                let mut end = cap_start;
                while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                    end += 1;
                }
                let tok = &text[tok_start..end];
                if let Some(cap) = parse_capture(tok) {
                    self.span_ix.push(SpanEntry {
                        range: (base + tok_start)..(base + end),
                        rule:  rule.clone(),
                        kind:  SpanKind::Capture {
                            op_path: op_path.clone(),
                            var:     cap.name,
                        },
                    });
                    i = end;
                    continue;
                }
            }

            // Literal token — emit MatchSite for the whole paren content range.
            // We emit one MatchSite per op (not per literal token) to avoid overlap.
            // The MatchSite range here covers the whole paren content so hover
            // on any literal triggers hover_match.
            // Literal token — emit one MatchSite per paren slot (not per literal char).
            // Don't double-emit; check if already added for this op_path.
            let already_has_match = self.span_ix.iter().any(|e| {
                matches!(&e.kind, SpanKind::MatchSite { op_path: p, .. }
                    if p.as_ref() == op_path.as_ref() && e.range.start == base + tok_start)
            });
            if !already_has_match {
                // Emit MatchSite spanning from current literal to end of paren text.
                // This is a coarse range — sufficient for the seed tests.
                self.span_ix.push(SpanEntry {
                    range: (base + tok_start)..(base + bytes.len()),
                    rule:  rule.clone(),
                    kind:  SpanKind::MatchSite {
                        op_path:    op_path.clone(),
                        parse_site: parse_site.clone(),
                    },
                });
            }

            // Advance past this non-$ character region.
            while i < bytes.len() && bytes[i] != b'$' { i += 1; }
        }
    }
}

// ---------------------------------------------------------------------------
// Pipeline tree walk
// ---------------------------------------------------------------------------

/// Resolve an op by span_ix path within a named rule.
///
/// Path encoding (matching build_spans_for_invs):
///   [rule_pipeline_idx, 0]                  → the RuleOp itself
///   [rule_pipeline_idx, 0, fork_idx, op_idx] → inner op in brace body
///
/// `rule_pipeline_idx` is redundant once we've found the pipeline by name,
/// but is present in the stored SpanKind paths.
fn resolve_op_in_rule(
    rule:      &Arc<str>,
    pipelines: &[(Arc<str>, Pipeline)],
    op_path:   &[usize],
) -> Option<Arc<dyn Op>> {
    let (_, pipeline) = pipelines.iter().find(|(r, _)| r == rule)?;
    // Top-level pipeline is a Seq wrapping a single `Pipeline::Op(RuleOp)`
    // (lower_rules emits each top-level rule as a one-element Seq). Unwrap
    // the Seq to land on the RuleOp, then follow op_path into its body.
    let rule_op_pipeline = match pipeline {
        Pipeline::Seq(children) if children.len() == 1 => &children[0],
        _ => pipeline,
    };
    let k = match rule_op_pipeline {
        Pipeline::Op(_) => "Op", Pipeline::Seq(c) => { eprintln!("UNWRAP_Seq children={}", c.len()); "Seq" },
        Pipeline::Fork(a) => { eprintln!("UNWRAP_Fork arms={}", a.len()); "Fork" },
        Pipeline::Switch{..} => "Switch",
    };
    eprintln!("UNWRAP rule_op_pipeline kind={}", k);
    match rule_op_pipeline {
        Pipeline::Op(lop) => {
            if op_path.len() == 2 { return Some(lop.op.clone()); }
            if op_path.len() < 4 || op_path.len() % 2 != 0 { return None; }
            let body = lop.op.body_pipeline()?;
            resolve_body_op(body, &op_path[2..])
        }
        _ => resolve_op(rule_op_pipeline, op_path),
    }
}

/// Reach an op inside a rule body pipeline using (fork_idx, op_idx).
///
/// body may be:
///   Pipeline::Fork(arms)  — multi-pipe rule; fork_idx selects the arm
///   Pipeline::Seq(ops)    — single-pipe multi-op; fork_idx must be 0
///   Pipeline::Op(arc)     — single-pipe single-op; fork_idx and op_idx must be 0
fn resolve_body_op(body: &Pipeline, path: &[usize]) -> Option<Arc<dyn Op>> {
    let kind = match body {
        Pipeline::Op(_)      => "Op".to_string(),
        Pipeline::Seq(c)     => format!("Seq({})", c.len()),
        Pipeline::Fork(a)    => format!("Fork({})", a.len()),
        Pipeline::Switch{..} => "Switch".to_string(),
    };
    eprintln!("BODY kind={} path={:?}", kind, path);
    if path.len() < 2 || path.len() % 2 != 0 { return None; }
    let fork_idx = path[0];
    let op_idx   = path[1];
    let arm_pipeline: &Pipeline = match body {
        Pipeline::Fork(arms) => &arms.get(fork_idx)?.pipeline,
        _ if fork_idx == 0   => body,
        _                    => return None,
    };
    let (selected, subbody): (Arc<dyn Op>, Option<&Pipeline>) = match arm_pipeline {
        Pipeline::Seq(children) => {
            let op = match children.get(op_idx)? {
                Pipeline::Op(lop) => lop.op.clone(),
                _ => return None,
            };
            // T6 framework-fork convention: Op is followed by a Fork sibling in the Seq
            // iff the Op has a brace body. Pick it up for further descent.
            let next = children.get(op_idx + 1);
            let subbody = match next {
                Some(p @ Pipeline::Fork(_)) => Some(p),
                _ => None,
            };
            (op, subbody)
        }
        Pipeline::Op(lop) if op_idx == 0 => (lop.op.clone(), None),
        _ => return None,
    };
    if path.len() == 2 {
        return Some(selected);
    }
    resolve_body_op(subbody?, &path[2..])
}

/// General pipeline tree walk for non-rule pipelines (Switch etc.).
fn resolve_op(pipeline: &Pipeline, path: &[usize]) -> Option<Arc<dyn Op>> {
    match pipeline {
        Pipeline::Op(lop) => {
            if path.is_empty() { Some(lop.op.clone()) } else { None }
        }
        Pipeline::Seq(children) => {
            let child = children.get(*path.first()?)?;
            resolve_op(child, &path[1..])
        }
        Pipeline::Fork(arms) => {
            let arm = arms.get(*path.first()?)?;
            resolve_op(&arm.pipeline, &path[1..])
        }
        Pipeline::Switch { arms, .. } => {
            let (_, arm_pl) = arms.get(*path.first()?)?;
            resolve_op(arm_pl, &path[1..])
        }
    }
}

/// Find the first Op in a rule's pipeline (for cross-ref delegation).
fn resolve_first_op_of_rule(
    rule:      &Arc<str>,
    pipelines: &[(Arc<str>, Pipeline)],
) -> Option<Arc<dyn Op>> {
    let (_, pipeline) = pipelines.iter().find(|(r, _)| r == rule)?;
    first_op(pipeline)
}

fn first_op(pipeline: &Pipeline) -> Option<Arc<dyn Op>> {
    match pipeline {
        Pipeline::Op(lop) => Some(lop.op.clone()),
        Pipeline::Seq(children) => children.first().and_then(first_op),
        Pipeline::Fork(arms) => arms.first().and_then(|a| first_op(&a.pipeline)),
        Pipeline::Switch { arms, .. } => arms.first().and_then(|(_, p)| first_op(p)),
    }
}

// ---------------------------------------------------------------------------
// ParseDiagWrapper — minimal shim so parse errors surface as Diagnostic
// ---------------------------------------------------------------------------

struct ParseDiagWrapper {
    code:    Arc<str>,
    message: Arc<str>,
    offset:  usize,
}

impl Diagnostic for ParseDiagWrapper {
    fn code(&self) -> &str { &self.code }
    fn severity(&self) -> crate::_0_types::Severity { crate::_0_types::Severity::Error }
    fn primary(&self) -> &ParseSite {
        // ParseSite requires a file+path; return a synthetic one.
        // This will not be called in hot paths; only for display.
        // We cannot store a ParseSite without Arc, so we use a once_cell pattern.
        // Simplest: panic-free stub via a static.
        static STUB: std::sync::OnceLock<ParseSite> = std::sync::OnceLock::new();
        STUB.get_or_init(|| ParseSite {
            file:       Arc::from(std::path::Path::new("<lsp>")),
            path:       Arc::from(vec![].into_boxed_slice()),
            byte_range: 0..0,
        })
    }
    fn render(&self, out: &mut dyn crate::_1_diagnostic::Renderer) {
        out.header(&self.code, self.severity(), &self.message);
    }
}

impl std::fmt::Debug for ParseDiagWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ParseDiagWrapper({}: {} @ {})", self.code, self.message, self.offset)
    }
}

// ---------------------------------------------------------------------------
// XRefDiag — cross-ref validation diagnostics
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum XRefDiag {
    UnknownRule {
        site:   ParseSite,
        target: Arc<str>,
        known:  Vec<Arc<str>>,
    },
    UnknownCapture {
        site:   ParseSite,
        target: Arc<str>,
        var:    Arc<str>,
        known:  Vec<Arc<str>>,
    },
    Cycle {
        site: ParseSite,
        path: Vec<Arc<str>>,
    },
}

impl Diagnostic for XRefDiag {
    fn code(&self) -> &str {
        match self {
            XRefDiag::UnknownRule    { .. } => "xref/unknown-rule",
            XRefDiag::UnknownCapture { .. } => "xref/unknown-capture",
            XRefDiag::Cycle          { .. } => "xref/cycle",
        }
    }
    fn severity(&self) -> crate::_0_types::Severity { crate::_0_types::Severity::Error }
    fn primary(&self) -> &ParseSite {
        match self {
            XRefDiag::UnknownRule    { site, .. } => site,
            XRefDiag::UnknownCapture { site, .. } => site,
            XRefDiag::Cycle          { site, .. } => site,
        }
    }
    fn render(&self, out: &mut dyn crate::_1_diagnostic::Renderer) {
        match self {
            XRefDiag::UnknownRule { site, target, known } => {
                let mut names: Vec<&str> = known.iter().map(|n| n.as_ref()).collect();
                names.sort();
                let msg = format!(
                    "cross-ref target rule `{target}` does not exist (known rules: {})",
                    if names.is_empty() { "<none>".to_string() } else { names.join(", ") },
                );
                out.header(self.code(), self.severity(), &msg);
                out.primary(site);
            }
            XRefDiag::UnknownCapture { site, target, var, known } => {
                let mut names: Vec<&str> = known.iter().map(|n| n.as_ref()).collect();
                names.sort();
                let msg = format!(
                    "rule `{target}` does not declare capture `${var}` (available: {})",
                    if names.is_empty() { "<none>".to_string() } else { names.join(", ") },
                );
                out.header(self.code(), self.severity(), &msg);
                out.primary(site);
            }
            XRefDiag::Cycle { site, path } => {
                let joined = path.iter().map(|s| s.as_ref()).collect::<Vec<_>>().join(" -> ");
                let msg = format!("rule cross-ref cycle: {joined}");
                out.header(self.code(), self.severity(), &msg);
                out.primary(site);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::{RuleFactory, JsonFactory, FsFactory, RepoFactory, RevFactory, ReadFactory};
    use crate::readers::{BufferOverlay, MemReader};

    fn make_registry() -> Arc<OperatorRegistry> {
        let mut r = OperatorRegistry::new();
        r.register(Arc::new(RuleFactory));
        r.register(Arc::new(RepoFactory));
        r.register(Arc::new(RevFactory));
        r.register(Arc::new(FsFactory));
        r.register(Arc::new(ReadFactory));
        r.register(Arc::new(JsonFactory));
        Arc::new(r)
    }

    fn empty_reader() -> Arc<BufferOverlay> {
        let config = Arc::new(Config {
            repos: vec![], revs: vec![], fs_exclude: vec![],
            sprf_files: vec![], shell_allow: vec![],
            runtime: RuntimeConfig {
                worker_threads: 1, buffer_size: 256,
                flush_interval_ms: 100, collect_witnesses: false,
            xref_cartesian_limit: 10_000,
            },
            content_hash: 0,
        });
        Arc::new(BufferOverlay::new(Arc::new(MemReader::new(config))))
    }

    #[test]
    fn brace_body_capture_emits_span() {
        let source = r#"rule(foo) { > json({ tag: $TAG }) }"#.to_string();
        let reader = empty_reader();
        let registry = make_registry();
        let session = DocSession::new(source.clone(), reader, registry);

        let tag_pos = source.find("$TAG").expect("$TAG must appear in source");
        let brace_start = source.find('{').expect("brace must exist");
        let brace_end = source.rfind('}').expect("closing brace must exist");

        let capture = session.span_ix.iter().find(|e| {
            matches!(&e.kind, SpanKind::Capture { var, .. } if var.as_ref() == "TAG")
                && e.range.start >= brace_start
                && e.range.end <= brace_end + 1
        });

        assert!(
            capture.is_some(),
            "expected Capture {{ var: TAG }} inside brace body; got spans: {:?}",
            session.span_ix.iter().map(|e| (&e.range, &e.kind)).collect::<Vec<_>>()
        );

        let entry = capture.unwrap();
        assert_eq!(entry.range.start, tag_pos, "capture range start should point at $TAG");
    }

    #[test]
    fn xref_unknown_rule_diag() {
        let source = r#"rule(a) { > json({ tag: $T }) }
rule(b) { > json({ v: ${nope.$T} }) }"#.to_string();
        let session = DocSession::new(source, empty_reader(), make_registry());
        let codes: Vec<&str> = session.parse_diagnostics().iter().map(|d| d.code()).collect();
        assert!(codes.contains(&"xref/unknown-rule"), "got: {:?}", codes);
    }

    #[test]
    fn xref_unknown_capture_diag() {
        let source = r#"rule(a) { > json({ tag: $T }) }
rule(b) { > json({ v: ${a.$MISSING} }) }"#.to_string();
        let session = DocSession::new(source, empty_reader(), make_registry());
        let codes: Vec<&str> = session.parse_diagnostics().iter().map(|d| d.code()).collect();
        assert!(codes.contains(&"xref/unknown-capture"), "got: {:?}", codes);
    }

    // Two rules cross-referencing each other through walker patterns form a
    // cycle. Rule DAG detects it and emits `xref/cycle`.
    #[test]
    fn xref_cycle_diag() {
        let source = r#"rule(a) { > json({ v: ${b.$T}, tag: $T }) }
rule(b) { > json({ v: ${a.$T}, tag: $T }) }"#.to_string();
        let session = DocSession::new(source, empty_reader(), make_registry());
        let codes: Vec<&str> = session.parse_diagnostics().iter().map(|d| d.code()).collect();
        assert!(codes.contains(&"xref/cycle"), "got: {:?}", codes);
    }

    // A valid DAG (a → b) must not fire `xref/cycle`.
    #[test]
    fn xref_cycle_no_false_positive() {
        let source = r#"rule(a) { > json({ tag: $T }) }
rule(b) { > json({ v: ${a.$T}, tag: $T }) }"#.to_string();
        let session = DocSession::new(source, empty_reader(), make_registry());
        let codes: Vec<&str> = session.parse_diagnostics().iter().map(|d| d.code()).collect();
        assert!(!codes.contains(&"xref/cycle"), "got: {:?}", codes);
    }

    #[test]
    fn xref_valid_no_diag() {
        let source = r#"rule(a) { > json({ tag: $T }) }
rule(b) { > json({ v: ${a.$T} }) }"#.to_string();
        let session = DocSession::new(source, empty_reader(), make_registry());
        let codes: Vec<&str> = session.parse_diagnostics().iter().map(|d| d.code()).collect();
        assert!(!codes.iter().any(|c| c.starts_with("xref/")), "got: {:?}", codes);
    }
}
