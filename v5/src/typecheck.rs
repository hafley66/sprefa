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
    /// Enum brands: brand name -> its closed set of allowed text literals. A brand
    /// declared `type sev = "a" | "b"` lands here (its `parent` is `"text"`); the
    /// `<:` form does not. A sub-brand `type x <: sev` inherits the set by walking
    /// the parent chain (see `enum_variants`).
    variants: HashMap<String, Vec<String>>,
}

impl Brands {
    pub fn from_program(prog: &Program) -> Result<Brands, String> {
        let mut parent: HashMap<String, String> = HashMap::new();
        let mut variants: HashMap<String, Vec<String>> = HashMap::new();
        for item in &prog.items {
            if let Item::Brand(b) = item {
                if parent.contains_key(&b.name) {
                    return Err(format!("duplicate brand `{}`", b.name));
                }
                if Type::parse(&b.name).is_some() {
                    return Err(format!("brand `{}` shadows a base type", b.name));
                }
                parent.insert(b.name.clone(), b.parent.clone());
                if let Some(vs) = &b.variants {
                    variants.insert(b.name.clone(), vs.clone());
                }
            }
        }
        // Ambient builtin enum brands (type_edge_kind, df_node_kind, ...): the
        // closed vocabularies carried by builtin relation columns, present
        // without any user `type` decl. A user decl reusing one of these names
        // is an error — the builtin set is engine-owned.
        for (name, vs) in crate::engine::builtin_enum_brands() {
            if parent.contains_key(*name) {
                return Err(format!(
                    "brand `{name}` shadows a built-in enum brand (its variants are engine-defined) — pick another name"));
            }
            parent.insert(name.to_string(), "text".into());
            variants.insert(name.to_string(), vs.iter().map(|v| v.to_string()).collect());
        }
        let brands = Brands { parent, variants };
        // Every parent must terminate at a base type without cycling.
        for name in brands.parent.keys() {
            brands.base_type(name).ok_or_else(|| {
                format!("brand `{name}` has no base type (unknown parent or cycle)")
            })?;
        }
        Ok(brands)
    }

    pub fn is_brand(&self, name: &str) -> bool {
        self.parent.contains_key(name)
    }

    /// The closed variant set of an enum brand, or of the nearest enum brand up
    /// the parent chain (so `type x <: sev` inherits `sev`'s literals). `None`
    /// when neither `name` nor any ancestor is an enum brand.
    pub fn enum_variants(&self, name: &str) -> Option<&[String]> {
        let mut cur = name.to_string();
        for _ in 0..self.parent.len() + 1 {
            if let Some(vs) = self.variants.get(&cur) {
                return Some(vs);
            }
            cur = self.parent.get(&cur)?.clone();
        }
        None
    }

    /// Walk the parent chain to a base `Type`. None on an unknown parent or a cycle.
    pub fn base_type(&self, name: &str) -> Option<Type> {
        let mut cur = name.to_string();
        for _ in 0..self.parent.len() + 1 {
            if let Some(t) = Type::parse(&cur) {
                return Some(t);
            }
            cur = self.parent.get(&cur)?.clone();
        }
        None
    }

    /// Is `anc` an ancestor of (or equal to) `desc` in the brand chain?
    pub fn is_ancestor(&self, anc: &str, desc: &str) -> bool {
        if anc == desc {
            return true;
        }
        let mut cur = desc.to_string();
        for _ in 0..self.parent.len() + 1 {
            let Some(p) = self.parent.get(&cur) else {
                return false;
            };
            if p == anc {
                return true;
            }
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
    interned: bool,
}

fn col_ty(col: &Col, brands: &Brands) -> ColTy {
    match &col.brand {
        Some(b) => ColTy {
            base: brands.base_type(b).unwrap_or(Type::Text),
            brand: Some(b.clone()),
            interned: col.interned(),
        },
        None => ColTy {
            base: col.ty,
            brand: None,
            interned: col.interned(),
        },
    }
}

fn is_path_base(t: Type) -> bool {
    matches!(t, Type::Path | Type::File | Type::Dir)
}

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
                            d.name, c.name
                        ));
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
    pub fn body(&self, name: &str) -> Option<&str> {
        self.named.get(name).map(|s| s.as_str())
    }
}

/// Resolve one path literal body to canonical text. `~/x` and `~` use the default
/// scan-root anchor; any other `name/` anchor prefix is rejected in v1 (named
/// anchor refs are deferred). Returns the canonical text, or the descriptor error.
fn resolve_literal(scheme: &str, body: &str) -> Result<Resolved, desc::DescError> {
    let spec =
        desc::scheme_spec(scheme).expect("lexer only emits a Scheme token for a registered scheme");
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
                for b in &mut r.body {
                    normalize_body_item(b, dl_path, &mut diags);
                }
            }
            Item::Query(q) => normalize_atom(&mut q.head, dl_path, &mut diags),
            Item::Gen(g) => {
                match &mut g.target {
                    GenTarget::Splice { path, l0, l1 } => {
                        for t in [path, l0, l1] {
                            normalize_term(t, dl_path, &mut diags);
                        }
                    }
                    GenTarget::Cursor { path, lo, hi, .. } => {
                        for t in [path, lo, hi] {
                            normalize_term(t, dl_path, &mut diags);
                        }
                    }
                    GenTarget::Zone { .. } => {}
                    GenTarget::File { .. } => {}
                }
                for b in &mut g.body {
                    normalize_body_item(b, dl_path, &mut diags);
                }
            }
            Item::Rel(_) | Item::Anchor(_) | Item::Brand(_) | Item::Shape(_) | Item::Shell(_) => {}
        }
    }
    diags
}

fn normalize_body_item(b: &mut BodyItem, dl_path: &str, diags: &mut Vec<TypeDiag>) {
    match b {
        BodyItem::Pos(a) | BodyItem::Neg(a) => normalize_atom(a, dl_path, diags),
        BodyItem::Scan {
            repo,
            rev,
            glob,
            path,
            rev_out,
        } => {
            for t in [repo, rev, glob, path, rev_out] {
                normalize_term(t, dl_path, diags);
            }
        }
        BodyItem::Match {
            path,
            rev,
            line,
            id,
            col,
            end_col,
            legacy_name,
            ..
        } => {
            if *legacy_name {
                diags.push(TypeDiag {
                    path: dl_path.to_string(), span: (0, 0),
                    severity: Severity::Warn, code: "deprecated-op-name".into(),
                    msg: "`match(...)` is deprecated; use `match_line(...)` instead. \
                          match_line is a LINE REGEX — correct only for flat text (ini/env/log/csv), \
                          never for structured source code. For source, use `match_ast(...)` \
                          (ast-grep structural matching) instead.".into(),
                });
            }
            for t in [path, rev, line] {
                normalize_term(t, dl_path, diags);
            }
            for t in [id, col, end_col].into_iter().flatten() {
                normalize_term(t, dl_path, diags);
            }
        }
        BodyItem::Ast {
            path,
            rev,
            line,
            end,
            id,
            ..
        } => {
            for t in [path, rev, line] {
                normalize_term(t, dl_path, diags);
            }
            if let Some(e) = end {
                normalize_term(e, dl_path, diags);
            }
            if let Some(t) = id {
                normalize_term(t, dl_path, diags);
            }
        }
        BodyItem::Sg {
            src,
            rev,
            line,
            col,
            end_line,
            end_col,
            id,
            legacy_name,
            ..
        } => {
            if *legacy_name {
                diags.push(TypeDiag {
                    path: dl_path.to_string(),
                    span: (0, 0),
                    severity: Severity::Warn,
                    code: "deprecated-op-name".into(),
                    msg: "`sg(...)` is deprecated; use `match_ast(...)` instead. \
                          match_ast is ast-grep structural matching — the correct tool for \
                          source code (it sees multi-line and AST-shaped constructs that a \
                          line regex like `match_line` cannot)."
                        .into(),
                });
            }
            for t in [src, line, col, end_line, end_col] {
                normalize_term(t, dl_path, diags);
            }
            if let Some(t) = rev {
                normalize_term(t, dl_path, diags);
            }
            if let Some(t) = id {
                normalize_term(t, dl_path, diags);
            }
        }
        BodyItem::AstYaml {
            path,
            rev,
            line,
            col,
            end_line,
            end_col,
            ..
        } => {
            for t in [path, rev, line, col, end_line, end_col] {
                normalize_term(t, dl_path, diags);
            }
        }
        BodyItem::JsonP {
            src, rev, out, id, ..
        } => {
            for t in [src, out] {
                normalize_term(t, dl_path, diags);
            }
            if let Some(t) = rev {
                normalize_term(t, dl_path, diags);
            }
            if let Some(t) = id {
                normalize_term(t, dl_path, diags);
            }
        }
        BodyItem::Json { src, rev, .. } => {
            normalize_term(src, dl_path, diags);
            if let Some(t) = rev {
                normalize_term(t, dl_path, diags);
            }
        }
        BodyItem::Cmd {
            path,
            rev,
            line,
            out,
            ..
        } => {
            for t in [path, rev, line, out] {
                normalize_term(t, dl_path, diags);
            }
        }
        BodyItem::Comment {
            path,
            rev,
            l0,
            l1,
            label,
            ..
        } => {
            for t in [path, rev, l0, l1, label] {
                normalize_term(t, dl_path, diags);
            }
        }
        BodyItem::Cmp(c) => {
            normalize_term(&mut c.lhs, dl_path, diags);
            normalize_term(&mut c.rhs, dl_path, diags);
        }
        BodyItem::Effect { args, outs, .. } => {
            for t in args.iter_mut().chain(outs.iter_mut()) {
                normalize_term(t, dl_path, diags);
            }
        }
        BodyItem::Closure { .. } | BodyItem::Scc { .. } | BodyItem::Node2vec { .. } => {}
    }
}

fn normalize_atom(a: &mut Atom, dl_path: &str, diags: &mut Vec<TypeDiag>) {
    for t in &mut a.terms {
        normalize_term(t, dl_path, diags);
    }
}

fn normalize_term(t: &mut Term, dl_path: &str, diags: &mut Vec<TypeDiag>) {
    let Term::PathLit { scheme, body, span } = t else {
        return;
    };
    let span = *span;
    match resolve_literal(scheme, body) {
        Ok(resolved) => {
            *t = Term::Str(resolved.text().to_string());
        }
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

    // A SOURCE rule's head is filled from scan/regex/AST captures via `val_of`, which
    // has no json machinery — the SQL agg/json lowering never runs for it. Refuse a
    // json aggregate or json constructor in a source-rule head loudly and derived-only,
    // the same style as the body-bind source-rule refusal (`val_of`'s head-inline note).
    if rule.is_source() {
        let is_json_fn = |t: &Term| {
            matches!(t, Term::Call { name, .. }
            if matches!(name.as_str(), "json" | "json_object" | "json_array"))
        };
        let json_agg = rule.aggs.iter().any(|a| {
            matches!(
                a,
                Some(AggFn::JsonGroupArray) | Some(AggFn::JsonGroupObject)
            )
        });
        if json_agg || rule.head.terms.iter().any(is_json_fn) {
            diags.push(TypeDiag {
                path: dl_path.to_string(),
                span: (0, 0),
                severity: Severity::Error,
                code: "json-in-source".into(),
                msg: format!(
                    "relation `{}` is a source rule (scan/match/ast/...); json aggregates \
                    and json constructors are derived-only — split the extraction into its own \
                    relation and build the json in a derived rule that reads it",
                    rule.head.rel
                ),
            });
        }
        // S6: a source-extract rule's body atom is legitimate ONLY when it
        // supplies an INPUT to the source op itself (the data-driven scan/rev
        // coordinate pattern `Engine::resolve_scan_bindings` compiles) — any
        // other plain `BodyItem::Pos`/`Neg` atom is ignored by file
        // extraction (`parse_file`'s dispatch loop has no arm for it) and
        // ends up neither filtering nor joining anything. This is the
        // typecheck-time twin of `desugar::reject_source_relation_joins`'s
        // tick-time bail — same `source_input_vars`/`term_vars`
        // classification (shared fn, so the two can never disagree on which
        // atom is legitimate), just surfaced before any scan runs, through
        // `--check`/`--parse-only`/the LSP. Same fix shape as the rel-level
        // source+derived co-heading bail (`tick.rs`): split the join into its
        // own relation and union/join it in a third derived rule.
        let source_inputs = crate::engine::desugar::source_input_vars(rule);
        let extra_atoms: Vec<&str> = rule
            .body
            .iter()
            .filter_map(|b| match b {
                BodyItem::Pos(a) | BodyItem::Neg(a) => {
                    let supplies_input = a
                        .terms
                        .iter()
                        .any(|t| crate::engine::desugar::term_vars(t, &source_inputs));
                    if supplies_input {
                        None
                    } else {
                        Some(a.rel.as_str())
                    }
                }
                _ => None,
            })
            .collect();
        if !extra_atoms.is_empty() {
            let atom_list = extra_atoms
                .iter()
                .map(|r| format!("`{r}`"))
                .collect::<Vec<_>>()
                .join(", ");
            let pronoun = if extra_atoms.len() == 1 { "it" } else { "them" };
            diags.push(TypeDiag {
                path: dl_path.to_string(),
                span: (0, 0),
                severity: Severity::Error,
                code: "source-rule-extra-atom".into(),
                msg: format!(
                    "relation `{}` is a source-extract rule (scan/match/ast/sg/json/\
                    comment) whose body also joins {atom_list}; that atom supplies no input to \
                    the source op (not the data-driven scan/rev coordinate pattern), so no \
                    extraction code path ever evaluates it against the scanned rows — it is \
                    silently dropped and does nothing. Put the source-extract rule and the join \
                    to {pronoun} in two separate relations and combine them in a third derived \
                    rule.",
                    rule.head.rel
                ),
            });
        }
    }

    let visit_atom = |a: &Atom, diags: &mut Vec<TypeDiag>, seen: &mut HashMap<String, ColTy>| {
        let Some(meta) = rels.get(&a.rel) else {
            return;
        };
        if a.terms.len() != meta.cols.len() {
            return;
        }
        for (i, term) in a.terms.iter().enumerate() {
            let cty = col_ty(&meta.cols[i], brands);
            match term {
                Term::PathLit { scheme, span, .. } => {
                    // A path literal in an int column is a type error.
                    if cty.base == Type::Int {
                        diags.push(TypeDiag {
                            path: dl_path.to_string(),
                            span: *span,
                            severity: Severity::Error,
                            code: "brand-mismatch".into(),
                            msg: format!(
                                "`{scheme}:` path literal cannot fill int column `{}`",
                                meta.cols[i].name
                            ),
                        });
                    } else if cty.base == Type::Text && cty.brand.is_none() {
                        // A typed literal flowing into a plain text column: grandfather
                        // with a coerce warning.
                        diags.push(TypeDiag {
                            path: dl_path.to_string(),
                            span: *span,
                            severity: Severity::Warn,
                            code: "coerce-text-path".into(),
                            msg: format!(
                                "`{scheme}:` path literal coerced into plain text column `{}`",
                                meta.cols[i].name
                            ),
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
                        path: dl_path.to_string(),
                        span: (0, 0),
                        severity: Severity::Error,
                        code: "brand-mismatch".into(),
                        msg: format!(
                            "string literal cannot fill int column `{}`",
                            meta.cols[i].name
                        ),
                    });
                }
                // A string literal filling an enum-branded column must be one of
                // the closed variant set (rule head, fact head, or a body pin).
                Term::Str(s) => {
                    if let Some(brand) = &cty.brand {
                        enum_lit_check(brand, s, brands, &meta.cols[i].name, dl_path, diags);
                    }
                }
                Term::Int(_) if is_path_base(cty.base) || cty.brand.is_some() => {
                    let what = match &cty.brand {
                        Some(b) => format!("brand `{b}`"),
                        None => "a path".into(),
                    };
                    diags.push(TypeDiag {
                        path: dl_path.to_string(),
                        span: (0, 0),
                        severity: Severity::Error,
                        code: "brand-mismatch".into(),
                        msg: format!(
                            "int literal cannot fill {what} column `{}`",
                            meta.cols[i].name
                        ),
                    });
                }
                Term::Var(v) if v != "_" => {
                    match seen.get(v).cloned() {
                        None => {
                            seen.insert(v.clone(), cty);
                        }
                        Some(prev) => {
                            unify(v, &prev, &cty, brands, dl_path, diags);
                            // Narrow toward the more specific (branded/path) type so a
                            // later text column does not re-warn against an already-known
                            // path var; keep the branded/path one when present.
                            let more_specific = (prev.brand.is_none() && cty.brand.is_some())
                                || (!is_path_base(prev.base) && is_path_base(cty.base));
                            if more_specific {
                                seen.insert(v.clone(), cty);
                            }
                        }
                    }
                }
                // An arithmetic expression fills a column matching its inferred
                // type: an int tree needs an int column (the historical rule); a
                // text `+` concatenation tree needs a text-base column. Operand
                // typing (incl. the mixed int/text `+` error) lives in `arith_ty`.
                Term::Arith { .. } => {
                    match arith_ty(term, seen, dl_path, diags) {
                        Some(Type::Int) => {
                            if cty.base != Type::Int {
                                diags.push(TypeDiag {
                                    path: dl_path.to_string(),
                                    span: (0, 0),
                                    severity: Severity::Error,
                                    code: "brand-mismatch".into(),
                                    msg: format!(
                                        "arithmetic expression cannot fill non-int column `{}`",
                                        meta.cols[i].name
                                    ),
                                });
                            }
                        }
                        Some(_) => {
                            if cty.base == Type::Int {
                                diags.push(TypeDiag {
                                    path: dl_path.to_string(),
                                    span: (0, 0),
                                    severity: Severity::Error,
                                    code: "brand-mismatch".into(),
                                    msg: format!(
                                        "text `+` concatenation cannot fill int column `{}`",
                                        meta.cols[i].name
                                    ),
                                });
                            }
                        }
                        // None = already diagnosed (mixed `+`) — don't cascade.
                        None => {}
                    }
                }
                // A string function call (`split`/`replace`) produces text: the
                // column it fills must be a text-base type. Arg vars unify as
                // text too (split/replace take text operands today). The
                // whitelist (split/replace) is enforced again at lower time.
                Term::Call { name, args } => {
                    // `int/1` produces an int (fills an int column); split/replace
                    // and the STR_FNS pass-throughs produce text (fill a text-base
                    // column). The json constructors produce text too (a JSON
                    // string) but are VARIADIC. All take text args.
                    let is_int = matches!(name.as_str(), "int" | "len" | "lines");
                    let str_fn = crate::lower::STR_FNS
                        .iter()
                        .find(|(n, _, _)| *n == name.as_str());
                    let is_json = matches!(name.as_str(), "json_object" | "json_array" | "json");
                    let is_sym_identity = name == "sym";
                    let known = is_int
                        || matches!(name.as_str(), "split" | "replace")
                        || str_fn.is_some()
                        || is_json
                        || is_sym_identity;
                    if !known {
                        let extra = crate::lower::STR_FNS
                            .iter()
                            .map(|(n, _, _)| *n)
                            .collect::<Vec<_>>()
                            .join(", ");
                        diags.push(TypeDiag {
                            path: dl_path.to_string(),
                            span: (0, 0),
                            severity: Severity::Error,
                            code: "unknown-function".into(),
                            msg: format!(
                                "unknown function `{name}` (known: split, replace, sym, int, \
                                json_object, json_array, json, {extra})"
                            ),
                        });
                    }
                    // Fixed-arity fns check an exact count; the json constructors
                    // enforce a variadic shape (json_object even>=2, json_array>=1,
                    // json==1) and emit the same `arity` code on a miss.
                    if is_json {
                        let ok = match name.as_str() {
                            "json_object" => args.len() >= 2 && args.len() % 2 == 0,
                            "json_array" => !args.is_empty(),
                            _ /* json */ => args.len() == 1,
                        };
                        if !ok {
                            let want = match name.as_str() {
                                "json_object" => "an even number of args >= 2 (key, value, ...)",
                                "json_array" => "at least 1 arg",
                                _ => "exactly 1 arg",
                            };
                            diags.push(TypeDiag {
                                path: dl_path.to_string(),
                                span: (0, 0),
                                severity: Severity::Error,
                                code: "arity".into(),
                                msg: format!(
                                    "function `{name}` expects {want}, got {}",
                                    args.len()
                                ),
                            });
                        }
                    } else {
                        let want = match (is_int, is_sym_identity, str_fn) {
                            (true, _, _) => 1,
                            (_, true, _) => 1,
                            (_, _, Some((_, _, k))) => *k,
                            _ => 3, // split/replace
                        };
                        if args.len() != want {
                            diags.push(TypeDiag {
                                path: dl_path.to_string(),
                                span: (0, 0),
                                severity: Severity::Error,
                                code: "arity".into(),
                                msg: format!(
                                    "function `{name}` expects {want} args, got {}",
                                    args.len()
                                ),
                            });
                        }
                    }
                    if is_int {
                        if cty.base != Type::Int {
                            diags.push(TypeDiag {
                                path: dl_path.to_string(),
                                span: (0, 0),
                                severity: Severity::Error,
                                code: "brand-mismatch".into(),
                                msg: format!(
                                    "`int(..)` cannot fill non-int column `{}`",
                                    meta.cols[i].name
                                ),
                            });
                        }
                    } else if !is_path_base(cty.base) && cty.base != Type::Text {
                        diags.push(TypeDiag {
                            path: dl_path.to_string(),
                            span: (0, 0),
                            severity: Severity::Error,
                            code: "brand-mismatch".into(),
                            msg: format!(
                                "string function `{name}` cannot fill non-text column `{}`",
                                meta.cols[i].name
                            ),
                        });
                    }
                    // The json constructors accept mixed-type values (an int column
                    // becomes a JSON number, text a JSON string), so their args do
                    // NOT unify as text — only split/replace/STR_FNS take text operands.
                    if !is_json {
                        let text_ty = ColTy {
                            base: Type::Text,
                            brand: None,
                            interned: false,
                        };
                        for a in args {
                            if let Term::Var(v) = a {
                                if v != "_" {
                                    match seen.get(v).cloned() {
                                        None => {
                                            seen.insert(v.clone(), text_ty.clone());
                                        }
                                        Some(prev) => {
                                            unify(v, &prev, &text_ty, brands, dl_path, diags)
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    };

    // Body atoms first, then computed binds, then the HEAD: a head expression
    // (`label(name + count)`) types its vars from the body's columns, and a
    // head over a bind var sees the bind's computed type. Literal/unification
    // checks inside visit_atom are order-independent.
    for b in &rule.body {
        match b {
            BodyItem::Pos(a) | BodyItem::Neg(a) => visit_atom(a, &mut diags, &mut seen),
            _ => {}
        }
    }
    check_body_binds(rule, &mut seen, dl_path, &mut diags);
    visit_atom(&rule.head, &mut diags, &mut seen);
    diags
}

/// Is this term a value-producing computation (Call or Arith)? The typecheck
/// twin of lower's `has_computation` — the gate that separates a body BIND
/// (`callee = replace(callee_q, ".", "::")`) from a plain Var=Var / Var=lit
/// equality filter.
fn is_computation(t: &Term) -> bool {
    matches!(t, Term::Call { .. } | Term::Arith { .. })
}

/// Every variable a computed expression consumes, recursively (Call args,
/// Arith sides). Wildcards excluded.
fn term_vars(t: &Term, out: &mut Vec<String>) {
    match t {
        Term::Var(v) if v != "_" => out.push(v.clone()),
        Term::Call { args, .. } => {
            for a in args {
                term_vars(a, out);
            }
        }
        Term::Arith { lhs, rhs, .. } => {
            term_vars(lhs, out);
            term_vars(rhs, out);
        }
        _ => {}
    }
}

/// Boundness + typing for body-level computed binds, derived-shaped bodies only
/// (a source rule's regex/AST captures are invisible here; the engine's `val_of`
/// refuses its constraints with the head-inline note at eval). Mirrors lower's
/// semantics exactly: a bind's RHS may consume any positive-atom var (SQL joins
/// are order-free — lower's canon holds every atom var before the Cmp pass) or
/// a var bound by an EARLIER bind; a later-bind or nowhere-bound var errors
/// naming the fix. Also records each bind var's type (so a text bind can feed
/// text `+` later) and runs `arith_ty` over filters for the mixed-`+` check.
fn check_body_binds(
    rule: &Rule,
    seen: &mut HashMap<String, ColTy>,
    dl_path: &str,
    diags: &mut Vec<TypeDiag>,
) {
    let derived_shape = !rule.body.is_empty()
        && rule
            .body
            .iter()
            .all(|b| matches!(b, BodyItem::Pos(_) | BodyItem::Neg(_) | BodyItem::Cmp(_)));
    if !derived_shape {
        return;
    }
    let mut atom_vars: std::collections::HashSet<String> = std::collections::HashSet::new();
    for item in &rule.body {
        if let BodyItem::Pos(a) = item {
            for t in &a.terms {
                if let Term::Var(v) = t {
                    if v != "_" {
                        atom_vars.insert(v.clone());
                    }
                }
            }
        }
    }
    let mut bind_vars: std::collections::HashSet<String> = std::collections::HashSet::new();
    for item in &rule.body {
        let BodyItem::Cmp(c) = item else { continue };
        let bind = if c.op == CmpOp::Eq {
            match (&c.lhs, &c.rhs) {
                (Term::Var(v), rhs)
                    if v != "_"
                        && !atom_vars.contains(v)
                        && !bind_vars.contains(v)
                        && is_computation(rhs) =>
                {
                    Some((v, rhs))
                }
                (lhs, Term::Var(v))
                    if v != "_"
                        && !atom_vars.contains(v)
                        && !bind_vars.contains(v)
                        && is_computation(lhs) =>
                {
                    Some((v, lhs))
                }
                _ => None,
            }
        } else {
            None
        };
        match bind {
            Some((target, expr)) => {
                let mut consumed = Vec::new();
                term_vars(expr, &mut consumed);
                for used in consumed {
                    if !atom_vars.contains(&used) && !bind_vars.contains(&used) {
                        diags.push(TypeDiag {
                            path: dl_path.to_string(), span: (0, 0),
                            severity: Severity::Error, code: "unbound-bind".into(),
                            msg: format!(
                                "bind `{used}` before computing `{target}` — `{used}` is not bound by a body atom or an earlier bind"),
                        });
                    }
                }
                let ety = match expr {
                    Term::Arith { .. } => arith_ty(expr, seen, dl_path, diags),
                    Term::Call { name, .. } => {
                        Some(if name == "int" { Type::Int } else { Type::Text })
                    }
                    _ => None,
                };
                if let Some(base) = ety {
                    seen.entry(target.clone()).or_insert(ColTy {
                        base,
                        brand: None,
                        interned: false,
                    });
                }
                bind_vars.insert(target.clone());
            }
            None => {
                // A plain filter: type any Arith side so a mixed `+` in a
                // comparison errors here instead of surprising at lower time.
                for side in [&c.lhs, &c.rhs] {
                    if matches!(side, Term::Arith { .. }) {
                        arith_ty(side, seen, dl_path, diags);
                    }
                }
            }
        }
    }
}

/// Infer an arithmetic tree's type. `+` is polymorphic: int + int = addition
/// (Some(Int)), text + text = concatenation (Some(Text)), mixed = the
/// `plus-mismatch` error naming the fix (returns None so callers don't
/// cascade). An unknown side (a var with no type yet) adopts the other side's
/// type; both-unknown keeps the historical int default. `-`/`*`/`/`/`%` stay
/// int-only, unifying var operands as int exactly like the pre-overload walk.
fn arith_ty(
    t: &Term,
    seen: &mut HashMap<String, ColTy>,
    dl_path: &str,
    diags: &mut Vec<TypeDiag>,
) -> Option<Type> {
    match t {
        Term::Int(_) => Some(Type::Int),
        Term::Str(_) | Term::Interp(_) => Some(Type::Text),
        Term::Call { name, .. } => Some(if name == "int" { Type::Int } else { Type::Text }),
        Term::Var(v) if v != "_" => seen.get(v).map(|c| c.base),
        Term::Arith {
            op: ArithOp::Add,
            lhs,
            rhs,
        } => {
            let lt = arith_ty(lhs, seen, dl_path, diags);
            let rt = arith_ty(rhs, seen, dl_path, diags);
            let is_text = |x: Type| x != Type::Int;
            match (lt, rt) {
                (Some(Type::Int), Some(Type::Int)) => Some(Type::Int),
                (Some(a), Some(b)) if is_text(a) && is_text(b) => Some(Type::Text),
                (Some(_), Some(_)) => {
                    diags.push(TypeDiag {
                        path: dl_path.to_string(), span: (0, 0),
                        severity: Severity::Error, code: "plus-mismatch".into(),
                        msg: "cannot `+` int and text — build the string with interpolation (\"${count}${name}\") or convert with int(..)".into(),
                    });
                    None
                }
                (Some(a), None) | (None, Some(a)) => {
                    // Adopt the known side's type for an untyped var operand.
                    let unknown = if lt.is_none() { lhs } else { rhs };
                    if let Term::Var(v) = unknown.as_ref() {
                        if v != "_" {
                            seen.entry(v.clone()).or_insert(ColTy {
                                base: a,
                                brand: None,
                                interned: false,
                            });
                        }
                    }
                    Some(a)
                }
                (None, None) => {
                    // Historical default: an arith over untyped vars is int.
                    for side in [lhs, rhs] {
                        if let Term::Var(v) = side.as_ref() {
                            if v != "_" {
                                seen.entry(v.clone()).or_insert(ColTy {
                                    base: Type::Int,
                                    brand: None,
                                    interned: false,
                                });
                            }
                        }
                    }
                    Some(Type::Int)
                }
            }
        }
        Term::Arith { op, lhs, rhs } => {
            for side in [lhs, rhs] {
                match arith_ty(side, seen, dl_path, diags) {
                    Some(x) if x != Type::Int => {
                        diags.push(TypeDiag {
                            path: dl_path.to_string(),
                            span: (0, 0),
                            severity: Severity::Error,
                            code: "plus-mismatch".into(),
                            msg: format!(
                                "`{}` needs int operands — only `+` concatenates text",
                                op.sql()
                            ),
                        });
                    }
                    Some(_) => {}
                    None => {
                        if let Term::Var(v) = side.as_ref() {
                            if v != "_" {
                                seen.entry(v.clone()).or_insert(ColTy {
                                    base: Type::Int,
                                    brand: None,
                                    interned: false,
                                });
                            }
                        }
                    }
                }
            }
            Some(Type::Int)
        }
        _ => None,
    }
}

/// Compare two column types a var unifies across. A brand vs a different,
/// unrelated brand is a hard error; a path/branded var meeting a plain text
/// column is a coerce warning. The diagnostic has no per-occurrence span (a var
/// is not a literal), so it points at the whole program file (span 0,0) which the
/// renderer surfaces at line 1.
fn unify(
    var: &str,
    a: &ColTy,
    b: &ColTy,
    brands: &Brands,
    dl_path: &str,
    diags: &mut Vec<TypeDiag>,
) {
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
        // An ENUM brand is exempt: its values ARE plain text (the brand is a
        // vocabulary gate on literals, not a path-like refinement), so joining
        // e.g. df_node.kind into a user text column is silent by design.
        (Some(x), None) if b.base == Type::Text => {
            if brands.enum_variants(x).is_none() {
                coerce(var, x, dl_path, diags);
            }
        }
        (None, Some(y)) if a.base == Type::Text => {
            if brands.enum_variants(y).is_none() {
                coerce(var, y, dl_path, diags);
            }
        }
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
        path: dl_path.to_string(),
        span: (0, 0),
        severity: Severity::Warn,
        code: "coerce-text-path".into(),
        msg: format!("variable `{var}` (brand `{brand}`) flows into a plain text column"),
    });
}

/// Check a string literal against an enum brand's closed variant set. A member is
/// silent; a non-member emits `enum-variant-unknown` with the allowed set and a
/// nearest-variant suggestion. A non-enum brand (the `<:` form) is a no-op.
/// Literals carry no per-occurrence span, so the diagnostic lands at line 1.
fn enum_lit_check(
    brand: &str,
    val: &str,
    brands: &Brands,
    col_name: &str,
    dl_path: &str,
    diags: &mut Vec<TypeDiag>,
) {
    let Some(variants) = brands.enum_variants(brand) else {
        return;
    };
    if variants.iter().any(|v| v == val) {
        return;
    }
    let allowed = variants
        .iter()
        .map(|v| format!("\"{v}\""))
        .collect::<Vec<_>>()
        .join(" | ");
    let hint = nearest_variant(val, variants)
        .map(|s| format!(" — did you mean \"{s}\"?"))
        .unwrap_or_default();
    diags.push(TypeDiag {
        path: dl_path.to_string(), span: (0, 0),
        severity: Severity::Error, code: "enum-variant-unknown".into(),
        msg: format!("\"{val}\" is not a variant of enum brand `{brand}` on column `{col_name}` (allowed: {allowed}){hint}"),
    });
}

/// The variant closest to `val` by Levenshtein distance, tie-broken by declaration
/// order. `None` when the set is empty. No distance cutoff — a suggestion always
/// helps against a closed set this small.
fn nearest_variant<'a>(val: &str, variants: &'a [String]) -> Option<&'a str> {
    variants
        .iter()
        .min_by_key(|v| edit_distance(val, v))
        .map(|s| s.as_str())
}

/// Levenshtein edit distance (single-row DP). Small inputs (enum variants), so the
/// allocation is negligible.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, &ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// Resolve every `rel <name>: <shape>.` decl to a plain `RelDecl` whose columns
/// are the referenced `type <shape>(...)` shape's, then drop the `Item::Shape`
/// declarations. Runs at load (frontend + the top of `check_and_normalize`) so all
/// downstream code — including the engine — only ever sees plain `RelDecl`s.
/// Idempotent: a second call finds no shapes and no `shape_ref`, so it is a no-op.
/// An unknown shape name (or a duplicate shape declaration) emits an error diag.
pub fn expand_shapes(items: &mut Vec<Item>, dl_path: &str, diags: &mut Vec<TypeDiag>) {
    let mut shapes: HashMap<String, Vec<Col>> = HashMap::new();
    for item in items.iter() {
        if let Item::Shape(s) = item {
            if shapes.insert(s.name.clone(), s.cols.clone()).is_some() {
                diags.push(TypeDiag {
                    path: dl_path.to_string(),
                    span: (0, 0),
                    severity: Severity::Error,
                    code: "duplicate-shape".into(),
                    msg: format!("duplicate shape `{}`", s.name),
                });
            }
        }
    }
    // A program that HEADS `type_decl_row` derives shapes at runtime (Phase 5): an
    // unresolved shape_ref is DEFERRED (the engine resolves it from the persisted
    // `_shapes` at the next tick's declare, or reports shape-pending), not a load
    // error. Without a type_decl_row head, an unresolved ref is the existing crisp
    // unknown-shape error.
    let derives_shapes = items
        .iter()
        .any(|it| matches!(it, Item::Rule(r) if r.head.rel == "type_decl_row"));
    for item in items.iter_mut() {
        if let Item::Rel(d) = item {
            let Some(sname) = d.shape_ref.clone() else {
                continue;
            };
            match shapes.get(&sname) {
                // Syntax `type` shape wins: fill columns and clear the ref.
                Some(cols) => {
                    d.cols = cols.clone();
                    d.shape_ref = None;
                }
                // No syntax shape. Defer (leave shape_ref set) when the program
                // derives shapes; else the unknown-shape error.
                None if derives_shapes => {}
                None => {
                    d.shape_ref = None;
                    diags.push(TypeDiag {
                        path: dl_path.to_string(), span: (0, 0),
                        severity: Severity::Error, code: "unknown-shape".into(),
                        msg: format!(
                            "rel `{}`: unknown shape `{sname}` — declare `type {sname}(...)` or use `rel {}(cols)`",
                            d.name, d.name),
                    });
                }
            }
        }
    }
    // Item::Shape decls are RETAINED (Phase 5): the engine reads the syntax shape
    // names to detect a shape-shadowed clash with a derived shape. They are inert
    // downstream (a no-op in every match arm). A program with no derived shapes is
    // unaffected — its refs all resolve here.
}

fn coerce_base(var: &str, base: Type, dl_path: &str, diags: &mut Vec<TypeDiag>) {
    let name = match base {
        Type::Path => "path",
        Type::File => "file",
        Type::Dir => "dir",
        _ => "path",
    };
    diags.push(TypeDiag {
        path: dl_path.to_string(),
        span: (0, 0),
        severity: Severity::Warn,
        code: "coerce-text-path".into(),
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
/// True if `param` appears in a shell template as the raw hole `{param}`, the
/// braced env form `${param}`, or the bare env form `$param` (terminated by a
/// non-identifier char, so `$prev` does not count as a use of `pre`).
fn param_referenced(body: &str, param: &str) -> bool {
    if body.contains(&format!("{{{param}}}")) || body.contains(&format!("${{{param}}}")) {
        return true;
    }
    let needle = format!("${param}");
    let mut from = 0;
    while let Some(i) = body[from..].find(&needle) {
        let end = from + i + needle.len();
        let next_ok = body[end..]
            .chars()
            .next()
            .map(|c| !c.is_ascii_alphanumeric() && c != '_')
            .unwrap_or(true);
        if next_ok {
            return true;
        }
        from = end;
    }
    false
}

fn check_effect(
    rule: &Rule,
    fns: &HashMap<&str, &ShellFn>,
    dl_path: &str,
    diags: &mut Vec<TypeDiag>,
) {
    let err = |code: &str, msg: String, diags: &mut Vec<TypeDiag>| {
        diags.push(TypeDiag {
            path: dl_path.to_string(),
            span: (0, 0),
            severity: Severity::Error,
            code: code.into(),
            msg,
        });
    };
    let n_eff = rule
        .body
        .iter()
        .filter(|b| matches!(b, BodyItem::Effect { .. }))
        .count();
    if n_eff == 0 {
        return;
    }
    if n_eff > 1 {
        err("multiple-effects",
            format!("rule `{}` has {n_eff} effect calls; at most one effect per rule (split into separate @async rules)", rule.head.rel),
            diags);
    }
    if !matches!(
        rule.temporal,
        Some(Temporal::Async) | Some(Temporal::Stream)
    ) {
        err(
            "effect-needs-async",
            format!(
                "effect call in rule `{}` requires `@async` or `@stream`; effects fire off-tick",
                rule.head.rel
            ),
            diags,
        );
    }
    let Some((name, args, outs)) = rule.effect() else {
        return;
    };
    let Some(f) = fns.get(name) else {
        // No declared `sh`. Legal only as the head-response/legacy form, where the
        // synthesized effect is named for the head rel; an explicit call to an
        // undeclared `sh` is a typo.
        if name != rule.head.rel {
            err(
                "unknown-sh",
                format!(
                    "effect call `{name}(..)` in rule `{}` resolves to no `sh` decl",
                    rule.head.rel
                ),
                diags,
            );
        }
        return;
    };
    if args.len() != f.params.len() {
        err(
            "effect-arity",
            format!(
                "`sh {name}` takes {} arg(s), called with {}",
                f.params.len(),
                args.len()
            ),
            diags,
        );
    }
    if outs.len() != f.outs.len() {
        err(
            "effect-arity",
            format!(
                "`sh {name}` returns {} value(s), bound to {}",
                f.outs.len(),
                outs.len()
            ),
            diags,
        );
    }
    for p in &f.params {
        // A param is "used" if it appears as the raw hole `{p}` OR the env-var
        // form `$p` / `${p}`. ShellEffectExec exports each arg both ways (commit
        // c8ebf46): `{p}` substitutes the raw text, `$p` lets the shell expand an
        // opaque value (an etag `W/"..."`, a JSON blob) without re-parsing its
        // quotes. A template that only needs the safe form should still pass.
        if !param_referenced(&f.body, p) {
            err(
                "unused-hole",
                format!(
                    "`sh {name}` param `{p}` never appears as `{{{p}}}` or `${p}` in the template"
                ),
                diags,
            );
        }
    }
    let crossed = match (rule.temporal, f.kind) {
        (Some(Temporal::Stream), ShellKind::Stream) => false,
        (Some(Temporal::Async), ShellKind::Read | ShellKind::Mutate) => false,
        _ => true,
    };
    if crossed {
        let want = match f.kind {
            ShellKind::Stream => "@stream",
            _ => "@async",
        };
        err(
            "temporal-kind-mismatch",
            format!(
                "`sh {name}` is `{}`; call it from {want}, not {}",
                shell_kind_word(f.kind),
                rule.temporal.map(temporal_word).unwrap_or("a bare rule")
            ),
            diags,
        );
    }
}

fn shell_kind_word(k: ShellKind) -> &'static str {
    match k {
        ShellKind::Read => "sh",
        ShellKind::Mutate => "sh!",
        ShellKind::Stream => "sh*",
    }
}

fn temporal_word(t: Temporal) -> &'static str {
    match t {
        Temporal::Next => "@next",
        Temporal::Async => "@async",
        Temporal::Stream => "@stream",
    }
}

/// Run both passes for a program: validate brands/anchors, check every rule, then
/// normalize the literals in place. Returns all diagnostics. The engine calls this
/// once at the start of a tick (before declare/lower). On a brand/anchor structural
/// error it returns a single error diagnostic (span 0,0) rather than panicking.
pub fn check_and_normalize(prog: &mut Program, dl_path: &str) -> Vec<TypeDiag> {
    let mut diags = Vec::new();
    // Expand `rel <name>: <shape>.` into plain RelDecls before any brand/rel table
    // is built. Idempotent for the frontend path (already expanded there); does the
    // work for the daemon `run_eval` snippet path that bypasses the frontend.
    expand_shapes(&mut prog.items, dl_path, &mut diags);
    let brands = match validate_brands(prog) {
        Ok(b) => b,
        Err(e) => {
            diags.push(TypeDiag {
                path: dl_path.to_string(),
                span: (0, 0),
                severity: Severity::Error,
                code: "brand-mismatch".into(),
                msg: e,
            });
            Brands::default()
        }
    };
    if let Err(e) = Anchors::from_program(prog) {
        diags.push(TypeDiag {
            path: dl_path.to_string(),
            span: (0, 0),
            severity: Severity::Error,
            code: "unknown-anchor".into(),
            msg: e,
        });
    }
    let rels = prog_rels(prog);
    let shell_fns: HashMap<&str, &ShellFn> = prog
        .items
        .iter()
        .filter_map(|it| {
            if let Item::Shell(f) = it {
                Some((f.name.as_str(), f))
            } else {
                None
            }
        })
        .collect();
    for item in &prog.items {
        if let Item::Rule(r) = item {
            diags.extend(check_rule_types(r, &rels, &brands, dl_path));
            check_effect(r, &shell_fns, dl_path, &mut diags);
        }
        // A query pin against an enum-branded column is checked too (rule heads and
        // facts flow through `check_rule_types`; queries do not, so cover them here).
        if let Item::Query(q) = item {
            if let Some(meta) = rels.get(&q.head.rel) {
                if q.head.terms.len() == meta.cols.len() {
                    for (i, term) in q.head.terms.iter().enumerate() {
                        if let Term::Str(s) = term {
                            let cty = col_ty(&meta.cols[i], &brands);
                            if let Some(brand) = &cty.brand {
                                enum_lit_check(
                                    brand,
                                    s,
                                    &brands,
                                    &meta.cols[i].name,
                                    dl_path,
                                    &mut diags,
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    // Every rule/query head must target a declared relation. Tables are created
    // only from `rel` decls (plus the engine's built-ins), so an undeclared head
    // would otherwise fail at execution as a raw SQLite `no such table: rel_X`.
    // Reporting it here routes a clear message through --check and the LSP.
    let builtins = crate::engine::builtin_rel_names();
    let mut reported: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for item in &prog.items {
        let (head, is_query) = match item {
            Item::Rule(r) => (&r.head.rel, false),
            Item::Query(q) => (&q.head.rel, true),
            _ => continue,
        };
        if rels.contains_key(head) || builtins.contains(head) {
            continue;
        }
        if !reported.insert(head.as_str()) {
            continue;
        }
        let role = if is_query {
            "queried"
        } else {
            "used as a rule head"
        };
        diags.push(TypeDiag {
            path: dl_path.to_string(),
            span: (0, 0),
            severity: Severity::Error,
            code: "unknown-relation".into(),
            msg: format!("relation `{head}` is {role} but never declared — add `rel {head}(...)`"),
        });
    }
    diags.extend(metavar_case_diags(prog, dl_path));
    diags.extend(stratify_diags(prog, dl_path));
    diags.extend(normalize_program(prog, dl_path));
    diags
}

/// Warn on an `sg`/`ast_yaml` pattern containing `$name` with a LOWERCASE
/// leading letter: ast-grep metavars are UPPERCASE (`$NAME`, `$$$ARGS`), so a
/// lowercase `$name` matches as LITERAL text, not a capture — the silent
/// zero-match sharp edge. Warn, not error: a literal `$` before lowercase is
/// legal (a shell snippet, a template var), so this is guidance, not a gate.
/// Surfaced through the normal TypeDiag path so --check / --lsp / --parse-only
/// all show it. Source ops carry no per-pattern byte span, so it lands at line 1
/// (span 0,0) like the brand/stratify diagnostics. One warn per distinct
/// lowercase name per pattern.
pub fn metavar_case_diags(prog: &Program, dl_path: &str) -> Vec<TypeDiag> {
    let re = regex::Regex::new(r"\$([a-z][A-Za-z0-9_]*)").unwrap();
    let mut diags = Vec::new();
    let scan = |pat: &str, diags: &mut Vec<TypeDiag>| {
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for c in re.captures_iter(pat) {
            let name = c.get(1).unwrap().as_str();
            if !seen.insert(name) {
                continue;
            }
            diags.push(TypeDiag {
                path: dl_path.to_string(), span: (0, 0),
                severity: Severity::Warn, code: "lowercase-metavar".into(),
                msg: format!(
                    "lowercase ${name} is literal text, not a capture; metavars are UPPERCASE ($NAME)"),
            });
        }
    };
    let visit = |body: &[BodyItem], diags: &mut Vec<TypeDiag>| {
        for b in body {
            match b {
                BodyItem::Sg { pattern, .. } => scan(pattern, diags),
                BodyItem::AstYaml { yaml, .. } => scan(yaml, diags),
                _ => {}
            }
        }
    };
    for item in &prog.items {
        match item {
            Item::Rule(r) => visit(&r.body, &mut diags),
            Item::Gen(g) => visit(&g.body, &mut diags),
            _ => {}
        }
    }
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
        if let Some(&i) = id.get(s) {
            return i;
        }
        let i = names.len() as u32;
        id.insert(s.to_string(), i);
        names.push(s.to_string());
        i
    };
    // (head, body, forcing) where forcing = the edge is a negation or aggregation.
    let mut edges: Vec<(u32, u32, bool)> = Vec::new();
    for item in &prog.items {
        let Item::Rule(r) = item else {
            continue;
        };
        // A temporal rule (`@next`/`@async`/`@stream`) crosses a tick boundary: its
        // head lands in a LATER tick (the next seed, or an async response), so its
        // body->head dependency does NOT hold within this tick's fixpoint. The
        // runtime excludes these rules from the derived set for exactly this reason
        // (engine.rs `all_derived`), so the static SCC graph must too — otherwise a
        // legitimate carry like `etag <- @next etag_next` with a negation elsewhere
        // in the loop false-flags as not-stratified. The tick boundary IS the
        // stratification for across-tick cycles.
        if r.temporal.is_some() {
            continue;
        }
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
    for &(h, b, _) in &edges {
        adj[h as usize].push(b);
    }
    let (comp, ncomp) = scc::tarjan(&adj);
    let mut reported: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
    for &(h, b, force) in &edges {
        if force && comp[h as usize] == comp[b as usize] && reported.insert((h, b)) {
            diags.push(TypeDiag {
                path: dl_path.to_string(),
                span: (0, 0),
                severity: Severity::Error,
                code: "not-stratified".into(),
                msg: format!(
                    "relation `{}` is aggregated or negated inside a recursive cycle with `{}`",
                    names[b as usize], names[h as usize]
                ),
            });
        }
    }

    // NULL-padded head inside a recursive cycle: a `_` head slot (explicit, or
    // named-arg padding for an unnamed column) lowers to SQL NULL, and a NULL row
    // never dedups in the fixpoint delta (NULL != NULL under INSERT OR IGNORE) —
    // the same row re-inserts every iteration and evaluation never converges.
    // Sink use (a non-recursive head like `diag`) is fine. `rebuild_derived`
    // carries the runtime defense for programs that skip the check path.
    let mut comp_size = vec![0usize; ncomp as usize];
    for &c in &comp {
        comp_size[c as usize] += 1;
    }
    let mut self_loop = vec![false; n];
    for &(h, b, _) in &edges {
        if h == b {
            self_loop[h as usize] = true;
        }
    }
    let mut null_reported: std::collections::HashSet<String> = std::collections::HashSet::new();
    for item in &prog.items {
        let Item::Rule(r) = item else {
            continue;
        };
        if r.temporal.is_some() || !r.head_null_pads() {
            continue;
        }
        let Some(&h) = id.get(&r.head.rel) else {
            continue;
        };
        let recursive = comp_size[comp[h as usize] as usize] > 1 || self_loop[h as usize];
        if recursive && null_reported.insert(r.head.rel.clone()) {
            diags.push(TypeDiag {
                path: dl_path.to_string(),
                span: (0, 0),
                severity: Severity::Error,
                code: "recursive-null-pad".into(),
                msg: format!(
                    "rule head for `{}` leaves column(s) NULL (`_` or named-arg padding) \
                    inside a recursive cycle — a NULL row never dedups in the fixpoint, so \
                    evaluation would not converge; bind every head column or break the cycle",
                    r.head.rel
                ),
            });
        }
    }

    // Agg head decl type check: Count/Sum land an Int (non-int col = mismatch);
    // the json aggregates land a Text json string (int col = mismatch). Min/Max
    // carry the arg's type (fixed_out None) and stay with per-var unification.
    for item in &prog.items {
        let Item::Rule(r) = item else {
            continue;
        };
        if !r.has_agg() {
            continue;
        }
        let Some(meta) = rels.get(&r.head.rel) else {
            continue;
        };
        if r.head.terms.len() != meta.cols.len() {
            continue;
        }
        for (i, f) in r.aggs.iter().enumerate() {
            let Some(f) = f else {
                continue;
            };
            let mismatch = match f.fixed_out() {
                // Count/Sum produce int: any non-int, non-branded column conflicts.
                Some(Type::Int) => meta.cols[i].ty != Type::Int && meta.cols[i].brand.is_none(),
                // json aggregates produce a text json string: an int column conflicts
                // (every path/text/branded column stores TEXT and is fine).
                Some(Type::Text) => meta.cols[i].ty == Type::Int,
                _ => false,
            };
            if mismatch {
                let out = f.fixed_out().map(|t| t.name()).unwrap_or("value");
                diags.push(TypeDiag {
                    path: dl_path.to_string(),
                    span: (0, 0),
                    severity: Severity::Error,
                    code: "brand-mismatch".into(),
                    msg: format!(
                        "`{}(...)` produces {} but head column `{}` of `{}` is `{}`",
                        f.sql().to_lowercase(),
                        out,
                        meta.cols[i].name,
                        r.head.rel,
                        meta.cols[i].ty.name()
                    ),
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
    // Builtin rels carrying an enum-branded column (type_edge, df_node,
    // checkout_done, ...) join the checked map so a literal pin against e.g.
    // `type_edge.kind` is vocabulary-checked. Builtins WITHOUT a branded column
    // stay out — their atoms skip type checking exactly as before (narrow blast
    // radius: only the closed-vocabulary rels opt in).
    for d in crate::engine::all_builtin_decls() {
        if d.cols.iter().any(|col| col.brand.is_some()) {
            rels.insert(
                d.name.clone(),
                RelMeta {
                    cols: d.cols.clone(),
                    ..Default::default()
                },
            );
        }
    }
    for item in &prog.items {
        if let Item::Rel(d) = item {
            rels.insert(
                d.name.clone(),
                RelMeta {
                    cols: d.cols.clone(),
                    ..Default::default()
                },
            );
        }
    }
    rels
}

#[cfg(test)]
mod shape_enum_tests {
    use super::*;

    fn program(src: &str) -> Program {
        crate::parse::parse(crate::lex::lex(src).unwrap()).unwrap()
    }
    fn diags(src: &str) -> Vec<TypeDiag> {
        let mut prog = program(src);
        check_and_normalize(&mut prog, "t.dl")
    }
    fn err_codes(src: &str) -> Vec<String> {
        diags(src)
            .into_iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| d.code)
            .collect()
    }

    #[test]
    fn enum_variants_walk_inherits() {
        let prog = program(r#"type severity = "error" | "warn"."#);
        let brands = Brands::from_program(&prog).unwrap();
        assert_eq!(
            brands.enum_variants("severity"),
            Some(&["error".to_string(), "warn".to_string()][..])
        );
        // A sub-brand inherits the parent enum's set.
        let prog = program(r#"type severity = "error" | "warn". type sev2 <: severity."#);
        let brands = Brands::from_program(&prog).unwrap();
        assert!(brands.enum_variants("sev2").is_some());
        // A plain nominal brand carries no variants.
        let prog = program("type sha <: text.");
        let brands = Brands::from_program(&prog).unwrap();
        assert!(brands.enum_variants("sha").is_none());
    }

    #[test]
    fn nearest_variant_picks_closest() {
        let vs = vec!["error".to_string(), "warn".to_string(), "info".to_string()];
        assert_eq!(nearest_variant("wrn", &vs), Some("warn"));
        assert_eq!(nearest_variant("eror", &vs), Some("error"));
        assert_eq!(edit_distance("warn", "warn"), 0);
        assert_eq!(edit_distance("wrn", "warn"), 1);
    }

    #[test]
    fn enum_literal_accept_and_reject() {
        let ok = r#"type severity = "error" | "warn".
rel finding(path: text, sev: severity).
finding("a.rs", "error").
? finding(path, "warn")."#;
        assert!(
            err_codes(ok).is_empty(),
            "valid enum literals must pass: {:?}",
            diags(ok)
        );

        let bad_head = r#"type severity = "error" | "warn".
rel finding(path: text, sev: severity).
finding("a.rs", "wrn")."#;
        assert_eq!(err_codes(bad_head), ["enum-variant-unknown"]);

        let bad_query = r#"type severity = "error" | "warn".
rel finding(path: text, sev: severity).
finding("a.rs", "error").
? finding(path, "eror")."#;
        assert_eq!(err_codes(bad_query), ["enum-variant-unknown"]);
    }

    #[test]
    fn shape_expands_and_unknown_shape_errors() {
        // A shape-referencing rel expands to the shape's columns.
        let src = r#"type finding(path: text, line: int, sev: text).
rel finding_rel: finding.
finding_rel("a.rs", 1, "x")."#;
        let mut prog = program(src);
        let ds = check_and_normalize(&mut prog, "t.dl");
        assert!(ds.iter().all(|d| d.severity != Severity::Error), "{ds:?}");
        // The rel now has the shape's 3 columns and its ref is cleared. (Item::Shape
        // is retained for the engine's shadow check — Phase 5 — so it is not
        // asserted absent anymore.)
        let rel = prog
            .items
            .iter()
            .find_map(|i| if let Item::Rel(d) = i { Some(d) } else { None })
            .unwrap();
        assert_eq!(rel.cols.len(), 3);
        assert!(rel.shape_ref.is_none());

        // An unknown shape name is an error naming the fix.
        let bad = "rel finding_rel: finding.\nfinding_rel().";
        let codes = err_codes(bad);
        assert!(codes.iter().any(|c| c == "unknown-shape"), "{codes:?}");
    }

    #[test]
    fn shape_ref_defers_when_type_decl_row_headed() {
        // A program HEADING type_decl_row derives shapes at runtime: an
        // unresolved shape_ref is deferred (kept, columns empty), not an
        // unknown-shape error — the engine resolves it from `_shapes` next tick.
        let src = r#"rel col_spec(shape: text, pos: int, col_name: text, type: text).
type_decl_row(shape, pos, col, type) <- col_spec(shape, pos, col, type).
rel point_rel: point."#;
        let mut prog = program(src);
        let ds = check_and_normalize(&mut prog, "t.dl");
        assert!(ds.iter().all(|d| d.severity != Severity::Error), "{ds:?}");
        let rel = prog
            .items
            .iter()
            .find_map(|i| match i {
                Item::Rel(d) if d.name == "point_rel" => Some(d),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            rel.shape_ref.as_deref(),
            Some("point"),
            "the ref survives for the engine"
        );
        assert!(rel.cols.is_empty(), "no columns invented at load");
    }

    #[test]
    fn ambient_builtin_brands_present_and_guarded() {
        // The builtin enum brands are present with NO user `type` decl.
        let brands = Brands::from_program(&program("rel unrelated(name: text).")).unwrap();
        let kinds = brands
            .enum_variants("type_edge_kind")
            .expect("ambient brand");
        assert!(kinds.iter().any(|k| k == "field") && kinds.iter().any(|k| k == "uses"));
        assert!(brands.enum_variants("checkout_action").is_some());
        // A user decl reusing the name is an error naming the conflict.
        let err = Brands::from_program(&program(r#"type type_edge_kind = "mine"."#)).unwrap_err();
        assert!(err.contains("shadows a built-in enum brand"), "{err}");
        // A literal pin against the builtin column routes the enum check.
        let bad = r#"hit(from_type) <- type_edge(from_type, to_type, "fields", repo).
rel hit(from_type: text)."#;
        assert_eq!(err_codes(bad), ["enum-variant-unknown"]);
    }

    #[test]
    fn plain_brand_still_checks() {
        // A `<:` brand keeps its existing mismatch behavior (int literal in a brand col).
        let bad = "type sha <: text.\nrel commit(id: sha).\ncommit(5).";
        assert!(err_codes(bad).iter().any(|c| c == "brand-mismatch"));
        let ok = "type sha <: text.\nrel commit(id: sha).\ncommit(\"abc\").";
        assert!(err_codes(ok).is_empty());
    }

    // --- body binds + `+` overload (S3/S4) ------------------------------------

    #[test]
    fn bind_unbound_rhs_var_names_the_fix() {
        // The RHS var is bound nowhere: error names both vars and the fix.
        let bad = r#"rel raw_edge(caller: text).
rel out_edge(callee: text).
out_edge(callee) <- raw_edge(caller), callee = replace(callee_q, ".", "::")."#;
        let ds = diags(bad);
        let hit = ds
            .iter()
            .find(|d| d.code == "unbound-bind")
            .expect("unbound-bind diag");
        assert!(
            hit.msg
                .contains("bind `callee_q` before computing `callee`"),
            "{}",
            hit.msg
        );

        // A LATER bind does not satisfy an earlier bind's RHS (bind chains are
        // ordered; only atom vars are order-free).
        let late = r#"rel raw_edge(caller: text).
rel out_edge(callee: text).
out_edge(callee) <- raw_edge(caller),
  callee = replace(stripped, ".", "::"),
  stripped = replace(caller, "()", "")."#;
        assert!(
            err_codes(late).contains(&"unbound-bind".to_string()),
            "{:?}",
            diags(late)
        );

        // In-order chain is clean.
        let ok = r#"rel raw_edge(caller: text).
rel out_edge(callee: text).
out_edge(callee) <- raw_edge(caller),
  stripped = replace(caller, "()", ""),
  callee = replace(stripped, ".", "::")."#;
        assert!(err_codes(ok).is_empty(), "{:?}", diags(ok));
    }

    #[test]
    fn plus_mixed_and_text_typing() {
        // int + text = plus-mismatch naming the interp/int() fix.
        let mixed = r#"rel item(name: text, count: int).
rel label(text_out: text).
label(name + count) <- item(name, count)."#;
        let ds = diags(mixed);
        let hit = ds
            .iter()
            .find(|d| d.code == "plus-mismatch")
            .expect("plus-mismatch diag");
        assert!(
            hit.msg.contains("interpolation") || hit.msg.contains("int(.."),
            "{}",
            hit.msg
        );

        // text + text is fine into a text column, an error into an int column.
        let ok = r#"rel base_url(host: text).
rel endpoint(url: text).
endpoint("https://" + host) <- base_url(host)."#;
        assert!(err_codes(ok).is_empty(), "{:?}", diags(ok));
        let bad_col = r#"rel base_url(host: text).
rel endpoint(url: int).
endpoint("https://" + host) <- base_url(host)."#;
        assert!(
            diags(bad_col)
                .iter()
                .any(|d| d.msg.contains("cannot fill int column")),
            "{:?}",
            diags(bad_col)
        );

        // int + int into an int column stays clean (regression).
        let int_ok = r#"rel hit(line: int).
rel next_line(value: int).
next_line(line + 1) <- hit(line)."#;
        assert!(err_codes(int_ok).is_empty(), "{:?}", diags(int_ok));

        // `-` stays int-only: a text operand errors.
        let sub_text = r#"rel base_url(host: text).
rel weird(value: int).
weird(host - 1) <- base_url(host)."#;
        assert!(
            diags(sub_text)
                .iter()
                .any(|d| d.code == "plus-mismatch" && d.msg.contains("needs int operands")),
            "{:?}",
            diags(sub_text)
        );
    }
}
