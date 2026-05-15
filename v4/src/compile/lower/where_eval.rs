//! Mid-stream `where` predicate evaluator.
//!
//! When a rule body contains a Stream op (`fs`, `ast`, `re`, ...) the
//! fuser picks the streamed-Rust kind and lowered Components run per
//! cursor. The fused-SQL path's predicate pushdown (see
//! `compile/fuser.rs::rewrite_pure_predicate`) does not apply; the
//! `where\`...\`` step must filter rows in Rust.
//!
//! Layer A — hand-rolled Pratt parser over `sql_where`'s lexer
//! (`scan_predicate`). Reused as `pub(crate)` to keep the predicate
//! vocabulary in lockstep with the LSP-side grammar. A separate runtime
//! lexer would drift.
//!
//! Layer B — `PredicateComponent`, a Tier-1 `Component<Cursor>` that
//! evaluates the parsed `PredExpr` against the row's terms and emits
//! `Node::Done` for false / NULL (SQL three-valued logic) or
//! `Node::Emit(cursor)` for true.
//!
//! Supported operator set (minimum viable per the plan):
//!   `=`, `!=`, `<>`, `<`, `<=`, `>`, `>=`
//!   `AND`, `OR`, `NOT`
//!   `IS NULL`, `IS NOT NULL`
//!   parens, string literals (single-quoted with `''` escape),
//!   numeric literals, `${NAME}` term refs.
//!
//! Anything past that surface (LIKE/GLOB, BETWEEN, IN, function calls,
//! arithmetic) returns a `lower/where-parse` diag. Keeps the
//! interpreter narrow; widen when a demo needs it.

use std::sync::Arc;

use effect_runtime::v2::{Component, Diag, Node, RenderCtx};

use crate::cst::dsls::sql_where::{scan_predicate, PredicateTokenKind};
use crate::Cursor;

// ── AST ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum PredExpr {
    Hole(Arc<str>),
    Lit(PredLit),
    Bin(PredOp, Box<PredExpr>, Box<PredExpr>),
    Not(Box<PredExpr>),
    IsNull(Box<PredExpr>),
    IsNotNull(Box<PredExpr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PredOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Debug, Clone)]
pub enum PredLit {
    Int(i64),
    Float(f64),
    Str(Arc<str>),
    Null,
}

// ── Parser ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Hole(Arc<str>),
    Ident(Arc<str>), // bare identifier (becomes NULL/keyword classification)
    Kw(Arc<str>),    // uppercased
    Str(Arc<str>),
    Num(Arc<str>),
    Punct(char),
    BangEq, // `!=`
    LtGt,   // `<>`
    LtEq,   // `<=`
    GtEq,   // `>=`
}

fn lex(raw: &str) -> Result<Vec<Tok>, String> {
    let bytes = raw.as_bytes();
    let tokens = scan_predicate(bytes);
    let mut out: Vec<Tok> = Vec::with_capacity(tokens.len());
    let mut i = 0;
    while i < tokens.len() {
        let t = &tokens[i];
        let slice = std::str::from_utf8(&bytes[t.range.clone()])
            .map_err(|_| "non-utf8 predicate text".to_string())?;
        match t.kind {
            PredicateTokenKind::HostHole => {
                let name = slice.trim_start_matches("${").trim_end_matches('}');
                if name.is_empty() {
                    return Err("empty `${}` in predicate".to_string());
                }
                out.push(Tok::Hole(Arc::<str>::from(name)));
            }
            PredicateTokenKind::String => {
                // Strip outer quotes; collapse `''` escape inside single-
                // quoted strings. sql_where's lexer accepts both `'` and
                // `"` quotes; treat both as string literals.
                if slice.len() < 2 {
                    return Err(format!("malformed string literal: {slice}"));
                }
                let q = slice.as_bytes()[0];
                let inner = &slice[1..slice.len() - 1];
                let unescaped = if q == b'\'' {
                    inner.replace("''", "'")
                } else {
                    inner.replace("\"\"", "\"")
                };
                out.push(Tok::Str(Arc::<str>::from(unescaped)));
            }
            PredicateTokenKind::Number => {
                out.push(Tok::Num(Arc::<str>::from(slice)));
            }
            PredicateTokenKind::Keyword => {
                out.push(Tok::Kw(Arc::<str>::from(slice.to_ascii_uppercase())));
            }
            PredicateTokenKind::Identifier => {
                // Bare identifiers are not normally legal in a predicate
                // (host holes are how captures are referenced). Keep as
                // Ident so the parser can produce a focused error.
                out.push(Tok::Ident(Arc::<str>::from(slice)));
            }
            PredicateTokenKind::Function => {
                // Functions not supported in the minimum-viable surface.
                // Parser surfaces a "function `X` not supported" error.
                out.push(Tok::Ident(Arc::<str>::from(slice)));
            }
            PredicateTokenKind::Operator => {
                let b = slice.as_bytes()[0];
                // Coalesce two-char operators by peeking the next token.
                let next = tokens.get(i + 1);
                let combined = next.and_then(|n| {
                    if n.range.start == t.range.end {
                        std::str::from_utf8(&bytes[n.range.clone()]).ok()
                    } else {
                        None
                    }
                });
                match (b, combined) {
                    (b'!', Some("=")) => {
                        out.push(Tok::BangEq);
                        i += 2;
                        continue;
                    }
                    (b'<', Some(">")) => {
                        out.push(Tok::LtGt);
                        i += 2;
                        continue;
                    }
                    (b'<', Some("=")) => {
                        out.push(Tok::LtEq);
                        i += 2;
                        continue;
                    }
                    (b'>', Some("=")) => {
                        out.push(Tok::GtEq);
                        i += 2;
                        continue;
                    }
                    _ => out.push(Tok::Punct(b as char)),
                }
            }
            PredicateTokenKind::Comment => { /* skip */ }
        }
        i += 1;
    }
    Ok(out)
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }
    fn bump(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }
    fn eat_kw(&mut self, kw: &str) -> bool {
        if matches!(self.peek(), Some(Tok::Kw(k)) if k.as_ref() == kw) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn eat_punct(&mut self, c: char) -> bool {
        if matches!(self.peek(), Some(Tok::Punct(p)) if *p == c) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
}

// Precedence climbing: OR < AND < NOT < comparison < IS NULL postfix < primary.

fn parse_expr(p: &mut Parser) -> Result<PredExpr, String> {
    parse_or(p)
}

fn parse_or(p: &mut Parser) -> Result<PredExpr, String> {
    let mut lhs = parse_and(p)?;
    while p.eat_kw("OR") {
        let rhs = parse_and(p)?;
        lhs = PredExpr::Bin(PredOp::Or, Box::new(lhs), Box::new(rhs));
    }
    Ok(lhs)
}

fn parse_and(p: &mut Parser) -> Result<PredExpr, String> {
    let mut lhs = parse_not(p)?;
    while p.eat_kw("AND") {
        let rhs = parse_not(p)?;
        lhs = PredExpr::Bin(PredOp::And, Box::new(lhs), Box::new(rhs));
    }
    Ok(lhs)
}

fn parse_not(p: &mut Parser) -> Result<PredExpr, String> {
    if p.eat_kw("NOT") {
        let inner = parse_not(p)?;
        Ok(PredExpr::Not(Box::new(inner)))
    } else {
        parse_cmp(p)
    }
}

fn parse_cmp(p: &mut Parser) -> Result<PredExpr, String> {
    let lhs = parse_postfix(p)?;
    let op = match p.peek() {
        Some(Tok::Punct('=')) => Some(PredOp::Eq),
        Some(Tok::BangEq) | Some(Tok::LtGt) => Some(PredOp::Ne),
        Some(Tok::Punct('<')) => Some(PredOp::Lt),
        Some(Tok::Punct('>')) => Some(PredOp::Gt),
        Some(Tok::LtEq) => Some(PredOp::Le),
        Some(Tok::GtEq) => Some(PredOp::Ge),
        _ => None,
    };
    if let Some(op) = op {
        p.bump();
        let rhs = parse_postfix(p)?;
        Ok(PredExpr::Bin(op, Box::new(lhs), Box::new(rhs)))
    } else {
        Ok(lhs)
    }
}

fn parse_postfix(p: &mut Parser) -> Result<PredExpr, String> {
    let inner = parse_primary(p)?;
    if p.eat_kw("IS") {
        let negate = p.eat_kw("NOT");
        if !p.eat_kw("NULL") {
            return Err("expected NULL after IS".to_string());
        }
        let node = if negate {
            PredExpr::IsNotNull(Box::new(inner))
        } else {
            PredExpr::IsNull(Box::new(inner))
        };
        return Ok(node);
    }
    Ok(inner)
}

fn parse_primary(p: &mut Parser) -> Result<PredExpr, String> {
    let t = p.bump().ok_or_else(|| "unexpected end of predicate".to_string())?;
    match t {
        Tok::Punct('(') => {
            let e = parse_expr(p)?;
            if !p.eat_punct(')') {
                return Err("expected `)`".to_string());
            }
            Ok(e)
        }
        Tok::Hole(name) => Ok(PredExpr::Hole(name)),
        Tok::Str(s) => Ok(PredExpr::Lit(PredLit::Str(s))),
        Tok::Num(n) => {
            let lit = if n.contains('.') {
                PredLit::Float(n.parse::<f64>().map_err(|e| format!("bad number `{n}`: {e}"))?)
            } else {
                PredLit::Int(n.parse::<i64>().map_err(|e| format!("bad number `{n}`: {e}"))?)
            };
            Ok(PredExpr::Lit(lit))
        }
        Tok::Kw(k) if k.as_ref() == "NULL" => Ok(PredExpr::Lit(PredLit::Null)),
        Tok::Kw(k) => Err(format!("unexpected keyword `{k}` in predicate primary")),
        Tok::Ident(s) => Err(format!(
            "bare identifier `{s}` not allowed; reference captures as `${{NAME}}`"
        )),
        Tok::Punct(c) => Err(format!("unexpected `{c}` in predicate")),
        Tok::BangEq | Tok::LtGt | Tok::LtEq | Tok::GtEq => {
            Err("unexpected comparison operator at start of expression".to_string())
        }
    }
}

pub fn parse_predicate(raw: &str) -> Result<PredExpr, String> {
    if raw.trim().is_empty() {
        return Err("empty predicate".to_string());
    }
    let toks = lex(raw)?;
    let mut p = Parser { toks, pos: 0 };
    let expr = parse_expr(&mut p)?;
    if p.pos != p.toks.len() {
        return Err(format!(
            "trailing tokens at predicate position {} ({} tokens unconsumed)",
            p.pos,
            p.toks.len() - p.pos,
        ));
    }
    Ok(expr)
}

// ── Interpreter ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum PredVal {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(Arc<str>),
    Null,
}

fn coerce_term(text: &str) -> PredVal {
    // SQLite-ish affinity: integer if all digits (with optional leading
    // `-`); float if it has a `.` and parses; otherwise string. Empty
    // string is NULL-ish here (cursor.get returning Some("") still maps
    // to Str("")); the explicit "missing" case is handled by the Hole
    // branch upstream.
    if let Ok(n) = text.parse::<i64>() {
        return PredVal::Int(n);
    }
    if text.contains('.') {
        if let Ok(f) = text.parse::<f64>() {
            return PredVal::Float(f);
        }
    }
    PredVal::Str(Arc::<str>::from(text))
}

fn coerce_lit(l: &PredLit) -> PredVal {
    match l {
        PredLit::Int(n) => PredVal::Int(*n),
        PredLit::Float(f) => PredVal::Float(*f),
        PredLit::Str(s) => PredVal::Str(s.clone()),
        PredLit::Null => PredVal::Null,
    }
}

fn eval_value(e: &PredExpr, c: &Cursor) -> PredVal {
    match e {
        PredExpr::Hole(name) => match c.get(name.as_ref()) {
            Some(s) => coerce_term(s),
            None => PredVal::Null,
        },
        PredExpr::Lit(l) => coerce_lit(l),
        PredExpr::Not(inner) => match to_bool(eval_value(inner, c)) {
            Some(b) => PredVal::Bool(!b),
            None => PredVal::Null,
        },
        PredExpr::IsNull(inner) => PredVal::Bool(matches!(eval_value(inner, c), PredVal::Null)),
        PredExpr::IsNotNull(inner) => PredVal::Bool(!matches!(eval_value(inner, c), PredVal::Null)),
        PredExpr::Bin(op, a, b) => apply_bin(*op, eval_value(a, c), eval_value(b, c)),
    }
}

fn apply_bin(op: PredOp, a: PredVal, b: PredVal) -> PredVal {
    match op {
        PredOp::And => {
            let av = to_bool(a);
            let bv = to_bool(b);
            // SQL three-valued AND: false dominates, NULL is "unknown".
            match (av, bv) {
                (Some(false), _) | (_, Some(false)) => PredVal::Bool(false),
                (Some(true), Some(true)) => PredVal::Bool(true),
                _ => PredVal::Null,
            }
        }
        PredOp::Or => {
            let av = to_bool(a);
            let bv = to_bool(b);
            match (av, bv) {
                (Some(true), _) | (_, Some(true)) => PredVal::Bool(true),
                (Some(false), Some(false)) => PredVal::Bool(false),
                _ => PredVal::Null,
            }
        }
        PredOp::Eq | PredOp::Ne | PredOp::Lt | PredOp::Le | PredOp::Gt | PredOp::Ge => {
            // Any NULL operand → NULL (SQL semantics; IS NULL is separate).
            if matches!(a, PredVal::Null) || matches!(b, PredVal::Null) {
                return PredVal::Null;
            }
            let ord = compare(&a, &b);
            let v = match op {
                PredOp::Eq => ord == std::cmp::Ordering::Equal,
                PredOp::Ne => ord != std::cmp::Ordering::Equal,
                PredOp::Lt => ord == std::cmp::Ordering::Less,
                PredOp::Le => ord != std::cmp::Ordering::Greater,
                PredOp::Gt => ord == std::cmp::Ordering::Greater,
                PredOp::Ge => ord != std::cmp::Ordering::Less,
                _ => unreachable!(),
            };
            PredVal::Bool(v)
        }
    }
}

fn compare(a: &PredVal, b: &PredVal) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    // Numeric-vs-numeric uses float promotion; numeric-vs-string falls
    // through to string compare on each operand's display form. That
    // mirrors what the fused path's SQLite affinity would observe for
    // a column declared with no type affinity.
    match (a, b) {
        (PredVal::Int(x), PredVal::Int(y)) => x.cmp(y),
        (PredVal::Float(x), PredVal::Float(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        (PredVal::Int(x), PredVal::Float(y)) => {
            (*x as f64).partial_cmp(y).unwrap_or(Ordering::Equal)
        }
        (PredVal::Float(x), PredVal::Int(y)) => {
            x.partial_cmp(&(*y as f64)).unwrap_or(Ordering::Equal)
        }
        (PredVal::Str(x), PredVal::Str(y)) => x.as_ref().cmp(y.as_ref()),
        (PredVal::Bool(x), PredVal::Bool(y)) => x.cmp(y),
        // Mixed numeric/string: try parsing the string as a number first;
        // if that fails fall back to stringifying both sides.
        (PredVal::Str(s), other) | (other, PredVal::Str(s)) => {
            let parsed = coerce_term(s);
            if matches!(parsed, PredVal::Str(_)) {
                // Both sides as strings.
                let l = display_value(a);
                let r = display_value(b);
                l.cmp(&r)
            } else {
                let l = if let PredVal::Str(_) = a {
                    parsed.clone()
                } else {
                    a.clone()
                };
                let r = if let PredVal::Str(_) = b {
                    parsed
                } else {
                    other.clone()
                };
                compare(&l, &r)
            }
        }
        _ => Ordering::Equal,
    }
}

fn display_value(v: &PredVal) -> String {
    match v {
        PredVal::Bool(b) => b.to_string(),
        PredVal::Int(i) => i.to_string(),
        PredVal::Float(f) => f.to_string(),
        PredVal::Str(s) => s.to_string(),
        PredVal::Null => String::new(),
    }
}

fn to_bool(v: PredVal) -> Option<bool> {
    match v {
        PredVal::Bool(b) => Some(b),
        PredVal::Int(n) => Some(n != 0),
        PredVal::Float(f) => Some(f != 0.0),
        PredVal::Str(s) => Some(!s.is_empty()),
        PredVal::Null => None,
    }
}

fn eval_pred(e: &PredExpr, c: &Cursor) -> bool {
    matches!(to_bool(eval_value(e, c)), Some(true))
}

// ── Component ───────────────────────────────────────────────────────

pub struct PredicateComponent {
    expr: PredExpr,
    /// Source text retained for kind/debug output. Stable across clones.
    #[allow(dead_code)]
    raw: Arc<str>,
}

impl PredicateComponent {
    pub fn new(expr: PredExpr, raw: Arc<str>) -> Self {
        Self { expr, raw }
    }
}

impl Component for PredicateComponent {
    type Next = Cursor;

    fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        if eval_pred(&self.expr, c) {
            Node::Emit(Arc::new(c.clone()))
        } else {
            Node::Done
        }
    }

    fn kind(&self) -> &'static str {
        "where"
    }
}

/// Build a `lower/where-parse` diag from a parse error message.
pub fn parse_diag(raw: &str, err: &str) -> Diag {
    Diag::error(
        "lower/where-parse",
        format!("where`{}`: {err}", raw.trim()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(raw: &str) -> PredExpr {
        parse_predicate(raw).expect("parse")
    }

    fn cur(pairs: &[(&str, &str)]) -> Cursor {
        let mut c = Cursor::default();
        for (k, v) in pairs {
            c.set(k, *v);
        }
        c
    }

    #[test]
    fn parse_eq_str_literal() {
        let e = p("${X} = 'main'");
        match e {
            PredExpr::Bin(PredOp::Eq, _, _) => {}
            _ => panic!("expected Eq, got {e:?}"),
        }
    }

    #[test]
    fn eval_eq_drops_non_matches() {
        let e = p("${X} = 'main'");
        assert!(eval_pred(&e, &cur(&[("X", "main")])));
        assert!(!eval_pred(&e, &cur(&[("X", "other")])));
    }

    #[test]
    fn eval_ne_via_bang_eq_and_ltgt() {
        let e1 = p("${X} != 'main'");
        let e2 = p("${X} <> 'main'");
        assert!(eval_pred(&e1, &cur(&[("X", "other")])));
        assert!(!eval_pred(&e2, &cur(&[("X", "main")])));
    }

    #[test]
    fn eval_numeric_compare() {
        let e = p("${LO} > 100");
        assert!(eval_pred(&e, &cur(&[("LO", "200")])));
        assert!(!eval_pred(&e, &cur(&[("LO", "50")])));
    }

    #[test]
    fn eval_or_and_combinations() {
        let e = p("${X} = 'foo' OR ${X} = 'bar'");
        assert!(eval_pred(&e, &cur(&[("X", "foo")])));
        assert!(eval_pred(&e, &cur(&[("X", "bar")])));
        assert!(!eval_pred(&e, &cur(&[("X", "baz")])));

        let e2 = p("${X} = 'foo' AND ${Y} > 5");
        assert!(eval_pred(&e2, &cur(&[("X", "foo"), ("Y", "10")])));
        assert!(!eval_pred(&e2, &cur(&[("X", "foo"), ("Y", "1")])));
    }

    #[test]
    fn eval_is_null_and_not_null() {
        let e1 = p("${MAYBE} IS NULL");
        let e2 = p("${MAYBE} IS NOT NULL");
        assert!(eval_pred(&e1, &cur(&[])));
        assert!(!eval_pred(&e1, &cur(&[("MAYBE", "x")])));
        assert!(!eval_pred(&e2, &cur(&[])));
        assert!(eval_pred(&e2, &cur(&[("MAYBE", "x")])));
    }

    #[test]
    fn eval_not() {
        let e = p("NOT ${X} = 'main'");
        assert!(!eval_pred(&e, &cur(&[("X", "main")])));
        assert!(eval_pred(&e, &cur(&[("X", "other")])));
    }

    #[test]
    fn eval_parens_change_precedence() {
        let e = p("(${X} = 'a' OR ${X} = 'b') AND ${Y} = 'z'");
        assert!(eval_pred(&e, &cur(&[("X", "a"), ("Y", "z")])));
        assert!(!eval_pred(&e, &cur(&[("X", "a"), ("Y", "q")])));
        assert!(!eval_pred(&e, &cur(&[("X", "c"), ("Y", "z")])));
    }

    #[test]
    fn eval_null_propagates_three_valued() {
        // Missing term → NULL → predicate falsified.
        let e = p("${MISSING} != 'x'");
        assert!(!eval_pred(&e, &cur(&[])));
    }

    #[test]
    fn unparseable_dsl_errors() {
        let err = parse_predicate("${X} =$$$").unwrap_err();
        assert!(err.contains("predicate") || err.contains("unexpected"), "{err}");
    }

    #[test]
    fn empty_predicate_errors() {
        assert!(parse_predicate("").is_err());
        assert!(parse_predicate("   ").is_err());
    }
}
