//! Body-item source/graph op parsers (`scan`/`match`/`ast`/`sg`/`json`/`cmd`/
//! `comment`/`closure`/`scc`/`node2vec`), relocated from the monolithic
//! `parse.rs` (decomposition plan step 11; the old refactor/file-splits branch
//! validated this exact cut).

use super::*;

impl Parser {
    /// Parse a parenthesized, comma-separated term list `(a, b, c)` (or `()`).
    /// Used for an effect call's response binds (`-> (status, body)`).
    pub(crate) fn paren_terms(&mut self) -> Result<Vec<Term>> {
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

    pub(crate) fn scan(&mut self) -> Result<BodyItem> {
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

    pub(crate) fn match_(&mut self, legacy_name: bool) -> Result<BodyItem> {
        self.ident()?; // match / match_line
        self.expect(Tok::LParen)?;
        let path = self.term()?; self.expect(Tok::Comma)?;
        let rev = self.term()?; self.expect(Tok::Comma)?;
        let regex = match self.next()? {
            Tok::Regex(r) => desugar_regex_holes(&r),
            other => bail!("expected regex literal in match_line, got {:?}", other),
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
                n => bail!("match_line expects 4 args (path, rev, /re/, line), +1 (id), \
                            +2 (col, end_col), or +3 (id, col, end_col), got {} trailing", n),
            }
        } else {
            let outs = Self::assign_outputs(&["id", "col", "end_col"], pos, named)?;
            let mut it = outs.into_iter();
            (opt(it.next().unwrap()), opt(it.next().unwrap()), opt(it.next().unwrap()))
        };
        Ok(BodyItem::Match { path, rev, regex, line, id, col, end_col, legacy_name })
    }

    pub(crate) fn ast(&mut self) -> Result<BodyItem> {
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

    /// FILE form `match_ast(path, rev, :lang, "pat", line, col, end_line, end_col, id)`
    /// vs TERM form `match_ast(:lang, src, "pat", line, col, end_line, end_col)`. The
    /// term form LEADS with `:lang` (a colon right after `(`), so a colon in the
    /// first slot dispatches the term form; anything else is the file form (where
    /// `:lang` sits third). The term form's `src` is a bound `str` value (an
    /// embedded-language body); spans are region-relative and there is no `id`.
    /// `legacy_name` is `true` when the program spelled it `sg` (the pre-rename
    /// name, kept working as a deprecated alias) — see the `Sg` variant doc.
    pub(crate) fn sg(&mut self, legacy_name: bool) -> Result<BodyItem> {
        self.ident()?; // sg / match_ast
        self.expect(Tok::LParen)?;
        if matches!(self.peek(), Some(Tok::Colon)) {
            return self.sg_term(legacy_name);
        }
        let path = self.term()?; self.expect(Tok::Comma)?;
        let rev = self.term()?; self.expect(Tok::Comma)?;
        self.expect(Tok::Colon)?;
        let lang = self.ident()?;
        self.expect(Tok::Comma)?;
        let pattern = match self.next()? {
            Tok::Str(s) => s,
            other => bail!("expected pattern string in match_ast(), got {:?}", other),
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
        // as `ast`/`match_line` so a rule joins `ref(id, _, _, lo, hi)` for the
        // codemod anchor (christmas #9, decision 3 — consistent across ast/
        // match_ast/json).
        let outs = Self::assign_outputs(&["line", "col", "end_line", "end_col", "id"], pos, named)?;
        let id = match &outs[4] { Term::Wild => None, t => Some(t.clone()) };
        self.expect(Tok::RParen)?;
        Ok(BodyItem::Sg {
            src: path, rev: Some(rev), lang, pattern,
            line: outs[0].clone(), col: outs[1].clone(),
            end_line: outs[2].clone(), end_col: outs[3].clone(),
            id, legacy_name,
        })
    }

    /// TERM form `match_ast(:lang, src, "pat", line, col, end_line, end_col)` —
    /// the embedded-language seam. `src` is a `str` value bound earlier in the
    /// rule (a styled-components css body, a markdown fence, a response column).
    /// Mirrors the term form of `jsonp`/`json`: no file, no `rev`, no `id`; the
    /// join binds `src` and this op extracts over the bound string. Spans are
    /// relative to the bound string. The colon after `(` was already peeked by
    /// `sg`/`match_ast`.
    pub(crate) fn sg_term(&mut self, legacy_name: bool) -> Result<BodyItem> {
        self.expect(Tok::Colon)?;
        let lang = self.ident()?;
        self.expect(Tok::Comma)?;
        let src = self.term()?;
        self.expect(Tok::Comma)?;
        let pattern = match self.next()? {
            Tok::Str(s) => s,
            other => bail!("expected pattern string in match_ast(:lang, src, \"pat\"), got {:?}", other),
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
            id: None, legacy_name,
        })
    }

    /// `ast_yaml(path, rev, :lang, `yaml body`, line, ...)` — mirrors `sg()`
    /// but the 4th arg is a (usually backtick, multiline) ast-grep RuleCore
    /// YAML body instead of a pattern string. The body lexes as a normal
    /// `Tok::Str` (the backtick form is multiline + raw), so only the field
    /// name differs from `sg()`. Span outputs share the kwarg/`_` form.
    pub(crate) fn ast_yaml(&mut self) -> Result<BodyItem> {
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
    pub(crate) fn jsonp(&mut self) -> Result<BodyItem> {
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
    pub(crate) fn json(&mut self) -> Result<BodyItem> {
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

    pub(crate) fn cmd(&mut self) -> Result<BodyItem> {
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

    /// `comment(p, rev, /open/[, /close/], l0: .., l1: .., label: ..)` — one
    /// regex is sequential mode, two is paired. Both regexes take `$NAME` holes.
    /// The three outputs (l0, l1, label) accept the kwarg form: positional
    /// (current), `_` for an unwanted slot, or `name: term` to bind only what
    /// you need and default the rest to `_`.
    pub(crate) fn comment(&mut self) -> Result<BodyItem> {
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

    pub(crate) fn closure(&mut self) -> Result<BodyItem> {
        self.ident()?; // closure
        self.expect(Tok::LParen)?;
        let rel = self.ident()?;
        self.expect(Tok::RParen)?;
        Ok(BodyItem::Closure { rel })
    }

    pub(crate) fn scc(&mut self) -> Result<BodyItem> {
        self.ident()?; // scc
        self.expect(Tok::LParen)?;
        let rel = self.ident()?;
        self.expect(Tok::RParen)?;
        Ok(BodyItem::Scc { rel })
    }

    pub(crate) fn node2vec(&mut self) -> Result<BodyItem> {
        self.ident()?; // node2vec
        self.expect(Tok::LParen)?;
        let rel = self.ident()?;
        self.expect(Tok::RParen)?;
        Ok(BodyItem::Node2vec { rel })
    }
}
