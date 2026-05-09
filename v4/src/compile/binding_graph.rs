//! Compile-time binding graph analysis over `Vec<PipeAst>`.
//!
//! Walks each pipe's steps in order, tracking which capture names are
//! bound at each point. Emits diagnostics for:
//!
//!   `lang/use-before-bind`  — `${X}` or bareword `X` read at a step
//!                             where X has not yet been bound.
//!   `lang/term-self-cycle`  — a single step both reads and writes the
//!                             same name (read happens before any
//!                             intervening binder of that name).
//!
//! Scope rule: a `{ block }` sub-pipe inherits the outer bound set;
//! binders inside the block are LOCAL to the block (don't leak outward).
//! Mirrors v3's `binding_graph::analyze_pipe` (see chat_log /
//! sprefa-archive bd memory `binding-graph-v3`).
//!
//! Bareword detection (CAPS / CAPS?) lives here too so the rules don't
//! diverge from `walk.rs::classify_slot`. The analyzer is a *separate*
//! pass — it does not lower; it only reads spans + names off the AST.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use effect_runtime::v2::Diag;

use crate::compile::ast::{OpCall, PipeAst};
use crate::compile::lower::op_def::default_plain_dsl_parse;
use crate::compile::lower::registry::Registry;

/// Top-level entry. Returns one diag vec per program (multiple pipes,
/// each scoped independently — top-level sibling pipes do not share a
/// binding scope).
pub fn analyze_program(program: &[PipeAst], reg: &Registry) -> Vec<Diag> {
    let mut diags = Vec::new();
    let rule_decls = collect_rule_decls(program);
    for pipe in program {
        let mut bound: HashSet<Arc<str>> = HashSet::new();
        analyze_pipe(pipe, reg, &rule_decls, &mut bound, &mut diags);
    }
    diags
}

#[derive(Clone, Debug)]
struct RuleDecl {
    cols: Vec<Arc<str>>,
    has_body: bool,
}

fn collect_rule_decls(program: &[PipeAst]) -> HashMap<Arc<str>, RuleDecl> {
    let mut out = HashMap::new();
    for pipe in program {
        for op in &pipe.steps {
            if op.name.as_ref() != "rule" {
                continue;
            }
            let Some(first) = op.args.first() else { continue; };
            let raw_name = first.raw.trim();
            let Some(name) = raw_name.strip_prefix(':') else { continue; };
            if !is_ident(name) {
                continue;
            }
            let mut cols = Vec::new();
            for arg in op.args.iter().skip(1) {
                let raw = arg.raw.trim();
                let raw = raw.strip_suffix('?')
                    .or_else(|| raw.strip_suffix('!'))
                    .unwrap_or(raw);
                if is_caps_ident(raw) {
                    cols.push(Arc::<str>::from(raw));
                }
            }
            out.insert(Arc::<str>::from(name), RuleDecl {
                cols,
                has_body: op.block.is_some(),
            });
        }
    }
    out
}

fn analyze_pipe(
    pipe:       &PipeAst,
    reg:        &Registry,
    rule_decls: &HashMap<Arc<str>, RuleDecl>,
    bound:      &mut HashSet<Arc<str>>,
    diags:      &mut Vec<Diag>,
) {
    for op in &pipe.steps {
        let (reads, mut binds) = collect_term_refs(op, reg);

        for r in &reads {
            if !bound.contains(r) {
                let msg = if bound.is_empty() {
                    format!(
                        "`{}` used before bound (no captures bound at this step)",
                        r
                    )
                } else {
                    let mut bn: Vec<&str> = bound.iter().map(|s| s.as_ref()).collect();
                    bn.sort();
                    format!(
                        "`{}` used before bound (in scope: {})",
                        r, bn.join(", ")
                    )
                };
                diags.push(
                    Diag::error("lang/use-before-bind", msg)
                        .with_span(op.span.lo, op.span.hi),
                );
            }
        }

        // Self-cycle: the step reads X *and* binds X without an
        // intervening binder of X earlier in the pipe. Symptom of
        // writing back into the same capture you sourced from in one
        // step — almost never what you want.
        for b in &binds {
            if reads.contains(b) && !bound.contains(b) {
                diags.push(
                    Diag::error(
                        "lang/term-self-cycle",
                        format!(
                            "`{}` is both read and bound in the same step \
                             with no prior binder",
                            b
                        ),
                    )
                    .with_span(op.span.lo, op.span.hi),
                );
            }
        }

        if op.apply {
            if let Some(decl) = rule_decls.get(&op.name) {
                if decl.has_body {
                    binds.extend(decl.cols.iter().cloned());
                }
            }
        }

        for b in binds {
            bound.insert(b);
        }

        // {block} — inherits outer bound set. Binds inside don't leak.
        if let Some(block) = &op.block {
            let mut local = bound.clone();
            analyze_pipe(block, reg, rule_decls, &mut local, diags);
        }
    }
}

/// Per-OpCall: gather (reads, binds) over flow + args + dsl.
///
/// Args carry CAPS / CAPS? barewords (the `walk::classify_slot`
/// desugar). dsl bodies carry `${IDENT}` interp holes (per
/// `default_plain_dsl_parse`). Block sub-pipes are recursed by the
/// caller; this fn handles only the op's own slots.
fn collect_term_refs(op: &OpCall, reg: &Registry) -> (Vec<Arc<str>>, Vec<Arc<str>>) {
    let mut reads: Vec<Arc<str>> = Vec::new();
    let mut binds: Vec<Arc<str>> = Vec::new();

    if op.flow.is_none()
        && op.args.is_empty()
        && op.dsl.is_none()
        && op.block.is_none()
        && !op.force
        && !op.apply
        && is_caps_ident(op.name.as_ref())
    {
        if op.predicate {
            binds.push(op.name.clone());
        } else {
            reads.push(op.name.clone());
        }
    }

    if let Some(flow) = &op.flow {
        slot_terms(flow.raw.as_ref(), &mut reads, &mut binds);
    }
    for arg in &op.args {
        slot_terms(arg.raw.as_ref(), &mut reads, &mut binds);
    }
    if op.name.as_ref() == "term" || op.name.as_ref() == "term_bind" {
        if let Some(arg) = op.args.first() {
            let raw = arg.raw.trim();
            let name = raw.strip_prefix(':').unwrap_or(raw);
            if is_caps_ident(name) {
                if op.name.as_ref() == "term_bind" {
                    binds.push(Arc::<str>::from(name));
                } else {
                    reads.push(Arc::<str>::from(name));
                }
            }
        }
    }
    if let Some(dsl) = &op.dsl {
        // Host pipe-hole scanner: ${X} = read, ${X?} = bind. SubPipe-
        // shape interps (`${ <pipe> }`, task #10) are opaque to the
        // binding analyzer for now — the inner pipe is self-contained
        // and walk_op runs binding_graph recursively on it via its own
        // walk_pipe entry.
        use crate::compile::lower::op_def::{InterpKind, InterpMode};
        for interp in default_plain_dsl_parse(dsl.raw.as_ref()) {
            match interp.kind {
                InterpKind::Term { mode, .. } => match mode {
                    InterpMode::Read => reads.push(interp.name),
                    InterpMode::Bind => binds.push(interp.name),
                },
                InterpKind::SubPipe { .. } => {}
            }
        }
        // Op-declared DSL binders — `re`'s `(?P<NAME>)`, `glob`'s
        // `<NAME>`, `ast`'s `$NAME` / `$$$REST`. The op tells us what
        // its own grammar binds at runtime, so the analyzer doesn't
        // false-positive on captures introduced by sub-grammar syntax.
        if let Some(def) = reg.get(&op.name) {
            for b in def.binders_in_dsl(dsl.raw.as_ref()) {
                binds.push(b.name);
            }
        }
    }
    // Op-declared imperative cursor binds (fs sets FS, ast sets LO/HI, etc.)
    if let Some(def) = reg.get(&op.name) {
        for name in def.cursor_binds() {
            binds.push(Arc::<str>::from(*name));
        }
    }
    // If a name appears in both reads and binds for the SAME step (e.g.
    // a json body where `${UID}` is a binder and the host-style interp
    // scanner also sees it as a read), the bind wins — the op's grammar
    // is authoritative for what its body does with the name. Without
    // this, every capture in a non-template dsl false-positives as
    // term-self-cycle.
    reads.retain(|r| !binds.iter().any(|b| b == r));
    (reads, binds)
}

/// Inspect a slot's raw text for a single CAPS / CAPS? bareword. Mirrors
/// `walk::classify_slot`'s ALL_CAPS arm so the two stay in lockstep.
/// Anything more complex than a single bareword falls through silently
/// — those slots don't contribute to the binding graph at this layer.
fn slot_terms(raw: &str, reads: &mut Vec<Arc<str>>, binds: &mut Vec<Arc<str>>) {
    let raw = raw.trim();
    if raw.is_empty() { return; }
    let raw = split_keyword_value(raw).unwrap_or(raw).trim();
    if let Some(stripped) = raw.strip_suffix('!') {
        if is_caps_ident(stripped) {
            binds.push(Arc::<str>::from(stripped));
            return;
        }
    }
    if let Some(stripped) = raw.strip_suffix('?') {
        if is_caps_ident(stripped) {
            binds.push(Arc::<str>::from(stripped));
            return;
        }
    }
    if is_caps_ident(raw) {
        reads.push(Arc::<str>::from(raw));
    }
}

fn split_keyword_value(raw: &str) -> Option<&str> {
    let (key, value) = raw.split_once(':')?;
    if is_ident(key.trim()) { Some(value) } else { None }
}

fn is_ident(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() { return false; }
    if !(bytes[0].is_ascii_alphabetic() || bytes[0] == b'_') { return false; }
    bytes[1..].iter().all(|b| b.is_ascii_alphanumeric() || *b == b'_')
}

fn is_caps_ident(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() { return false; }
    if !bytes[0].is_ascii_uppercase() { return false; }
    bytes[1..].iter().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || *b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::parse::host_parse;

    fn diag_codes(src: &str) -> Vec<String> {
        let (program, _parse_diags) = host_parse(src);
        let reg = crate::compile::lower::default_registry();
        analyze_program(&program, &reg)
            .iter()
            .map(|d| d.code.to_string())
            .collect()
    }

    #[test]
    fn read_before_bind_emits_diag() {
        // X is read inside the str template before any binder.
        let codes = diag_codes("rule(:r) { `hello ${X}` };");
        assert!(
            codes.iter().any(|c| c == "lang/use-before-bind"),
            "expected use-before-bind, got {:?}", codes
        );
    }

    #[test]
    fn bind_then_read_is_clean() {
        // X? at the rule arg position introduces X into outer scope;
        // the inner block's str then reads it cleanly.
        let codes = diag_codes("rule(:r, X?) { `hello ${X}` };");
        assert!(
            !codes.iter().any(|c| c == "lang/use-before-bind"),
            "unexpected use-before-bind, got {:?}", codes
        );
    }

    #[test]
    fn block_local_binds_dont_leak() {
        // X bound inside block should not satisfy a later top-level use.
        let codes = diag_codes("rule(:r) { X? } > rule(:s, X);");
        // The second `rule(:s, X)` reads X at top level; X was bound
        // only inside the first block, so it's out of scope.
        // (Note: top-level `> rule(:s, X)` becomes a CAPS read in the
        // arg slot.)
        assert!(
            codes.iter().any(|c| c == "lang/use-before-bind"),
            "expected use-before-bind, got {:?}", codes
        );
    }
}
