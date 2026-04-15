//! $rule[name]{body} — the binder.
//!
//! Responsibilities:
//!   - pre_register: add rule name to ProgramCtx.rules
//!   - parse: host_parse brace body, walk paren_src tokens to collect captures,
//!            create RuleTableSpec, recursively lower body ops, wrap in RuleOp
//!   - pipe (runtime): seed cursor stream from reader.repos × reader.revs,
//!                     run body pipeline, write one row per surviving cursor
//!
//! Seed: one empty-capture cursor per (repo, rev) combo.

use std::collections::HashMap;
use std::sync::Arc;

use futures::executor::block_on;
use futures_core::stream::BoxStream;
use futures_util::stream::{self, StreamExt};

use crate::_0_types::{Cursor, ParseSeg, ParseSite, SprfPath};
use crate::_1_diagnostic::{Diagnostic, Renderer};
use crate::_4_writer::{ColumnKind, ColumnSpec, ExtractionRow, RuleTableSpec};
use crate::_5_op::{
    BraceMode, CompletionItem, GrammarRef, Op, OpCtx, OpInvocation, Operator, Pipeline,
    ProgramCtx, RuleHandle,
};
use crate::_5_op::ForkBranch;
use crate::_8_parse::{host_parse_arm_brace, Pipe};
use crate::_10_registry::lower_chain;

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

pub struct RuleFactory;

impl Operator for RuleFactory {
    fn name(&self) -> &'static str { "rule" }
    fn paren_grammar(&self) -> GrammarRef { GrammarRef(Arc::from("rule-params")) }
    fn brace_mode(&self) -> BraceMode { BraceMode::CustomSprf }

    fn completion_item(&self) -> CompletionItem {
        CompletionItem {
            label:  "rule".to_string(),
            detail: "rule(name) { ... }".to_string(),
            doc:    "# rule\n\nDefines a named extraction rule with a brace body.\n\nCaptures declared in child ops are columns in the output table.".to_string(),
        }
    }

    fn pre_register(&self, inv: &OpInvocation, pctx: &mut ProgramCtx)
        -> Result<(), Vec<Box<dyn Diagnostic>>>
    {
        let name = rule_name(inv).map_err(|d| vec![d])?;
        pctx.rules.insert(name.clone(), RuleHandle {
            name,
            parse_site: inv.parse_site.clone(),
            captures:   vec![],
            depends_on: vec![],
        });
        Ok(())
    }

    fn parse(&self, inv: &OpInvocation, pctx: &mut ProgramCtx)
        -> Result<Pipeline, Vec<Box<dyn Diagnostic>>>
    {
        let name = rule_name(inv).map_err(|d| vec![d])?;
        let brace = inv.brace_src.as_ref().ok_or_else(|| vec![
            Box::new(RuleDiag::MissingBody { site: (*inv.parse_site).clone() })
                as Box<dyn Diagnostic>
        ])?;

        // Parse body pipes — re-enter parser with this rule's parse_path so children
        // get `<rule>/BraceChild[N]/...` ParseSites instead of fresh `Top[N]`.
        let body_pipes: Vec<Pipe> = crate::_8_parse::host_parse_arm_brace_abs(
            &brace.src,
            inv.parse_site.file.clone(),
            &inv.parse_site.path,
            brace.byte_range.start,
        ).map_err(|errs| errs.into_iter().map(|e| {
            Box::new(RuleDiag::BodyParse { site: (*inv.parse_site).clone(), msg: e.message })
                as Box<dyn Diagnostic>
        }).collect::<Vec<_>>())?;

        // Scan all body invocations for capture names, in order of first appearance.
        let all_invs: Vec<&OpInvocation> = body_pipes.iter()
            .flat_map(|p| p.ops.iter())
            .collect();
        let captures = collect_captures(&all_invs);

        // Validate every op exists before lowering.
        let registry = pctx.registry.clone();
        let mut all_diags: Vec<Box<dyn Diagnostic>> = Vec::new();
        for b_inv in &all_invs {
            if registry.resolve(&b_inv.name).is_none() {
                all_diags.push(Box::new(RuleDiag::UnknownBodyOp {
                    site: (*b_inv.parse_site).clone(),
                    name: b_inv.name.clone(),
                }));
            }
        }
        if !all_diags.is_empty() { return Err(all_diags); }

        // Each pipe (`>` chain) lowers to Pipeline::Seq. Multiple pipes inside a
        // brace = Fork (distribute parent). Single pipe = no wrapping.
        let mut body_pipelines = Vec::with_capacity(body_pipes.len());
        for pipe in &body_pipes {
            let chain = lower_chain(&pipe.ops, pctx, &mut all_diags);
            let p = if chain.len() == 1 {
                chain.into_iter().next().unwrap()
            } else {
                Pipeline::Seq(chain)
            };
            body_pipelines.push(p);
        }
        if !all_diags.is_empty() { return Err(all_diags); }

        // Collect cross-ref targets across body invocations (paren/bracket slots).
        // De-dup and drop self-refs. Walker-embedded xrefs are not yet surfaced.
        let mut depends_on: Vec<Arc<str>> = Vec::new();
        for b_inv in &all_invs {
            for xr in &b_inv.crossrefs {
                if xr.rule == name { continue; }
                if !depends_on.iter().any(|r| r == &xr.rule) {
                    depends_on.push(xr.rule.clone());
                }
            }
        }

        // Update handle captures + deps now that we know them.
        if let Some(h) = pctx.rules.get_mut(&name) {
            h.captures   = captures.clone();
            h.depends_on = depends_on;
        }

        // Build table spec.
        let spec = RuleTableSpec {
            namespace: Arc::from(""),
            rule:      name.clone(),
            columns:   captures.iter().map(|c| ColumnSpec {
                capture_name: c.clone(),
                kind:         ColumnKind::Both,
            }).collect(),
            cross_fks: vec![],
        };

        // Build Fork branches with each pipe's parse_site (= first op's site, which
        // already encodes BraceChild[N] from host_parse_arm_brace).
        let body = match body_pipelines.len() {
            0 => Pipeline::Seq(vec![]),
            1 => body_pipelines.into_iter().next().unwrap(),
            _ => {
                let arms: Vec<ForkBranch> = body_pipelines.into_iter()
                    .zip(body_pipes.iter())
                    .map(|(pl, pipe)| ForkBranch {
                        parse_site: pipe.ops.first()
                            .map(|op| op.parse_site.clone())
                            .unwrap_or_else(|| inv.parse_site.clone()),
                        pipeline:   pl,
                    })
                    .collect();
                Pipeline::Fork(arms)
            }
        };

        Ok(Pipeline::Op(Arc::new(RuleOp {
            name,
            spec,
            captures,
            body,
            parse_site: inv.parse_site.clone(),
        }).into()))
    }
}

// ---------------------------------------------------------------------------
// Op
// ---------------------------------------------------------------------------

pub struct RuleOp {
    pub name:       Arc<str>,
    pub spec:       RuleTableSpec,
    pub captures:   Vec<Arc<str>>,
    pub body:       Pipeline,
    pub parse_site: Arc<ParseSite>,
}


impl Op for RuleOp {
    fn name(&self) -> &'static str { "rule" }
    fn step(&self) -> u16 { 0 }
    fn parse_site(&self) -> &Arc<ParseSite> { &self.parse_site }

    fn hover_self(&self) -> String {
        let cols: Vec<String> = self.captures.iter()
            .map(|c| format!("- `${}`", c))
            .collect();
        let body = if cols.is_empty() {
            String::from("_(no captures)_")
        } else {
            cols.join("\n")
        };
        format!(
            "# rule: {}\n\n{}\n\n\
             <details><summary>pipeline shape</summary>\n\n\
             ```mermaid\n\
             flowchart LR\n\
             repo --> rev --> fs --> json\n\
             ```\n\n\
             </details>\n\n\
             <details><summary>columns</summary>\n\n\
             | col | kind |\n| --- | --- |\n{}\n\n\
             </details>",
            self.name,
            body,
            self.captures.iter()
                .map(|c| format!("| `${}` | Both |", c))
                .collect::<Vec<_>>()
                .join("\n"),
        )
    }

    fn body_pipeline(&self) -> Option<&crate::_5_op::Pipeline> { Some(&self.body) }

    fn with_body(&self, new_body: Pipeline) -> Option<Arc<dyn Op>> {
        Some(Arc::new(RuleOp {
            name:       self.name.clone(),
            spec:       self.spec.clone(),
            captures:   self.captures.clone(),
            body:       new_body,
            parse_site: self.parse_site.clone(),
        }))
    }

    fn hover_capture(&self, cap: &str, cursors: &[crate::_0_types::Cursor]) -> Option<String> {
        let mut vals: Vec<&str> = Vec::new();
        for c in cursors {
            if let Some(capture) = c.captures.get(cap) {
                let v = capture.value.as_ref();
                if !vals.contains(&v) {
                    vals.push(v);
                    if vals.len() >= 20 { break; }
                }
            }
        }
        if vals.is_empty() { return None; }
        let lines: Vec<String> = vals.iter().map(|v| format!("- `{}`", v)).collect();
        Some(format!("**`${cap}`** values:\n\n{}", lines.join("\n")))
    }

    fn pipe(&self, _input: BoxStream<'static, Cursor>, ctx: OpCtx)
        -> BoxStream<'static, Cursor>
    {
        // Seed: cartesian of all repos × their revs. Runner-fed input ignored;
        // $rule is a source. Later, input could narrow the seed.
        let reader = ctx.reader.clone();
        let run_id = ctx.run_id;
        let parse_site = self.parse_site.clone();

        let seed: Vec<Cursor> = {
            // cfg.repos is the source of truth when non-empty; reader.repos() is
            // the catalog fallback (used before any scan-loop growth or when the
            // caller passes an empty Config on purpose). Mirror for revs: per-repo
            // reader lookup, filtered by cfg.revs when non-empty.
            let cfg = &ctx.config;
            let repos_vec: Vec<Arc<str>> = if !cfg.repos.is_empty() {
                cfg.repos.clone()
            } else {
                block_on(reader.repos().next()).unwrap_or_default()
            };
            let cfg_revs_filter: Option<&[Arc<str>]> =
                if cfg.revs.is_empty() { None } else { Some(&cfg.revs) };
            let mut out = Vec::new();
            for repo in &repos_vec {
                let reader_revs: Vec<Arc<str>> =
                    block_on(reader.revs(repo).next()).unwrap_or_default();
                let revs_vec: Vec<Arc<str>> = match cfg_revs_filter {
                    None    => reader_revs,
                    Some(f) => reader_revs.into_iter()
                        .filter(|rv| f.iter().any(|w| w.as_ref() == rv.as_ref()))
                        .collect(),
                };
                for rev in revs_vec {
                    out.push(Cursor {
                        run_id,
                        repo: repo.clone(),
                        rev,
                        fs: None,
                        captures: HashMap::new(),
                        fks: HashMap::new(),
                        // Empty path; outer Pipeline::Op tags us with PathSeg::Op{rule}.
                        path: SprfPath(Arc::from(Vec::<crate::_0_types::PathSeg>::new().into_boxed_slice())),
                        evidence: Vec::new(),
                        content: None,
                    });
                }
            }
            out
        };

        let seed_stream: BoxStream<'static, Cursor> =
            stream::iter(seed).boxed();

        // Run body.
        let after_body = self.body.run(seed_stream, ctx.clone());

        // Sink: write one row per cursor to both the writer (SQL-bound rows)
        // and the ResultStore (live Captures w/ scan_pointer + Tri). The store
        // is what the scan-loop demand-walks between passes, so every cursor's
        // captures must land there keyed by rule name.
        let writer     = ctx.writer.clone();
        let spec       = self.spec.clone();
        let captures   = self.captures.clone();
        let rule_name  = self.name.clone();
        let store      = ctx.result_store.clone();
        let store_tail = store.clone();
        let rule_tail  = rule_name.clone();
        let _          = writer.create_rule_table(&spec);

        let mark_tail = async move {
            store_tail.mark_complete(&rule_tail);
        };

        after_body
            .map(move |c| {
                let row = cursor_to_row(&c, &captures);
                let _ = writer.write_rows(&spec, std::slice::from_ref(&row));
                store.append(&rule_name, c.captures.clone());
                Some(c)
            })
            .chain(futures_util::stream::once(mark_tail).map(|()| None))
            .filter_map(|x| async move { x })
            .boxed()
    }
}

fn cursor_to_row(c: &Cursor, captures: &[Arc<str>]) -> ExtractionRow {
    ExtractionRow {
        repo:      c.repo.clone(),
        rev:       c.rev.clone(),
        file:      c.fs.as_ref().map(|fp| Arc::from(fp.0.to_string_lossy().as_ref())),
        file_hash: None,
        captures:  captures.iter().map(|name| {
            let v = c.captures.get(name).map(|cap| cap.value.clone());
            // ref_id/string_id lookup belongs to real writer wiring; for now pass None.
            let _ = v;
            (name.clone(), None, None)
        }).collect(),
        fks: vec![],
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn rule_name(inv: &OpInvocation) -> Result<Arc<str>, Box<dyn Diagnostic>> {
    let p = inv.paren_src.as_ref().ok_or_else(|| Box::new(RuleDiag::MissingName {
        site: (*inv.parse_site).clone(),
    }) as Box<dyn Diagnostic>)?;
    let n = p.src.trim();
    if n.is_empty() {
        return Err(Box::new(RuleDiag::MissingName { site: (*inv.parse_site).clone() }));
    }
    Ok(Arc::from(n))
}

fn collect_captures(invs: &[&OpInvocation]) -> Vec<Arc<str>> {
    let mut seen: Vec<Arc<str>> = Vec::new();
    for inv in invs {
        // Paren slot: either a bare $CAP token OR a brace-pattern body containing
        // $CAP references embedded in key-value syntax. Scan the raw text for all
        // $IDENT occurrences rather than classifying the whole token.
        if let Some(p) = &inv.paren_src {
            scan_dollar_idents(&p.src, &mut seen);
        }
        // Brace slot (SubChain brace bodies are lowered separately, but other brace
        // bodies like inline patterns may also carry captures).
        if let Some(b) = &inv.brace_src {
            scan_dollar_idents(&b.src, &mut seen);
        }
    }
    seen
}

/// Scan `text` for `$IDENT` tokens (skipping `$$prov` and `${cross_ref}` forms).
/// Appends each unique capture name to `seen`.
fn scan_dollar_idents(text: &str, seen: &mut Vec<Arc<str>>) {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'$' {
            i += 1;
            continue;
        }
        // Skip $$ (provenance) and ${ (cross-ref)
        if i + 1 < bytes.len() && (bytes[i + 1] == b'$' || bytes[i + 1] == b'{') {
            i += 2;
            continue;
        }
        // Expect an ident start after $
        let start = i + 1;
        if start >= bytes.len() || !(bytes[start].is_ascii_alphabetic() || bytes[start] == b'_') {
            i += 1;
            continue;
        }
        let mut end = start;
        while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
            end += 1;
        }
        let name = &text[start..end];
        if !seen.iter().any(|x: &Arc<str>| x.as_ref() == name) {
            seen.push(Arc::from(name));
        }
        i = end;
    }
}

// Silence unused import of ParseSeg until brace-body ParseSeg rewriting lands.
#[allow(dead_code)]
fn _keep_parse_seg(_: ParseSeg) {}

// ---------------------------------------------------------------------------
// Diagnostics — rule-owned
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum RuleDiag {
    MissingName { site: ParseSite },
    MissingBody { site: ParseSite },
    BodyParse   { site: ParseSite, msg: Arc<str> },
    UnknownBodyOp { site: ParseSite, name: Arc<str> },
}

impl Diagnostic for RuleDiag {
    fn code(&self) -> &str {
        match self {
            RuleDiag::MissingName   { .. } => "rule/missing-name",
            RuleDiag::MissingBody   { .. } => "rule/missing-body",
            RuleDiag::BodyParse     { .. } => "rule/body-parse",
            RuleDiag::UnknownBodyOp { .. } => "rule/unknown-body-op",
        }
    }
    fn severity(&self) -> crate::_0_types::Severity { crate::_0_types::Severity::Error }
    fn primary(&self) -> &ParseSite {
        match self {
            RuleDiag::MissingName   { site }        => site,
            RuleDiag::MissingBody   { site }        => site,
            RuleDiag::BodyParse     { site, .. }    => site,
            RuleDiag::UnknownBodyOp { site, .. }    => site,
        }
    }
    fn render(&self, out: &mut dyn Renderer) {
        match self {
            RuleDiag::MissingName { site } => {
                out.header(self.code(), self.severity(),
                    "rule requires a name, e.g. rule(my_rule){...}");
                out.primary(site);
            }
            RuleDiag::MissingBody { site } => {
                out.header(self.code(), self.severity(),
                    "rule requires a brace body");
                out.primary(site);
            }
            RuleDiag::BodyParse { site, msg } => {
                out.header(self.code(), self.severity(),
                    &format!("failed to parse rule body: {msg}"));
                out.primary(site);
            }
            RuleDiag::UnknownBodyOp { site, name } => {
                out.header(self.code(), self.severity(),
                    &format!("unknown operator `{name}` in rule body"));
                out.primary(site);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use crate::_10_registry::{lower_rules, OperatorRegistry};
    use crate::_5_op::ProgramCtx;
    use crate::_8_parse::host_parse;
    use crate::{Config, RuntimeConfig};
    use crate::ops::{RepoFactory, RevFactory};
    use super::RuleFactory;

    fn make_config() -> Arc<Config> {
        Arc::new(Config {
            repos: vec![], revs: vec![], fs_exclude: vec![],
            sprf_files: vec![], shell_allow: vec![],
            runtime: RuntimeConfig {
                worker_threads: 1, buffer_size: 256,
                flush_interval_ms: 100, collect_witnesses: false,
            xref_cartesian_limit: 10_000,
            },
            content_hash: 0,
        })
    }

    fn make_registry() -> Arc<OperatorRegistry> {
        let mut r = OperatorRegistry::new();
        r.register(Arc::new(RuleFactory));
        r.register(Arc::new(RepoFactory));
        r.register(Arc::new(RevFactory));
        Arc::new(r)
    }

    fn lower(src: &str) -> crate::LowerOutcome {
        let file = Arc::from(PathBuf::from("<test>").as_path());
        let invs = host_parse(src, file).unwrap();
        let pctx = ProgramCtx::new(make_config(), make_registry());
        lower_rules(invs, pctx)
    }

    // rule body with arm syntax (`> op`) parses clean.
    #[test]
    fn rule_body_arm_syntax_parses_clean() {
        let outcome = lower("rule(foo) { > repo(myorg/*); };");
        assert!(
            outcome.diags.is_empty(),
            "expected no diags, got {:?}",
            outcome.diags.iter().map(|d| d.code()).collect::<Vec<_>>()
        );
    }

    // rule body without leading `>` produces rule/body-parse with message mentioning `>`.
    #[test]
    fn rule_body_missing_arm_arrow_gives_body_parse_diag() {
        let outcome = lower("rule(foo) { repo(*); };");
        let codes: Vec<&str> = outcome.diags.iter().map(|d| d.code()).collect();
        assert!(
            codes.iter().any(|c| *c == "rule/body-parse"),
            "expected rule/body-parse, got {codes:?}"
        );
    }

    // empty rule body is valid.
    #[test]
    fn rule_body_empty_is_valid() {
        let outcome = lower("rule(foo) {};");
        assert!(
            outcome.diags.is_empty(),
            "expected no diags for empty body, got {:?}",
            outcome.diags.iter().map(|d| d.code()).collect::<Vec<_>>()
        );
    }
}
