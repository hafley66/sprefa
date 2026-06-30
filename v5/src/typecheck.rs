//! Lower-time path-literal normalization and rule type checking (spec T2).
//!
//! Two passes run at program load, before any lowering:
//!   1. `normalize_program` resolves every `Term::PathLit` (`fs:`/`glob:`) through
//!      `desc` into canonical text and rewrites it to `Term::Str` in place. A
//!      resolution failure (escapes root, unknown anchor) becomes a `TypeDiag`.
//!   2. `check_rule_types` unifies each body var's declared type across atoms via
//!      the rel column metadata + the brand table, emitting the spec's diagnostic
//!      codes (brand-mismatch, coerce-text-path, plus PathLit-in-int as an error).
//!
//! Both produce `TypeDiag { path, span, severity, code, msg }` so the `--check`
//! path renders and exit-fails on error severity exactly like the `diag` relation.

use std::collections::HashMap;

use crate::ast::*;
use crate::desc::{self, Resolved};
use crate::scc;

/// The brand table: brand name -> declared parent (a base `Type` keyword or a
/// prior brand). Built from every `type X <: Y` item. `from_program` rejects a
/// duplicate brand, an unknown/non-existent parent, and a cycle.
#[derive(Clone, Debug, Default)]
pub struct Brands {
    parent: HashMap<String, String>,
}

impl Brands {
    pub fn from_program(prog: &Program) -> Result<Brands, String> {
        let mut parent: HashMap<String, String> = HashMap::new();
        for item in &prog.items {
            if let Item::Brand(b) = item {
                if parent.contains_key(&b.name) {
                    return Err(format!("duplicate brand `{}`", b.name));
                }
                if Type::parse(&b.name).is_some() {
                    return Err(format!("brand `{}` shadows a base type", b.name));
                }
                parent.insert(b.name.clone(), b.parent.clone());
            }
        }
        let brands = Brands { parent };
        // Every parent must terminate at a base type without cycling.
        for name in brands.parent.keys() {
            brands.base_type(name)
                .ok_or_else(|| format!("brand `{name}` has no base type (unknown parent or cycle)"))?;
        }
        Ok(brands)
    }

    pub fn is_brand(&self, name: &str) -> bool { self.parent.contains_key(name) }

    /// Walk the parent chain to a base `Type`. None on an unknown parent or a cycle.
    pub fn base_type(&self, name: &str) -> Option<Type> {
        let mut cur = name.to_string();
        for _ in 0..self.parent.len() + 1 {
            if let Some(t) = Type::parse(&cur) { return Some(t); }
            cur = self.parent.get(&cur)?.clone();
        }
        None
    }

    /// Is `anc` an ancestor of (or equal to) `desc` in the brand chain?
    pub fn is_ancestor(&self, anc: &str, desc: &str) -> bool {
        if anc == desc { return true; }
        let mut cur = desc.to_string();
        for _ in 0..self.parent.len() + 1 {
            let Some(p) = self.parent.get(&cur) else { return false; };
            if p == anc { return true; }
            cur = p.clone();
        }
        false
    }
}

/// A column's resolved type: the brand name when branded (with its base type),
/// else just the base type.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ColTy {
    base: Type,
    brand: Option<String>,
}

fn col_ty(col: &Col, brands: &Brands) -> ColTy {
    match &col.brand {
        Some(b) => ColTy { base: brands.base_type(b).unwrap_or(Type::Text), brand: Some(b.clone()) },
        None => ColTy { base: col.ty, brand: None },
    }
}

fn is_path_base(t: Type) -> bool { matches!(t, Type::Path | Type::File | Type::Dir) }

/// Resolve the brand parent (or base) chain so the chains are validated; returns
/// the effective base storage type of every column. The engine calls this to
/// confirm a referenced brand actually exists before declaring tables.
pub fn validate_brands(prog: &Program) -> Result<Brands, String> {
    let brands = Brands::from_program(prog)?;
    for item in &prog.items {
        if let Item::Rel(d) = item {
            for c in &d.cols {
                if let Some(b) = &c.brand {
                    if !brands.is_brand(b) {
                        return Err(format!(
                            "relation `{}` column `{}` references unknown type/brand `{b}`",
                            d.name, c.name));
                    }
                }
            }
        }
    }
    Ok(brands)
}

/// Collected anchor table. v1 only ever resolves the default `~` anchor (the scan
/// root); named anchors are accepted and validated (no duplicate) but their
/// bodies are not referenced in `fs:`/`glob:` literals yet (deferred with `rs:`).
#[derive(Clone, Debug, Default)]
pub struct Anchors {
    /// name -> raw fs body. `~` is implicit (scan root) and need not be declared.
    named: HashMap<String, String>,
}

impl Anchors {
    pub fn from_program(prog: &Program) -> Result<Anchors, String> {
        let mut named: HashMap<String, String> = HashMap::new();
        for item in &prog.items {
            if let Item::Anchor(a) = item {
                if a.name == "~" {
                    return Err("the default `~` anchor (scan root) cannot be redeclared".into());
                }
                if named.insert(a.name.clone(), a.body.clone()).is_some() {
                    return Err(format!("duplicate anchor `{}`", a.name));
                }
            }
        }
        Ok(Anchors { named })
    }

    /// The raw `fs:` body declared for a named anchor, if any. v1 does not yet
    /// reference these in literal bodies (deferred with `rs:`); this accessor
    /// keeps the table live for T3+ and the validation tests.
    pub fn body(&self, name: &str) -> Option<&str> { self.named.get(name).map(|s| s.as_str()) }
}

/// Resolve one path literal body to canonical text. `~/x` and `~` use the default
/// scan-root anchor; any other `name/` anchor prefix is rejected in v1 (named
/// anchor refs are deferred). Returns the canonical text, or the descriptor error.
fn resolve_literal(scheme: &str, body: &str) -> Result<Resolved, desc::DescError> {
    let spec = desc::scheme_spec(scheme)
        .expect("lexer only emits a Scheme token for a registered scheme");
    let (anchored, rest) = desc::strip_anchor(body);
    // A non-`~` leading anchor (`name/...`) is not a path component in v1; reject
    // it as an unknown anchor so a typo never silently becomes a literal dir.
    if !anchored {
        if let Some((head, _)) = body.split_once('/') {
            if head.starts_with('~') && head != "~" {
                return Err(desc::DescError::UnknownAnchor(head.to_string()));
            }
        }
    }
    desc::resolve(rest, spec.concrete, anchored)
}

/// Rewrite every `Term::PathLit` in the program to its canonical `Term::Str`.
/// `dl_path` attributes any failure diagnostic. Returns the diagnostics (only
/// error-severity ones arise here: a literal either resolves or it does not).
pub fn normalize_program(prog: &mut Program, dl_path: &str) -> Vec<TypeDiag> {
    let mut diags = Vec::new();
    for item in &mut prog.items {
        match item {
            Item::Rule(r) => {
                normalize_atom(&mut r.head, dl_path, &mut diags);
                for b in &mut r.body { normalize_body_item(b, dl_path, &mut diags); }
            }
            Item::Query(q) => normalize_atom(&mut q.head, dl_path, &mut diags),
            Item::Gen(g) => {
                match &mut g.target {
                    GenTarget::Splice { path, l0, l1 } => {
                        for t in [path, l0, l1] { normalize_term(t, dl_path, &mut diags); }
                    }
                    GenTarget::Cursor { path, lo, hi, .. } => {
                        for t in [path, lo, hi] { normalize_term(t, dl_path, &mut diags); }
                    }
                    GenTarget::File { .. } => {}
                }
                for b in &mut g.body { normalize_body_item(b, dl_path, &mut diags); }
            }
            Item::Rel(_) | Item::Anchor(_) | Item::Brand(_) | Item::Shell(_) => {}
        }
    }
    diags
}

fn normalize_body_item(b: &mut BodyItem, dl_path: &str, diags: &mut Vec<TypeDiag>) {
    match b {
        BodyItem::Pos(a) | BodyItem::Neg(a) => normalize_atom(a, dl_path, diags),
        BodyItem::Scan { repo, rev, glob, path, rev_out } => {
            for t in [repo, rev, glob, path, rev_out] { normalize_term(t, dl_path, diags); }
        }
        BodyItem::Match { path, rev, line, id, col, end_col, .. } => {
            for t in [path, rev, line] { normalize_term(t, dl_path, diags); }
            for t in [id, col, end_col].into_iter().flatten() { normalize_term(t, dl_path, diags); }
        }
        BodyItem::Ast { path, rev, line, end, id, .. } => {
            for t in [path, rev, line] { normalize_term(t, dl_path, diags); }
            if let Some(e) = end { normalize_term(e, dl_path, diags); }
            if let Some(t) = id { normalize_term(t, dl_path, diags); }
        }
        BodyItem::Sg { path, rev, line, col, end_line, end_col, id, .. } => {
            for t in [path, rev, line, col, end_line, end_col] { normalize_term(t, dl_path, diags); }
            if let Some(t) = id { normalize_term(t, dl_path, diags); }
        }
        BodyItem::AstYaml { path, rev, line, col, end_line, end_col, .. } => {
            for t in [path, rev, line, col, end_line, end_col] { normalize_term(t, dl_path, diags); }
        }
        BodyItem::JsonP { src, rev, out, id, .. } => {
            for t in [src, out] { normalize_term(t, dl_path, diags); }
            if let Some(t) = rev { normalize_term(t, dl_path, diags); }
            if let Some(t) = id { normalize_term(t, dl_path, diags); }
        }
        BodyItem::Json { src, rev, .. } => {
            normalize_term(src, dl_path, diags);
            if let Some(t) = rev { normalize_term(t, dl_path, diags); }
        }
        BodyItem::Cmd { path, rev, line, out, .. } => {
            for t in [path, rev, line, out] { normalize_term(t, dl_path, diags); }
        }
        BodyItem::Comment { path, rev, l0, l1, label, .. } => {
            for t in [path, rev, l0, l1, label] { normalize_term(t, dl_path, diags); }
        }
        BodyItem::Cmp(c) => {
            normalize_term(&mut c.lhs, dl_path, diags);
            normalize_term(&mut c.rhs, dl_path, diags);
        }
        BodyItem::Effect { args, outs, .. } => {
            for t in args.iter_mut().chain(outs.iter_mut()) { normalize_term(t, dl_path, diags); }
        }
        BodyItem::Closure { .. } | BodyItem::Scc { .. } | BodyItem::Node2vec { .. } => {}
    }
}

fn normalize_atom(a: &mut Atom, dl_path: &str, diags: &mut Vec<TypeDiag>) {
    for t in &mut a.terms { normalize_term(t, dl_path, diags); }
}

fn normalize_term(t: &mut Term, dl_path: &str, diags: &mut Vec<TypeDiag>) {
    let Term::PathLit { scheme, body, span } = t else { return; };
    let span = *span;
    match resolve_literal(scheme, body) {
        Ok(resolved) => { *t = Term::Str(resolved.text().to_string()); }
        Err(e) => {
            diags.push(TypeDiag {
                path: dl_path.to_string(),
                span,
                severity: Severity::Error,
                code: e.code().to_string(),
                msg: e.msg(),
            });
            // Leave a benign empty string so downstream lowering does not also
            // trip the not-normalized bail; the error diagnostic already failed
            // the check.
            *t = Term::Str(String::new());
        }
    }
}

/// Type-check one rule's body vars against declared column types. Must run BEFORE
/// `normalize_program` rewrites the PathLits (it inspects the literal terms to
/// flag a PathLit landing in an int column). Emits the spec's diagnostic codes.
///
/// pseudo:
///   for each body atom: bind each var -> its column's ColTy, and each PathLit
///     -> its column's ColTy (for the int-column + coerce checks).
///   a var seen at two branded columns with incompatible brands -> brand-mismatch.
///   a path/branded var meeting a plain `text` column -> coerce-text-path (warn).
///   a PathLit in an int column -> error.
pub fn check_rule_types(rule: &Rule, rels: &Rels, brands: &Brands, dl_path: &str) -> Vec<TypeDiag> {
    let mut diags = Vec::new();
    // var name -> the brand/path types it has been bound to so far, with one span
    // per occurrence (so a mismatch points at the second, conflicting site).
    let mut seen: HashMap<String, ColTy> = HashMap::new();

    let visit_atom = |a: &Atom, diags: &mut Vec<TypeDiag>, seen: &mut HashMap<String, ColTy>| {
        let Some(meta) = rels.get(&a.rel) else { return; };
        if a.terms.len() != meta.cols.len() { return; }
        for (i, term) in a.terms.iter().enumerate() {
            let cty = col_ty(&meta.cols[i], brands);
            match term {
                Term::PathLit { scheme, span, .. } => {
                    // A path literal in an int column is a type error.
                    if cty.base == Type::Int {
                        diags.push(TypeDiag {
                            path: dl_path.to_string(), span: *span,
                            severity: Severity::Error, code: "brand-mismatch".into(),
                            msg: format!("`{scheme}:` path literal cannot fill int column `{}`", meta.cols[i].name),
                        });
                    } else if cty.base == Type::Text && cty.brand.is_none() {
                        // A typed literal flowing into a plain text column: grandfather
                        // with a coerce warning.
                        diags.push(TypeDiag {
                            path: dl_path.to_string(), span: *span,
                            severity: Severity::Warn, code: "coerce-text-path".into(),
                            msg: format!("`{scheme}:` path literal coerced into plain text column `{}`", meta.cols[i].name),
                        });
                    }
                }
                // A plain string literal in an int column, or an int literal in a
                // path/branded column, is a datatype conflict: without this check it
                // passes typecheck and crashes the tick on a SQLite datatype mismatch
                // at head insert. `Str`/`Int` literals carry no span, so the
                // diagnostic lands at line 1 (span 0,0).
                Term::Str(_) if cty.base == Type::Int => {
                    diags.push(TypeDiag {
                        path: dl_path.to_string(), span: (0, 0),
                        severity: Severity::Error, code: "brand-mismatch".into(),
                        msg: format!("string literal cannot fill int column `{}`", meta.cols[i].name),
                    });
                }
                Term::Int(_) if is_path_base(cty.base) || cty.brand.is_some() => {
                    let what = match &cty.brand { Some(b) => format!("brand `{b}`"), None => "a path".into() };
                    diags.push(TypeDiag {
                        path: dl_path.to_string(), span: (0, 0),
                        severity: Severity::Error, code: "brand-mismatch".into(),
                        msg: format!("int literal cannot fill {what} column `{}`", meta.cols[i].name),
                    });
                }
                Term::Var(v) if v != "_" => {
                    match seen.get(v).cloned() {
                        None => { seen.insert(v.clone(), cty); }
                        Some(prev) => {
                            unify(v, &prev, &cty, brands, dl_path, diags);
                            // Narrow toward the more specific (branded/path) type so a
                            // later text column does not re-warn against an already-known
                            // path var; keep the branded/path one when present.
                            let more_specific = (prev.brand.is_none() && cty.brand.is_some())
                                || (!is_path_base(prev.base) && is_path_base(cty.base));
                            if more_specific { seen.insert(v.clone(), cty); }
                        }
                    }
                }
                // An arithmetic expression produces an int: the column it fills
                // must be int, and every var operand unifies as int.
                Term::Arith { .. } => {
                    if cty.base != Type::Int {
                        diags.push(TypeDiag {
                            path: dl_path.to_string(), span: (0, 0),
                            severity: Severity::Error, code: "brand-mismatch".into(),
                            msg: format!("arithmetic expression cannot fill non-int column `{}`", meta.cols[i].name),
                        });
                    }
                    let int_ty = ColTy { base: Type::Int, brand: None };
                    let mut stack = vec![term];
                    while let Some(t) = stack.pop() {
                        match t {
                            Term::Arith { lhs, rhs, .. } => { stack.push(lhs); stack.push(rhs); }
                            Term::Var(v) if v != "_" => match seen.get(v).cloned() {
                                None => { seen.insert(v.clone(), int_ty.clone()); }
                                Some(prev) => unify(v, &prev, &int_ty, brands, dl_path, diags),
                            },
                            _ => {}
                        }
                    }
                }
                // A string function call (`split`/`replace`) produces text: the
                // column it fills must be a text-base type. Arg vars unify as
                // text too (split/replace take text operands today). The
                // whitelist (split/replace) is enforced again at lower time.
                Term::Call { name, args } => {
                    // `int/1` produces an int (fills an int column); split/replace
                    // and the STR_FNS pass-throughs produce text (fill a text-base
                    // column). All take text args.
                    let is_int = name == "int";
                    let str_fn = crate::lower::STR_FNS.iter().find(|(n, _, _)| *n == name.as_str());
                    let known = is_int || matches!(name.as_str(), "split" | "replace") || str_fn.is_some();
                    if !known {
                        let extra = crate::lower::STR_FNS.iter()
                            .map(|(n, _, _)| *n).collect::<Vec<_>>().join(", ");
                        diags.push(TypeDiag {
                            path: dl_path.to_string(), span: (0, 0),
                            severity: Severity::Error, code: "unknown-function".into(),
                            msg: format!("unknown function `{name}` (known: split, replace, int, {extra})"),
                        });
                    }
                    let want = match (is_int, str_fn) {
                        (true, _) => 1,
                        (_, Some((_, _, k))) => *k,
                        _ => 3, // split/replace
                    };
                    if args.len() != want {
                        diags.push(TypeDiag {
                            path: dl_path.to_string(), span: (0, 0),
                            severity: Severity::Error, code: "arity".into(),
                            msg: format!("function `{name}` expects {want} args, got {}", args.len()),
                        });
                    }
                    if is_int {
                        if cty.base != Type::Int {
                            diags.push(TypeDiag {
                                path: dl_path.to_string(), span: (0, 0),
                                severity: Severity::Error, code: "brand-mismatch".into(),
                                msg: format!("`int(..)` cannot fill non-int column `{}`", meta.cols[i].name),
                            });
                        }
                    } else if !is_path_base(cty.base) && cty.base != Type::Text {
                        diags.push(TypeDiag {
                            path: dl_path.to_string(), span: (0, 0),
                            severity: Severity::Error, code: "brand-mismatch".into(),
                            msg: format!("string function `{name}` cannot fill non-text column `{}`", meta.cols[i].name),
                        });
                    }
                    let text_ty = ColTy { base: Type::Text, brand: None };
                    for a in args {
                        if let Term::Var(v) = a {
                            if v != "_" { match seen.get(v).cloned() {
                                None => { seen.insert(v.clone(), text_ty.clone()); }
                                Some(prev) => unify(v, &prev, &text_ty, brands, dl_path, diags),
                            }}
                        }
                    }
                }
                _ => {}
            }
        }
    };

    visit_atom(&rule.head, &mut diags, &mut seen);
    for b in &rule.body {
        match b {
            BodyItem::Pos(a) | BodyItem::Neg(a) => visit_atom(a, &mut diags, &mut seen),
            _ => {}
        }
    }
    diags
}

/// Compare two column types a var unifies across. A brand vs a different,
/// unrelated brand is a hard error; a path/branded var meeting a plain text
/// column is a coerce warning. The diagnostic has no per-occurrence span (a var
/// is not a literal), so it points at the whole program file (span 0,0) which the
/// renderer surfaces at line 1.
fn unify(var: &str, a: &ColTy, b: &ColTy, brands: &Brands, dl_path: &str, diags: &mut Vec<TypeDiag>) {
    match (&a.brand, &b.brand) {
        (Some(x), Some(y)) => {
            if x != y && !brands.is_ancestor(x, y) && !brands.is_ancestor(y, x) {
                diags.push(TypeDiag {
                    path: dl_path.to_string(), span: (0, 0),
                    severity: Severity::Error, code: "brand-mismatch".into(),
                    msg: format!("variable `{var}` is brand `{x}` and brand `{y}` (neither is an ancestor of the other)"),
                });
            }
        }
        // A branded/path var meeting a plain text column: grandfather with a warn.
        (Some(x), None) if b.base == Type::Text => coerce(var, x, dl_path, diags),
        (None, Some(y)) if a.base == Type::Text => coerce(var, y, dl_path, diags),
        // A path-shaped base type meeting a plain text column also warns.
        (None, None) => {
            if a.base == Type::Text && is_path_base(b.base) {
                coerce_base(var, b.base, dl_path, diags);
            } else if b.base == Type::Text && is_path_base(a.base) {
                coerce_base(var, a.base, dl_path, diags);
            }
        }
        _ => {}
    }
}

fn coerce(var: &str, brand: &str, dl_path: &str, diags: &mut Vec<TypeDiag>) {
    diags.push(TypeDiag {
        path: dl_path.to_string(), span: (0, 0),
        severity: Severity::Warn, code: "coerce-text-path".into(),
        msg: format!("variable `{var}` (brand `{brand}`) flows into a plain text column"),
    });
}

fn coerce_base(var: &str, base: Type, dl_path: &str, diags: &mut Vec<TypeDiag>) {
    let name = match base { Type::Path => "path", Type::File => "file", Type::Dir => "dir", _ => "path" };
    diags.push(TypeDiag {
        path: dl_path.to_string(), span: (0, 0),
        severity: Severity::Warn, code: "coerce-text-path".into(),
        msg: format!("variable `{var}` ({name}) flows into a plain text column"),
    });
}

/// Bind a rule's `BodyItem::Effect` to its `sh` decl and the temporal modifier.
/// Runs after `desugar_effects`, so the head-response form already carries a
/// synthesized Effect named for its head rel. Checks, in order:
///   - at most one effect per rule (a second is a hard error);
///   - an effect requires `@async`/`@stream` (effects fire off-tick);
///   - an explicit call (`name != head.rel`) must resolve to a declared `sh`;
///     a head-response effect with no matching decl is the legacy `effect_cmd`
///     path and binds at the daemon, so it is left alone;
///   - when the decl is known: arg arity = params, out arity = decl outs, every
///     `{param}` hole appears in the body text, and the temporal axis agrees with
///     the `sh` kind (`@async`↔`sh`/`sh!`, `@stream`↔`sh*`).
fn check_effect(rule: &Rule, fns: &HashMap<&str, &ShellFn>, dl_path: &str, diags: &mut Vec<TypeDiag>) {
    let err = |code: &str, msg: String, diags: &mut Vec<TypeDiag>| {
        diags.push(TypeDiag {
            path: dl_path.to_string(), span: (0, 0),
            severity: Severity::Error, code: code.into(), msg,
        });
    };
    let n_eff = rule.body.iter().filter(|b| matches!(b, BodyItem::Effect { .. })).count();
    if n_eff == 0 { return; }
    if n_eff > 1 {
        err("multiple-effects",
            format!("rule `{}` has {n_eff} effect calls; at most one effect per rule (split into separate @async rules)", rule.head.rel),
            diags);
    }
    if !matches!(rule.temporal, Some(Temporal::Async) | Some(Temporal::Stream)) {
        err("effect-needs-async",
            format!("effect call in rule `{}` requires `@async` or `@stream`; effects fire off-tick", rule.head.rel),
            diags);
    }
    let Some((name, args, outs)) = rule.effect() else { return; };
    let Some(f) = fns.get(name) else {
        // No declared `sh`. Legal only as the head-response/legacy form, where the
        // synthesized effect is named for the head rel; an explicit call to an
        // undeclared `sh` is a typo.
        if name != rule.head.rel {
            err("unknown-sh",
                format!("effect call `{name}(..)` in rule `{}` resolves to no `sh` decl", rule.head.rel),
                diags);
        }
        return;
    };
    if args.len() != f.params.len() {
        err("effect-arity",
            format!("`sh {name}` takes {} arg(s), called with {}", f.params.len(), args.len()),
            diags);
    }
    if outs.len() != f.outs.len() {
        err("effect-arity",
            format!("`sh {name}` returns {} value(s), bound to {}", f.outs.len(), outs.len()),
            diags);
    }
    for p in &f.params {
        if !f.body.contains(&format!("{{{p}}}")) {
            err("unused-hole",
                format!("`sh {name}` param `{p}` never appears as `{{{p}}}` in the template"),
                diags);
        }
    }
    let crossed = match (rule.temporal, f.kind) {
        (Some(Temporal::Stream), ShellKind::Stream) => false,
        (Some(Temporal::Async), ShellKind::Read | ShellKind::Mutate) => false,
        _ => true,
    };
    if crossed {
        let want = match f.kind { ShellKind::Stream => "@stream", _ => "@async" };
        err("temporal-kind-mismatch",
            format!("`sh {name}` is `{}`; call it from {want}, not {}",
                shell_kind_word(f.kind),
                rule.temporal.map(temporal_word).unwrap_or("a bare rule")),
            diags);
    }
}

fn shell_kind_word(k: ShellKind) -> &'static str {
    match k { ShellKind::Read => "sh", ShellKind::Mutate => "sh!", ShellKind::Stream => "sh*" }
}

fn temporal_word(t: Temporal) -> &'static str {
    match t { Temporal::Next => "@next", Temporal::Async => "@async", Temporal::Stream => "@stream" }
}

/// Run both passes for a program: validate brands/anchors, check every rule, then
/// normalize the literals in place. Returns all diagnostics. The engine calls this
/// once at the start of a tick (before declare/lower). On a brand/anchor structural
/// error it returns a single error diagnostic (span 0,0) rather than panicking.
pub fn check_and_normalize(prog: &mut Program, dl_path: &str) -> Vec<TypeDiag> {
    let mut diags = Vec::new();
    let brands = match validate_brands(prog) {
        Ok(b) => b,
        Err(e) => {
            diags.push(TypeDiag {
                path: dl_path.to_string(), span: (0, 0),
                severity: Severity::Error, code: "brand-mismatch".into(), msg: e,
            });
            Brands::default()
        }
    };
    if let Err(e) = Anchors::from_program(prog) {
        diags.push(TypeDiag {
            path: dl_path.to_string(), span: (0, 0),
            severity: Severity::Error, code: "unknown-anchor".into(), msg: e,
        });
    }
    let rels = prog_rels(prog);
    let shell_fns: HashMap<&str, &ShellFn> = prog.items.iter()
        .filter_map(|it| if let Item::Shell(f) = it { Some((f.name.as_str(), f)) } else { None })
        .collect();
    for item in &prog.items {
        if let Item::Rule(r) = item {
            diags.extend(check_rule_types(r, &rels, &brands, dl_path));
            check_effect(r, &shell_fns, dl_path, &mut diags);
        }
    }
    diags.extend(stratify_diags(prog, dl_path));
    diags.extend(normalize_program(prog, dl_path));
    diags
}

/// Stratification check, program level. Builds the rel dependency graph (the same
/// shape `rebuild_derived`/`stratify` walk) and reports a `not-stratified` error
/// for any aggregation or negation edge whose endpoints fall in one SCC: such an
/// edge would force a relation to read a not-yet-finished version of itself. This
/// runs in the typecheck path so the diagnostic flows to `--check` and LSP exactly
/// like the brand diagnostics (no engine-side bail surfaces to the editor).
///
/// The agg head decl is also checked here: a `count(_)`/`sum(_)` head column must
/// be `int`; a `min(_)`/`max(_)` column takes the argument's type (left to the
/// existing per-var unification, so only the Count/Sum=int mismatch is flagged).
pub fn stratify_diags(prog: &Program, dl_path: &str) -> Vec<TypeDiag> {
    let mut diags = Vec::new();
    let rels = prog_rels(prog);

    // Intern rel names and build adjacency + the forcing-edge list.
    let mut id: HashMap<String, u32> = HashMap::new();
    let mut names: Vec<String> = Vec::new();
    let intern = |s: &str, id: &mut HashMap<String, u32>, names: &mut Vec<String>| -> u32 {
        if let Some(&i) = id.get(s) { return i; }
        let i = names.len() as u32; id.insert(s.to_string(), i); names.push(s.to_string()); i
    };
    // (head, body, forcing) where forcing = the edge is a negation or aggregation.
    let mut edges: Vec<(u32, u32, bool)> = Vec::new();
    for item in &prog.items {
        let Item::Rule(r) = item else { continue; };
        let h = intern(&r.head.rel, &mut id, &mut names);
        let agg = r.has_agg();
        for b in &r.body {
            let (rel, force) = match b {
                BodyItem::Pos(a) => (a.rel.as_str(), agg),
                BodyItem::Neg(a) => (a.rel.as_str(), true),
                _ => continue,
            };
            let bi = intern(rel, &mut id, &mut names);
            edges.push((h, bi, force));
        }
    }
    let n = names.len();
    let mut adj = vec![Vec::new(); n];
    for &(h, b, _) in &edges { adj[h as usize].push(b); }
    let (comp, _) = scc::tarjan(&adj);
    let mut reported: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
    for &(h, b, force) in &edges {
        if force && comp[h as usize] == comp[b as usize] && reported.insert((h, b)) {
            diags.push(TypeDiag {
                path: dl_path.to_string(), span: (0, 0),
                severity: Severity::Error, code: "not-stratified".into(),
                msg: format!("relation `{}` is aggregated or negated inside a recursive cycle with `{}`",
                    names[b as usize], names[h as usize]),
            });
        }
    }

    // Agg head decl type check: Count/Sum land an Int, so a non-int head column for
    // a count/sum term is a brand-mismatch.
    for item in &prog.items {
        let Item::Rule(r) = item else { continue; };
        if !r.has_agg() { continue; }
        let Some(meta) = rels.get(&r.head.rel) else { continue; };
        if r.head.terms.len() != meta.cols.len() { continue; }
        for (i, f) in r.aggs.iter().enumerate() {
            let Some(f) = f else { continue; };
            if f.fixed_out() == Some(Type::Int) && meta.cols[i].ty != Type::Int && meta.cols[i].brand.is_none() {
                diags.push(TypeDiag {
                    path: dl_path.to_string(), span: (0, 0),
                    severity: Severity::Error, code: "brand-mismatch".into(),
                    msg: format!("`{}(...)` produces an int but head column `{}` of `{}` is `{}`",
                        f.sql().to_lowercase(), meta.cols[i].name, r.head.rel, meta.cols[i].ty.sql()),
                });
            }
        }
    }
    diags
}

/// Build a `Rels` map from the program's own `rel` decls only (built-in relations
/// like `file`/`module_*` are added by the engine; the type checker only needs the
/// author-declared schemas to resolve a body var's column type). Missing relations
/// are simply skipped during checking (the lowerer reports unknown relations).
fn prog_rels(prog: &Program) -> Rels {
    let mut rels = Rels::new();
    for item in &prog.items {
        if let Item::Rel(d) = item {
            rels.insert(d.name.clone(), RelMeta { cols: d.cols.clone() });
        }
    }
    rels
}
