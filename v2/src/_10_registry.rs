//! OperatorRegistry and two-pass lower shell. No op logic.

use std::collections::HashMap;
use std::sync::Arc;

use crate::_0_types::{ParseSite, Severity};
use crate::_1_diagnostic::{Diagnostic, Renderer};
use crate::_0_types::Cursor;
use crate::_5_op::{BraceMode, ForkBranch, Operator, OpInvocation, Pipeline, ProgramCtx};
use crate::_8_parse::{scan_slot_scan_pointers, Pipe, ScanPointerOccurrence};

pub type ScanPointerFn = fn(&Cursor) -> Option<Arc<str>>;

pub struct OperatorRegistry {
    by_name:       HashMap<Arc<str>, Arc<dyn Operator>>,
    scan_pointers: HashMap<&'static str, (Arc<str>, ScanPointerFn)>,
}

impl OperatorRegistry {
    pub fn new() -> Self {
        Self {
            by_name:       HashMap::new(),
            scan_pointers: HashMap::new(),
        }
    }

    pub fn register(&mut self, op: Arc<dyn Operator>) {
        let op_name: Arc<str> = Arc::from(op.name());
        self.by_name.insert(op_name.clone(), op.clone());
        for a in op.aliases() {
            self.by_name.insert(Arc::from(*a), op.clone());
        }
        for sp in op.scan_pointers() {
            if let Some((prior_owner, _)) = self.scan_pointers.get(sp.sigil) {
                panic!(
                    "duplicate scan_pointer sigil `{}` — declared by op `{}` and op `{}`",
                    sp.sigil, prior_owner, op_name,
                );
            }
            self.scan_pointers.insert(sp.sigil, (op_name.clone(), sp.read));
        }
    }

    pub fn resolve(&self, name: &str) -> Option<Arc<dyn Operator>> {
        self.by_name.get(name).cloned()
    }

    /// Iterate registered operators. Aliases surface as repeated entries
    /// (same `Arc`, different key); callers that want each op once should
    /// dedup by `Arc::as_ptr`. Used by the LSP completion driver to
    /// enumerate head-position suggestions.
    pub fn iter(&self) -> impl Iterator<Item = &Arc<dyn Operator>> {
        self.by_name.values()
    }

    pub fn lookup_scan_pointer(&self, sigil: &str) -> Option<ScanPointerFn> {
        self.scan_pointers.get(sigil).map(|(_, f)| *f)
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
        let op_opt = registry.resolve(&inv.name);
        // Scan brace_src only when the body is opaque to lower_chain (walker
        // patterns). DefaultFork / CustomSprf recurse into the body, so inner
        // invocations get their own pass and double-counting would result.
        let scan_brace = matches!(
            op_opt.as_ref().map(|op| op.brace_mode()),
            Some(BraceMode::WalkerPattern),
        );
        validate_scan_pointers(inv, &registry, scan_brace, diags);
        let Some(op) = op_opt else { continue; };
        match op.parse(inv, pctx) {
            Err(mut ds) => diags.append(&mut ds),
            Ok(mut p) => {
                // Attach xrefs scanned at host-parse time to the lowered op so
                // expand_xrefs can seed cursors at runtime. Only single-op
                // results carry the inv's crossrefs; nested shapes (Seq/Fork
                // produced by an Operator) leave the xref slice empty.
                if let Pipeline::Op(ref mut lop) = p {
                    if !inv.crossrefs.is_empty() {
                        lop.xrefs = Arc::from(inv.crossrefs.clone().into_boxed_slice());
                    }
                }
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

// ---------------------------------------------------------------------------
// UnknownScanPointerDiag — sigil not owned by any registered op
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct UnknownScanPointerDiag {
    sigil: Arc<str>,
    site:  ParseSite,
}

impl Diagnostic for UnknownScanPointerDiag {
    fn code(&self) -> &str { "xref/unknown-scan-pointer" }
    fn severity(&self) -> Severity { Severity::Error }
    fn primary(&self) -> &ParseSite { &self.site }
    fn render(&self, out: &mut dyn Renderer) {
        out.header(
            self.code(),
            self.severity(),
            &format!("unknown scan-pointer sigil `$${}` — no registered op declares it", self.sigil),
        );
        out.primary(&self.site);
    }
}

fn validate_scan_pointers(
    inv:        &OpInvocation,
    registry:   &OperatorRegistry,
    scan_brace: bool,
    diags:      &mut Vec<Box<dyn Diagnostic>>,
) {
    let mut occs: Vec<ScanPointerOccurrence> = Vec::new();
    if let Some(p) = &inv.paren_src { scan_slot_scan_pointers(&p.src, p.byte_range.start, &mut occs); }
    if scan_brace {
        if let Some(br) = &inv.brace_src { scan_slot_scan_pointers(&br.src, br.byte_range.start, &mut occs); }
    }
    for b in &inv.brackets          { scan_slot_scan_pointers(&b.src, b.byte_range.start, &mut occs); }
    for occ in occs {
        if registry.lookup_scan_pointer(&occ.sigil).is_some() { continue; }
        let site = ParseSite {
            file:       inv.parse_site.file.clone(),
            path:       inv.parse_site.path.clone(),
            byte_range: occ.byte_range.clone(),
        };
        diags.push(Box::new(UnknownScanPointerDiag { sigil: occ.sigil, site }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::_5_op::{GrammarRef, ScanPointer};

    struct StubOp {
        name:          &'static str,
        scan_pointers: &'static [ScanPointer],
    }

    impl Operator for StubOp {
        fn name(&self) -> &'static str { self.name }
        fn paren_grammar(&self) -> GrammarRef { GrammarRef(Arc::from("stub")) }
        fn parse(&self, _inv: &OpInvocation, _pctx: &mut ProgramCtx)
            -> Result<Pipeline, Vec<Box<dyn Diagnostic>>>
        { unreachable!("stub: parse not exercised in registry tests") }
        fn scan_pointers(&self) -> &'static [ScanPointer] { self.scan_pointers }
    }

    #[test]
    fn scan_pointer_dispatch_empty_by_default() {
        let mut reg = OperatorRegistry::new();
        reg.register(Arc::new(crate::ops::RuleFactory));
        assert!(reg.lookup_scan_pointer("anything").is_none());
        assert!(reg.lookup_scan_pointer("repo").is_none());
    }

    #[test]
    #[should_panic(expected = "duplicate scan_pointer sigil `dup`")]
    fn scan_pointer_dispatch_panics_on_duplicate() {
        static DUP_A: &[ScanPointer] = &[ScanPointer { sigil: "dup", read: |_| None }];
        static DUP_B: &[ScanPointer] = &[ScanPointer { sigil: "dup", read: |_| None }];
        let mut reg = OperatorRegistry::new();
        reg.register(Arc::new(StubOp { name: "op_a", scan_pointers: DUP_A }));
        reg.register(Arc::new(StubOp { name: "op_b", scan_pointers: DUP_B }));
    }

    fn site_at(range: std::ops::Range<usize>) -> Arc<crate::_0_types::ParseSite> {
        use std::path::Path;
        Arc::new(crate::_0_types::ParseSite {
            file:       Arc::from(Path::new("test.sprf")),
            path:       Arc::from(Vec::<crate::_0_types::ParseSeg>::new().into_boxed_slice()),
            byte_range: range,
        })
    }

    fn inv_with_paren(paren_src: &str, abs_start: usize) -> OpInvocation {
        use crate::_5_op::ParenSlot;
        OpInvocation {
            name:       Arc::from("stub"),
            brackets:   vec![],
            paren_src:  Some(ParenSlot {
                src:        Arc::from(paren_src),
                byte_range: abs_start..(abs_start + paren_src.len()),
            }),
            brace_src:  None,
            parse_site: site_at(abs_start..(abs_start + paren_src.len() + 2)),
            crossrefs:  vec![],
        }
    }

    #[test]
    fn validate_scan_pointers_emits_diag_on_unknown_sigil() {
        static REPO_SP: &[ScanPointer] = &[ScanPointer { sigil: "repo", read: |_| None }];
        let mut reg = OperatorRegistry::new();
        reg.register(Arc::new(StubOp { name: "known", scan_pointers: REPO_SP }));
        let inv = inv_with_paren("$$bogus($X)", 100);
        let mut diags: Vec<Box<dyn Diagnostic>> = Vec::new();
        validate_scan_pointers(&inv, &reg, false, &mut diags);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code(), "xref/unknown-scan-pointer");
        let site = diags[0].primary();
        assert_eq!(site.byte_range.start, 100);
        assert_eq!(site.byte_range.end,   100 + "$$bogus".len());
    }

    #[test]
    fn validate_scan_pointers_silent_on_known_sigil() {
        static REPO_SP: &[ScanPointer] = &[ScanPointer { sigil: "repo", read: |_| None }];
        let mut reg = OperatorRegistry::new();
        reg.register(Arc::new(StubOp { name: "known", scan_pointers: REPO_SP }));
        let inv = inv_with_paren("$$repo($R)", 0);
        let mut diags: Vec<Box<dyn Diagnostic>> = Vec::new();
        validate_scan_pointers(&inv, &reg, false, &mut diags);
        assert!(diags.is_empty(), "unexpected diags: {}", diags.len());
    }
}
