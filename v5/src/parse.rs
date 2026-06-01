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
            Some(Tok::Question) => Ok(Item::Query(self.query()?)),
            _ => Ok(Item::Rule(self.rule()?)),
        }
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
            let ty = Type::parse(&tname).ok_or_else(|| anyhow::anyhow!("unknown type {tname}"))?;
            cols.push(Col { name: cname, ty });
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
        let head = self.atom()?;
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
        Ok(Rule { head, body })
    }

    fn query(&mut self) -> Result<Query> {
        self.expect(Tok::Question)?;
        let head = self.atom()?;
        let mut wheres = Vec::new();
        if matches!(self.peek(), Some(Tok::Ident(s)) if s == "where") {
            self.ident()?;
            loop {
                wheres.push(self.constraint()?);
                if matches!(self.peek(), Some(Tok::Comma)) { self.next()?; } else { break; }
            }
        }
        self.expect(Tok::Dot)?;
        Ok(Query { head, wheres })
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
        let rev = self.term()?; self.expect(Tok::Comma)?;
        let glob = self.term()?; self.expect(Tok::Comma)?;
        let path = self.term()?; self.expect(Tok::Comma)?;
        let rev_out = self.term()?;
        self.expect(Tok::RParen)?;
        Ok(BodyItem::Scan { rev, glob, path, rev_out })
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
            other => bail!("expected term, got {:?}", other),
        }
    }
}
