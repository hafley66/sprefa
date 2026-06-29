# Plan: `sh` template decls + the effect runtime increments

Companion to `docs/research-reactive-effectful-datalog.md` §9/§10. The temporal core
(`@next` carry, `@async` queue, `drain_effects`, `ShellEffectExec`, `DL_POLL_SECS`) is built
(commits bd7eb6e / 89fc8d7 / f84fa17). This plans the next increments. Signature-first per
the planning protocol. Nothing here is built.

Grounding facts (verified in tree):
- `def name(p1, p2) <- body.` is an EXISTING parameterized rule template (`RuleTemplate`,
  parse.rs:81). `sh` is its shell-callable sibling — same "named, parameterized, spliced"
  shape, different expansion target.
- Backtick multiline bodies already lex to `Tok::Str` (lex.rs:82). The `sh` body needs no new
  lexer work; `{param}` holes are plain-brace string-replace like the `cmd` op's `{file}`.
- `Tok::Arrow` is `<-` only. `->` does not lex yet — it is the one new token.
- `Tok::Star` exists, so `sh*` lexes as `Ident("sh")` + `Star` with no lexer change.

---

## Phase 0 — parallel/batch drain (warm-up, no new surface)

The smallest increment and it kills a live N+1: `drain_effects` runs one `sh -c` per row,
serially. This is the per-row-write ban in spawn space.

```rust
// engine.rs — drain_effects inner loop goes from serial to a bounded pool.
// pseudo:
//   let jobs: Vec<(id, kind, args)> = pending WHERE done=0;
//   let results: Vec<(id, Vec<Value>)> = jobs.par_iter()         // rayon, like the cmd op
//       .map(|(id,kind,args)| (id, exec.run(kind,args)))
//       .collect();                                              // collect-then-flush
//   for (id, row) in results { insert_rows(head, [row]); mark_done(id); }
```

- Lifetime: the pool is per-drain, no state.
- Uniqueness: each `mark_done` is its own row; parallel-safe (no shared write target but the
  per-kind head rel, and `insert_rows` is OR IGNORE).
- Budget: add a per-poll spawn budget mirroring `cmd_budget()` / `CMD_COUNT`, so a broad
  request set is a loud bail, not a fork storm.
- Open: engine-owned rayon (this) vs shell-owned `xargs -P` batch (§9.5b). Do rayon first;
  the `xargs` batch rides the `sh*` body form (Phase 2) for free.

This phase is independent of everything below. Land it alone.

---

## Phase 1 — the `sh` template decl + effect-call body item (the language surface)

### 1.1 Type signatures

```rust
// lex.rs — one new token.
Tok::ThinArrow            // "->"  (effect output arrow; distinct from Arrow "<-")

// ast.rs — a new Item variant, sibling to RRuleTemplate/Rel/Rule.
pub struct ShellFn {
    pub name: String,            // call name == effect kind
    pub params: Vec<String>,     // {hole} names == request args, in order
    pub outs: Vec<Col>,          // typed response tuple (reuses Col { name, ty })
    pub body: String,            // raw shell (a Tok::Str backtick block); {param} holes
    pub streaming: bool,         // sh*  => yields many rows over time (Phase 3)
}
pub enum Item { /* Rel, Rule, Query, ... */ Shell(ShellFn) }

// ast.rs — a new BodyItem: the effect CALL inside an @async/@stream rule body.
BodyItem::Effect {
    name: String,                // resolves to a ShellFn at typecheck
    args: Vec<Term>,             // fill the params (must be body-bound vars/lits)
    outs: Vec<Term>,             // bind the response columns (new vars)
}
```

### 1.2 Surface grammar

```
sh gh(repo, path) -> (status: int, body: str) {
    gh api "repos/{repo}/contents/{path}" -w "\n%{http_code}"
}

sh sha(file) -> (digest: str) = `git hash-object {file}`.

resp(repo, path, status, body) <-@async
    want(repo, path), gh(repo, path) -> (status, body).
```

- `sh` (and `sh*`) reserved at item-leading position. A rel named `sh` becomes illegal — same
  cost as `ref` being reserved. **Decision D-1 below.**
- Body: a brace block `{ ... }` OR `= \`...\`.` single-line. Both capture a raw Tok::Str.
- In a rule body, `name(args) -> (outs)` parses as `BodyItem::Effect`. Needs `->`.

### 1.3 Pseudo-bodies (parse + typecheck + drain threading)

```rust
// parse.rs — at item dispatch, mirroring the `def` template arm (parse.rs:63).
//   if peek == Ident("sh"):
//     streaming = (peek2 == Star); consume sh [*]
//     name = ident; params = paren_idents();
//     expect ThinArrow; outs = paren_typed_cols();
//     body = brace_raw() | (expect Eq, backtick Str, expect Dot);
//     push Item::Shell(ShellFn{..})

// typecheck.rs — bind every BodyItem::Effect to a declared ShellFn:
//   - name must resolve to a ShellFn (else error, like unknown relation)
//   - args.len() == params.len(); each {param} hole must appear in body
//   - outs.len() == ShellFn.outs.len(); outs are fresh vars, typed from ShellFn.outs
//   - the rule carrying an Effect MUST be @async (or @stream); a plain rule with an
//     Effect body item is an error (effects only fire off-tick)

// engine.rs — the drain model CHANGES from the §8 built form. See Decision D-3.
//   rebuild_async: project the Effect.args (not the head terms) into pending_effect;
//                  kind = ShellFn.name; args_json = {param: value}.
//   drain_effects: run ShellFn.body with holes filled -> outs; then evaluate the
//                  rule HEAD over (body solution  UNION  {out_col: out_value}).
//                  The head is now an ordinary projection, not the response itself.
```

### 1.4 Instance lifetimes

| state | holder | lifetime |
|---|---|---|
| `ShellFn` registry (name -> decl) | parsed `Program` | the program's life; rebuilt on reload |
| `BodyItem::Effect` | the `@async`/`@stream` rule | static, part of the rule |
| the filled command line | per drained job | one subprocess call |

No new persistent state in Phase 1 beyond what `pending_effect` already holds — except the
drain must reconstruct the head from (body solution + effect outs), so `args_json` must carry
the FULL body solution, not just the effect args. **Decision D-4.**

### 1.5 Decisions needing taste

- **D-1 — reserve `sh`/`sh*`?** Yes (cheap, like `ref`/`use`/`def`). Alternative: a sigil
  (`$gh(...)`) to avoid reservation. Reservation reads best; sigil is uglier but collision-free.
- **D-2 — add `->`?** Yes, one lexer arm (`-` then `>`). It is the natural effect-output arrow
  and reads against the `<-` neck. Alternative: a `yields` keyword (no new token, more verbose).
- **D-3 — the model change (the big one).** §8 built "head IS the response, unbound head terms
  are the outputs." The `sh`-fn form is "effect call in the BODY produces outputs, head is a
  normal projection." The body form is cleaner and is what §9.2 shows. Options:
  - (a) Migrate: the body-effect form is the only form; the head-response form is dropped.
  - (b) Keep both: head-response stays as zero-ceremony sugar, body-effect is the named/typed
    upgrade. More surface, two code paths in the drain.
  Lean (a) for one model, but it rewrites the three §8 tests. Decide before building.
- **D-4 — `pending_effect.args_json` carries the full body solution** (so drain can rebuild a
  head that mixes body vars and effect outs), not just the effect args. Slightly bigger rows;
  the digest id should key on the EFFECT args only (so the same request dedups even if an
  unrelated body var differs). So: `id = blake3(kind, effect_args)`, `args_json = full body
  solution`. Two fields, two roles.
- **D-5 — `sh` decl subsumes `effect_cmd(kind, template)` rows.** Once `sh`-fns exist, the
  daemon builds `ShellEffectExec` from the program's `ShellFn` registry, not from an
  `effect_cmd` relation. Drop `effect_cmd`, or keep it as a dynamic (data-driven) fallback for
  templates not known at parse time. Lean: drop it; `sh` is the one way.

### 1.6 Resolutions (2026-06-29, after the taste pass)

- **D-1 — RESOLVED: reserve `sh` / `sh!` / `sh*`.** They join the item-leading keyword club
  (`rel`/`def`/`use`/`query`). Cost: a rel/var can no longer be named `sh` (loud error, trivial
  rename) — the same deal `ref`/`use`/`def` already took.
- **D-2 — RESOLVED: add `->` (`Tok::ThinArrow`), its OWN highlight scope, not the neck's.**
  `<-` stays `keyword.operator.arrow.dl`; `->` is `keyword.operator.effect.dl` (own color) in
  the tmLanguage AND a new `effect_arrow` node in `tree-sitter-dl`. The asymmetry is the point:
  `<-` is deduction, `->` reaches out and comes back.
- **D-3 — RESOLVED: both, via desugar.** The head-response form lowers to the body-effect form
  with an anonymous `sh`-fn (bound head terms -> effect args, unbound head terms -> effect outs).
  The runtime sees ONE model (body-effect); the §8 tests keep passing on the sugar.
- **D-4 — RESOLVED (owned, mechanical):** two fields. `id = blake3(kind, effect_args)` is the
  identity / interrupt / cache key; `args_json` is the full body solution, the head-rebuild
  payload.
- **D-5 — RESOLVED: the bang is the read/mutate line.** Three decl forms, all already lex
  (`Star`, `Bang` exist):
  - `sh f(..)`  = READ/query: cached + deduped by digest, at-least-once OK, parallel, killable,
    auto-retry, CALM-monotone downstream.
  - `sh! f(..)` = MUTATE: idempotency key + two-phase mark (claim/run/commit), exactly-once,
    never cached, not auto-retried, the §10 idempotency guard binds here. Also the capability
    boundary (a loaded snippet may be allowed `sh` but not `sh!`).
  - `sh* f(..)` = STREAM (read, many rows over time): the `@stream` generator (Phase 4).
  `sh` subsumes `effect_cmd`; the daemon builds the executor from the `ShellFn` registry.

---

## Phase 2 — `every(N)` clock relation (per-stream cadence)

```rust
// A built-in source relation, non-empty only on the wall-clock boundary.
//   every(secs: int)            // 1-col: emits a single row on the tick where
//                               // now_secs % secs == 0 (daemon base resolution)
// engine.rs: refresh it in the source phase from SystemTime; the daemon poll loop
// runs at the base resolution and each every(N)-gated rule self-throttles.
```

- Retires `DL_POLL_SECS` as the only knob (it becomes "base scheduler tick").
- Cadence is now a fact, per-rule, composable (`every(30), weekday()`).
- Small: one source op, no storage. Independent of Phase 1.

---

## Phase 3 — job table + reconcile drain (the structural move)

The guard-bearing change (§9.4, §10.3). `pending_effect` grows into a job table; the drain
becomes a desired-vs-running reconcile.

```
pending_effect(
  id TEXT PRIMARY KEY,          -- blake3(kind, effect_args): identity AND interrupt key
  kind TEXT, args_json TEXT, req_tx INTEGER,
  state TEXT,                   -- queued | running | done | failed
  pid INTEGER, started_at INTEGER,
  idem_key TEXT                 -- D-3 of §10: exactly-once for MUTATING effects
)
```

```rust
// drain becomes reconcile(desired, running):
//   desired = effect-arg digests the @async rules derive THIS tick
//   running = rows with a live pid
//   desired \ running  -> spawn (.spawn(), record pid; rayon-bounded)
//   running \ desired   -> SIGTERM (input changed/retracted; kill by digest)
//   running & over-timeout -> kill, state=failed
//   exited              -> reap, project head, state=done
```

Guards land HERE (design them in now, per §10.3):
- determinism quarantine: effect-tainted rels are a marked subspace; CALM check at stratify.
- mutating idempotency: a `sh!`-fn (bang = mutating) requires an `idem_key` and a two-phase
  mark (claim -> run -> commit), so a crash mid-flight does not double-POST.
- no DD creep: the reconcile is a flat `SELECT` diff per poll, never a dependency graph.

This phase is the precondition for streaming.

---

## Phase 4 — `sh*` streaming generator (`@stream`, the coroutine shape)

```
sh* events(repo) -> (kind: str, at: str) {
    gh api "repos/{repo}/events" --paginate --jq '.[] | "\(.type)\t\(.created_at)"'
}
event(repo, kind, at) <-@stream events(repo) -> (kind, at).
```

```rust
// ast.rs: Temporal::Stream;  a @stream rule carries a sh* Effect.
// runtime: a sh* job stays state=running (never auto-done); its child is fed through a
//   reader thread that batches stdout lines into the head rel between ticks. @next carries
//   the read cursor / last-seen line so a restart resumes. Rides the Phase 3 job table.
```

The genuinely new primitive (Bloom has @next/@async, not this). The "yield + coroutines"
direction, kept on the tx spine.

---

## Phase 5 — native `HttpEffectExec` + etag/304 (the last ghcacher piece)

Sibling `EffectExec` impl (ureq/reqwest) returning `(status, headers_json, body)`; dispatch by
the `sh`-fn body scheme or an attribute. The etag/304 guard is an `@async` reading last tick's
etag carry (`@next`) to skip an unchanged fetch — clean native, awkward in shell. This closes
the conditional-request cache: the point where the poller becomes an actual cache.

---

## Build order

`Phase 0 (parallel drain)` -> `Phase 2 (every)` -> `Phase 1 (sh decl)` -> `Phase 3 (job
table + guards)` -> `Phase 4 (sh* stream)` -> `Phase 5 (http/etag)`.

Phases 0 and 2 are small, independent, and ship value alone. Phase 1 is the taste-defining
surface (decide D-1..D-5 first). Phase 3 is where the §10 guards must be designed in. Phases 4
and 5 ride Phase 3.

---

## Zoom Level 3 — logic filled in (one step from real)

The interesting logic written out against the real tree (lex.rs byte-loop, parse.rs `def`
template arm at 81, ast.rs `Item`/`BodyItem`/`Term`/`Col`, engine.rs `rebuild_async` /
`drain_effects` / `pending_effect`). `// ...` marks boring boilerplate. Reflects the §1.6
resolutions: reserve `sh`, `->` token + own scope, desugar both surfaces, two-field
`pending_effect`, `sh`/`sh!`/`sh*` = read/mutate/stream.

### Z3.1 Lexer — the `->` arm (lex.rs)

```rust
// In the byte loop. Today `-` lexes to Minus (arithmetic). Peek for `>` first.
b'-' => {
    if b.get(i + 1) == Some(&b'>') { out.push(Tok::ThinArrow); i += 2; }
    else                           { out.push(Tok::Minus);     i += 1; }
}
// `sh` is NOT a lexer keyword — it lexes as Ident("sh"); the PARSER reserves it at
// item-leading position. `sh*` = Ident("sh") + Star, `sh!` = Ident("sh") + Bang,
// all three already lex with zero new arms beyond ThinArrow.
```

### Z3.2 Grammar highlight (two files, lockstep)

```jsonc
// editors/vscode-dl/syntaxes/dl.tmLanguage.json — sibling to "rule-arrow"
"effect-arrow": { "name": "keyword.operator.effect.dl", "match": "->" },
"sh-keyword":   { "name": "keyword.control.sh.dl",      "match": "\\bsh[!*]?(?=\\s+[a-z_])" },
// add both to the top-level patterns list. The lookahead keeps `sh` highlighted only
// when it heads a decl (followed by a fn name), not when it's incidental text.

// tree-sitter-dl/src/grammar.json — new nodes
//   effect_arrow: "->"
//   shell_fn:  seq( choice("sh","sh!","sh*"), name, params, "->", out_cols, body )
//   effect_call (a body item): seq( name, "(", args, ")", "->", "(", outs, ")" )
```

### Z3.3 AST (ast.rs)

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellKind { Read, Mutate, Stream }     // sh | sh! | sh*

#[derive(Clone, Debug)]
pub struct ShellFn {
    pub name: String,
    pub params: Vec<String>,
    pub outs: Vec<Col>,          // reuse Col { name, ty }
    pub body: String,            // raw shell, {param} holes; a Tok::Str backtick/brace block
    pub kind: ShellKind,
}
pub enum Item { /* Rel, Rule, Query, ... */ Shell(ShellFn) }

// the effect CALL inside an @async/@stream rule body
BodyItem::Effect { name: String, args: Vec<Term>, outs: Vec<Term> }

pub enum Temporal { Next, Async, Stream }       // + Stream for sh*

impl Rule {
    pub fn is_stream(&self) -> bool { self.temporal == Some(Temporal::Stream) }
    // the single Effect body item, if any (validated to be 0-or-1 at typecheck)
    pub fn effect(&self) -> Option<(&str, &[Term], &[Term])> {
        self.body.iter().find_map(|b| match b {
            BodyItem::Effect { name, args, outs } => Some((name.as_str(), &args[..], &outs[..])),
            _ => None,
        })
    }
}
```

### Z3.4 Parser — `sh` decl + Effect body item (parse.rs)

```rust
// item dispatch, mirroring the `def` arm (parse.rs:63). Reserve sh/sh!/sh*.
if let Some(Tok::Ident(w)) = self.peek() {
    if w == "sh" {
        let kind = match self.peek2() {
            Some(Tok::Bang) => ShellKind::Mutate,
            Some(Tok::Star) => ShellKind::Stream,
            _               => ShellKind::Read,
        };
        return self.shell_fn(kind);
    }
}

fn shell_fn(&mut self, kind: ShellKind) -> Result<Item> {
    self.next();                                   // consume `sh`
    if kind != ShellKind::Read { self.next(); }    // consume `!` or `*`
    let name = self.ident()?;
    let params = self.paren_idents()?;             // (repo, path)
    self.expect(Tok::ThinArrow)?;                  // ->
    let outs = self.paren_cols()?;                 // (status: int, body: str)  (typed)
    // body: a brace block `{ ... }` OR `= <backtick Str> .`
    let body = match self.peek() {
        Some(Tok::LBrace) => self.brace_raw()?,    // capture raw text to matching }
        Some(Tok::Eq)     => { self.next(); let s = self.str_lit()?; self.expect(Tok::Dot)?; s }
        other => bail!("sh fn `{name}`: expected `{{ shell }}` or `= `...`.`, got {other:?}"),
    };
    // hole-coverage check deferred to typecheck (needs the param set anyway)
    Ok(Item::Shell(ShellFn { name, params, outs, body, kind }))
}

// inside body parsing (where Pos/Neg atoms are read): an Effect call.
//   gh(repo, path) -> (status, body)
// Disambiguate from a plain atom by the trailing `-> (`. Parse the atom-shaped
// `name(args)`, then if the next token is ThinArrow, it's an Effect, else a Pos atom.
fn body_atom_or_effect(&mut self) -> Result<BodyItem> {
    let name = self.ident()?;
    let args = self.paren_terms()?;
    if self.peek() == Some(&Tok::ThinArrow) {
        self.next();
        let outs = self.paren_terms()?;            // fresh vars to bind
        return Ok(BodyItem::Effect { name, args, outs });
    }
    Ok(BodyItem::Pos(Atom { rel: name, terms: args }))
}
```

### Z3.5 Desugar — head-response sugar -> body-effect (frontend, post-parse)

```rust
// D-3: the §8 head-response form lowers to the canonical body-effect form so the
// runtime has ONE model. Run over every @async/@stream rule that has NO Effect body
// item yet (a rule WITH an explicit Effect is already canonical).
fn desugar_head_response(rule: &mut Rule, rels: &Rels) {
    if !rule.is_async() && !rule.is_stream() { return; }
    if rule.effect().is_some() { return; }                  // already body-effect
    let bound = body_bound_vars(&rule.body);                // distinct Pos-atom vars
    // bound head terms -> effect args; unbound head vars -> effect outs (the response)
    let mut args = Vec::new();
    let mut outs = Vec::new();
    for t in &rule.head.terms {
        match t {
            Term::Var(v) if bound.contains(v) => args.push(t.clone()),
            Term::Var(_)                      => outs.push(t.clone()),
            _lit                              => args.push(t.clone()),
        }
    }
    // kind = the head rel name (the §8 convention); its template comes from an
    // anonymous ShellFn the frontend synthesizes from the legacy effect_cmd row, OR
    // (transition) the daemon still maps kind->template. The body item makes the
    // drain uniform regardless.
    rule.body.push(BodyItem::Effect { name: rule.head.rel.clone(), args, outs });
}
```

### Z3.6 Typecheck — bind Effect to a ShellFn (typecheck.rs)

```rust
// after the ShellFn registry is built from Item::Shell:
fn check_effect(rule: &Rule, fns: &HashMap<String, &ShellFn>, errs: &mut Vec<TypeDiag>) {
    let n_eff = rule.body.iter().filter(|b| matches!(b, BodyItem::Effect{..})).count();
    if n_eff > 1 { errs.push(/* "at most one effect per rule" */); }
    if n_eff == 1 && rule.temporal.is_none() {
        errs.push(/* "an effect call requires @async/@stream; effects fire off-tick" */);
    }
    if let Some((name, args, outs)) = rule.effect() {
        let Some(f) = fns.get(name) else { errs.push(/* unknown sh fn */); return };
        if args.len() != f.params.len() { errs.push(/* arity */); }
        if outs.len() != f.outs.len()   { errs.push(/* out arity */); }
        // {param} holes must all appear in the body text
        for p in &f.params { if !f.body.contains(&format!("{{{p}}}")) {
            errs.push(/* unused param / missing hole */); } }
        // a Stream rule must call a sh* fn; @async must call sh|sh!; not crossed
        match (rule.temporal, f.kind) {
            (Some(Temporal::Stream), ShellKind::Stream) => {}
            (Some(Temporal::Async),  ShellKind::Read | ShellKind::Mutate) => {}
            _ => errs.push(/* "temporal modifier and sh kind disagree" */),
        }
        // bind each out var's type from f.outs into the rule's var env
        // ...
    }
}
```

### Z3.7 Engine — `rebuild_async` over the body-effect model (engine.rs)

```rust
fn rebuild_async(&self, async_rules: &[&Rule], cur: i64) -> Result<()> {
    let mut rows: Vec<Vec<Value>> = Vec::new();
    for r in async_rules {
        let (kind, eff_args, _outs) = r.effect().expect("desugared: every @async has an Effect");
        // bound vars of the body (for the full solution) AND the effect-arg exprs.
        let body_vars = body_bound_vars(&r.body);                  // ordered distinct
        let sel = lower_body_projection(&r.body, &self.rels, &body_vars)?;   // SELECT DISTINCT ...
        for sol in self.run_select_as_objs(&sel, &body_vars)? {    // Vec<serde_json Map>
            // effect args = eval each arg Term against this solution (var -> sol[var], lits as-is)
            let arg_obj = eval_terms_to_obj(eff_args, &sol, &self.shell_fns[kind]?.params);
            let args_json = serde_json::Value::Object(arg_obj.clone()).to_string();
            let id = blake3::hash(format!("{kind}\0{args_json}").as_bytes()).to_hex().to_string();
            let full = serde_json::Value::Object(sol).to_string();  // D-4: the rebuild payload
            // (id, kind, effect_args_json, full_solution_json, req_tx, state='queued')
            rows.push(vec![ Value::Text(id), Value::Text(kind.into()),
                            Value::Text(args_json), Value::Text(full),
                            Value::Int(cur), Value::Text("queued".into()) ]);
        }
    }
    self.db.insert_rows("pending_effect",
        &["id","kind","args_json","full_json","req_tx","state"], &rows)?;   // OR IGNORE on id
    Ok(())
}
```

### Z3.8 Engine — `drain_effects` rebuilds the head, honors the bang (engine.rs)

```rust
pub fn drain_effects(&mut self, prog: &Program, exec: &dyn EffectExec) -> Result<usize> {
    let fns = shell_fn_registry(prog);                         // name -> &ShellFn
    // desired-vs-running reconcile is Phase 3; this Phase-1 form just runs queued rows.
    let queued = self.select_pending("state = 'queued'")?;     // (id, kind, args_json, full_json)
    let mut drained = 0;
    for job in queued {
        let f = fns.get(&job.kind).ok_or_else(|| /* kind not in program */)?;
        // MUTATE (sh!): two-phase claim so a crash mid-flight cannot double-fire.
        if f.kind == ShellKind::Mutate {
            // claim: queued -> running, only if still queued (atomic guard)
            if self.db.conn().execute(
                "UPDATE pending_effect SET state='running' WHERE id=?1 AND state='queued'",
                [&job.id])? == 0 { continue; }                 // someone else claimed it
        }
        let args: Map = serde_json::from_str(&job.args_json)?;
        let outs = exec.run(&job.kind, &args)?;                // the real sh -c
        if outs.len() != f.outs.len() { bail!(/* arity at runtime */); }
        // rebuild the HEAD row over (full body solution) UNION (out_col -> out_val).
        let rule = async_rule_for(prog, &job.kind);            // the @async rule whose effect kind matches
        let mut env: Map = serde_json::from_str(&job.full_json)?;   // D-4 payload
        for (c, v) in f.outs.iter().zip(outs) { env.insert(c.name.clone(), json!(v)); }
        let head_row: Vec<Value> = rule.head.terms.iter()
            .map(|t| eval_term_in_env(t, &env)).collect();     // body vars + effect outs both resolve
        let cols = self.rels[&rule.head.rel].cols.iter().map(|c| c.name.as_str()).collect::<Vec<_>>();
        self.db.insert_rows(&tbl(&rule.head.rel), &cols, &[head_row])?;
        // READ effects: at-least-once is fine -> done. MUTATE: commit the two-phase.
        self.db.conn().execute("UPDATE pending_effect SET state='done' WHERE id=?1", [&job.id])?;
        drained += 1;
    }
    Ok(drained)
}
```

### Z3.9 `every(N)` clock source (Phase 2, engine.rs source phase)

```rust
// refresh_builtin_rels-adjacent: every(secs) is non-empty only on the boundary tick.
fn refresh_every(&self, secs_set: &[i64]) -> Result<()> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
    let base = poll_base_secs();                                // daemon resolution, e.g. 1
    let mut rows = Vec::new();
    for &n in secs_set {                                        // each every(N) literal in the program
        if now % n < base { rows.push(vec![Value::Int(n)]); }  // crossed an N-boundary this tick
    }
    self.db.conn().execute("DELETE FROM rel_every", [])?;      // it is ephemeral, not derived
    self.db.insert_rows("rel_every", &["secs"], &rows)?;
    Ok(())
}
// rule usage: resp(..) <-@async want(..), gh(..)->(..), every(30).
//   the every(30) Pos atom is empty except on 30s boundaries, so the rule (and its
//   pending_effect emission) self-throttles. DL_POLL_SECS degrades to poll_base_secs.
```

### Z3.10 Phase 3 reconcile (job table) — the desired-vs-running loop

```rust
fn reconcile_effects(&mut self, prog: &Program, exec: &dyn EffectExec) -> Result<()> {
    // desired = the effect-arg digests the @async rules derive THIS tick (rebuild_async
    //   already wrote them as state='queued'); running = rows with a live pid.
    let desired: HashSet<String> = self.select_pending("state IN ('queued','running')")?
        .into_iter().map(|j| j.id).collect();
    let running: Vec<Job> = self.select_pending("state='running' AND pid IS NOT NULL")?;
    // 1. running but no longer desired -> the input changed/retracted: kill by digest.
    for j in &running {
        if !desired.contains(&j.id) {
            kill_pid(j.pid);                                   // SIGTERM; the digest IS the key
            self.mark(&j.id, "failed")?;
        } else if over_timeout(j) {
            kill_pid(j.pid); self.mark(&j.id, "failed")?;
        }
    }
    // 2. queued and not running -> spawn (rayon-bounded; per-poll spawn budget).
    let to_spawn = self.select_pending("state='queued'")?;
    spawn_budget_guard(to_spawn.len())?;                       // loud bail, mirrors cmd_budget
    let results: Vec<(String, Result<Vec<String>>)> = to_spawn.par_iter()
        .map(|j| (j.id.clone(), exec.run(&j.kind, &parse_args(&j.args_json)))).collect();
    // 3. reap: project heads, flip state (READ -> done; MUTATE already two-phased in Z3.8).
    for (id, res) in results { /* insert head row on Ok, mark done|failed ... */ }
    Ok(())
}
// Guard hooks (design in now, §10.3): effect-tainted rels marked at stratify (determinism
// quarantine); sh! requires idem_key for exactly-once; this stays a flat SELECT diff, never
// a reactive dependency graph (no DD creep).
```

### Z3.11 What stays `// ...` (boring, deferred to build time)

- serde row <-> struct (`Job`, `select_pending`, `run_select_as_objs`, `eval_terms_to_obj`).
- `brace_raw` / `paren_cols` / `paren_idents` parser helpers (mechanical token munching).
- `kill_pid` / `over_timeout` / `spawn_budget_guard` (libc signal, a timestamp compare, an atomic).
- tmLanguage/tree-sitter JSON beyond the two new nodes.
- the `sh*` reader-thread plumbing (Phase 4) — stdout lines -> batched rel inserts between ticks.

### Z3.12 Test seams (what each phase asserts)

| phase | test |
|---|---|
| 0 parallel drain | N queued effects drain under one bounded pool; spawn budget bails loudly over cap |
| 1 sh decl | `sh gh(..) -> (..)` parses; an Effect body item binds; desugar makes head-response identical to body-effect (same `rel_resp`); a `sh!` two-phases; arity/hole/temporal-cross errors fire |
| 2 every | `every(30)` rel empty off-boundary, 1 row on-boundary; a gated @async emits only on the boundary |
| 3 job table | input change kills the stale pid; over-timeout -> failed; reap projects the head |
| 4 sh* stream | a `tail -f`-shaped sh* appends rows across ticks; @next carries the cursor across a restart |
| 5 http/etag | a 304 (etag carried via @next) skips the fetch and reuses the prior body |
