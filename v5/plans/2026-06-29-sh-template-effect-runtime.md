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
