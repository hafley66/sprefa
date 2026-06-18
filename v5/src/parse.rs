use anyhow::{bail, Result};

use crate::ast::*;
use crate::lex::Tok;

pub struct Parser { toks: Vec<Tok>, i: usize }

pub fn parse(toks: Vec<Tok>) -> Result<Program> {
    let mut p = Parser { toks, i: 0 };
    let mut prog = Program::default();
    while p.peek().is_some() {
        prog.items.push(p.item()?);
    }
    Ok(prog)
}

/// Parse the EXTENDED module surface. Same loop as `parse`, but a top-level
/// `use "path".` produces `SurfaceItem::Use`; every other item is wrapped
/// unchanged as `SurfaceItem::Core`. The frontend's `expand` is the only place
/// the surface disappears into the frozen core IR. The lexer does not need a
/// `use` keyword: it lexes as `Tok::Ident("use")` followed by `Tok::Str(p)`,
/// which this loop recognizes.
pub fn parse_surface(toks: Vec<Tok>) -> Result<Vec<SurfaceItem>> {
    let mut p = Parser { toks, i: 0 };
    let mut out = Vec::new();
    while p.peek().is_some() {
        out.push(p.surface_item()?);
    }
    Ok(out)
}

impl Parser {
    fn peek(&self) -> Option<&Tok> { self.toks.get(self.i) }
    fn peek2(&self) -> Option<&Tok> { self.toks.get(self.i + 1) }
    fn next(&mut self) -> Result<Tok> {
        let t = self.toks.get(self.i).cloned();
        self.i += 1;
        t.ok_or_else(|| anyhow::anyhow!("unexpected end of input"))
    }
    fn expect(&mut self, want: Tok) -> Result<()> {
        let got = self.next()?;
        if got != want { bail!("expected {:?}, got {:?}", want, got); }
        Ok(())
    }
    fn ident(&mut self) -> Result<String> {
        match self.next()? {
            Tok::Ident(s) => Ok(s),
            other => bail!("expected identifier, got {:?}", other),
        }
    }

    /// One surface-level form. `use "path".` -> `Use`; `def name(params) <- ...`
    /// -> `Def`; everything else falls through to the core item parser and is
    /// wrapped as `Core`. Lookaheads:
    ///   - `use` followed by `Tok::Str` is the module import (`use "path".`).
    ///     A `rel use(...)` still parses as a rel because the second token is
    ///     `(`, not a string.
    ///   - `def` followed by an ident is a template (`def name(p) <- ...`).
    ///     A rule whose head rel is literally `def` (`def(...) <- ...`) still
    ///     parses as a rule because the second token is `(`, not an ident.
    fn surface_item(&mut self) -> Result<SurfaceItem> {
        if let Some(Tok::Ident(s)) = self.peek() {
            if s == "use" && matches!(self.peek2(), Some(Tok::Str(_))) {
                let path = self.use_path()?;
                return Ok(SurfaceItem::Use(Import { path }));
            }
            if s == "def" && matches!(self.peek2(), Some(Tok::Ident(_))) {
                let tpl = self.def_template()?;
                return Ok(SurfaceItem::Def(tpl));
            }
        }
        Ok(SurfaceItem::Core(self.item()?))
    }

    /// `def name(p1, p2) <- body.` — a parameterized rule template. The body
    /// is the same comma-separated form as a rule body; the params are idents
    /// the body references by `Term::Var`. At each call site
    /// (`name(args)` as a body atom) the frontend inlines a clone of the body
    /// with params substituted by args and non-param internal vars
    /// alpha-renamed.
    fn def_template(&mut self) -> Result<RuleTemplate> {
        self.ident()?; // "def"
        let name = self.ident()?;
        self.expect(Tok::LParen)?;
        let mut params = Vec::new();
        if !matches!(self.peek(), Some(Tok::RParen)) {
            loop {
                params.push(self.ident()?);
                match self.next()? {
                    Tok::Comma => continue,
                    Tok::RParen => break,
                    other => bail!("expected , or ) in def params, got {:?}", other),
                }
            }
        }
        self.expect(Tok::Arrow)?;
        let mut body = Vec::new();
        loop {
            body.push(self.body_item()?);
            match self.next()? {
                Tok::Comma => continue,
                Tok::Dot => break,
                other => bail!("expected , or . in def body, got {:?}", other),
            }
        }
        Ok(RuleTemplate { name, params, body })
    }

    /// `use "path".` — the string literal is the module path, resolved against
    /// the loader's include roots. Stricter than the rest of the grammar: only
    /// a literal string (not a var) is accepted, since `use` is compile-time.
    fn use_path(&mut self) -> Result<String> {
        self.ident()?; // "use"
        let path = match self.next()? {
            Tok::Str(s) => s,
            other => bail!("`use` expects a string literal, got {:?}", other),
        };
        self.expect(Tok::Dot)?;
        Ok(path)
    }

    fn item(&mut self) -> Result<Item> {
        match self.peek() {
            Some(Tok::Ident(s)) if s == "rel" => Ok(Item::Rel(self.rel_decl()?)),
            Some(Tok::Ident(s)) if s == "anchor" => Ok(Item::Anchor(self.anchor_decl()?)),
            Some(Tok::Ident(s)) if s == "type" => Ok(Item::Brand(self.brand_decl()?)),
            Some(Tok::Ident(s)) if s == "gen" => Ok(Item::Gen(self.gen_rule()?)),
            Some(Tok::Question) => Ok(Item::Query(self.query()?)),
            _ => Ok(Item::Rule(self.rule()?)),
        }
    }

    /// `anchor <name> = fs:<body>.` `<name>` is `~` or an ident.
    fn anchor_decl(&mut self) -> Result<AnchorDecl> {
        self.ident()?; // "anchor"
        let name = match self.peek() {
            // `~` lexes as Glob only when doubled; a lone `~` errors in the lexer,
            // so the default-anchor name is written as the bare ident `tilde`-free
            // form is impossible. Accept an ident name only; `~` default is implicit.
            Some(Tok::Ident(_)) => self.ident()?,
            other => bail!("anchor name must be an identifier, got {other:?}"),
        };
        self.expect(Tok::Eq)?;
        let (body, span) = match self.next()? {
            Tok::Scheme { scheme, body, span } => {
                if scheme != "fs" { bail!("anchor must be an `fs:` literal, got `{scheme}:`"); }
                (body, span)
            }
            other => bail!("anchor must be assigned an `fs:` literal, got {other:?}"),
        };
        self.expect(Tok::Dot)?;
        Ok(AnchorDecl { name, body, span })
    }

    /// `type <ident> <: <parent>.`
    fn brand_decl(&mut self) -> Result<BrandDecl> {
        self.ident()?; // "type"
        let name = self.ident()?;
        self.expect(Tok::Lt2)?;
        let parent = self.ident()?;
        self.expect(Tok::Dot)?;
        Ok(BrandDecl { name, parent })
    }

    fn rel_decl(&mut self) -> Result<RelDecl> {
        self.ident()?; // "rel"
        let name = self.ident()?;
        self.expect(Tok::LParen)?;
        let mut cols = Vec::new();
        loop {
            let cname = self.ident()?;
            self.expect(Tok::Colon)?;
            let tname = self.ident()?;
            // A keyword that is not a base type is taken as a brand reference; its
            // base storage type is resolved from the `type X <: Y` chain at load
            // (brands store as text until then). check_rule_types uses the name.
            let col = match Type::parse(&tname) {
                Some(ty) => Col { name: cname, ty, brand: None },
                None => Col { name: cname, ty: Type::Text, brand: Some(tname) },
            };
            cols.push(col);
            match self.next()? {
                Tok::Comma => continue,
                Tok::RParen => break,
                other => bail!("expected , or ) in rel decl, got {:?}", other),
            }
        }
        self.expect(Tok::Dot)?;
        Ok(RelDecl { name, cols })
    }

    fn rule(&mut self) -> Result<Rule> {
        let (head, aggs) = self.head_atom()?;
        // ground fact: `slide(1, "intro").` is a rule with an empty body; the
        // head must be all literals (lowering rejects unbound vars)
        match self.next()? {
            Tok::Dot => {
                if aggs.iter().any(|a| a.is_some()) {
                    bail!("aggregate not allowed in a fact head");
                }
                return Ok(Rule { head, body: Vec::new(), aggs });
            }
            Tok::Arrow => {}
            other => bail!("expected <- or . after rule head, got {:?}", other),
        }
        let mut body = Vec::new();
        loop {
            body.push(self.body_item()?);
            match self.next()? {
                Tok::Comma => continue,
                Tok::Dot => break,
                other => bail!("expected , or . in rule body, got {:?}", other),
            }
        }
        Ok(Rule { head, body, aggs })
    }

    /// Parse a rule head, allowing aggregate calls in term positions:
    /// `fan_out(F, count(T)) <- ...`. Returns the head atom (the aggregate's arg
    /// flows in as the term) plus a parallel `aggs` vec marking which terms are
    /// aggregated. Aggregates are head-position only; a body `count(...)` parses as
    /// a relation atom against a relation named `count` and is rejected at lowering
    /// if no such relation exists (an agg call in the body is never special).
    fn head_atom(&mut self) -> Result<(Atom, Vec<Option<AggFn>>)> {
        let rel = self.ident()?;
        self.expect(Tok::LParen)?;
        let mut terms = Vec::new();
        let mut aggs: Vec<Option<AggFn>> = Vec::new();
        if !matches!(self.peek(), Some(Tok::RParen)) {
            loop {
                // An `aggfn(arg)` call: a known aggregate name immediately followed
                // by `(`. Otherwise a plain term.
                let is_agg = matches!((self.peek(), self.peek2()),
                    (Some(Tok::Ident(s)), Some(Tok::LParen)) if AggFn::parse(s).is_some());
                if is_agg {
                    let fname = self.ident()?;
                    let f = AggFn::parse(&fname).unwrap();
                    self.expect(Tok::LParen)?;
                    let arg = self.term()?;
                    self.expect(Tok::RParen)?;
                    terms.push(arg);
                    aggs.push(Some(f));
                } else {
                    terms.push(self.expr()?);
                    aggs.push(None);
                }
                match self.next()? {
                    Tok::Comma => continue,
                    Tok::RParen => break,
                    other => bail!("expected , or ) in atom, got {:?}", other),
                }
            }
        } else {
            self.next()?; // RParen
        }
        if !aggs.iter().any(|a| a.is_some()) { aggs.clear(); }
        Ok((Atom { rel, terms }, aggs))
    }

    fn query(&mut self) -> Result<Query> {
        self.expect(Tok::Question)?;
        let head = self.atom()?;
        if matches!(self.peek(), Some(Tok::Ident(s)) if s == "where") {
            bail!("`where` was removed. Filter by nesting: pin a column with a \
                   literal head term (`? rel(\"X\", y).`), or derive a filtered \
                   relation and query it (`r(...) <- rel(...), col =~ \"...\". ? r(...).`).");
        }
        self.expect(Tok::Dot)?;
        Ok(Query { head })
    }

    fn atom(&mut self) -> Result<Atom> {
        let rel = self.ident()?;
        self.expect(Tok::LParen)?;
        let mut terms = Vec::new();
        if !matches!(self.peek(), Some(Tok::RParen)) {
            loop {
                terms.push(self.term()?);
                match self.next()? {
                    Tok::Comma => continue,
                    Tok::RParen => break,
                    other => bail!("expected , or ) in atom, got {:?}", other),
                }
            }
        } else {
            self.next()?; // RParen
        }
        Ok(Atom { rel, terms })
    }

    fn body_item(&mut self) -> Result<BodyItem> {
        if matches!(self.peek(), Some(Tok::Bang)) {
            self.next()?;
            return Ok(BodyItem::Neg(self.atom()?));
        }
        // scan / match / ast / json builtins
        if let Some(Tok::Ident(s)) = self.peek() {
            if s == "scan" { return self.scan(); }
            if s == "match" { return self.match_(); }
            if s == "ast" { return self.ast(); }
            if s == "sg" { return self.sg(); }
            if s == "json" { return self.json(); }
            if s == "cmd" { return self.cmd(); }
            if s == "comment" { return self.comment(); }
            if s == "closure" { return self.closure(); }
            // An aggregate call in body position is a parse error: aggregation is
            // head-only (`fan_out(F, count(T)) <- type_edge(F, T, _).`).
            if AggFn::parse(s).is_some() && matches!(self.peek2(), Some(Tok::LParen)) {
                bail!("aggregate `{s}(...)` is only allowed in a rule head, not the body");
            }
            // relation atom vs constraint: lookahead for '('
            if matches!(self.peek2(), Some(Tok::LParen)) {
                return Ok(BodyItem::Pos(self.atom()?));
            }
        }
        Ok(BodyItem::Cmp(self.constraint()?))
    }

    fn scan(&mut self) -> Result<BodyItem> {
        self.ident()?; // scan
        self.expect(Tok::LParen)?;
        // Comma-separated terms. 3-ary `scan(glob, path, rev_out)` defaults the
        // repo to "." (self) and rev to WORK (live disk); 4-ary
        // `scan(rev, glob, path, rev_out)` defaults the repo to "."; 5-ary
        // `scan(repo, rev, glob, path, rev_out)` names a repo coordinate
        // (slug / path / ".") that flows as a value.
        let mut terms = vec![self.term()?];
        while matches!(self.peek(), Some(Tok::Comma)) {
            self.next()?; // ,
            terms.push(self.term()?);
        }
        self.expect(Tok::RParen)?;
        let (repo, rev, glob, path, rev_out) = match terms.len() {
            3 => { let mut t = terms.into_iter();
                (Term::Str(".".into()), Term::Str("WORK".into()), t.next().unwrap(), t.next().unwrap(), t.next().unwrap()) }
            4 => { let mut t = terms.into_iter();
                (Term::Str(".".into()), t.next().unwrap(), t.next().unwrap(), t.next().unwrap(), t.next().unwrap()) }
            5 => { let mut t = terms.into_iter();
                (t.next().unwrap(), t.next().unwrap(), t.next().unwrap(), t.next().unwrap(), t.next().unwrap()) }
            n => bail!("scan expects 3 args (glob, path, rev), 4 (rev, glob, path, rev), or 5 (repo, rev, glob, path, rev), got {n}"),
        };
        Ok(BodyItem::Scan { repo, rev, glob, path, rev_out })
    }

    fn match_(&mut self) -> Result<BodyItem> {
        self.ident()?; // match
        self.expect(Tok::LParen)?;
        let path = self.term()?; self.expect(Tok::Comma)?;
        let rev = self.term()?; self.expect(Tok::Comma)?;
        let regex = match self.next()? {
            Tok::Regex(r) => desugar_regex_holes(&r),
            other => bail!("expected regex literal in match, got {:?}", other),
        };
        self.expect(Tok::Comma)?;
        let line = self.term()?;
        self.expect(Tok::RParen)?;
        Ok(BodyItem::Match { path, rev, regex, line })
    }

    fn ast(&mut self) -> Result<BodyItem> {
        self.ident()?; // ast
        self.expect(Tok::LParen)?;
        let path = self.term()?; self.expect(Tok::Comma)?;
        let rev = self.term()?; self.expect(Tok::Comma)?;
        self.expect(Tok::Colon)?;
        let lang = self.ident()?;
        self.expect(Tok::Comma)?;
        let query = match self.next()? {
            Tok::Str(s) => s,
            other => bail!("expected query string in ast(), got {:?}", other),
        };
        self.expect(Tok::Comma)?;
        let line = self.term()?;
        // optional 6th term binds the match's end line (for body-span queries)
        let end = if matches!(self.peek(), Some(Tok::Comma)) {
            self.next()?; Some(self.term()?)
        } else { None };
        self.expect(Tok::RParen)?;
        Ok(BodyItem::Ast { path, rev, lang, query, line, end })
    }

    fn sg(&mut self) -> Result<BodyItem> {
        self.ident()?; // sg
        self.expect(Tok::LParen)?;
        let path = self.term()?; self.expect(Tok::Comma)?;
        let rev = self.term()?; self.expect(Tok::Comma)?;
        self.expect(Tok::Colon)?;
        let lang = self.ident()?;
        self.expect(Tok::Comma)?;
        let pattern = match self.next()? {
            Tok::Str(s) => s,
            other => bail!("expected pattern string in sg(), got {:?}", other),
        };
        self.expect(Tok::Comma)?;
        let line = self.term()?;
        // optional trailing span binds: sg(.., line [, col [, end_line [, end_col]]]).
        // 0-based byte columns, 1-based lines. Each only parsed if the prior is present.
        let col = if matches!(self.peek(), Some(Tok::Comma)) { self.next()?; Some(self.term()?) } else { None };
        let end_line = if col.is_some() && matches!(self.peek(), Some(Tok::Comma)) { self.next()?; Some(self.term()?) } else { None };
        let end_col = if end_line.is_some() && matches!(self.peek(), Some(Tok::Comma)) { self.next()?; Some(self.term()?) } else { None };
        self.expect(Tok::RParen)?;
        Ok(BodyItem::Sg { path, rev, lang, pattern, line, col, end_line, end_col })
    }

    fn json(&mut self) -> Result<BodyItem> {
        self.ident()?; // json
        self.expect(Tok::LParen)?;
        let path = self.term()?; self.expect(Tok::Comma)?;
        let rev = self.term()?; self.expect(Tok::Comma)?;
        let jpath = match self.next()? {
            Tok::Str(s) => s,
            other => bail!("expected json path string in json(), got {:?}", other),
        };
        self.expect(Tok::Comma)?;
        let out = self.term()?;
        self.expect(Tok::RParen)?;
        Ok(BodyItem::Json { path, rev, jpath, out })
    }

    fn cmd(&mut self) -> Result<BodyItem> {
        self.ident()?; // cmd
        self.expect(Tok::LParen)?;
        let path = self.term()?; self.expect(Tok::Comma)?;
        let rev = self.term()?; self.expect(Tok::Comma)?;
        let template = match self.next()? {
            Tok::Str(s) => s,
            other => bail!("expected command string in cmd(), got {:?}", other),
        };
        self.expect(Tok::Comma)?;
        let line = self.term()?;
        self.expect(Tok::Comma)?;
        let out = self.term()?;
        self.expect(Tok::RParen)?;
        Ok(BodyItem::Cmd { path, rev, template, line, out })
    }

    /// `gen("path", "tmpl") <- body.` (file form) or
    /// `gen(p, l0, l1, "tmpl") <- body.` (splice form). `gen` is a reserved
    /// head; the body parses like any rule body.
    fn gen_rule(&mut self) -> Result<GenRule> {
        self.ident()?; // gen
        self.expect(Tok::LParen)?;
        let mut args: Vec<Term> = vec![self.term()?];
        while matches!(self.peek(), Some(Tok::Comma)) {
            self.next()?;
            args.push(self.term()?);
        }
        self.expect(Tok::RParen)?;
        let tmpl_of = |t: Term| -> Result<String> {
            match t {
                Term::Str(s) => Ok(s),
                other => bail!("gen expects a template string here, got {other:?}"),
            }
        };
        let (target, row_tmpl) = match args.len() {
            2 => {
                let mut it = args.into_iter();
                let path_tmpl = tmpl_of(it.next().unwrap())?;
                (GenTarget::File { path_tmpl }, tmpl_of(it.next().unwrap())?)
            }
            4 => {
                let mut it = args.into_iter();
                let (path, l0, l1) = (it.next().unwrap(), it.next().unwrap(), it.next().unwrap());
                (GenTarget::Splice { path, l0, l1 }, tmpl_of(it.next().unwrap())?)
            }
            n => bail!("gen expects 2 args (\"path\", \"tmpl\") or 4 (p, l0, l1, \"tmpl\"), got {n}"),
        };
        self.expect(Tok::Arrow)?;
        let mut body = Vec::new();
        loop {
            body.push(self.body_item()?);
            match self.next()? {
                Tok::Comma => continue,
                Tok::Dot => break,
                other => bail!("expected , or . in gen body, got {:?}", other),
            }
        }
        Ok(GenRule { target, row_tmpl, body })
    }

    /// `comment(p, rev, /open/[, /close/], l0, l1, label)` — one regex is
    /// sequential mode, two is paired. Both regexes take `$NAME` holes.
    fn comment(&mut self) -> Result<BodyItem> {
        self.ident()?; // comment
        self.expect(Tok::LParen)?;
        let path = self.term()?; self.expect(Tok::Comma)?;
        let rev = self.term()?; self.expect(Tok::Comma)?;
        let open = match self.next()? {
            Tok::Regex(r) => desugar_regex_holes(&r),
            other => bail!("expected open regex literal in comment(), got {:?}", other),
        };
        self.expect(Tok::Comma)?;
        let close = if matches!(self.peek(), Some(Tok::Regex(_))) {
            let Tok::Regex(r) = self.next()? else { unreachable!() };
            self.expect(Tok::Comma)?;
            Some(desugar_regex_holes(&r))
        } else { None };
        let l0 = self.term()?; self.expect(Tok::Comma)?;
        let l1 = self.term()?; self.expect(Tok::Comma)?;
        let label = self.term()?;
        self.expect(Tok::RParen)?;
        Ok(BodyItem::Comment { path, rev, open, close, l0, l1, label })
    }

    fn closure(&mut self) -> Result<BodyItem> {
        self.ident()?; // closure
        self.expect(Tok::LParen)?;
        let rel = self.ident()?;
        self.expect(Tok::RParen)?;
        Ok(BodyItem::Closure { rel })
    }

    fn constraint(&mut self) -> Result<Constraint> {
        let lhs = self.expr()?;
        let op = match self.next()? {
            Tok::Eq => CmpOp::Eq, Tok::Ne => CmpOp::Ne,
            Tok::Lt => CmpOp::Lt, Tok::Le => CmpOp::Le,
            Tok::Gt => CmpOp::Gt, Tok::Ge => CmpOp::Ge,
            Tok::Match => CmpOp::Match, Tok::Glob => CmpOp::Glob,
            other => bail!("expected comparison operator, got {:?}", other),
        };
        let rhs = self.expr()?;
        Ok(Constraint { lhs, op, rhs })
    }

    /// Int arithmetic over terms, in non-binding positions only (rule heads and
    /// comparison sides): `expr := mul (('+'|'-') mul)*`. Body atoms keep plain
    /// `term()` — an operator there is a parse error, since an atom position
    /// binds rather than computes.
    fn expr(&mut self) -> Result<Term> {
        let mut lhs = self.mul_expr()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Plus) => ArithOp::Add,
                Some(Tok::Minus) => ArithOp::Sub,
                _ => return Ok(lhs),
            };
            self.next()?;
            let rhs = self.mul_expr()?;
            lhs = Term::Arith { op, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
    }

    /// `mul := term (('*'|'/'|'%') term)*` — binds tighter than +/-.
    fn mul_expr(&mut self) -> Result<Term> {
        let mut lhs = self.term()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Star) => ArithOp::Mul,
                Some(Tok::Slash) => ArithOp::Div,
                Some(Tok::Percent) => ArithOp::Mod,
                _ => return Ok(lhs),
            };
            self.next()?;
            let rhs = self.term()?;
            lhs = Term::Arith { op, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
    }

    fn term(&mut self) -> Result<Term> {
        match self.next()? {
            Tok::Ident(s) => Ok(if s == "_" { Term::Wild } else { Term::Var(s) }),
            Tok::Str(s) => Ok(Term::Str(s)),
            Tok::InterpStr(parts) => Ok(Term::Interp(parts.into_iter().map(|p| match p {
                crate::lex::StrPart::Lit(s) => InterpPart::Lit(s),
                crate::lex::StrPart::Var(v) => InterpPart::Var(v),
            }).collect())),
            Tok::Int(n) => Ok(Term::Int(n)),
            Tok::Scheme { scheme, body, span } => Ok(Term::PathLit { scheme, body, span }),
            // Parenthesized sub-expression: `(a + b) * 2`.
            Tok::LParen => {
                let inner = self.expr()?;
                self.expect(Tok::RParen)?;
                Ok(inner)
            }
            other => bail!("expected term, got {:?}", other),
        }
    }
}

/// Desugar `$NAME` holes in a /regex/ literal to lazy named capture groups:
/// `/TODO\($WHO\)/` becomes `TODO\((?P<WHO>.*?)\)`. `\$` (escaped), a bare `$`
/// (the EOL anchor), and `$1`-style digit tails pass through untouched. Runs at
/// parse time so the rule digest, typecheck, and the engine all see the
/// desugared form.
fn desugar_regex_holes(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut chars = src.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            out.push(c);
            if let Some(c2) = chars.next() { out.push(c2); }
        } else if c == '$' {
            let mut name = String::new();
            while let Some(&c2) = chars.peek() {
                if c2.is_ascii_alphanumeric() || c2 == '_' { name.push(c2); chars.next(); } else { break; }
            }
            if name.is_empty() || name.as_bytes()[0].is_ascii_digit() {
                out.push('$');
                out.push_str(&name);
            } else {
                out.push_str("(?P<");
                out.push_str(&name);
                out.push_str(">.*?)");
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::desugar_regex_holes;

    #[test]
    fn holes_anchors_and_escapes() {
        assert_eq!(desugar_regex_holes(r"TODO\($WHO\)"), r"TODO\((?P<WHO>.*?)\)");
        assert_eq!(desugar_regex_holes(r"fn $name\("), r"fn (?P<name>.*?)\(");
        // EOL anchor and escaped dollar untouched.
        assert_eq!(desugar_regex_holes(r"\}$"), r"\}$");
        assert_eq!(desugar_regex_holes(r"\$5"), r"\$5");
        // Digit tail is not an ident hole.
        assert_eq!(desugar_regex_holes(r"$1"), r"$1");
        // Hand-written named groups pass through.
        assert_eq!(desugar_regex_holes(r"struct (?<name>\w+)"), r"struct (?<name>\w+)");
    }
}
