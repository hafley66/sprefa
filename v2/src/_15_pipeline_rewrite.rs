//! Pure Pipeline rewrites for LSP partial evaluation.
//!
//! `truncate_and_substitute` replaces the op at `at` with `wildcard` and
//! drops everything sequenced after it in the containing Seq / Fork arm.
//! Fork siblings (other arms) are preserved as-is — arms are parallel, not
//! sequential, so "downstream" only means "later in this Seq".
//!
//! No side effects, no DocSession, no ctx. Framework path tagging is still
//! owned by `Pipeline::run` at execution time.
//!
//! Path encoding (mirrors analysis::resolve_op_in_rule + resolve_body_op):
//!   - At a `Seq(children)` node, the next index selects a child AND
//!     truncates: the returned Seq contains `children[..=idx]` with the
//!     selected child rewritten recursively.
//!   - At a `Fork(arms)` node, the next index selects an arm; every other
//!     arm is preserved (Fork is parallel fan-out, not a sequence).
//!   - At a `Switch { arms }` node, same as Fork: select one arm, keep
//!     the others. Switch isn't used in LSP completion flows today but
//!     handled for totality.
//!   - At an `Op` node, the path must be empty; the op is replaced with
//!     `wildcard`.
//!
//! The canonical rule-level call sites produce paths `[i, 0]` (the RuleOp
//! of rule `i` at top level, where top-level is a `Seq` of rule pipelines
//! and each rule is wrapped as `Seq([RuleOp])`) and `[i, 0, fork_idx, j]`
//! (op `j` inside fork arm `fork_idx` of rule `i`, reached via the
//! RuleOp's brace body's `Pipeline::Fork`).

use std::sync::Arc;

use crate::_5_op::{ForkBranch, LoweredOp, Op, Pipeline};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Rewrite `p` so the node at `at` is replaced with `Pipeline::Op(wildcard)`
/// and every downstream element in any containing `Seq` along the path is
/// dropped. See module doc for the path encoding.
///
/// Pure function — no ctx, no side effects. Framework path tagging happens
/// at run time inside `Pipeline::run`; this rewrite only produces a new
/// tree shape for the runner to evaluate.
///
/// Invalid paths (out-of-bounds index, attempt to descend past a leaf Op,
/// empty path, etc.) panic with a clear message. Invalid paths are a
/// programmer error: the caller builds `at` from `span_ix`, which is
/// produced by the same lowering pass that built `p`, so a mismatch means
/// a bug in the caller.
pub fn truncate_and_substitute(
    p: &Pipeline,
    at: &[usize],
    wildcard: Arc<dyn Op>,
) -> Pipeline {
    walk(p, at, &wildcard)
}

fn walk(p: &Pipeline, at: &[usize], wildcard: &Arc<dyn Op>) -> Pipeline {
    if at.is_empty() {
        // Replace whatever node we're on with the wildcard Op. Any pending
        // subtree below `p` is dropped (desired: "truncate downstream").
        return Pipeline::Op(LoweredOp::bare(wildcard.clone()));
    }
    let idx = at[0];
    let rest = &at[1..];

    match p {
        Pipeline::Op(_) => panic!(
            "truncate_and_substitute: path has {} more segment(s) but current node is Op \
             (cannot descend past a leaf Op)",
            at.len()
        ),

        Pipeline::Seq(children) => {
            if idx >= children.len() {
                panic!(
                    "truncate_and_substitute: Seq index {idx} out of bounds ({})",
                    children.len()
                );
            }
            // Keep children[..idx] verbatim; rewrite children[idx]; drop tail.
            let mut out: Vec<Pipeline> =
                children[..idx].iter().map(clone_pipeline).collect();
            out.push(walk(&children[idx], rest, wildcard));
            Pipeline::Seq(out)
        }

        Pipeline::Fork(arms) => {
            if idx >= arms.len() {
                panic!(
                    "truncate_and_substitute: Fork arm {idx} out of bounds ({})",
                    arms.len()
                );
            }
            // Preserve every arm; only the selected arm is rewritten.
            let out: Vec<ForkBranch> = arms
                .iter()
                .enumerate()
                .map(|(i, arm)| {
                    if i == idx {
                        ForkBranch {
                            parse_site: arm.parse_site.clone(),
                            pipeline: walk(&arm.pipeline, rest, wildcard),
                        }
                    } else {
                        clone_fork_branch(arm)
                    }
                })
                .collect();
            Pipeline::Fork(out)
        }

        Pipeline::Switch { on, arms } => {
            if idx >= arms.len() {
                panic!(
                    "truncate_and_substitute: Switch arm {idx} out of bounds ({})",
                    arms.len()
                );
            }
            let out: Vec<(Arc<str>, Pipeline)> = arms
                .iter()
                .enumerate()
                .map(|(i, (name, arm))| {
                    if i == idx {
                        (name.clone(), walk(arm, rest, wildcard))
                    } else {
                        (name.clone(), clone_pipeline(arm))
                    }
                })
                .collect();
            Pipeline::Switch { on: on.clone(), arms: out }
        }
    }
}

// ---------------------------------------------------------------------------
// Clone helpers — Pipeline itself isn't Clone because it holds `ForkBranch`
// and `Switch` variants that can't derive Clone trivially. LoweredOp is
// Clone; everything else recurses.
// ---------------------------------------------------------------------------

fn clone_pipeline(p: &Pipeline) -> Pipeline {
    match p {
        Pipeline::Op(lop) => Pipeline::Op(lop.clone()),
        Pipeline::Seq(children) => Pipeline::Seq(children.iter().map(clone_pipeline).collect()),
        Pipeline::Fork(arms) => Pipeline::Fork(arms.iter().map(clone_fork_branch).collect()),
        Pipeline::Switch { on, arms } => Pipeline::Switch {
            on: on.clone(),
            arms: arms
                .iter()
                .map(|(name, pl)| (name.clone(), clone_pipeline(pl)))
                .collect(),
        },
    }
}

fn clone_fork_branch(a: &ForkBranch) -> ForkBranch {
    ForkBranch {
        parse_site: a.parse_site.clone(),
        pipeline: clone_pipeline(&a.pipeline),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::_0_types::{Cursor, ParseSeg, ParseSite};
    use crate::_5_op::{ForkBranch, LoweredOp, Op, OpCtx};
    use futures_util::stream::BoxStream;
    use std::path::PathBuf;
    use std::sync::Arc;

    // --- minimal stub Op for test fixtures -----------------------------------

    struct StubOp {
        name:       &'static str,
        step:       u16,
        parse_site: Arc<ParseSite>,
    }

    impl Op for StubOp {
        fn pipe(&self, input: BoxStream<'static, Cursor>, _ctx: OpCtx)
            -> BoxStream<'static, Cursor> { input }
        fn name(&self) -> &'static str { self.name }
        fn step(&self) -> u16 { self.step }
        fn parse_site(&self) -> &Arc<ParseSite> { &self.parse_site }
    }

    fn ps() -> Arc<ParseSite> {
        Arc::new(ParseSite {
            file:       Arc::from(PathBuf::from("stub").as_path()),
            path:       Arc::from(Vec::<ParseSeg>::new().into_boxed_slice()),
            byte_range: 0..0,
        })
    }

    fn stub(name: &'static str, step: u16) -> Arc<dyn Op> {
        Arc::new(StubOp { name, step, parse_site: ps() })
    }

    fn op_pipeline(name: &'static str, step: u16) -> Pipeline {
        Pipeline::Op(LoweredOp::bare(stub(name, step)))
    }

    fn names_in(p: &Pipeline) -> Vec<String> {
        let mut out = Vec::new();
        collect_names(p, &mut out);
        out
    }

    fn collect_names(p: &Pipeline, out: &mut Vec<String>) {
        match p {
            Pipeline::Op(lop) => out.push(lop.op.name().to_string()),
            Pipeline::Seq(children) => {
                out.push("[".into());
                for c in children { collect_names(c, out); }
                out.push("]".into());
            }
            Pipeline::Fork(arms) => {
                out.push("{".into());
                for a in arms { collect_names(&a.pipeline, out); }
                out.push("}".into());
            }
            Pipeline::Switch { .. } => out.push("<switch>".into()),
        }
    }

    // --- test cases ----------------------------------------------------------

    #[test]
    fn replace_middle_of_top_level_seq_drops_tail() {
        // [a, b, c, d], replace index 1 (b) with W → [a, W].
        let p = Pipeline::Seq(vec![
            op_pipeline("a", 0),
            op_pipeline("b", 1),
            op_pipeline("c", 2),
            op_pipeline("d", 3),
        ]);
        let out = truncate_and_substitute(&p, &[1], stub("W", 0));
        assert_eq!(names_in(&out), vec!["[", "a", "W", "]"]);
    }

    #[test]
    fn replace_last_of_top_level_seq_no_tail() {
        let p = Pipeline::Seq(vec![
            op_pipeline("a", 0),
            op_pipeline("b", 1),
        ]);
        let out = truncate_and_substitute(&p, &[1], stub("W", 0));
        assert_eq!(names_in(&out), vec!["[", "a", "W", "]"]);
    }

    #[test]
    fn minimal_single_rule_op_empty_path_replaces_root() {
        // `[i, 0]` semantics: caller has already isolated the rule pipeline,
        // and the path here is empty — the whole `p` is replaced.
        let p = op_pipeline("rule_a", 0);
        let out = truncate_and_substitute(&p, &[], stub("W", 0));
        assert_eq!(names_in(&out), vec!["W"]);
    }

    #[test]
    fn rule_wrap_seq_of_one_path_zero_replaces_inner_op() {
        // Seq([ Op(rule_a) ]), path [0] descends into the child and replaces
        // it. Outer Seq length is preserved (1 child kept).
        let p = Pipeline::Seq(vec![op_pipeline("rule_a", 0)]);
        let out = truncate_and_substitute(&p, &[0], stub("W", 0));
        assert_eq!(names_in(&out), vec!["[", "W", "]"]);
    }

    #[test]
    fn top_level_rules_seq_i_zero_replaces_rule_i() {
        // Top: Seq([ Seq([RuleOp_0]), Seq([RuleOp_1]) ])
        // Path [1, 0] → replaces RuleOp_1. Rule 0 stays; rule 1's wrapper Seq
        // keeps its single slot but that slot is now W. Everything after
        // children[1] in the top Seq is dropped (nothing to drop here).
        let p = Pipeline::Seq(vec![
            Pipeline::Seq(vec![op_pipeline("rule_0", 0)]),
            Pipeline::Seq(vec![op_pipeline("rule_1", 0)]),
        ]);
        let out = truncate_and_substitute(&p, &[1, 0], stub("W", 0));
        assert_eq!(names_in(&out), vec!["[", "[", "rule_0", "]", "[", "W", "]", "]"]);
    }

    #[test]
    fn fork_arm_replace_inside_only_affects_that_arm() {
        // body = Fork(arm0=Seq[x,y,z], arm1=Seq[p,q,r], arm2=Seq[m,n])
        // Path [1, 1] → in arm 1, replace op 1 with W, truncate tail.
        // Other arms preserved.
        let arm = |names: &[&'static str]| ForkBranch {
            parse_site: ps(),
            pipeline: Pipeline::Seq(
                names.iter().enumerate().map(|(i, n)| op_pipeline(n, i as u16)).collect(),
            ),
        };
        let body = Pipeline::Fork(vec![
            arm(&["x", "y", "z"]),
            arm(&["p", "q", "r"]),
            arm(&["m", "n"]),
        ]);
        let out = truncate_and_substitute(&body, &[1, 1], stub("W", 0));
        assert_eq!(
            names_in(&out),
            vec!["{", "[", "x", "y", "z", "]", "[", "p", "W", "]", "[", "m", "n", "]", "}"],
        );
    }

    #[test]
    #[should_panic(expected = "out of bounds")]
    fn out_of_bounds_index_panics() {
        let p = Pipeline::Seq(vec![op_pipeline("a", 0), op_pipeline("b", 1)]);
        let _ = truncate_and_substitute(&p, &[5], stub("W", 0));
    }

    #[test]
    #[should_panic(expected = "cannot descend past a leaf Op")]
    fn descend_past_op_panics() {
        let p = op_pipeline("a", 0);
        let _ = truncate_and_substitute(&p, &[0], stub("W", 0));
    }
}
