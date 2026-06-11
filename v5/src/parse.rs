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

    fn item(&mut self) -> Result<Item> {
        match self.peek() {
            Some(Tok::Ident(s)) if s == "rel" => Ok(Item::Rel(self.rel_decl()?)),
            Some(Tok::Ident(s)) if s == "anchor" => Ok(Item::Anchor(self.anchor_decl()?)),
            Some(Tok::Ident(s)) if s == "type" => Ok(Item::Brand(self.brand_decl()?)),
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
        self.expect(Tok::Arrow)?;
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
                    terms.push(self.term()?);
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
        // Comma-separated terms. 4-ary `scan(rev, glob, path, rev_out)` defaults
        // the repo to "." (self); 5-ary `scan(repo, rev, glob, path, rev_out)`
        // names a repo coordinate (slug / path / ".") that flows as a value.
        let mut terms = vec![self.term()?];
        while matches!(self.peek(), Some(Tok::Comma)) {
            self.next()?; // ,
            terms.push(self.term()?);
        }
        self.expect(Tok::RParen)?;
        let (repo, rev, glob, path, rev_out) = match terms.len() {
            4 => { let mut t = terms.into_iter();
                (Term::Str(".".into()), t.next().unwrap(), t.next().unwrap(), t.next().unwrap(), t.next().unwrap()) }
            5 => { let mut t = terms.into_iter();
                (t.next().unwrap(), t.next().unwrap(), t.next().unwrap(), t.next().unwrap(), t.next().unwrap()) }
            n => bail!("scan expects 4 args (rev, glob, path, rev) or 5 (repo, rev, glob, path, rev), got {n}"),
        };
        Ok(BodyItem::Scan { repo, rev, glob, path, rev_out })
    }

    fn match_(&mut self) -> Result<BodyItem> {
        self.ident()?; // match
        self.expect(Tok::LParen)?;
        let path = self.term()?; self.expect(Tok::Comma)?;
        let rev = self.term()?; self.expect(Tok::Comma)?;
        let regex = match self.next()? {
            Tok::Regex(r) => r,
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

    fn closure(&mut self) -> Result<BodyItem> {
        self.ident()?; // closure
        self.expect(Tok::LParen)?;
        let rel = self.ident()?;
        self.expect(Tok::RParen)?;
        Ok(BodyItem::Closure { rel })
    }

    fn constraint(&mut self) -> Result<Constraint> {
        let lhs = self.term()?;
        let op = match self.next()? {
            Tok::Eq => CmpOp::Eq, Tok::Ne => CmpOp::Ne,
            Tok::Lt => CmpOp::Lt, Tok::Le => CmpOp::Le,
            Tok::Gt => CmpOp::Gt, Tok::Ge => CmpOp::Ge,
            Tok::Match => CmpOp::Match, Tok::Glob => CmpOp::Glob,
            other => bail!("expected comparison operator, got {:?}", other),
        };
        let rhs = self.term()?;
        Ok(Constraint { lhs, op, rhs })
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
            other => bail!("expected term, got {:?}", other),
        }
    }
}
