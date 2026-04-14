//! Task 11 — Phase 0 gate test. Exercises the permissive `$$sigil` grammar
//! through `host_parse` → `lower_rules`, validating that unknown sigils emit
//! diags and that registering a new factory silences them without touching
//! any central file.
//!
//! Scan pointers appear inside `json()` walker bodies (BraceMode::WalkerPattern),
//! which is where the framework's validate_scan_pointers pass catches them.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use v2::_5_op::{BraceMode, CompletionItem, GrammarRef, Operator, ScanPointer};
use v2::ops::{FsFactory, JsonFactory, RepoFactory, RevFactory, RuleFactory};
use v2::{
    host_parse, lower_rules, Config, Diagnostic, OpInvocation, OperatorRegistry, Pipeline,
    ProgramCtx, RuntimeConfig,
};

// Throwaway factory adding `$$foo` / `$$foo_norm`. Proves Phase 0 success
// criterion: new sigils drop in via registry alone; no core-file edits.
struct FooFactory;

impl Operator for FooFactory {
    fn name(&self) -> &'static str {
        "foo_stub"
    }
    fn paren_grammar(&self) -> GrammarRef {
        GrammarRef(Arc::from("foo-arg"))
    }
    fn brace_mode(&self) -> BraceMode {
        BraceMode::DefaultFork
    }

    fn scan_pointers(&self) -> &'static [ScanPointer] {
        static P: &[ScanPointer] = &[
            ScanPointer {
                sigil: "foo",
                read: |_| Some(Arc::from("foo-value")),
            },
            ScanPointer {
                sigil: "foo_norm",
                read: |_| Some(Arc::from("foo-value")),
            },
        ];
        P
    }

    fn parse(
        &self,
        _inv: &OpInvocation,
        _pctx: &mut ProgramCtx,
    ) -> Result<Pipeline, Vec<Box<dyn Diagnostic>>> {
        unreachable!("FooFactory not invoked in Phase 0 tests")
    }

    fn completion_item(&self) -> CompletionItem {
        CompletionItem {
            label: "foo_stub".to_string(),
            detail: "".to_string(),
            doc: "".to_string(),
        }
    }
}

fn make_pctx(extra: Option<Arc<dyn Operator>>) -> ProgramCtx {
    let mut reg = OperatorRegistry::new();
    reg.register(Arc::new(RuleFactory));
    reg.register(Arc::new(RepoFactory));
    reg.register(Arc::new(RevFactory));
    reg.register(Arc::new(FsFactory));
    reg.register(Arc::new(JsonFactory));
    if let Some(op) = extra {
        reg.register(op);
    }
    let config = Config {
        repos: vec![],
        revs: vec![],
        fs_exclude: vec![],
        sprf_files: vec![],
        shell_allow: vec![],
        runtime: RuntimeConfig {
            worker_threads: 1,
            buffer_size: 64,
            flush_interval_ms: 100,
            collect_witnesses: false,
            xref_cartesian_limit: 10_000,
        },
        content_hash: 0,
    };
    ProgramCtx::new(Arc::new(config), Arc::new(reg))
}

fn fake_file() -> Arc<Path> {
    Arc::from(PathBuf::from("test.sprf").as_path())
}

fn lower(src: &str, pctx: ProgramCtx) -> Vec<Box<dyn Diagnostic>> {
    let pipes = host_parse(src, fake_file()).expect("host_parse");
    lower_rules(pipes, pctx).diags
}

fn unknown_codes(diags: &[Box<dyn Diagnostic>]) -> Vec<&str> {
    diags
        .iter()
        .map(|d| d.code())
        .filter(|c| *c == "xref/unknown-scan-pointer")
        .collect()
}

#[test]
fn scan_pointer_sigils_parse_and_lower_cleanly() {
    let src = r#"
        rule(svc) {
            > repo(org/*) > rev(main) > fs(**/services.yaml)
              > json({ image: { repository: $$repo($R), tag: $$rev_norm($V), path: $$fs($F) } })
        };
    "#;
    let diags = lower(src, make_pctx(None));
    assert!(
        unknown_codes(&diags).is_empty(),
        "unexpected unknown-sigil diags: {:?}",
        diags.iter().map(|d| d.code()).collect::<Vec<_>>()
    );
}

#[test]
fn scan_pointer_unknown_sigil_emits_diag() {
    let src = r#"
        rule(svc) {
            > repo(org/*) > fs(**/a.yaml)
              > json({ x: $$totally_fake($X) })
        };
    "#;
    let diags = lower(src, make_pctx(None));
    assert_eq!(
        unknown_codes(&diags).len(),
        1,
        "expected exactly one unknown-sigil diag, got: {:?}",
        diags.iter().map(|d| d.code()).collect::<Vec<_>>()
    );
}

#[test]
fn throwaway_factory_adds_sigils_without_central_edits() {
    let src = r#"
        rule(demo) {
            > repo(org/*) > fs(**/a.yaml)
              > json({ a: $$foo($X), b: $$foo_norm($Y) })
        };
    "#;
    let without = lower(src, make_pctx(None));
    let with = lower(src, make_pctx(Some(Arc::new(FooFactory))));
    assert_eq!(
        unknown_codes(&without).len(),
        2,
        "both $$foo and $$foo_norm should be unknown without FooFactory"
    );
    assert!(
        unknown_codes(&with).is_empty(),
        "registering FooFactory silences both; got: {:?}",
        with.iter().map(|d| d.code()).collect::<Vec<_>>()
    );
}
