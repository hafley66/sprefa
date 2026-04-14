//! OperatorRegistry and two-pass lower shell. No op logic.

use std::collections::HashMap;
use std::sync::Arc;

use crate::_0_types::{ParseSite, Severity};
use crate::_1_diagnostic::{Diagnostic, Renderer};
use crate::_5_op::{BraceMode, ForkBranch, Operator, OpInvocation, Pipeline, ProgramCtx};
use crate::_8_parse::{host_parse_arm_brace, Pipe};

pub struct OperatorRegistry {
    by_name: HashMap<Arc<str>, Arc<dyn Operator>>,
}

impl OperatorRegistry {
    pub fn new() -> Self { Self { by_name: HashMap::new() } }

    pub fn register(&mut self, op: Arc<dyn Operator>) {
        self.by_name.insert(Arc::from(op.name()), op.clone());
        for a in op.aliases() {
            self.by_name.insert(Arc::from(*a), op.clone());
        }
    }

    pub fn resolve(&self, name: &str) -> Option<Arc<dyn Operator>> {
        self.by_name.get(name).cloned()
    }
}

impl Default for OperatorRegistry {
    fn default() -> Self { Self::new() }
}

pub struct LowerOutcome {
    pub pipelines: Vec<Pipeline>,
    pub pctx:      ProgramCtx,
    pub diags:     Vec<Box<dyn Diagnostic>>,
}

/// Lower a flat list of pipes (top-level program) into Pipelines.
/// Each pipe → Pipeline::Seq of its ops. Multiple pipes at the top level
/// become independent Pipelines (caller decides how to compose).
pub fn lower_rules(pipes: Vec<Pipe>, mut pctx: ProgramCtx) -> LowerOutcome {
    let mut diags = Vec::new();
    let registry = pctx.registry.clone();

    let all_invs: Vec<&OpInvocation> = pipes.iter().flat_map(|p| p.ops.iter()).collect();
    for inv in &all_invs {
        let Some(op) = registry.resolve(&inv.name) else { continue; };
        if let Err(mut ds) = op.pre_register(inv, &mut pctx) { diags.append(&mut ds); }
    }

    let mut pipelines = Vec::with_capacity(pipes.len());
    for pipe in &pipes {
        let chain = lower_chain(&pipe.ops, &mut pctx, &mut diags);
        if chain.len() == 1 {
            pipelines.push(chain.into_iter().next().unwrap());
        } else {
            pipelines.push(Pipeline::Seq(chain));
        }
    }

    LowerOutcome { pipelines, pctx, diags }
}

/// Lower a single chain (`a > b > c`) to a Vec<Pipeline>. Caller wraps in Seq.
pub fn lower_chain(
    chain: &[OpInvocation],
    pctx:  &mut ProgramCtx,
    diags: &mut Vec<Box<dyn Diagnostic>>,
) -> Vec<Pipeline> {
    let registry = pctx.registry.clone();
    let mut out = Vec::with_capacity(chain.len());
    for inv in chain {
        let Some(op) = registry.resolve(&inv.name) else { continue; };
        match op.parse(inv, pctx) {
            Err(mut ds) => diags.append(&mut ds),
            Ok(p) => {
                if op.brace_mode() == BraceMode::DefaultFork {
                    if let Some(brace) = &inv.brace_src {
                        match crate::_8_parse::host_parse_arm_brace_abs(
                            &brace.src,
                            inv.parse_site.file.clone(),
                            &inv.parse_site.path,
                            brace.byte_range.start,
                        ) {
                            Err(errs) => {
                                for e in errs {
                                    diags.push(Box::new(ArmBraceDiag {
                                        message: e.message,
                                        site:    (*inv.parse_site).clone(),
                                    }));
                                }
                                out.push(p);
                            }
                            Ok(arm_pipes) => {
                                if arm_pipes.is_empty() {
                                    out.push(p);
                                } else {
                                    let arms: Vec<ForkBranch> = arm_pipes
                                        .iter()
                                        .map(|pipe| {
                                            let mut arm_diags = Vec::new();
                                            let chain = lower_chain(&pipe.ops, pctx, &mut arm_diags);
                                            diags.append(&mut arm_diags);
                                            let pipeline = if chain.len() == 1 {
                                                chain.into_iter().next().unwrap()
                                            } else {
                                                Pipeline::Seq(chain)
                                            };
                                            let parse_site = pipe.ops.first()
                                                .map(|op| op.parse_site.clone())
                                                .unwrap_or_else(|| inv.parse_site.clone());
                                            ForkBranch { parse_site, pipeline }
                                        })
                                        .collect();
                                    let fork = Pipeline::Fork(arms);
                                    out.push(Pipeline::Seq(vec![p, fork]));
                                }
                            }
                        }
                        continue;
                    }
                }
                out.push(p);
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// ArmBraceDiag — surface host_parse_arm_brace errors as Diagnostic
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct ArmBraceDiag {
    message: Arc<str>,
    site:    ParseSite,
}

impl Diagnostic for ArmBraceDiag {
    fn code(&self) -> &str { "parse/arm-brace" }
    fn severity(&self) -> Severity { Severity::Error }
    fn primary(&self) -> &ParseSite { &self.site }
    fn render(&self, out: &mut dyn Renderer) {
        out.header("parse/arm-brace", self.severity(), &self.message);
    }
}
