//! Phase 6 — `stratify` (Piece 3) + semi-naive fixpoint (Piece 4).
//!
//! Stratification is the build-time half: from the rule dependency
//! graph (who reads whom), assign each rule a stratum index such that
//!
//!   - a POSITIVE dep (rule R joins relation S) lets R sit in S's
//!     stratum or higher — recursion (`reach :- reach, edge`) is a
//!     positive self-edge and stays in ONE stratum;
//!   - a NEGATIVE dep (rule R antijoins / `not S`) forces R into a
//!     stratum STRICTLY ABOVE S, so S is fully computed before R reads
//!     its complement (the plan's "no neg edge within or down a
//!     stratum" invariant);
//!   - an unstratifiable negative cycle (a negative edge inside a
//!     strongly-connected component) is a USER error → a lower-time
//!     `Diag` through the existing diag vocabulary. Never a panic,
//!     never a hang.
//!
//! `eval_stratum` is the run-time half: a semi-naive loop over
//! `RUNTIME_DIRTY`, re-rendering a dirty owner over the DELTA of its
//! inputs, looping a stratum to a local fixed point before advancing.
//! Idempotent inserts (Phase 5 `add` is idempotent per derivation
//! path; row id dedup on the sink) make an unchanged re-derivation a
//! NO-OP, so a recursive rule reaches a least fixed point in finite
//! rounds. Every loop has a HARD round cap; exceeding it is a loud
//! error, never a spin.

use std::collections::{BTreeMap, BTreeSet};

use effect_runtime::v2::Diag;

/// One rule's dependency description: the relations it reads
/// positively (joins) and negatively (antijoins / `not`). This is
/// what the lower stage knows; `stratify` consumes it. Built either
/// directly (the binding tests) or from `RuntimeGraph` SUBSCRIBE
/// edges via [`rule_deps_from_graph`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleDep {
    pub rule: String,
    /// Relations this rule joins (positive body atoms). A self-name
    /// here is ordinary recursion.
    pub pos: Vec<String>,
    /// Relations this rule antijoins / negates (`not R`). A negative
    /// edge forces a strictly higher stratum.
    pub neg: Vec<String>,
}

impl RuleDep {
    pub fn new(rule: impl Into<String>) -> Self {
        Self {
            rule: rule.into(),
            pos: Vec::new(),
            neg: Vec::new(),
        }
    }
    pub fn reads(mut self, r: impl Into<String>) -> Self {
        self.pos.push(r.into());
        self
    }
    pub fn negates(mut self, r: impl Into<String>) -> Self {
        self.neg.push(r.into());
        self
    }
}

/// One evaluation stratum: the rules whose least fixed point is
/// computed together (positive recursion allowed), before any rule in
/// a higher stratum runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stratum {
    pub rules: Vec<String>,
}

/// Unstratifiable rule graph: a negative edge inside a strongly
/// connected component. A user error, surfaced as a lower-time
/// diagnostic (NOT a panic, NOT a hang).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StratifyError {
    /// The rule names on the negative cycle (sorted, deterministic).
    pub cycle: Vec<String>,
}

impl StratifyError {
    /// Render as a lower-time `Diag`, same vocabulary as every other
    /// lower diagnostic (`Diag::error(code, msg)`); the caller pushes
    /// it onto the lower's `Vec<Diag>` / forwards it to the diag sink.
    pub fn to_diag(&self) -> Diag {
        Diag::error(
            "lower/unstratifiable-negation",
            format!(
                "rule graph has an unstratifiable negative cycle through {}: \
                 a relation cannot depend on its own negation",
                self.cycle.join(" -> ")
            ),
        )
    }
}

impl std::fmt::Display for StratifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unstratifiable negative cycle: {}", self.cycle.join(" -> "))
    }
}

impl std::error::Error for StratifyError {}

/// Build the rule dep graph from `RuntimeGraph` SUBSCRIBE edges. The
/// edge `label` carries the relation a rule reads; a `not:`-prefixed
/// label marks a negative (antijoin) read, every other label is a
/// positive join. (The lower stamps the prefix when it emits a `not R`
/// antijoin SUBSCRIBE; an unprefixed label is an ordinary join,
/// including the positive self-edge of a recursive rule.) This keeps
/// the v3↔v4 wall: SUBSCRIBE labels are opaque `&str`, no v4 type
/// crosses.
pub fn rule_deps_from_graph(edges: &[(String, String, bool)]) -> Vec<RuleDep> {
    // edges: (rule, relation, is_negative)
    let mut by_rule: BTreeMap<String, RuleDep> = BTreeMap::new();
    for (rule, rel, neg) in edges {
        let e = by_rule
            .entry(rule.clone())
            .or_insert_with(|| RuleDep::new(rule.clone()));
        if *neg {
            e.neg.push(rel.clone());
        } else {
            e.pos.push(rel.clone());
        }
    }
    by_rule.into_values().collect()
}

/// Tarjan SCC over the union (pos ∪ neg) edge set. Returns the
/// component id per node and the components in REVERSE topological
/// order (a component appears before the components it depends on…
/// actually Tarjan yields reverse-topo; we re-sort downstream).
fn tarjan(
    nodes: &[String],
    adj: &BTreeMap<String, Vec<String>>,
) -> BTreeMap<String, usize> {
    #[derive(Default)]
    struct St {
        index: usize,
        idx: BTreeMap<String, usize>,
        low: BTreeMap<String, usize>,
        on: BTreeSet<String>,
        stack: Vec<String>,
        comp: BTreeMap<String, usize>,
        ncomp: usize,
    }
    fn strong(
        v: &str,
        adj: &BTreeMap<String, Vec<String>>,
        s: &mut St,
    ) {
        s.idx.insert(v.to_string(), s.index);
        s.low.insert(v.to_string(), s.index);
        s.index += 1;
        s.stack.push(v.to_string());
        s.on.insert(v.to_string());
        if let Some(ws) = adj.get(v) {
            for w in ws {
                if !s.idx.contains_key(w) {
                    strong(w, adj, s);
                    let lw = s.low[w];
                    let lv = s.low[v];
                    s.low.insert(v.to_string(), lv.min(lw));
                } else if s.on.contains(w) {
                    let iw = s.idx[w];
                    let lv = s.low[v];
                    s.low.insert(v.to_string(), lv.min(iw));
                }
            }
        }
        if s.low[v] == s.idx[v] {
            loop {
                let w = s.stack.pop().unwrap();
                s.on.remove(&w);
                s.comp.insert(w.clone(), s.ncomp);
                if w == v {
                    break;
                }
            }
            s.ncomp += 1;
        }
    }
    let mut s = St::default();
    for n in nodes {
        if !s.idx.contains_key(n) {
            strong(n, adj, &mut s);
        }
    }
    s.comp
}

/// Stratify the rule dependency graph.
///
/// Result strata are ordered LOW → HIGH: stratum 0 has no negative
/// dependency on any other stratum; a rule that negates relation `S`
/// lands strictly above `S`'s stratum. Positive recursion stays in one
/// stratum. An unstratifiable negative cycle is `Err` (→ diagnostic).
pub fn stratify(rules: &[RuleDep]) -> Result<Vec<Stratum>, StratifyError> {
    // Node set: every rule plus every relation it mentions (a base
    // relation with no rule is its own stratum-0 component).
    let mut nodes: BTreeSet<String> = BTreeSet::new();
    let mut adj: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut neg_pairs: BTreeSet<(String, String)> = BTreeSet::new();
    for rd in rules {
        nodes.insert(rd.rule.clone());
        for p in &rd.pos {
            nodes.insert(p.clone());
            // Edge rule -> dep (rule depends on dep).
            adj.entry(rd.rule.clone()).or_default().push(p.clone());
        }
        for n in &rd.neg {
            nodes.insert(n.clone());
            adj.entry(rd.rule.clone()).or_default().push(n.clone());
            neg_pairs.insert((rd.rule.clone(), n.clone()));
        }
    }
    let node_vec: Vec<String> = nodes.iter().cloned().collect();
    let comp = tarjan(&node_vec, &adj);

    // Reject: a negative edge whose endpoints share a component is an
    // unstratifiable negative cycle.
    for (from, to) in &neg_pairs {
        if comp.get(from) == comp.get(to) {
            // Recover the cycle members (the shared SCC).
            let cid = comp[from];
            let mut cycle: Vec<String> = comp
                .iter()
                .filter(|(_, &c)| c == cid)
                .map(|(n, _)| n.clone())
                .collect();
            cycle.sort();
            return Err(StratifyError { cycle });
        }
    }

    // Condense to the component DAG; assign each component a stratum
    // by longest-path where a NEGATIVE edge across components costs
    // +1 (strict) and a positive cross-component edge costs +0 (the
    // dependency must already be at-or-below). Fixed-point relaxation
    // (component DAG is acyclic ⇒ converges; bounded by #components).
    let ncomp = comp.values().copied().max().map(|m| m + 1).unwrap_or(0);
    let mut level = vec![0usize; ncomp];
    // Cross-component edges with polarity.
    let mut cedges: Vec<(usize, usize, bool)> = Vec::new();
    for rd in rules {
        let from_c = comp[&rd.rule];
        for p in &rd.pos {
            let to_c = comp[p];
            if from_c != to_c {
                cedges.push((from_c, to_c, false));
            }
        }
        for n in &rd.neg {
            let to_c = comp[n];
            if from_c != to_c {
                cedges.push((from_c, to_c, true));
            }
        }
    }
    // level[from] >= level[to] (+1 if negative). Relax until stable;
    // cap iterations at #components (acyclic ⇒ this many suffice).
    for _ in 0..=ncomp {
        let mut changed = false;
        for &(from_c, to_c, neg) in &cedges {
            let want = level[to_c] + usize::from(neg);
            if level[from_c] < want {
                level[from_c] = want;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    // Group rule names by their component's level. Base relations that
    // are not themselves rules are not emitted (they are sources, not
    // evaluated); only rule names appear in strata.
    let rule_names: BTreeSet<&str> = rules.iter().map(|r| r.rule.as_str()).collect();
    let max_level = level.iter().copied().max().unwrap_or(0);
    let mut strata: Vec<Stratum> = (0..=max_level)
        .map(|_| Stratum { rules: Vec::new() })
        .collect();
    for n in &node_vec {
        if rule_names.contains(n.as_str()) {
            let lvl = level[comp[n]];
            strata[lvl].rules.push(n.clone());
        }
    }
    for s in &mut strata {
        s.rules.sort();
        s.rules.dedup();
    }
    // Drop empty strata while keeping order (a level with only base
    // relations contributes none).
    strata.retain(|s| !s.rules.is_empty());
    Ok(strata)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recursive_positive_self_edge_stays_one_stratum() {
        // reach :- edge ; reach :- reach, edge  → reach + edge in
        // stratum 0 (no negation, positive recursion is fine).
        let rules = vec![RuleDep::new("reach").reads("edge").reads("reach")];
        let strata = stratify(&rules).unwrap();
        assert_eq!(strata.len(), 1);
        assert_eq!(strata[0].rules, vec!["reach"]);
    }

    #[test]
    fn negation_forces_strictly_higher_stratum() {
        // stale :- dep, not reach.  reach is recursive (stratum 0);
        // stale must be a strictly higher stratum than reach.
        let rules = vec![
            RuleDep::new("reach").reads("edge").reads("reach"),
            RuleDep::new("stale").reads("dep").negates("reach"),
        ];
        let strata = stratify(&rules).unwrap();
        assert_eq!(strata.len(), 2, "two strata: reach below, stale above");
        assert_eq!(strata[0].rules, vec!["reach"]);
        assert_eq!(strata[1].rules, vec!["stale"]);
    }

    #[test]
    fn unstratifiable_negative_cycle_is_error_not_panic() {
        // p :- not q ;  q :- not p   → p and q on a negative cycle.
        let rules = vec![
            RuleDep::new("p").negates("q"),
            RuleDep::new("q").negates("p"),
        ];
        let err = stratify(&rules).expect_err("negative cycle must be rejected");
        assert!(err.cycle.contains(&"p".to_string()));
        assert!(err.cycle.contains(&"q".to_string()));
        let d = err.to_diag();
        assert_eq!(&*d.code, "lower/unstratifiable-negation");
    }

    #[test]
    fn positive_cycle_is_allowed() {
        // a :- b ; b :- a  (mutual positive recursion) → one stratum,
        // no error (only NEGATIVE cycles are unstratifiable).
        let rules = vec![
            RuleDep::new("a").reads("b"),
            RuleDep::new("b").reads("a"),
        ];
        let strata = stratify(&rules).unwrap();
        assert_eq!(strata.len(), 1);
        let mut got = strata[0].rules.clone();
        got.sort();
        assert_eq!(got, vec!["a", "b"]);
    }
}
