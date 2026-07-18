//! Mixed source+derived / extract+derived rel desugar: rewrite a `Program` so
//! a rel headed by both a source-shaped rule (scan/match/ast/sg/json/cmd/
//! comment) and a derived rule — or by a term-extract rule (json/jsonp body
//! form) and a derived rule — splits into two hidden twin rels plus a
//! synthesized union, instead of the engine bailing and telling the user to
//! do the split by hand. See plans/2026-07-10-mixed-rel-desugar.md.
//!
//! The synthesized program is exactly what a user writing the manual pattern
//! (`examples/anim-self.dl`'s pin/fpin -> span_of) produces: a hidden `__src`
//! twin gets every source/extract-classified rule's head, a hidden `__drv`
//! twin gets every derived-classified rule's head (facts included — a ground
//! fact on a mixed rel is derived-side), and the now-plain-derived visible rel
//! gets two synthesized union rules reading the twins. Body reads are NEVER
//! rewritten: every other rule, `?` query, closure, and the panel keeps
//! reading the visible name, and a self-recursive read of the visible rel
//! inside one of its own derived rules becomes an ordinary recursive
//! rel-component like any other (`rel_components`/`stratify` do not need to
//! know this rewrite happened).
//!
//! Runs once per tick, immediately before rule classification in
//! `tick_report`/`tick_paths` (both share this chokepoint, so both ticking
//! modes see the identical rewritten program). Pure and uncached — cheap
//! enough on a per-tick basis that caching would be premature (see the plan's
//! "no caching" note); the common case (nothing mixed) skips the rebuild
//! entirely via the `Option` return, so a program with no mixed rel pays only
//! the one classification pass, no clone.

use anyhow::{bail, Result};
use std::collections::{HashMap, HashSet};

use crate::ast::{Atom, BodyItem, Item, Program, RelDecl, Rule, Term};

/// Reserved suffix for a mixed rel's hidden source/extract-side twin table.
pub const SRC_SUFFIX: &str = "__src";
/// Reserved suffix for a mixed rel's hidden derived-side twin table.
pub const DRV_SUFFIX: &str = "__drv";

/// Which hazard this rel's mix is: the original source+derived bail, or its
/// term-extract+derived twin. Both rewrite identically (see module doc); the
/// kind is kept only for error attribution and D4 telemetry display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MixedKind {
    SourceDerived,
    ExtractDerived,
}

/// One rel this tick's desugar rewrote: the visible name a program/query
/// still uses, and its two hidden twins. Per-tick, used only for diagnostics/
/// telemetry attribution — never stored, never consulted by the fixpoint.
#[derive(Clone, Debug)]
pub struct MixedRel {
    pub visible: String,
    pub src_twin: String,
    pub drv_twin: String,
    pub kind: MixedKind,
}

/// Which axis-relevant class a rule's head puts it in. Mirrors (does not
/// replace) `tick_report`'s own classification; only the axis this desugar
/// acts on. `Other` covers every rule kind that already lives outside the
/// source/derived overlap check today (@next/@async/@stream/repo-sink/
/// checkout-sink/closure-seed/scc/node2vec) — a rel mixing one of THOSE with
/// source/derived is a different, pre-existing hazard (its own bail already
/// fires downstream) that this stage does not touch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HeadClass {
    Source,
    Extract,
    Derived,
    Other,
}

fn classify(rule: &Rule) -> HeadClass {
    // Order matches `tick_report`'s `extract_rules` filter exactly:
    // `r.has_term_extract() && !r.is_source()` — a rule with both wins as
    // Source (impossible in practice: the term forms require `rev: None`,
    // the source forms of the same ops require `rev: Some`).
    if rule.is_next()
        || rule.is_async()
        || rule.is_stream()
        || rule.is_repo_sink()
        || rule.is_checkout_sink()
        || rule.closure_edge().is_some()
        || rule.scc_edge().is_some()
        || rule.node2vec_edge().is_some()
    {
        HeadClass::Other
    } else if rule.is_source() {
        HeadClass::Source
    } else if rule.has_term_extract() {
        HeadClass::Extract
    } else {
        HeadClass::Derived
    }
}

/// Rewrite `prog` so every mixed rel splits into hidden twins + a synthesized
/// union. Returns `None` when nothing needs rewriting (the overwhelmingly
/// common case) so the call site can keep borrowing the original `Program`
/// instead of paying a full-program clone every tick; `Some((rewritten,
/// mixed))` otherwise.
///
/// Bails (old bail text, narrowed) for the excluded categories the plan keeps
/// refused: a lattice (`key`/`merge`) rel mixed this way, since the union
/// step's upsert semantics need a real design. `@in`/`@out` port rels and
/// reserved builtin sinks (diag, hover_note, the demand sinks, ...) are never
/// rewritten by construction (see `visible_decls` below) — they fall through
/// to their own existing bails unchanged, exactly as before this arc.
pub fn desugar_mixed_rels(prog: &Program) -> Result<Option<(Program, Vec<MixedRel>)>> {
    reject_source_relation_joins(prog)?;
    reject_reserved_twin_names(prog)?;

    // One decl per user-declared rel name (first occurrence; declare_all
    // itself enforces that a re-declaration is identical or a hard error, so
    // any decl found here carries the rel's real column list). A `rel name:
    // shape.` decl (shape_ref still set — the computed-shape form) is
    // skipped: its columns are not known until Phase 5 resolves them, so a
    // mixed rel declared this way is out of scope for this stage and falls
    // through to the old bail, same as before.
    let mut decls: HashMap<&str, &RelDecl> = HashMap::new();
    for item in &prog.items {
        if let Item::Rel(d) = item {
            if d.shape_ref.is_some() {
                continue;
            }
            decls.entry(d.name.as_str()).or_insert(d);
        }
    }

    // Per-rel head classes present, in first-seen order (deterministic twin
    // synthesis order tick over tick, so `derived_program_digest` over the
    // rewritten program stays stable and does not force a spurious full
    // rebuild every tick).
    let mut order: Vec<String> = Vec::new();
    let mut classes: HashMap<String, Vec<HeadClass>> = HashMap::new();
    for item in &prog.items {
        if let Item::Rule(r) = item {
            let c = classify(r);
            if c == HeadClass::Other {
                continue;
            }
            let entry = classes.entry(r.head.rel.clone()).or_insert_with(Vec::new);
            if entry.is_empty() {
                order.push(r.head.rel.clone());
            }
            if !entry.contains(&c) {
                entry.push(c);
            }
        }
    }

    let mut mixed: Vec<MixedRel> = Vec::new();
    for rel in &order {
        let cs = &classes[rel];
        let has_source = cs.contains(&HeadClass::Source);
        let has_extract = cs.contains(&HeadClass::Extract);
        let has_derived = cs.contains(&HeadClass::Derived);
        if !has_derived || (!has_source && !has_extract) {
            continue;
        }
        let Some(decl) = decls.get(rel.as_str()) else {
            // No user `rel` decl to clone columns from: a reserved builtin
            // sink (diag/hover_note/the demand sinks/...) that a program
            // cannot `rel`-declare, or a `rel name: shape.` computed decl.
            // Leave unrewritten — the existing bail(s) still fire as before.
            continue;
        };
        // Exclusion: a lattice rel's union step needs a real upsert-winner
        // design (which side wins a key collision is order-dependent) — not
        // designed yet, so keep refusing this combination loudly.
        if decl.key.is_some() || decl.merge.is_some() {
            bail!(
                "relation '{rel}' is written by both a source/extract rule and a derived \
                   rule, and carries a key(...)/merge(...) lattice qualifier; lattice rels \
                   cannot be mixed yet — split the source/extract rule into its own relation \
                   and union it into '{rel}' by hand (see examples/anim-self.dl's pin/fpin \
                   -> span_of)."
            );
        }
        // Exclusion: `@in`/`@out` port rels already have their own head bail
        // (an @in port's rows are injected by the serving loop; a source or
        // derived rule heading it collides). Leave unrewritten.
        if decl.port.is_some() {
            continue;
        }
        // Triple mix (source AND extract AND derived all heading one rel):
        // out of scope for this stage — no in-tree `.dl` does this, and the
        // manual split for it is not a simple two-twin union. Leave
        // unrewritten; the old source+derived bail still fires first.
        if has_source && has_extract {
            continue;
        }
        let kind = if has_source {
            MixedKind::SourceDerived
        } else {
            MixedKind::ExtractDerived
        };
        mixed.push(MixedRel {
            visible: rel.clone(),
            src_twin: format!("{rel}{SRC_SUFFIX}"),
            drv_twin: format!("{rel}{DRV_SUFFIX}"),
            kind,
        });
    }

    if mixed.is_empty() {
        return Ok(None);
    }

    let twin_of: HashMap<&str, &MixedRel> = mixed.iter().map(|m| (m.visible.as_str(), m)).collect();
    let mut out = Program {
        items: Vec::with_capacity(prog.items.len() + mixed.len() * 4),
    };
    for item in &prog.items {
        match item {
            // The visible decl is untouched: it becomes an ordinary derived
            // rel from here on (the union rules' head), so `?`/other rules/
            // the panel keep reading it under its own name with no change.
            Item::Rule(r) if twin_of.contains_key(r.head.rel.as_str()) => {
                let m = twin_of[r.head.rel.as_str()];
                match classify(r) {
                    HeadClass::Source | HeadClass::Extract => {
                        let mut rewritten = r.clone();
                        rewritten.head.rel = m.src_twin.clone();
                        out.items.push(Item::Rule(rewritten));
                    }
                    HeadClass::Derived => {
                        let mut rewritten = r.clone();
                        rewritten.head.rel = m.drv_twin.clone();
                        out.items.push(Item::Rule(rewritten));
                    }
                    // A rule sharing this rel's name but classified Other
                    // (e.g. a stray @next rule on the same name) is left
                    // exactly as written — its own existing bail (headed by
                    // both @next and a source/derived rule) still fires
                    // downstream, unaffected by this rewrite.
                    HeadClass::Other => out.items.push(Item::Rule(r.clone())),
                }
            }
            other => out.items.push(other.clone()),
        }
    }
    // Synthesized twin decls + the two union rules, appended once per mixed
    // rel in `mixed`'s first-seen order (stable output, so the derived-
    // program digest does not spuriously move tick to tick).
    for m in &mixed {
        let visible_decl = decls[m.visible.as_str()];
        let cols = visible_decl.cols.clone();
        let mut src_decl = (*visible_decl).clone();
        src_decl.name = m.src_twin.clone();
        src_decl.key = None;
        src_decl.merge = None;
        src_decl.port = None;
        let mut drv_decl = (*visible_decl).clone();
        drv_decl.name = m.drv_twin.clone();
        drv_decl.key = None;
        drv_decl.merge = None;
        drv_decl.port = None;
        out.items.push(Item::Rel(src_decl));
        out.items.push(Item::Rel(drv_decl));

        let vars: Vec<Term> = cols.iter().map(|c| Term::Var(c.name.clone())).collect();
        let n = cols.len();
        let union_rule = |from_rel: &str| Rule {
            head: Atom {
                rel: m.visible.clone(),
                terms: vars.clone(),
                named: Vec::new(),
            },
            body: vec![BodyItem::Pos(Atom {
                rel: from_rel.to_string(),
                terms: vars.clone(),
                named: Vec::new(),
            })],
            aggs: vec![None; n],
            agg_args2: vec![None; n],
            origin: None,
            temporal: None,
        };
        out.items.push(Item::Rule(union_rule(&m.src_twin)));
        out.items.push(Item::Rule(union_rule(&m.drv_twin)));
    }
    Ok(Some((out, mixed)))
}

/// File-source rules are evaluated by the extraction pass, not SQL. A relation
/// atom is legitimate when it binds an INPUT to a source op (the data-driven
/// scan/rev pattern); `resolve_scan_bindings` compiles precisely that body slice
/// before extracting. An atom with no such binding, however, is ignored by file
/// extraction and must be refused rather than silently treated as a filter.
///
/// This is the runtime backstop (a tick-time bail, so it also catches a
/// program built by RPC/snippet paths that skip the frontend's static pass).
/// `check_rule_types` (src/typecheck.rs, code `source-rule-extra-atom`) is its
/// typecheck-time twin, sharing this fn's `source_input_vars`/`term_vars`
/// classification so the two can never disagree on which atom is legitimate —
/// the typecheck diag fires first for the common `--check`/`--parse-only`/LSP
/// path, surfacing the defect before any scan runs; this bail is what a
/// program reaches if it somehow skips that pass.
fn reject_source_relation_joins(prog: &Program) -> Result<()> {
    for item in &prog.items {
        let Item::Rule(rule) = item else {
            continue;
        };
        if !rule.is_source() {
            continue;
        }
        let inputs = source_input_vars(rule);
        for body in &rule.body {
            let rel = match body {
                BodyItem::Pos(atom) | BodyItem::Neg(atom) => atom,
                _ => continue,
            };
            if rel.terms.iter().any(|term| term_vars(term, &inputs)) {
                continue;
            }
            bail!(
                "source rule for relation '{}' mixes source extraction with relation atom '{}' in its body; source rules cannot join relations — write the scan/match rule into a separate relation, then join '{}' in a derived rule",
                rule.head.rel, rel.rel, rel.rel);
        }
    }
    Ok(())
}

/// Variables used as INPUTS to file extraction ops: `scan`'s OWN `repo`/`rev`
/// coordinate slots, the ONLY place a body atom is ever actually consumed
/// (`Engine::resolve_scan_bindings`'s data-driven-scan SELECT, when either is
/// a `Term::Var`). `scan`'s `path`/`rev_out` OUTPUTS, `glob` (always a
/// literal — a data-driven glob has no resolver), and every other source op's
/// `path`/`rev`/`src` field are deliberately excluded even though they are
/// syntactically required to reference a bound var: that var is always an
/// ALIAS of scan's own path/rev_out output (there is no other way to get a
/// valid file coordinate into `match`/`ast`/`sg`/`json`/`cmd`/`comment`), so a
/// relation atom sharing ONLY one of those is still an ignored post-extract
/// filter — the exact "tag/allowlist keyed on the scanned path" shape a real
/// program is most likely to write (see the `source-rule-extra-atom` typecheck
/// diag and the fail-pre-fix receipt in `tests/it/source_rule_extra_atom.rs`).
pub(crate) fn source_input_vars(rule: &Rule) -> HashSet<String> {
    let mut vars = HashSet::new();
    let mut add = |term: &Term| collect_term_vars(term, &mut vars);
    for body in &rule.body {
        if let BodyItem::Scan { repo, rev, .. } = body {
            add(repo);
            add(rev);
        }
    }
    vars
}

pub(crate) fn term_vars(term: &Term, wanted: &HashSet<String>) -> bool {
    match term {
        Term::Var(name) => wanted.contains(name),
        Term::Interp(parts) => parts
            .iter()
            .any(|p| matches!(p, crate::ast::InterpPart::Var(name) if wanted.contains(name))),
        Term::Arith { lhs, rhs, .. } => term_vars(lhs, wanted) || term_vars(rhs, wanted),
        Term::Call { args, .. } => args.iter().any(|arg| term_vars(arg, wanted)),
        _ => false,
    }
}

fn collect_term_vars(term: &Term, out: &mut HashSet<String>) {
    match term {
        Term::Var(name) => {
            out.insert(name.clone());
        }
        Term::Interp(parts) => {
            for part in parts {
                if let crate::ast::InterpPart::Var(name) = part {
                    out.insert(name.clone());
                }
            }
        }
        Term::Arith { lhs, rhs, .. } => {
            collect_term_vars(lhs, out);
            collect_term_vars(rhs, out);
        }
        Term::Call { args, .. } => {
            for arg in args {
                collect_term_vars(arg, out);
            }
        }
        _ => {}
    }
}

/// A user-declared rel name may never end in the twin suffixes — reserved for
/// this desugar's own hidden tables. Checked over the ORIGINAL program's
/// decls (before any rewrite), so the twins this function itself synthesizes
/// never have to pass the check. Cheap (a name scan over already-visited
/// items), runs unconditionally every tick.
fn reject_reserved_twin_names(prog: &Program) -> Result<()> {
    for item in &prog.items {
        if let Item::Rel(d) = item {
            if d.name.ends_with(SRC_SUFFIX) || d.name.ends_with(DRV_SUFFIX) {
                bail!(
                    "relation '{}' ends in `{SRC_SUFFIX}`/`{DRV_SUFFIX}`, reserved for the \
                       mixed source+derived rel desugar's own hidden twin tables; pick another \
                       name",
                    d.name
                );
            }
        }
    }
    Ok(())
}

/// The user-facing rel name for a rel that may be a desugar twin — strips a
/// trailing `__src`/`__drv`. Telemetry surfaces (`rel_count`/`stmt_ms`/
/// perf.jsonl's `derived_rebuilt`) use this so a mixed rel's row count/rebuild
/// cost attributes to the name the user wrote, not the hidden twin (D4).
pub fn display_rel_name(rel: &str) -> &str {
    rel.strip_suffix(SRC_SUFFIX)
        .or_else(|| rel.strip_suffix(DRV_SUFFIX))
        .unwrap_or(rel)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Bare parse (no named-arg resolution/typecheck), matching the existing
    // `engine::tests::program` helper — fine here since every test program
    // below is fully positional.
    fn parse(src: &str) -> Program {
        crate::parse::parse(crate::lex::lex(src).unwrap()).unwrap()
    }

    #[test]
    fn untouched_program_returns_none() {
        let prog = parse("rel plain(x: text).\nplain(\"a\").\n? plain(x).\n");
        assert!(desugar_mixed_rels(&prog).unwrap().is_none());
    }

    #[test]
    fn source_plus_derived_rewrites_to_twins_and_union() {
        let prog = parse(
            "rel seen(p: file).\n\
             seen(p) <- scan(\"WORK\", \"src/**/*.txt\", p, rev), match(p, rev, /./, line).\n\
             rel mixed(x: text).\n\
             mixed(p) <- scan(\"WORK\", \"src/**/*.txt\", p, rev), match(p, rev, /./, line).\n\
             mixed(x) <- seen(x).\n\
             ? mixed(x).\n",
        );
        let (rewritten, mixed) = desugar_mixed_rels(&prog).unwrap().expect("should desugar");
        assert_eq!(mixed.len(), 1);
        assert_eq!(mixed[0].visible, "mixed");
        assert_eq!(mixed[0].kind, MixedKind::SourceDerived);
        let rule_heads: Vec<&str> = rewritten
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Rule(r) => Some(r.head.rel.as_str()),
                _ => None,
            })
            .collect();
        assert!(rule_heads.contains(&"mixed__src"));
        assert!(rule_heads.contains(&"mixed__drv"));
        // Two union rules head "mixed" now (reading the twins), plus the
        // untouched `seen` rule.
        assert_eq!(rule_heads.iter().filter(|r| **r == "mixed").count(), 2);
    }

    #[test]
    fn extract_plus_derived_rewrites_to_twins_and_union() {
        let prog = parse(
            "rel src(x: text).\n\
             src(\"keep\").\n\
             rel body_rel(b: text).\n\
             body_rel(\"{\\\"n\\\": 7}\").\n\
             rel mixed(v: text).\n\
             mixed(n) <- body_rel(b), jsonp(b, \"n\", n).\n\
             mixed(x) <- src(x).\n\
             ? mixed(v).\n",
        );
        let (_rewritten, mixed) = desugar_mixed_rels(&prog).unwrap().expect("should desugar");
        assert_eq!(mixed.len(), 1);
        assert_eq!(mixed[0].kind, MixedKind::ExtractDerived);
    }

    #[test]
    fn reserved_twin_suffix_in_user_decl_is_rejected() {
        let prog = parse("rel foo__src(x: text).\nfoo__src(\"a\").\n");
        let err = desugar_mixed_rels(&prog).unwrap_err();
        assert!(
            err.to_string().contains("reserved"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn lattice_mixed_rel_still_bails() {
        let prog = parse(
            "rel mixed(k: text, v: int) key(k) merge(MaxBy(v)).\n\
             mixed(p, 1) <- scan(\"WORK\", \"src/**/*.txt\", p, rev), match(p, rev, /./, line).\n\
             mixed(k, 2) <- mixed(k, _).\n",
        );
        let err = desugar_mixed_rels(&prog).unwrap_err();
        assert!(
            err.to_string().contains("lattice rels cannot be mixed yet"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn display_rel_name_strips_twin_suffixes() {
        assert_eq!(display_rel_name("orders__src"), "orders");
        assert_eq!(display_rel_name("orders__drv"), "orders");
        assert_eq!(display_rel_name("orders"), "orders");
    }
}
