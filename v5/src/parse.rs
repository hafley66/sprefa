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
                return Ok(Rule { head, body: Vec::new(), aggs, origin: None });
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
        Ok(Rule { head, body, aggs, origin: None })
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
            if s == "ast_yaml" { return self.ast_yaml(); }
            if s == "jsonp" { return self.jsonp(); }
            if s == "json" { return self.json(); }
            if s == "cmd" { return self.cmd(); }
            if s == "comment" { return self.comment(); }
            if s == "closure" { return self.closure(); }
            if s == "scc" { return self.scc(); }
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
        // Optional trailing args after `line`, disambiguated by count:
        //   1 ⇒ `id`        — the spine id of the whole-match span; a rule joins
        //                     `ref(id, _, _, lo, hi)` to feed `gen(...)`.
        //   2 ⇒ `col, end_col` — the whole-match span's 0-based byte columns in
        //                     `line`, for sub-line diagnostic spans.
        //   3 ⇒ `id, col, end_col` — both.
        // The 4-arg (zero-trailing) form keeps named-captures-only spine behavior.
        let mut trailing = Vec::new();
        while matches!(self.peek(), Some(Tok::Comma)) {
            self.next()?; // ,
            trailing.push(self.term()?);
        }
        self.expect(Tok::RParen)?;
        let (id, col, end_col) = match trailing.len() {
            0 => (None, None, None),
            1 => (Some(trailing.remove(0)), None, None),
            2 => (None, Some(trailing.remove(0)), Some(trailing.remove(0))),
            3 => { let id = trailing.remove(0);
                   (Some(id), Some(trailing.remove(0)), Some(trailing.remove(0))) }
            n => bail!("match expects 4 args (path, rev, /re/, line), +1 (id), \
                        +2 (col, end_col), or +3 (id, col, end_col), got {} trailing", n),
        };
        Ok(BodyItem::Match { path, rev, regex, line, id, col, end_col })
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
        // optional 7th term binds the spine id of the WHOLE-match span (the
        // captures' min..max byte range). A rule joins `ref(id, _, _, lo, hi)`
        // off it for the codemod anchor: the bytes this ast match covered.
        // Mirrors `match`'s 5th-arg `id` (christmas #9). Omit `end` with `_` to
        // bind only the id.
        let id = if matches!(self.peek(), Some(Tok::Comma)) {
            self.next()?; Some(self.term()?)
        } else { None };
        self.expect(Tok::RParen)?;
        Ok(BodyItem::Ast { path, rev, lang, query, line, end, id })
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
        // Trailing span outputs (line, col, end_line, end_col) accept the
        // kwarg/`_` form: positional, `name: term`, or omitted entirely. Zero
        // outputs is valid (a file-existence filter on the pattern). 0-based
        // byte columns, 1-based lines.
        let (pos, named) = if matches!(self.peek(), Some(Tok::Comma)) {
            self.next()?;
            self.parse_kwarg_terms()?
        } else {
            (Vec::new(), Vec::new())
        };
        // `id` is the trailing 5th output: the spine id of the WHOLE-match span
        // (captures' min..max byte range), bound through the same located-id path
        // as `ast`/`match` so a rule joins `ref(id, _, _, lo, hi)` for the codemod
        // anchor (christmas #9, decision 3 — consistent across ast/sg/json).
        let outs = Self::assign_outputs(&["line", "col", "end_line", "end_col", "id"], pos, named)?;
        let id = match &outs[4] { Term::Wild => None, t => Some(t.clone()) };
        self.expect(Tok::RParen)?;
        Ok(BodyItem::Sg {
            path, rev, lang, pattern,
            line: outs[0].clone(), col: outs[1].clone(),
            end_line: outs[2].clone(), end_col: outs[3].clone(),
            id,
        })
    }

    /// `ast_yaml(path, rev, :lang, `yaml body`, line, ...)` — mirrors `sg()`
    /// but the 4th arg is a (usually backtick, multiline) ast-grep RuleCore
    /// YAML body instead of a pattern string. The body lexes as a normal
    /// `Tok::Str` (the backtick form is multiline + raw), so only the field
    /// name differs from `sg()`. Span outputs share the kwarg/`_` form.
    fn ast_yaml(&mut self) -> Result<BodyItem> {
        self.ident()?; // ast_yaml
        self.expect(Tok::LParen)?;
        let path = self.term()?; self.expect(Tok::Comma)?;
        let rev = self.term()?; self.expect(Tok::Comma)?;
        self.expect(Tok::Colon)?;
        let lang = self.ident()?;
        self.expect(Tok::Comma)?;
        let yaml = match self.next()? {
            Tok::Str(s) => s,
            other => bail!("expected ast_yaml body string in ast_yaml(), got {:?}", other),
        };
        let (pos, named) = if matches!(self.peek(), Some(Tok::Comma)) {
            self.next()?;
            self.parse_kwarg_terms()?
        } else {
            (Vec::new(), Vec::new())
        };
        let outs = Self::assign_outputs(&["line", "col", "end_line", "end_col"], pos, named)?;
        self.expect(Tok::RParen)?;
        Ok(BodyItem::AstYaml {
            path, rev, lang, yaml,
            line: outs[0].clone(), col: outs[1].clone(),
            end_line: outs[2].clone(), end_col: outs[3].clone(),
        })
    }

    fn jsonp(&mut self) -> Result<BodyItem> {
        self.ident()?; // jsonp
        self.expect(Tok::LParen)?;
        let path = self.term()?; self.expect(Tok::Comma)?;
        let rev = self.term()?; self.expect(Tok::Comma)?;
        let jpath = match self.next()? {
            Tok::Str(s) => s,
            other => bail!("expected json path string in jsonp(), got {:?}", other),
        };
        self.expect(Tok::Comma)?;
        let out = self.term()?;
        // Trailing optional `id`: the spine id of the matched value's byte span,
        // bound through the same located-id path as ast/sg/match (christmas #9,
        // decision 3). Omitted = no id binding (existing 4-arg form unchanged).
        let id = if matches!(self.peek(), Some(Tok::Comma)) {
            self.next()?; Some(self.term()?)
        } else { None };
        self.expect(Tok::RParen)?;
        Ok(BodyItem::JsonP { path, rev, jpath, out, id })
    }

    /// `json(path, rev, q:{ $k: $v })` — declarative brace pattern. The 3rd
    /// arg is a `q:{...}` PathLit (a structured, highlightable token), NOT a
    /// string; a string here is the dotted-path form, redirected to `jsonp`.
    fn json(&mut self) -> Result<BodyItem> {
        self.ident()?; // json
        self.expect(Tok::LParen)?;
        let path = self.term()?; self.expect(Tok::Comma)?;
        let rev = self.term()?; self.expect(Tok::Comma)?;
        let pat = match self.term()? {
            Term::PathLit { body, .. } => body,
            Term::Str(_) => bail!(
                "json takes a brace-pattern literal (`q:{{ $k: $v }}`); for the \
                 dotted-string form use `jsonp(path, rev, \"a.b\", out)`"
            ),
            other => bail!("json 3rd arg must be a `q:{{...}}` brace-pattern literal, got {:?}", other),
        };
        self.expect(Tok::RParen)?;
        // Validate at parse time so malformed bodies fail fast and capture
        // names are discovered before the engine runs.
        if let Err(e) = crate::datapath::parse_pattern(&pat) {
            bail!("json pattern error: {e}");
        }
        Ok(BodyItem::Json { path, rev, pat })
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

    /// `gen("path", "tmpl") <- body.` (file form),
    /// `gen(p, l0, l1, "tmpl") <- body.` (line-splice form), or
    /// `gen(:mode, p, lo, hi, "tmpl") <- body.` (byte-splice form, port of v4
    /// `write_cursor`). `gen` is a reserved head; the body parses like any rule body.
    fn gen_rule(&mut self) -> Result<GenRule> {
        self.ident()?; // gen
        self.expect(Tok::LParen)?;
        // The byte-splice form leads with a `:mode` atom (`:replace`/`:append`/
        // `:prepend`/`:wrap`); peek for Tok::Colon to dispatch before falling
        // back to the term-list parser used by the file/line-splice forms.
        let (target, row_tmpl) = if matches!(self.peek(), Some(Tok::Colon)) {
            self.next()?; // colon
            let mode_ident = self.ident()?;
            let mode = match mode_ident.as_str() {
                "replace" => SpliceMode::Replace,
                "append" => SpliceMode::Append,
                "prepend" => SpliceMode::Prepend,
                "wrap" => SpliceMode::Wrap,
                other => bail!("unknown splice mode :{other}; expected :replace|:append|:prepend|:wrap"),
            };
            self.expect(Tok::Comma)?;
            let path = self.term()?;
            self.expect(Tok::Comma)?;
            let lo = self.term()?;
            self.expect(Tok::Comma)?;
            let hi = self.term()?;
            self.expect(Tok::Comma)?;
            let tmpl_term = self.term()?;
            self.expect(Tok::RParen)?;
            let row_tmpl = match tmpl_term {
                Term::Str(s) => s,
                other => bail!("gen expects a template string here, got {other:?}"),
            };
            (GenTarget::Cursor { mode, path, lo, hi }, row_tmpl)
        } else {
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
            match args.len() {
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
                n => bail!("gen expects 2 args (\"path\", \"tmpl\"), 4 (p, l0, l1, \"tmpl\"), or 5 (:mode, p, lo, hi, \"tmpl\"); got {n}"),
            }
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

    /// Parse trailing op output args, each a bare `term` (positional) or
    /// `name: term` (named). The kwarg form requires a space after the colon
    /// (`label: x`) — `label:x` collides with the scheme-literal lexer, which
    /// rejects unknown `word:` adjacencies. Stops at RParen.
    fn parse_kwarg_terms(&mut self) -> Result<(Vec<Term>, Vec<(String, Term)>)> {
        let mut pos = Vec::new();
        let mut named = Vec::new();
        if matches!(self.peek(), Some(Tok::RParen)) { return Ok((pos, named)); }
        loop {
            if let (Some(Tok::Ident(name)), Some(Tok::Colon)) =
                (self.peek().cloned(), self.peek2().cloned()) {
                self.next()?; // ident
                self.next()?; // colon
                named.push((name, self.term()?));
            } else {
                pos.push(self.term()?);
            }
            match self.peek() {
                Some(Tok::Comma) => {
                    self.next()?;
                    if matches!(self.peek(), Some(Tok::RParen)) { break; }
                }
                Some(Tok::RParen) => break,
                other => bail!("expected , or ) in arg list, got {:?}", other),
            }
        }
        Ok((pos, named))
    }

    /// Assign positional + named args to a fixed set of output field names.
    /// Positional fill in order; named fill by name; unassigned default to `_`
    /// (Term::Wild), so `comment(p, rev, /re/, label: lab)` skips l0/l1. A named
    /// arg with an unknown name is a parse error (catches typos early).
    fn assign_outputs(names: &[&str], pos: Vec<Term>, named: Vec<(String, Term)>) -> Result<Vec<Term>> {
        let mut out: Vec<Option<Term>> = vec![None; names.len()];
        for (i, t) in pos.into_iter().enumerate() {
            if i >= names.len() {
                bail!("too many positional output args (expected one of: {})", names.join(", "));
            }
            out[i] = Some(t);
        }
        for (name, t) in named {
            let i = match names.iter().position(|n| *n == name.as_str()) {
                Some(i) => i,
                None => bail!("unknown output arg `{name}` (known: {})", names.join(", ")),
            };
            out[i] = Some(t);
        }
        Ok(out.into_iter().map(|o| o.unwrap_or(Term::Wild)).collect())
    }

    /// `comment(p, rev, /open/[, /close/], l0: .., l1: .., label: ..)` — one
    /// regex is sequential mode, two is paired. Both regexes take `$NAME` holes.
    /// The three outputs (l0, l1, label) accept the kwarg form: positional
    /// (current), `_` for an unwanted slot, or `name: term` to bind only what
    /// you need and default the rest to `_`.
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
        let (pos, named) = self.parse_kwarg_terms()?;
        let outs = Self::assign_outputs(&["l0", "l1", "label"], pos, named)?;
        self.expect(Tok::RParen)?;
        Ok(BodyItem::Comment {
            path, rev, open, close,
            l0: outs[0].clone(), l1: outs[1].clone(), label: outs[2].clone(),
        })
    }

    fn closure(&mut self) -> Result<BodyItem> {
        self.ident()?; // closure
        self.expect(Tok::LParen)?;
        let rel = self.ident()?;
        self.expect(Tok::RParen)?;
        Ok(BodyItem::Closure { rel })
    }

    fn scc(&mut self) -> Result<BodyItem> {
        self.ident()?; // scc
        self.expect(Tok::LParen)?;
        let rel = self.ident()?;
        self.expect(Tok::RParen)?;
        Ok(BodyItem::Scc { rel })
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
        // `=~` takes a /regex/ literal (the unified regex syntax — same form
        // match/comment/sg use). Strings no longer accepted; the old
        // `x =~ "pat"` form was a second regex syntax that invited ambiguity.
        // `~~` (glob) still takes a string because `/` is a path separator
        // inside globs, not a delimiter.
        let rhs = if op == CmpOp::Match {
            match self.next()? {
                Tok::Regex(r) => Term::Str(desugar_regex_holes(&r)),
                other => bail!("expected /regex/ after =~ (not a string), got {:?}", other),
            }
        } else {
            self.expr()?
        };
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
        // Unary minus: `-1` desugars to `0 - 1` (Arith). Lets split's negative
        // idx read naturally: `split(path, "/", -1)`. Only triggers when `-` is
        // the first token of a term, so binary `a - 1` still parses as subtraction.
        if matches!(self.peek(), Some(Tok::Minus)) {
            self.next()?;
            let inner = self.term()?;
            return Ok(Term::Arith {
                op: ArithOp::Sub, lhs: Box::new(Term::Int(0)), rhs: Box::new(inner)
            });
        }
        // Function call: `ident(args)` in term position. Body-leading atoms are
        // routed to `atom()` by `body_item` before term() runs, so this only
        // fires in head args, comparison sides, arithmetic operands, and nested
        // call args — everywhere a value expression is wanted, not a binding.
        if let Some(Tok::Ident(s)) = self.peek().cloned() {
            if s != "_" && matches!(self.peek2(), Some(Tok::LParen)) {
                self.next()?; // ident
                self.expect(Tok::LParen)?;
                let mut args = Vec::new();
                if !matches!(self.peek(), Some(Tok::RParen)) {
                    loop {
                        args.push(self.expr()?);
                        match self.next()? {
                            Tok::Comma => continue,
                            Tok::RParen => break,
                            other => bail!("expected , or ) in call args, got {:?}", other),
                        }
                    }
                } else {
                    self.next()?; // RParen
                }
                return Ok(Term::Call { name: s, args });
            }
        }
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
    // Names already turned into a capture group in THIS pattern. A repeated hole
    // would emit a second `(?P<name>...)`, which the regex crate rejects as a
    // duplicate capture group (christmas #30). The first occurrence captures;
    // repeats dedupe to a non-capturing `.*?` so the pattern compiles and the
    // var binds once. (The crate has no backreferences, so "same value twice"
    // can't be a regex constraint regardless.)
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
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
            } else if seen.insert(name.clone()) {
                out.push_str("(?P<");
                out.push_str(&name);
                out.push_str(">.*?)");
            } else {
                out.push_str("(?:.*?)");
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

    /// christmas #30 repro: a `$NAME` hole used twice in one pattern must not
    /// emit two identical named groups (the regex crate rejects that as
    /// "duplicate capture group name"). The first occurrence captures; repeats
    /// dedupe to a non-capturing `.*?` so the pattern compiles and `NAME` binds
    /// once (the crate has no backreferences, so "same value twice" isn't
    /// expressible at the regex layer anyway — use two vars + a comparison).
    #[test]
    fn repeated_hole_dedupes_to_noncapturing() {
        let desugared = desugar_regex_holes(r"$k=$k");
        assert_eq!(desugared, r"(?P<k>.*?)=(?:.*?)");
        // The desugared form must be a valid regex (no duplicate group).
        assert!(regex::Regex::new(&desugared).is_ok(),
            "desugared pattern must compile: {desugared}");
        // Three uses: first captures, the next two are anonymous.
        assert_eq!(desugar_regex_holes(r"$x.$x.$x"), r"(?P<x>.*?).(?:.*?).(?:.*?)");
        // Distinct names each keep their own capture.
        assert_eq!(desugar_regex_holes(r"$a=$b"), r"(?P<a>.*?)=(?P<b>.*?)");
    }
}
