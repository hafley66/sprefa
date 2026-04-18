//! T6: framework fork lowering via DefaultFork + host_parse_arm_brace.

use std::path::PathBuf;
use std::sync::Arc;

use v2::{
    Config, OperatorRegistry, ProgramCtx,
    host_parse, lower_rules,
};
use v2::ops::{FsFactory, RepoFactory, RevFactory, RuleFactory};

fn make_config() -> Arc<Config> {
    Arc::new(Config::test_default())
}

fn make_registry() -> Arc<OperatorRegistry> {
    let mut r = OperatorRegistry::new();
    r.register(Arc::new(RuleFactory));
    r.register(Arc::new(RepoFactory));
    r.register(Arc::new(RevFactory));
    r.register(Arc::new(FsFactory));
    Arc::new(r)
}

fn lower(src: &str) -> v2::LowerOutcome {
    let file = Arc::from(PathBuf::from("<test>").as_path());
    let pipes = host_parse(src, file).unwrap();
    let pctx = ProgramCtx::new(make_config(), make_registry());
    lower_rules(pipes, pctx)
}

// repo($R) { > rev(*); } — wildcard rev is banned (6a guardrail) to keep
// the worktree materialization set bounded. Arm lowers but surfaces the diag.
#[test]
fn fork_arm_rev_star_banned() {
    let src = r#"rule(foo) { > repo($R) { > rev(*); }; };"#;
    let outcome = lower(src);
    let codes: Vec<&str> = outcome.diags.iter().map(|d| d.code()).collect();
    assert!(
        codes.contains(&"rev/unbounded-wildcard"),
        "expected rev/unbounded-wildcard, got {codes:?}"
    );
}

// repo($R) { > rev(main); } — clean, no diags.
#[test]
fn fork_arm_rev_main_clean() {
    let src = r#"rule(foo) { > repo($R) { > rev(main); }; };"#;
    let outcome = lower(src);
    assert!(
        outcome.diags.is_empty(),
        "expected no diags, got {:?}",
        outcome.diags.iter().map(|d| d.code()).collect::<Vec<_>>()
    );
}

// repo($R) { rev(main); } — missing arm `>` → parse/arm-brace diag.
#[test]
fn fork_arm_missing_arrow_diag() {
    let src = r#"rule(foo) { > repo($R) { rev(main); }; };"#;
    let outcome = lower(src);
    let codes: Vec<&str> = outcome.diags.iter().map(|d| d.code()).collect();
    assert!(
        codes.contains(&"parse/arm-brace"),
        "expected parse/arm-brace in diags, got {codes:?}"
    );
}

// rule(foo) { fs(**/x.yaml); } — single-arm rule body, clean.
#[test]
fn fork_arm_fs_single_arm_clean() {
    let src = r#"rule(foo) { > fs(**/x.yaml); };"#;
    let outcome = lower(src);
    assert!(
        outcome.diags.is_empty(),
        "expected no diags, got {:?}",
        outcome.diags.iter().map(|d| d.code()).collect::<Vec<_>>()
    );
}
