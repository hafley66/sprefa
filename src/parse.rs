use anyhow::{bail, Result};

use crate::ast::*;
use crate::lex::Tok;

pub struct Parser { toks: Vec<Tok>, i: usize }

// ARCH {"url":"20-parse","role":"frontend"}
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
            Some(Tok::Ident(s)) if s == "type" => self.type_decl(),
            Some(Tok::Ident(s)) if s == "gen" => Ok(Item::Gen(self.gen_rule()?)),
            // `sh`/`sh!`/`sh*` heading an ident (the fn name) is a shell-fn decl;
            // a rule/rel literally named `sh` (`sh(...)`) still parses as such
            // because its second token is `(`, not an ident/bang/star.
            Some(Tok::Ident(s)) if s == "sh"
                && matches!(self.peek2(), Some(Tok::Ident(_) | Tok::Bang | Tok::Star)) =>
                Ok(Item::Shell(self.shell_fn()?)),
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

    /// A `type` decl in one of three forms, disambiguated on the token after the
    /// name: `<:` = nominal brand, `=` = enum brand (closed literal set), `(` =
    /// named row shape.
    ///   `type <ident> <: <parent>.`
    ///   `type <ident> = "lit" | "lit" | ... .`
    ///   `type <ident>(col: ty, ...).`
    fn type_decl(&mut self) -> Result<Item> {
        self.ident()?; // "type"
        let name = self.ident()?;
        match self.peek() {
            Some(Tok::Lt2) => {
                self.next()?;
                let parent = self.ident()?;
                self.expect(Tok::Dot)?;
                Ok(Item::Brand(BrandDecl { name, parent, variants: None }))
            }
            Some(Tok::Eq) => {
                self.next()?;
                let mut variants = vec![self.str_lit()?];
                while matches!(self.peek(), Some(Tok::Pipe)) {
                    self.next()?; // `|`
                    variants.push(self.str_lit()?);
                }
                self.expect(Tok::Dot)?;
                Ok(Item::Brand(BrandDecl { name, parent: "text".into(), variants: Some(variants) }))
            }
            Some(Tok::LParen) => {
                self.next()?; // `(`
                let mut cols = Vec::new();
                loop {
                    let cname = self.ident()?;
                    self.expect(Tok::Colon)?;
                    let tname = self.ident()?;
                    let col = match Type::parse(&tname) {
                        Some(ty) => Col { name: cname, ty, brand: None, raw: false },
                        None => Col { name: cname, ty: Type::Text, brand: Some(tname), raw: false },
                    };
                    cols.push(col);
                    match self.next()? {
                        Tok::Comma => continue,
                        Tok::RParen => break,
                        other => bail!("expected , or ) in type shape `{name}`, got {:?}", other),
                    }
                }
                self.expect(Tok::Dot)?;
                Ok(Item::Shape(ShapeDecl { name, cols }))
            }
            other => bail!("type `{name}`: expected `<:` (brand), `=` (enum), or `(` (shape), got {:?}", other),
        }
    }

    /// A bare string literal (enum variant). Rejects any non-string token with a
    /// message naming the fix, since enum variants are text literals.
    fn str_lit(&mut self) -> Result<String> {
        match self.next()? {
            Tok::Str(s) => Ok(s),
            other => bail!("enum brand variants must be string literals like \"warn\", got {:?}", other),
        }
    }

    fn rel_decl(&mut self) -> Result<RelDecl> {
        self.ident()?; // "rel"
        let name = self.ident()?;
        // `rel <name>: <shape>.` — the columns come from a named `type` shape,
        // resolved at load by `typecheck::expand_shapes`. `cols` stays empty until
        // then; no qualifiers are accepted on this form.
        if matches!(self.peek(), Some(Tok::Colon)) {
            self.next()?; // `:`
            let shape = self.ident()?;
            self.expect(Tok::Dot)?;
            return Ok(RelDecl { name, shape_ref: Some(shape), ..Default::default() });
        }
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
                Some(ty) => Col { name: cname, ty, brand: None, raw: false },
                None => Col { name: cname, ty: Type::Text, brand: Some(tname), raw: false },
            };
            cols.push(col);
            match self.next()? {
                Tok::Comma => continue,
                Tok::RParen => break,
                other => bail!("expected , or ) in rel decl, got {:?}", other),
            }
        }
        // Optional qualifiers between `)` and `.`: `key(c1, c2, ...)`,
        // `merge(MaxBy(col))`, and the port markers `@in(class)`/`@out(class)`.
        // Any order, all optional. `key` narrows the conflict target (FD /
        // choice-domain); `merge` sets the lattice merge; `@in`/`@out` mark the
        // rel as a boundary port (the `@` sigil = tick-boundary axis, like
        // rule-level @next/@async/@stream).
        let mut key: Option<Vec<String>> = None;
        let mut merge: Option<crate::ast::MergeFn> = None;
        let mut port: Option<crate::ast::Port> = None;
        loop {
            match self.peek() {
                Some(Tok::Ident(q)) => {
                    let q = q.clone();
                    self.next()?;
                    match q.as_str() {
                        "key" => {
                            self.expect(Tok::LParen)?;
                            let mut cols = Vec::new();
                            loop {
                                cols.push(self.ident()?);
                                match self.next()? {
                                    Tok::Comma => continue,
                                    Tok::RParen => break,
                                    other => bail!("expected , or ) in key(...), got {:?}", other),
                                }
                            }
                            key = Some(cols);
                        }
                        "merge" => {
                            self.expect(Tok::LParen)?;
                            let fname = self.ident()?;
                            self.expect(Tok::LParen)?;
                            let arg = self.ident()?;
                            self.expect(Tok::RParen)?;
                            self.expect(Tok::RParen)?;
                            merge = Some(crate::ast::MergeFn::parse(&fname, &arg)
                                .ok_or_else(|| anyhow::anyhow!("unknown merge function {fname}"))?);
                        }
                        _ => bail!("expected `.` or rel qualifier (key/merge/@in/@out), got {q:?}"),
                    }
                }
                Some(Tok::At(w)) if w == "in" || w == "out" => {
                    let dir = if w == "in" { crate::ast::PortDir::In } else { crate::ast::PortDir::Out };
                    self.next()?;
                    self.expect(Tok::LParen)?;
                    let class = self.ident()?;
                    self.expect(Tok::RParen)?;
                    if port.is_some() {
                        bail!("rel {name}: at most one @in/@out port qualifier");
                    }
                    port = Some(crate::ast::Port { dir, class });
                }
                _ => break,
            }
        }
        self.expect(Tok::Dot)?;
        Ok(RelDecl { name, cols, key, merge, port, ..Default::default() })
    }

    /// `sh[!|*] name(p1, p2) -> (c1: t1, c2: t2) = `cmd {p1}`.` — a shell-fn
    /// decl. The bang/star (already consumed-as-tokens) selects the kind; params
    /// are bare idents (the `{hole}` names); outs are typed columns like a rel
    /// decl; the body is a backtick string (`= `...`.`). The brace `{ shell }`
    /// body form is deferred (the lexer tokenizes inside braces).
    fn shell_fn(&mut self) -> Result<ShellFn> {
        self.ident()?; // "sh"
        let kind = match self.peek() {
            Some(Tok::Bang) => { self.next()?; ShellKind::Mutate }
            Some(Tok::Star) => { self.next()?; ShellKind::Stream }
            _ => ShellKind::Read,
        };
        let name = self.ident()?;
        self.expect(Tok::LParen)?;
        let mut params = Vec::new();
        if matches!(self.peek(), Some(Tok::RParen)) {
            self.next()?;
        } else {
            loop {
                params.push(self.ident()?);
                match self.next()? {
                    Tok::Comma => continue,
                    Tok::RParen => break,
                    other => bail!("expected , or ) in sh params, got {:?}", other),
                }
            }
        }
        self.expect(Tok::ThinArrow)?;
        self.expect(Tok::LParen)?;
        let mut outs = Vec::new();
        loop {
            let cname = self.ident()?;
            self.expect(Tok::Colon)?;
            let tname = self.ident()?;
            let col = match Type::parse(&tname) {
                Some(ty) => Col { name: cname, ty, brand: None, raw: false },
                None => Col { name: cname, ty: Type::Text, brand: Some(tname), raw: false },
            };
            outs.push(col);
            match self.next()? {
                Tok::Comma => continue,
                Tok::RParen => break,
                other => bail!("expected , or ) in sh outs, got {:?}", other),
            }
        }
        self.expect(Tok::Eq)?;
        let body = match self.next()? {
            Tok::Str(s) => s,
            other => bail!("sh `{}`: expected a `backtick` body after =, got {:?}", name, other),
        };
        self.expect(Tok::Dot)?;
        Ok(ShellFn { name, params, outs, body, kind })
    }

    fn rule(&mut self) -> Result<Rule> {
        let (head, aggs, agg_args2) = self.head_atom()?;
        // ground fact: `slide(1, "intro").` is a rule with an empty body; the
        // head must be all literals (lowering rejects unbound vars)
        match self.next()? {
            Tok::Dot => {
                if aggs.iter().any(|a| a.is_some()) {
                    bail!("aggregate not allowed in a fact head");
                }
                return Ok(Rule { head, body: Vec::new(), aggs, agg_args2, origin: None, temporal: None });
            }
            Tok::Arrow => {}
            other => bail!("expected <- or . after rule head, got {:?}", other),
        }
        // Optional temporal modifier immediately after the neck: `<- @next ...`.
        let temporal = if matches!(self.peek(), Some(Tok::At(_))) {
            match self.next()? {
                Tok::At(w) => Some(match w.as_str() {
                    "next"   => Temporal::Next,
                    "async"  => Temporal::Async,
                    "stream" => Temporal::Stream,
                    other => bail!("unknown rule modifier `@{other}` (known: @next, @async, @stream)"),
                }),
                _ => unreachable!(),
            }
        } else {
            None
        };
        let mut body = Vec::new();
        loop {
            body.push(self.body_item()?);
            match self.next()? {
                Tok::Comma => continue,
                Tok::Dot => break,
                other => bail!("expected , or . in rule body, got {:?}", other),
            }
        }
        Ok(Rule { head, body, aggs, agg_args2, origin: None, temporal })
    }

    /// Parse a rule head, allowing aggregate calls in term positions:
    /// `fan_out(F, count(T)) <- ...`. Returns the head atom (the aggregate's arg
    /// flows in as the term) plus a parallel `aggs` vec marking which terms are
    /// aggregated. Aggregates are head-position only; a body `count(...)` parses as
    /// a relation atom against a relation named `count` and is rejected at lowering
    /// if no such relation exists (an agg call in the body is never special).
    fn head_atom(&mut self) -> Result<(Atom, Vec<Option<AggFn>>, Vec<Option<Term>>)> {
        let rel = self.ident()?;
        self.expect(Tok::LParen)?;
        let mut terms = Vec::new();
        let mut aggs: Vec<Option<AggFn>> = Vec::new();
        // Parallel to `aggs`: the second arg of a two-arg aggregate
        // (`json_group_object(key, value)`), else `None`.
        let mut agg_args2: Vec<Option<Term>> = Vec::new();
        let mut named: Vec<(String, Term)> = Vec::new();
        if !matches!(self.peek(), Some(Tok::RParen)) {
            loop {
                // A `col: term` named arg in head position (`diag(path: p, msg: m)
                // <- ...`): the frontend resolves it to a positional slot once the
                // rel schema is known, padding unnamed columns with `Term::Wild`
                // (which the head lowers to NULL). Named args never carry an
                // aggregate — `aggs` stays parallel to the POSITIONAL terms only,
                // and resolve pads its length out, so mixing named args with an
                // aggregate head is rejected below.
                if matches!((self.peek(), self.peek2()), (Some(Tok::Ident(_)), Some(Tok::Colon))) {
                    let col = self.ident()?;
                    self.expect(Tok::Colon)?;
                    named.push((col, self.expr()?));
                    match self.next()? {
                        Tok::Comma => continue,
                        Tok::RParen => break,
                        other => bail!("expected , or ) in atom, got {:?}", other),
                    }
                }
                // An `aggfn(arg)` call: a known aggregate name immediately followed
                // by `(`. Otherwise a plain term.
                let is_agg = matches!((self.peek(), self.peek2()),
                    (Some(Tok::Ident(s)), Some(Tok::LParen)) if AggFn::parse(s).is_some());
                if is_agg {
                    let fname = self.ident()?;
                    let f = AggFn::parse(&fname).unwrap();
                    self.expect(Tok::LParen)?;
                    let arg = self.term()?;
                    // `json_group_object(key, value)` is two-arg: parse the value
                    // into `agg_args2`. Every other aggregate is one-arg.
                    let arg2 = if f.is_two_arg() {
                        self.expect(Tok::Comma)?;
                        Some(self.term()?)
                    } else {
                        None
                    };
                    self.expect(Tok::RParen)?;
                    terms.push(arg);
                    aggs.push(Some(f));
                    agg_args2.push(arg2);
                } else {
                    terms.push(self.expr()?);
                    aggs.push(None);
                    agg_args2.push(None);
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
        if !named.is_empty() && aggs.iter().any(|a| a.is_some()) {
            bail!("a rule head can't mix named args with an aggregate; write the aggregate head fully positional");
        }
        if !aggs.iter().any(|a| a.is_some()) { aggs.clear(); agg_args2.clear(); }
        Ok((Atom { rel, terms, named }, aggs, agg_args2))
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
        // Args are positional terms or `col: term` named args, in any order. The
        // frontend resolves the named args to positional slots once every rel
        // schema is known (a body atom can forward-reference a rel). Shared with
        // the source-op kwarg form via `parse_kwarg_terms`.
        let (terms, named) = self.parse_kwarg_terms()?;
        self.expect(Tok::RParen)?;
        Ok(Atom { rel, terms, named })
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
            if s == "node2vec" { return self.node2vec(); }
            // An aggregate call in body position is a parse error: aggregation is
            // head-only (`fan_out(F, count(T)) <- type_edge(F, T, _).`).
            if AggFn::parse(s).is_some() && matches!(self.peek2(), Some(Tok::LParen)) {
                bail!("aggregate `{s}(...)` is only allowed in a rule head, not the body");
            }
            // relation atom vs constraint: lookahead for '('
            if matches!(self.peek2(), Some(Tok::LParen)) {
                let atom = self.atom()?;
                // An effect call: `name(args) -> (outs)`. The trailing `->` after
                // the arg list is the only disambiguator from a plain Pos atom.
                // `args` fill the `sh` template holes; `outs` are fresh response
                // vars. Resolves to a `ShellFn` at typecheck; fires off-tick.
                if matches!(self.peek(), Some(Tok::ThinArrow)) {
                    self.next()?; // ->
                    let outs = self.paren_terms()?;
                    return Ok(BodyItem::Effect { name: atom.rel, args: atom.terms, outs });
                }
                return Ok(BodyItem::Pos(atom));
            }
        }
        Ok(BodyItem::Cmp(self.constraint()?))
    }

    /// Parse a parenthesized, comma-separated term list `(a, b, c)` (or `()`).
    /// Used for an effect call's response binds (`-> (status, body)`).
    fn paren_terms(&mut self) -> Result<Vec<Term>> {
        self.expect(Tok::LParen)?;
        let mut terms = Vec::new();
        if !matches!(self.peek(), Some(Tok::RParen)) {
            loop {
                terms.push(self.term()?);
                match self.next()? {
                    Tok::Comma => continue,
                    Tok::RParen => break,
                    other => bail!("expected , or ) in effect outputs, got {:?}", other),
                }
            }
        } else {
            self.next()?; // RParen
        }
        Ok(terms)
    }

    fn scan(&mut self) -> Result<BodyItem> {
        self.ident()?; // scan
        self.expect(Tok::LParen)?;
        // Leading positional args are the coordinate prefix (repo?, rev?, glob);
        // the two OUTPUTS (path, rev_out) follow, positionally OR by name. `_` or
        // an omitted rev_out is a don't-care (rev not bound). The repo defaults to
        // "." (self) and rev to WORK (live disk):
        //   scan(glob, path)                       repo=".", rev=WORK, no rev_out
        //   scan(glob, path, rev_out)              repo=".", rev=WORK
        //   scan(rev, glob, path, rev_out)         repo="."
        //   scan(repo, rev, glob, path, rev_out)   named repo coordinate (slug / path / ".")
        // Naming an output (`path: p`, `rev_out: r`) leaves the remaining
        // positionals as pure inputs (1=glob, 2=rev+glob, 3=repo+rev+glob).
        let (pos, named) = self.parse_kwarg_terms()?;
        self.expect(Tok::RParen)?;
        let mut path_named: Option<Term> = None;
        let mut rev_out_named: Option<Term> = None;
        for (name, t) in named {
            match name.as_str() {
                "path" => path_named = Some(t),
                "rev_out" => rev_out_named = Some(t),
                other => bail!("unknown scan output arg `{other}` (known: path, rev_out)"),
            }
        }
        // Split the positional list into the input coordinate prefix and any
        // positional outputs. `n_inputs` = how many leading positionals are the
        // repo/rev/glob coordinate; the coordinate itself is dispatched by count.
        let coord = |inputs: Vec<Term>| -> Result<(Term, Term, Term)> {
            match inputs.len() {
                1 => { let mut t = inputs.into_iter();
                    Ok((Term::Str(".".into()), Term::Str("WORK".into()), t.next().unwrap())) }
                2 => { let mut t = inputs.into_iter();
                    Ok((Term::Str(".".into()), t.next().unwrap(), t.next().unwrap())) }
                3 => { let mut t = inputs.into_iter();
                    Ok((t.next().unwrap(), t.next().unwrap(), t.next().unwrap())) }
                n => bail!("scan coordinate expects glob, rev+glob, or repo+rev+glob, got {n} input arg(s)"),
            }
        };
        let (repo, rev, glob, path, rev_out) = if path_named.is_some() {
            // path is named: every positional is an input coordinate.
            let (repo, rev, glob) = coord(pos)?;
            (repo, rev, glob, path_named.unwrap(), rev_out_named.unwrap_or(Term::Wild))
        } else if let Some(ro) = rev_out_named {
            // rev_out named, path positional (the last positional).
            let mut pos = pos;
            let path = pos.pop().ok_or_else(|| anyhow::anyhow!("scan missing path output"))?;
            let (repo, rev, glob) = coord(pos)?;
            (repo, rev, glob, path, ro)
        } else {
            // Fully positional: last one or two positionals are the outputs.
            match pos.len() {
                2 => { let mut t = pos.into_iter();
                    (Term::Str(".".into()), Term::Str("WORK".into()), t.next().unwrap(), t.next().unwrap(), Term::Wild) }
                3 => { let mut t = pos.into_iter();
                    (Term::Str(".".into()), Term::Str("WORK".into()), t.next().unwrap(), t.next().unwrap(), t.next().unwrap()) }
                4 => { let mut t = pos.into_iter();
                    (Term::Str(".".into()), t.next().unwrap(), t.next().unwrap(), t.next().unwrap(), t.next().unwrap()) }
                5 => { let mut t = pos.into_iter();
                    (t.next().unwrap(), t.next().unwrap(), t.next().unwrap(), t.next().unwrap(), t.next().unwrap()) }
                n => bail!("scan expects 2 args (glob, path), 3 (glob, path, rev_out), 4 (rev, glob, path, rev_out), or 5 (repo, rev, glob, path, rev_out), got {n}"),
            }
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
        // Named trailing outputs (`id:`/`col:`/`end_col:`) share the kwarg/`_`
        // form: bind only what you want, no positional counting. Positional-only
        // trailing keeps the count-disambiguated form (1⇒id, 2⇒col+end_col,
        // 3⇒all) for backward compatibility.
        let (pos, named) = if matches!(self.peek(), Some(Tok::Comma)) {
            self.next()?; // consume the comma after `line`
            self.parse_kwarg_terms()?
        } else {
            (Vec::new(), Vec::new())
        };
        self.expect(Tok::RParen)?;
        let opt = |t: Term| if matches!(t, Term::Wild) { None } else { Some(t) };
        let (id, col, end_col) = if named.is_empty() {
            let mut trailing = pos;
            match trailing.len() {
                0 => (None, None, None),
                1 => (Some(trailing.remove(0)), None, None),
                2 => (None, Some(trailing.remove(0)), Some(trailing.remove(0))),
                3 => { let id = trailing.remove(0);
                       (Some(id), Some(trailing.remove(0)), Some(trailing.remove(0))) }
                n => bail!("match expects 4 args (path, rev, /re/, line), +1 (id), \
                            +2 (col, end_col), or +3 (id, col, end_col), got {} trailing", n),
            }
        } else {
            let outs = Self::assign_outputs(&["id", "col", "end_col"], pos, named)?;
            let mut it = outs.into_iter();
            (opt(it.next().unwrap()), opt(it.next().unwrap()), opt(it.next().unwrap()))
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
        // Optional trailing outputs `end` (the match's end line, for body-span
        // queries) and `id` (the spine id of the WHOLE-match span — the captures'
        // min..max byte range; a rule joins `ref(id, _, _, lo, hi)` off it for the
        // codemod anchor, mirroring `match`'s 5th-arg `id`, christmas #9). They
        // take the kwarg/`_` form: positional (`, end`, `, end, id`, `, _, id`
        // to bind only id) OR named (`end:`/`id:`).
        let (pos, named) = if matches!(self.peek(), Some(Tok::Comma)) {
            self.next()?; // consume the comma after `line`
            self.parse_kwarg_terms()?
        } else {
            (Vec::new(), Vec::new())
        };
        self.expect(Tok::RParen)?;
        let opt = |t: Term| if matches!(t, Term::Wild) { None } else { Some(t) };
        let (end, id) = if named.is_empty() {
            let mut trailing = pos;
            match trailing.len() {
                0 => (None, None),
                1 => (Some(trailing.remove(0)), None),
                2 => { let end = trailing.remove(0); (Some(end), Some(trailing.remove(0))) }
                n => bail!("ast expects a query, line, +1 (end), or +2 (end, id) trailing outputs, got {n}"),
            }
        } else {
            let outs = Self::assign_outputs(&["end", "id"], pos, named)?;
            let mut it = outs.into_iter();
            (opt(it.next().unwrap()), opt(it.next().unwrap()))
        };
        Ok(BodyItem::Ast { path, rev, lang, query, line, end, id })
    }

    /// FILE form `sg(path, rev, :lang, "pat", line, col, end_line, end_col, id)`
    /// vs TERM form `sg(:lang, src, "pat", line, col, end_line, end_col)`. The
    /// term form LEADS with `:lang` (a colon right after `(`), so a colon in the
    /// first slot dispatches the term form; anything else is the file form (where
    /// `:lang` sits third). The term form's `src` is a bound `str` value (an
    /// embedded-language body); spans are region-relative and there is no `id`.
    fn sg(&mut self) -> Result<BodyItem> {
        self.ident()?; // sg
        self.expect(Tok::LParen)?;
        if matches!(self.peek(), Some(Tok::Colon)) {
            return self.sg_term();
        }
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
            src: path, rev: Some(rev), lang, pattern,
            line: outs[0].clone(), col: outs[1].clone(),
            end_line: outs[2].clone(), end_col: outs[3].clone(),
            id,
        })
    }

    /// TERM form `sg(:lang, src, "pat", line, col, end_line, end_col)` — the
    /// embedded-language seam. `src` is a `str` value bound earlier in the rule
    /// (a styled-components css body, a markdown fence, a response column). Mirrors
    /// the term form of `jsonp`/`json`: no file, no `rev`, no `id`; the join binds
    /// `src` and this op extracts over the bound string. Spans are relative to the
    /// bound string. The colon after `(` was already peeked by `sg`.
    fn sg_term(&mut self) -> Result<BodyItem> {
        self.expect(Tok::Colon)?;
        let lang = self.ident()?;
        self.expect(Tok::Comma)?;
        let src = self.term()?;
        self.expect(Tok::Comma)?;
        let pattern = match self.next()? {
            Tok::Str(s) => s,
            other => bail!("expected pattern string in sg(:lang, src, \"pat\"), got {:?}", other),
        };
        let (pos, named) = if matches!(self.peek(), Some(Tok::Comma)) {
            self.next()?;
            self.parse_kwarg_terms()?
        } else {
            (Vec::new(), Vec::new())
        };
        // No `id` slot: a term source has no file to locate against. Spans are
        // region-relative (byte 0 = start of `src`); the caller carries the
        // enclosing region's own line to reach file coordinates.
        let outs = Self::assign_outputs(&["line", "col", "end_line", "end_col"], pos, named)?;
        self.expect(Tok::RParen)?;
        Ok(BodyItem::Sg {
            src, rev: None, lang, pattern,
            line: outs[0].clone(), col: outs[1].clone(),
            end_line: outs[2].clone(), end_col: outs[3].clone(),
            id: None,
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

    /// FILE form `jsonp(path, rev, "a.b", out, id?)` vs TERM form
    /// `jsonp(src, "a.b", out, id?)` (a bound `str` content value, no rev). The
    /// jpath is the string literal immediately followed by the `out` var (never a
    /// string), so a string in the SECOND slot followed by a non-string is the
    /// term form; a string in the second slot followed by another string is a
    /// string-literal `rev` (file form). A non-string second is `rev` (file form).
    fn jsonp(&mut self) -> Result<BodyItem> {
        self.ident()?; // jsonp
        self.expect(Tok::LParen)?;
        let first = self.term()?;
        self.expect(Tok::Comma)?;
        let second = self.term()?;
        let (src, rev, jpath, out) = match second {
            Term::Str(jp) => {
                self.expect(Tok::Comma)?;
                match self.term()? {
                    // file form: first=path, second=rev(str literal), third=jpath.
                    Term::Str(jp2) => {
                        self.expect(Tok::Comma)?;
                        (first, Some(Term::Str(jp)), jp2, self.term()?)
                    }
                    // term form: first=src, second=jpath, third=out.
                    out => (first, None, jp, out),
                }
            }
            rev => {
                // file form: first=path, second=rev (non-string), then jpath, out.
                self.expect(Tok::Comma)?;
                let jpath = match self.next()? {
                    Tok::Str(s) => s,
                    other => bail!("expected json path string in jsonp(), got {:?}", other),
                };
                self.expect(Tok::Comma)?;
                (first, Some(rev), jpath, self.term()?)
            }
        };
        // Trailing optional `id`: the spine id of the matched value's byte span,
        // bound through the same located-id path as ast/sg/match (christmas #9).
        // Only the FILE form locates (the term source has no file); an id on a
        // term-form jsonp is rejected in the engine.
        let id = if matches!(self.peek(), Some(Tok::Comma)) {
            self.next()?; Some(self.term()?)
        } else { None };
        self.expect(Tok::RParen)?;
        Ok(BodyItem::JsonP { src, rev, jpath, out, id })
    }

    /// FILE form `json(path, rev, q:{...})` vs TERM form `json(src, q:{...})`.
    /// The pattern is always a `q:{...}` PathLit; a PathLit in the SECOND slot is
    /// the term form (src then pattern), otherwise the second slot is `rev` and
    /// the pattern is third (file form). A `rev` may be a string literal.
    fn json(&mut self) -> Result<BodyItem> {
        self.ident()?; // json
        self.expect(Tok::LParen)?;
        let first = self.term()?;
        self.expect(Tok::Comma)?;
        let second = self.term()?;
        let (src, rev, pat) = match second {
            Term::PathLit { body, .. } => (first, None, body),
            rev => {
                self.expect(Tok::Comma)?;
                let pat = match self.term()? {
                    Term::PathLit { body, .. } => body,
                    Term::Str(_) => bail!(
                        "json takes a brace-pattern literal (`q:{{ $k: $v }}`); for the \
                         dotted-string form use `jsonp(...)`"
                    ),
                    other => bail!("json pattern arg must be a `q:{{...}}` brace-pattern literal, got {:?}", other),
                };
                (first, Some(rev), pat)
            }
        };
        self.expect(Tok::RParen)?;
        // Validate at parse time so malformed bodies fail fast and capture
        // names are discovered before the engine runs.
        if let Err(e) = crate::datapath::parse_pattern(&pat) {
            bail!("json pattern error: {e}");
        }
        Ok(BodyItem::Json { src, rev, pat })
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
        // The byte-splice form leads with a `:mode` atom; peek for Tok::Colon to
        // dispatch before falling back to the term-list parser used by the
        // file/line-splice forms. Canonical modes: :replace/:append/:prepend/
        // :wrap. Aliases: :insert_after == :append, :insert_before == :prepend.
        // :delete is :replace with an empty payload and takes NO template
        // (`gen(:delete, p, lo, hi)`).
        let (target, row_tmpl) = if matches!(self.peek(), Some(Tok::Colon)) {
            self.next()?; // colon
            let mode_ident = self.ident()?;
            // :zone is its own dispatch (no SpliceMode — it targets named
            // markers, not byte ranges); peel off before the SpliceMode match.
            if mode_ident == "zone" {
                self.expect(Tok::Comma)?;
                let mut args: Vec<Term> = vec![self.term()?];
                while matches!(self.peek(), Some(Tok::Comma)) {
                    self.next()?;
                    args.push(self.term()?);
                }
                self.expect(Tok::RParen)?;
                if args.len() != 3 {
                    bail!("gen(:zone, ...) expects p, name, \"tmpl\" (got {} args after the mode)", args.len());
                }
                let tmpl_of = |t: Term| -> Result<String> {
                    match t {
                        Term::Str(s) => Ok(s),
                        other => bail!("gen expects a template string here, got {other:?}"),
                    }
                };
                let mut it = args.into_iter();
                let (path, name) = (it.next().unwrap(), it.next().unwrap());
                let row_tmpl = tmpl_of(it.next().unwrap())?;
                let body = self.collect_gen_body()?;
                // Path + name are `{var}`-hole templates (literal when no holes),
                // same shape as a `File` path_tmpl — a rule can fan over many
                // files / zones via body bindings.
                let (path_tmpl, name_tmpl) = match (path, name) {
                    (Term::Str(p), Term::Str(n)) => (p, n),
                    (other_p, other_n) => bail!(
                        "gen(:zone, ...) path and name must be string literals (got {:?}, {:?}); \
                         use a `{{var}}` hole inside the literal to fan over body bindings",
                        other_p, other_n),
                };
                return Ok(GenRule { target: GenTarget::Zone { path_tmpl, name_tmpl }, row_tmpl, body });
            }
            let (mode, is_delete) = match mode_ident.as_str() {
                "replace" => (SpliceMode::Replace, false),
                "append" | "insert_after" => (SpliceMode::Append, false),
                "prepend" | "insert_before" => (SpliceMode::Prepend, false),
                "wrap" => (SpliceMode::Wrap, false),
                "delete" => (SpliceMode::Replace, true),
                other => bail!("unknown splice mode :{other}; expected :replace|:append|:prepend|:wrap|:insert_after|:insert_before|:delete|:zone"),
            };
            // Collect every term after the mode tag; arity disambiguates the
            // File-append form (`:append, "path", "tmpl"` = 2) from the byte
            // Cursor form (`:mode, p, lo, hi[, "tmpl"]` = 3 for :delete, 4 else)
            // from the named-marker Zone form (`:zone, p, name, "tmpl"` = 3).
            self.expect(Tok::Comma)?;
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
            // File-append: `gen(:append, "path", "tmpl")`. Whole-file emit whose
            // rules concatenate in program order; only :append has clear whole-
            // file semantics (the byte modes need lo/hi coordinates).
            if args.len() == 2 && mode == SpliceMode::Append {
                let mut it = args.into_iter();
                let path_tmpl = tmpl_of(it.next().unwrap())?;
                (GenTarget::File { path_tmpl, append: true }, tmpl_of(it.next().unwrap())?)
            } else {
                if args.len() != if is_delete { 3 } else { 4 } {
                    bail!("gen(:{mode_ident}, ...) expects p, lo, hi{} (got {} args after the mode); \
                           use gen(:append, \"path\", \"tmpl\") for the whole-file append form",
                          if is_delete { "" } else { ", \"tmpl\"" }, args.len());
                }
                let mut it = args.into_iter();
                let (path, lo, hi) = (it.next().unwrap(), it.next().unwrap(), it.next().unwrap());
                let row_tmpl = if is_delete { String::new() } else { tmpl_of(it.next().unwrap())? };
                (GenTarget::Cursor { mode, path, lo, hi }, row_tmpl)
            }
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
                    (GenTarget::File { path_tmpl, append: false }, tmpl_of(it.next().unwrap())?)
                }
                4 => {
                    let mut it = args.into_iter();
                    let (path, l0, l1) = (it.next().unwrap(), it.next().unwrap(), it.next().unwrap());
                    (GenTarget::Splice { path, l0, l1 }, tmpl_of(it.next().unwrap())?)
                }
                n => bail!("gen expects 2 args (\"path\", \"tmpl\"), 4 (p, l0, l1, \"tmpl\"), 5 (:mode, p, lo, hi, \"tmpl\"), or 4 (:zone, p, name, \"tmpl\"); got {n}"),
            }
        };
        let body = self.collect_gen_body()?;
        Ok(GenRule { target, row_tmpl, body })
    }

    /// Shared body collector for every `gen` target form: `<- body [, body]*.`.
    /// A `gen` body is the same shape as a rule body (positive atoms + negation
    /// + comparison), so this is `body_item` separated by `,` and terminated by
    /// `.` — extracted so the early-return forms (`:zone`) share it.
    fn collect_gen_body(&mut self) -> Result<Vec<BodyItem>> {
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
        Ok(body)
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
                named.push((name, self.expr()?));
            } else {
                pos.push(self.expr()?);
            }
            match self.peek() {
                Some(Tok::Comma) => {
                    self.next()?;
                    if matches!(self.peek(), Some(Tok::RParen)) { break; }
                }
                Some(Tok::RParen) => break,
                Some(t) => bail!("expected , or ) in arg list, got {:?}", t),
                None => bail!("expected , or ) in arg list, got end of input"),
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

    fn node2vec(&mut self) -> Result<BodyItem> {
        self.ident()?; // node2vec
        self.expect(Tok::LParen)?;
        let rel = self.ident()?;
        self.expect(Tok::RParen)?;
        Ok(BodyItem::Node2vec { rel })
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

    use crate::ast::{Item, Temporal};
    use crate::lex::lex;

    fn one_rule(src: &str) -> crate::ast::Rule {
        let prog = super::parse(lex(src).unwrap()).unwrap();
        match prog.items.into_iter().next().unwrap() {
            Item::Rule(r) => r,
            other => panic!("expected a rule, got {other:?}"),
        }
    }

    fn one_item(src: &str) -> Item {
        super::parse(lex(src).unwrap()).unwrap().items.into_iter().next().unwrap()
    }

    #[test]
    fn json_agg_parse_round_trip() {
        use crate::ast::AggFn;
        // One-arg json_group_array: the arg lands in terms, no second arg.
        let r = one_rule("group_rels(rel_group, json_group_array(rel_name)) <- rel_catalog(rel_name, rel_group, cols, doc).");
        assert_eq!(r.aggs, vec![None, Some(AggFn::JsonGroupArray)]);
        assert!(r.agg_args2.iter().all(|a| a.is_none()));
        assert!(matches!(&r.head.terms[1], Term::Var(v) if v == "rel_name"));

        // Two-arg json_group_object: key in terms, value in agg_args2.
        let r = one_rule("obj_of(g, json_group_object(k, v)) <- src(g, k, v).");
        assert_eq!(r.aggs, vec![None, Some(AggFn::JsonGroupObject)]);
        assert!(matches!(&r.head.terms[1], Term::Var(v) if v == "k"));
        assert!(matches!(&r.agg_args2[1], Some(Term::Var(v)) if v == "v"));

        // A one-arg call in the two-arg slot is a parse error (missing value).
        let e = super::parse(lex("obj_of(g, json_group_object(k)) <- src(g, k, v).").unwrap()).unwrap_err().to_string();
        assert!(e.contains("expected") || e.contains(','), "{e}");

        // No aggregate anywhere: both parallel vecs are empty.
        let r = one_rule("plain(a, b) <- src2(a, b).");
        assert!(r.aggs.is_empty());
        assert!(r.agg_args2.is_empty());
    }

    #[test]
    fn type_decl_three_arms() {
        // `<:` nominal brand: parent set, no variants.
        match one_item("type sha <: text.") {
            Item::Brand(b) => { assert_eq!(b.name, "sha"); assert_eq!(b.parent, "text"); assert!(b.variants.is_none()); }
            other => panic!("expected a brand, got {other:?}"),
        }
        // `=` enum brand: closed string set, parent defaults to text.
        match one_item(r#"type severity = "error" | "warn" | "info"."#) {
            Item::Brand(b) => {
                assert_eq!(b.name, "severity");
                assert_eq!(b.parent, "text");
                assert_eq!(b.variants.as_deref(), Some(&["error".to_string(), "warn".to_string(), "info".to_string()][..]));
            }
            other => panic!("expected an enum brand, got {other:?}"),
        }
        // `(cols)` named shape: column list captured.
        match one_item("type finding(path: text, line: int, sev: severity).") {
            Item::Shape(s) => {
                assert_eq!(s.name, "finding");
                let names: Vec<&str> = s.cols.iter().map(|c| c.name.as_str()).collect();
                assert_eq!(names, ["path", "line", "sev"]);
                // A non-base column type is recorded as a brand reference.
                assert_eq!(s.cols[2].brand.as_deref(), Some("severity"));
            }
            other => panic!("expected a shape, got {other:?}"),
        }
    }

    #[test]
    fn rel_shape_ref_and_type_arm_errors() {
        // `rel name: shape.` records the shape name, cols empty (expanded at load).
        match one_item("rel finding_rel: finding.") {
            Item::Rel(d) => { assert_eq!(d.name, "finding_rel"); assert_eq!(d.shape_ref.as_deref(), Some("finding")); assert!(d.cols.is_empty()); }
            other => panic!("expected a rel, got {other:?}"),
        }
        // A bogus token after the type name names the fix.
        let e = super::parse(lex("type foo bar.").unwrap()).unwrap_err().to_string();
        assert!(e.contains("expected `<:` (brand), `=` (enum), or `(` (shape)"), "{e}");
        // Enum variants must be string literals.
        let e = super::parse(lex("type sev = warn | info.").unwrap()).unwrap_err().to_string();
        assert!(e.contains("enum brand variants must be string literals"), "{e}");
    }

    #[test]
    fn temporal_modifiers_parse() {
        // No modifier: today's deductive rule.
        assert_eq!(one_rule("p(X) <- q(X).").temporal, None);
        // `@next` and `@async` after the neck, with and without a leading space.
        assert_eq!(one_rule("p(X) <- @next q(X).").temporal, Some(Temporal::Next));
        assert_eq!(one_rule("p(X) <-@next q(X).").temporal, Some(Temporal::Next));
        assert_eq!(one_rule("p(X) <- @async q(X).").temporal, Some(Temporal::Async));
        // A ground fact carries no modifier.
        assert_eq!(one_rule("p(1).").temporal, None);
    }

    #[test]
    fn unknown_modifier_and_lone_at_are_errors() {
        // Unknown `@word` after a neck is a parse error.
        let e = super::parse(lex("p(X) <- @soon q(X).").unwrap()).unwrap_err();
        assert!(e.to_string().contains("unknown rule modifier `@soon`"), "{e}");
        // A lone `@` is a lex error.
        let e = lex("p(X) <- @ q(X).").unwrap_err();
        assert!(e.to_string().contains("lone '@'"), "{e}");
    }

    use crate::ast::{BodyItem, Term};

    fn body_of(src: &str) -> Vec<BodyItem> {
        one_rule(src).body
    }
    fn str_is(t: &Term, s: &str) -> bool { matches!(t, Term::Str(v) if v == s) }
    fn var_is(t: &Term, s: &str) -> bool { matches!(t, Term::Var(v) if v == s) }
    fn scan_of(src: &str) -> (Term, Term, Term, Term, Term) {
        for b in body_of(src) {
            if let BodyItem::Scan { repo, rev, glob, path, rev_out } = b {
                return (repo, rev, glob, path, rev_out);
            }
        }
        panic!("no scan in rule: {src}");
    }
    fn parse_err(src: &str) -> String {
        super::parse(lex(src).unwrap()).unwrap_err().to_string()
    }

    #[test]
    fn scan_wild_and_omitted_rev_out() {
        // 4-ary with `_` rev_out: rev bound as input, rev_out don't-care.
        let (repo, rev, glob, path, rev_out) = scan_of("f(P) <- scan(\"WORK\", \"**/*.rs\", P, _).");
        assert!(str_is(&repo, "."));
        assert!(str_is(&rev, "WORK"));
        assert!(str_is(&glob, "**/*.rs"));
        assert!(var_is(&path, "P"));
        assert!(matches!(rev_out, Term::Wild));
        // 2-ary: rev_out omitted entirely (repo=".", rev=WORK).
        let (repo, rev, glob, path, rev_out) = scan_of("f(P) <- scan(\"**/*.rs\", P).");
        assert!(str_is(&repo, "."));
        assert!(str_is(&rev, "WORK"));
        assert!(str_is(&glob, "**/*.rs"));
        assert!(var_is(&path, "P"));
        assert!(matches!(rev_out, Term::Wild));
    }

    #[test]
    fn scan_positional_forms_unchanged() {
        // 3-ary (glob, path, rev_out).
        let (repo, rev, _g, path, rev_out) = scan_of("f(P,R) <- scan(\"**/*.rs\", P, R).");
        assert!(str_is(&repo, "."));
        assert!(str_is(&rev, "WORK"));
        assert!(var_is(&path, "P"));
        assert!(var_is(&rev_out, "R"));
        // 5-ary (repo, rev, glob, path, rev_out).
        let (repo, rev, glob, path, rev_out) =
            scan_of("f(P,R) <- scan(\"me\", \"HEAD\", \"**/*.rs\", P, R).");
        assert!(str_is(&repo, "me"));
        assert!(str_is(&rev, "HEAD"));
        assert!(str_is(&glob, "**/*.rs"));
        assert!(var_is(&path, "P"));
        assert!(var_is(&rev_out, "R"));
    }

    #[test]
    fn scan_named_outputs() {
        // Named path, rev_out omitted; single positional input = glob.
        let (repo, rev, glob, path, rev_out) = scan_of("f(P) <- scan(\"**/*.rs\", path: P).");
        assert!(str_is(&repo, "."));
        assert!(str_is(&rev, "WORK"));
        assert!(str_is(&glob, "**/*.rs"));
        assert!(var_is(&path, "P"));
        assert!(matches!(rev_out, Term::Wild));
        // Named path + rev_out; two positional inputs = rev, glob.
        let (repo, rev, glob, path, rev_out) =
            scan_of("f(P,R) <- scan(\"HEAD\", \"**/*.rs\", path: P, rev_out: R).");
        assert!(str_is(&repo, "."));
        assert!(str_is(&rev, "HEAD"));
        assert!(str_is(&glob, "**/*.rs"));
        assert!(var_is(&path, "P"));
        assert!(var_is(&rev_out, "R"));
    }

    #[test]
    fn scan_unknown_named_output_is_error() {
        let e = parse_err("f(P) <- scan(\"**/*.rs\", pat: P).");
        assert!(e.contains("unknown scan output arg `pat`"), "{e}");
    }

    #[test]
    fn match_named_outputs_and_wild() {
        // Named end_col only: id/col default to `_` (None).
        for b in body_of("f(P,L,E) <- scan(\"**/*.rs\", P), match(P, \"WORK\", /x/, L, end_col: E).") {
            if let BodyItem::Match { id, col, end_col, .. } = b {
                assert!(id.is_none());
                assert!(col.is_none());
                assert!(matches!(end_col, Some(t) if var_is(&t, "E")));
                return;
            }
        }
        panic!("no match body item");
    }

    #[test]
    fn match_unknown_named_output_is_error() {
        let e = parse_err("f(P,L) <- scan(\"**/*.rs\", P), match(P, \"WORK\", /x/, L, foo: L).");
        assert!(e.contains("unknown output arg `foo`"), "{e}");
    }

    #[test]
    fn ast_named_id_output() {
        // Named id, end omitted.
        for b in body_of("f(P,L,I) <- scan(\"**/*.rs\", P), ast(P, \"WORK\", :rust, \"(x) @c\", L, id: I).") {
            if let BodyItem::Ast { end, id, .. } = b {
                assert!(end.is_none());
                assert!(matches!(id, Some(t) if var_is(&t, "I")));
                return;
            }
        }
        panic!("no ast body item");
    }

    /// A body bind (`callee = replace(..)`) and a bare var-var equality parse
    /// as DIFFERENT Cmp shapes: the RHS Term (Call vs Var) is the bind gate
    /// downstream (lower's has_computation), so `alias = other` can never be
    /// misread as a computed bind and stays the equality filter it always was.
    #[test]
    fn body_bind_and_var_equality_parse_apart() {
        let body = body_of(r#"resolved(callee) <- raw_edge(callee_q), callee = replace(callee_q, ".", "::")."#);
        let cmp = body.iter().find_map(|b| if let BodyItem::Cmp(c) = b { Some(c) } else { None }).unwrap();
        assert!(matches!(&cmp.lhs, Term::Var(v) if v == "callee"));
        assert!(matches!(&cmp.rhs, Term::Call { name, .. } if name == "replace"));

        let body = body_of("same(a) <- pair(a, b), a = b.");
        let cmp = body.iter().find_map(|b| if let BodyItem::Cmp(c) = b { Some(c) } else { None }).unwrap();
        assert!(matches!(&cmp.lhs, Term::Var(v) if v == "a"));
        assert!(matches!(&cmp.rhs, Term::Var(v) if v == "b"), "var=var stays a plain equality");
    }
}
