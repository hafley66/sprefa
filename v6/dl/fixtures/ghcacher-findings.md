# ghcacher expression in v6 dl — findings + grading receipts

Phase 1 of plans/2026-07-27-v5-port-perf-header.md. Program: `v6/dl/fixtures/ghcacher.dl6`.
Twin: `examples/gh-cache.dl` (v5, 141 lines). Extraction-lab discipline held: zero
changes to `v6/dl/src/`, `v6/dl/grammar/`, or `package.json` — verified at the bottom
of this file (`git status` diff scope).

Everything below is read from the actual grammar/runtime source or reproduced against
a live server in this worktree; nothing is inferred from the v5 doc alone. Per the
repo's standing "doubt yourself before asserting" law, sections that rest on static
reading rather than a live reproduction say so explicitly, and the one place I could
not fully root-cause (the crash in finding F7) says that too instead of guessing.

## Grading 1 — parse/accept check

Booted the server against a scratch db in this worktree, non-default port, exactly as
instructed (`src/main.ts:17-18` reads `DL_DB_PATH`/`DL_PORT`; route shapes read from
`src/6_http.ts:113-120`, confirmed against `tests/golden/curl-session.sh:100-127`'s
working invocation pattern):

```
cd v6/dl
DL_DB_PATH=<scratch>/mvp.sqlite DL_PORT=17174 node --experimental-transform-types src/main.ts
curl -s -X POST http://127.0.0.1:17174/edb/program --data-binary @fixtures/ghcacher.dl6
```

Result: **HTTP 200, accepted.**

```json
{"loaded":true,"rels":["watch","etag","poll","resp","stars","full_name","change_log",
"file","node","edge","sig","site","const","span_line","diag","__resp_fetch",
"__req_fetch","__resp_extract_stars","__req_extract_stars","__resp_extract_full_name",
"__req_extract_full_name","__lit_0","__lit_1","__lit_2","__lit_3"],
"minted":["__lit_0","__lit_1","__req_fetch","__resp_fetch","__req_extract_stars",
"__resp_extract_stars","__req_extract_full_name","__resp_extract_full_name",
"__lit_2","__lit_3"]}
```

Reproduced twice (independent server processes, independent scratch db paths), same
result both times.

Environment note: `gh` (GitHub CLI) and `jq` are both present in this sandbox
(`/opt/homebrew/bin/gh`, `/opt/homebrew/bin/jq`), and `gh api repos/cli/cli -i` was
run directly and returned a real `HTTP/2.0 200 OK` with a real ETag — this sandbox has
live network access and an authenticated `gh`. That materially changes what could be
graded (see Grading 2): this is not a "no-network, parse-only" grading, it is a real
run against the real GitHub API.

## Grading 2 — marble list (what actually ran)

Bridge-time facts: the program seeds `watch("repos/cli/cli").` directly, so the boot
fixpoint (`3_runtime.ts:854`, `evalProgramSql` run synchronously inside `DlRuntime.boot`
before the server ever answers the POST) already derives `etag(ep,"")`, `poll(ep,"")`,
and the `__req_fetch(ep,prev)` demand row for `(ep="repos/cli/cli", prev="")` — no
external stimulus needed. `HostRunner`'s boot-replay (`1_hosts.ts:405-424`) picks that
row up the instant something subscribes to `hostRunner.effects$` (main.ts's one
subscription does this for the live server).

**Marble list, as observed:**

| tick | event |
|---|---|
| boot (before tick 1) | fixpoint derives `etag(repos/cli/cli, "")`, `poll(repos/cli/cli, "")`, `__req_fetch(repos/cli/cli, "")` — all synchronous, no host effect yet |
| effect | `HostRunner` fires `fetch(ep="repos/cli/cli", prev="")`: a REAL `gh api repos/cli/cli -i` subprocess, which genuinely hit GitHub and returned `200`, a real ETag, and the full repo JSON body |
| tick 1 (attempted) | `HostRunner` calls `runtime.commit({insert: {__resp_fetch: [...]}})` to land that response — **this commit throws** (`SQLITE_ERROR: no such column: NaN`, see F7) and kills `deltas$` for the whole process. No further ticks. `stars`/`full_name`/`change_log`/the `extract_stars`/`extract_full_name` probes never get a chance to fire — their own support (`resp`) never lands. |

So the honest marble list is: **one successful boot-time derivation, one real host effect
that itself succeeded (a genuine 200 from GitHub), and a fatal crash on the very first
attempt to commit that effect's response.** Everything downstream of `resp` (stars,
full_name, change_log, the two `jq`-based extraction probes) never ran even once — not
because the surface can't express them, but because the engine died before reaching
them. Content salts (`salt_minting = content_addressed`) never got a chance to be
exercised more than once either, so I cannot report on the v5 12x-retick dedupe receipt
this arc's Phase 0 grading asks for later — there is only one effect firing in this
run's history.

Reproduced twice independently (once via the live HTTP server, once via a direct
in-process driver that boots `DlRuntime` and subscribes `deltas$`/`HostRunner.effects$`
exactly as `6_http.ts:runProgram$` does) — same crash, same stack, both times, and both
times on the very first tick (no `DELTA tick=...` line was ever logged before the
error in the direct driver run).

Server was killed after each run (`kill <pid>`); `lsof -i :<port>` confirmed empty
before moving to the next test. No process was left running.

## Findings

Each finding cites file:line for every grammar/runtime claim, per the style law.

### F1 — no `@async`/`@next` in the grammar at all — verdict: inexpressible as v5 spells it, but subsumed differently

`grep -n "@" v6/dl/grammar/dl.langium` and every `.dl6` fixture under `v6/dl/fixtures/`
returns zero matches. `BodyItem` (`grammar/dl.langium:93-94`) has exactly five
alternatives (`NegItem | ProbeItem | MutationItem | CompareItem | RelRefItem`) — no
annotation production exists anywhere in the grammar.

v6's entire effect-firing surface is the `?` probe (`ProbeItem`, `grammar/dl.langium:
102-103`): `host?(args)` mints a `__req_h` demand rule + `__resp_h` EDB rel
(`0_ast_bridge.ts:25-35`), and `HostRunner` (`1_hosts.ts:357-523`) answers it out of
band, content-addressed on `(host, identityCols, saltCols)` (`1_hosts.ts:436-440`).
There is no separate `@async` marker to write because every probe already is one — this
maps directly onto the `salt_minting = content_addressed` ruling with no gap.

`@next` (v5's temporal carry, "read what I derived last tick") has no v6 counterpart at
all. See F4 for what breaks without it and F6 for where the surface's OTHER mechanism
(content-addressed identity chains) accidentally covers part of the same ground.

### F2 — no clock/tick builtin, no scheduler — verdict: inexpressible; this is the SLOT-SWR-defining gap

`v6/dl/src/5_diag.ts:48-56` is the entire builtin rel surface v6 ships:
`file, node, edge, sig, site, const, span_line` (the spine) plus `diag`. No
`clock(secs, bucket)`, no wall-clock anything. `grep -n "setInterval\|interval\|POLL\|
schedule\|cron" v6/dl/src/*.ts` returns nothing — `v6/dl/src/main.ts` has no timer of
any kind; the whole app is `serveDl(cfg).subscribe(...)` reacting only to HTTP
requests (`main.ts:22-35`). Nothing in the language or the runtime changes on its own
with wall-clock time.

v5's whole rate/cadence mechanic (`examples/gh-cache.dl:45-56`: `clock(300,b)` salts
the request id so an unchanged resource still gets re-checked every 300s) has no v6
spelling. Concretely, in this program: `poll` only re-fires when `etag` actually
changes value. Once a resource's etag stops changing (a steady-state 304), `poll`'s
identity stops changing too, `fetch`'s content-addressed cache never re-fires it
(zero salts used — see F6), and the endpoint is never re-checked again, ever. v5's
"12/14 revs fetched" cadence-driven receipt (plan doc, the v5 yardstick table) has no
possible v6 analog without adding something outside the language to drive it.

**This is SLOT-SWR's answer.** Two candidate spellings, both genuinely available in
the current surface:

- **Spelling A -- in-language, demand-driven, chosen for the .dl6 file.** A consumer rule
  reads `resp`/`stars`/`etag` unconditionally (whatever is cached now, however stale)
  while a probe demanding a fresh fetch sits in the SAME rule body. Content-addressed
  dedup caps actual subprocess/network calls to once per distinct identity, so this
  costs nothing extra when nothing changed. Trade-off: with no periodic salt, "revalidate"
  only ever fires again when SOMETHING ELSE (an external POST, a changed watch row)
  changes the identity — there is no bounded staleness window, matching the gap above.
  Zero new dependencies, zero external actor, but genuinely open-loop past the first
  detected change.
- **Spelling B — external cadence, closer to v5's clock bucket.** Add an EDB rel (e.g.
  `revalidate_bucket(bucket: int)`) that an external cron POSTs to on a timer
  (`POST /edb/revalidate_bucket`, per `6_http.ts:299-320`'s `handleEdbInsert`), then
  splice it into the probe as a salt (`fetch?(ep, prev, bucket, status, tag, body)`,
  matching the salt mechanism at `0_ast_bridge.ts:539-548`). This reproduces v5's
  cadence exactly, but moves "the clock" out of the language entirely and into
  operator-run tooling — which is a real answer, but not an in-language one, and not
  what "the git/fs spine is HOSTED IN THE LANGUAGE" (CLAUDE.md, `spine_residency`)
  asks for.

I used spelling A in `ghcacher.dl6` because it is the only one that stays entirely
inside the grammar with no external actor; spelling B is a legitimate answer but is
closer to "workaround via ops tooling" than "the language expresses it," which the lab
protocol asks me to call out rather than silently pick.

### F3 — no `jsonp`/`json` term-extraction — verdict: inexpressible directly; approximable at a real cost

`grep -c "jsonp\|json(" v6/dl/grammar/dl.langium` and the same over `5_diag.ts`'s
builtin list are both zero — there is no body-item extraction op in v6 at all beyond
the probe mechanism. v5's `jsonp(body, "field", value)` (examples/gh-cache.dl:110,113)
and especially the array-explode `json(body, q:[... {...} ...])` form
(examples/gh-cache.dl:121-124, which turns one JSON array into one row per element with
correlated nested fields) have no v6 equivalent whatsoever.

The only substitute the surface offers is another `sh` host chained by a second probe
off the already-bound `body` value (`extract_stars`/`extract_full_name` in
`ghcacher.dl6`), which works for a scalar field (verdict: **expressible, at a real
cost** — a subprocess per distinct body instead of an in-process parse) but has no
analog for the array-explode case at all (verdict: **inexpressible** — `pull_request`
from `examples/gh-cache.dl:120-124` is not attempted in `ghcacher.dl6` for this reason).

### F4 — v5's negation-based etag bootstrap is a stratification violation in v6, reproduced

v5's `poll(ep,"",b) <- watch(ep), !etag(ep,_), clock(300,b).` (examples/gh-cache.dl:66)
relies on `@next` freezing `etag` at tick-start, so the negation never closes a cycle
back through `resp`. Ported literally (drop `@next`, keep the negation) the same shape
is a negation edge inside a cycle: `poll` negatively depends on `etag`, `etag` depends
on `resp`, `resp` depends on `poll` (via the fetch probe). `v6/dl/src/0_ast_bridge.ts:
946-953` calls `stratify()` at load time and turns a caught `NonStratifiableError` into
a `400` with diag code `"non-stratifiable"`.

Reproduced directly against a live server:

```
rel watch(ep: text).
watch("repos/cli/cli").
rel etag(ep: text, tag: text).
rel poll(ep: text, prev: text).
poll(ep, prev) <- watch(ep), etag(ep, prev).
poll(ep, "") <- watch(ep), !etag(ep, _).
sh fetch(ep: text, prev: text, status: int, tag: text, body: text) = `printf '200\nabc\n{}'`.
rel resp(ep: text, status: int, tag: text, body: text).
resp(ep, status, tag, body) <- poll(ep, prev), fetch?(ep, prev, status, tag, body).
etag(ep, tag) <- resp(ep, 200, tag, _).
```

Response: `HTTP 400`,
`{"diags":[{"code":"non-stratifiable","message":"relation \`etag\` is aggregated or
negated inside a recursive cycle with \`poll\`","line":0,"col":0}]}`.

This is a genuinely useful, correctly-firing rail (CLAUDE.md's tabling ruling: "the
not_stratified guard IS semantics" — confirmed, not just asserted). `ghcacher.dl6` works
around it the way described in the file's own header comment: `etag` is the union of
an always-true bootstrap fact and the resp-derived value (ordinary positive recursion,
stratifiable — the same shape as `conformance.dl6`'s `proves_recursion`). Verdict:
**the v5 idiom is inexpressible as-is; a semantically different (weaker — see the
`poll(ep,"")` note in the .dl6 header) workaround is expressible.**

### F5 — `sh` declarations split input/output columns differently than v5

v5: `sh fetch(ep, prev) -> (status: int, tag: text, body: text) = \`...\`.` — explicit
arrow split. v6's `ShDecl` grammar production (`grammar/dl.langium:56-58`) has no `->`
at all: `'sh' name=ID '(' columns ')' '=' template '.'` — every column, input and
output, is declared together, and `inputCols` is inferred by scanning the template text
for `{col}`/`$col` (`0_ast_bridge.ts:449-451`). A second, load-bearing constraint: when
a probe passes no extra (salt) args, the probe's positional args are matched against
`host.columns` in DECLARED order verbatim (`0_ast_bridge.ts:547-548`, the zero-salt
branch uses `hostColumnNames` unfiltered) — so inputs must be declared textually before
outputs, or a zero-salt probe call silently binds the wrong columns. This is a real,
non-obvious authoring constraint with no diagnostic pointing at it; `ghcacher.dl6`'s
header comment calls it out. Verdict: **expressible, different shape, one undocumented
footgun.**

### F6 — content-addressed accumulation covers v5's `change_log` `@next` carry, narrowly

v5 needs `@next` for `change_log` because a plain derived rule recomputes from current
support every tick, and an entity that stops being currently derivable would vanish
(examples/gh-cache.dl:126-137). v6 has no `@next`, so `change_log` in `ghcacher.dl6` is
a plain derived rule instead — and it works, for a specific reason: `fetch`'s probe
uses zero salts, so `1_hosts.ts:86`'s "absent any salt_N key... no supersession ever
fires" applies — a landed `__resp_fetch` row is NEVER retracted once inserted, because
supersession (the mechanism that WOULD retract it) only fires between rows that share
identity but differ in salt, and there are no salts here. Since `resp`'s identity is
`(ep, prev)` and `prev` only ever advances to genuinely new values (never repeats an
old one — bounded by F2's cadence gap), every `resp` row that ever lands is permanent,
and `stars`/`full_name`/`change_log` inherit that permanence through the join, with
ordinary rel set-semantics (`2_schema.ts:16-17`) doing the `(ep,kind,val)` dedup v5's
accumulator did explicitly.

I have **not** verified this empirically past one effect firing (F7's crash prevented
a second real fetch from ever landing) — this is read from the code
(`1_hosts.ts:436-511`'s supersession logic, `3_runtime.ts:574-589`'s
diff-against-mirror recompute) and reasoned through, not observed running for more than
one tick. Flagging per the "doubt yourself" law: **this is my best-supported reading of
the source, not a confirmed empirical result**, and it is narrow — it only holds
because nothing in this chain uses a salt or `rel(1)`. A salted probe (the natural
spelling for adding a witness column, e.g. `content_hash` in `sg-rail.dl6`) or a
`rel(1)` anywhere in the chain would break it immediately (see F8). Verdict:
**expressible, but by a coincidence of this program's specific shape, not a general
substitute for `@next`.**

### F7 — the program crashes the live engine on the first real host response (unresolved root cause)

Reproduced twice (HTTP server + a direct in-process driver subscribing `deltas$`/
`HostRunner.effects$` exactly as `6_http.ts:runProgram$` wires them): the instant
`fetch`'s real `gh api repos/cli/cli` response (genuine `200`, a real multi-KB JSON
body, a real ETag) reaches `HostRunner.runEffectOnce`'s `runtime.commit(...)` call
(`1_hosts.ts:491-494`), the commit throws:

```
LibsqlError: SQLITE_ERROR: no such column: NaN
    at ... Sqlite3Client.execute (.../lib-esm/sqlite3.js:83:16)
    at <anonymous> (v6/dl/src/3_runtime.ts:273:88)
```

`3_runtime.ts:273` is the generic `execute$` wrapper (`defer(() => from(db.execute(sql)))`)
— the SQL text itself is never surfaced by the libsql client's error object (`LibsqlError`
carries `code`/`extendedCode`/`rawCode`/`cause`, no statement text —
`@libsql/core/lib-esm/api.js:2-18`), so I could not identify the exact statement or
column without adding trace instrumentation to `src/`, which the lab protocol forbids.
This is fatal, not caught: `main.ts:28-34`'s documented behavior ("a fault raised later
by the tick loop... stays on the stream and reaches main.ts, which is fatal") is
confirmed exactly — the whole process exits.

What I ruled out by reading the write path (not asserted, checked): `encodeSurfaceRowByColumns`
(`3_runtime.ts:151-173`) throws explicitly on a non-number value for an int column
rather than silently producing NaN, and that throw is a `commit: non-numeric value in
rel ...` `Error`, a different message than what was observed. `foldRowDigest`/
`rowDigest`/`effectDigest` (`0_digest.ts:20-24`, `2_schema.ts:94-95`) narrow through
`BigInt.asUintN`, which can only ever return a finite integer or throw a `RangeError`
mid-computation — it structurally cannot hand back a floating `NaN` as its *return
value* for the final SQL splice, ruling that whole call path out as the *direct* source
of the embedded literal.

I do not know what does produce it. Two candidates I did not get to rule in or out:
something in the SQL fixpoint's own re-evaluation of `resp`/`stars`/`full_name` against
a real multi-KB `body` string (untested at this size in this codebase's own test suite,
as far as I found), or something in `RelStore`/support-edge bookkeeping
(`3_runtime.ts:906-945`, `retract_dred`) triggered by the first-ever EDB write to a
host response rel outside the existing test fixtures' data shapes. Both are guesses,
not findings — recorded as open questions, not conclusions.

**Verdict: not a language-expressiveness gap** (the program is accepted, the rule
shapes are legal) **but a runtime defect that blocks grading past one effect firing.**
Per the standing failure-ledger law (CLAUDE.md) this likely wants its own
docs/failure-modes.md entry and a real investigation with source access, which is out
of scope for this lab (extraction-lab discipline: no engine changes).

### F8 — `rel(1)` retention is silently inert on a rule-headed rel, and its OWN semantics is a global sweep, not per-key

`rel(1)` (v5's mental model: "Key-upsert, single row per key") only governs EDB-style
`commit()` writes (`applyRelWrite`, `3_runtime.ts:420-490`); a rule-headed (derived)
rel is recomputed and diffed every tick via `diffAgainstTables`
(`3_runtime.ts:574-589`), which reads no retention at all. Declaring `rel(1) etag(...)`
and then heading `etag` with a rule (as `ghcacher.dl6` does) would make the `(1)` marker
a silent no-op — no diagnostic, no warning. Separately, even where retention-1 DOES
apply (a plain EDB commit), its "keep newest only" sweep (`3_runtime.ts:457-469`)
retracts every row of the WHOLE TABLE not in the current insert batch — a global sweep,
not a per-key upsert. For a single-endpoint program like this one that distinction is
invisible; for a real multi-endpoint `watch` list it would be actively wrong (watching
endpoint B would evict endpoint A's cached etag). `ghcacher.dl6` deliberately avoids
`rel(1)` for this reason. `grammar/dl.langium:38` ("Key(text): parses, accepted,
semantically inert this slice") independently confirms the finer-grained per-key
primitive v5 uses is not implemented at all yet. Verdict: **the v5 `Key(text)`
per-key-upsert model has no v6 implementation; the nearest v6 primitive (`rel(1)`)
means something structurally different (whole-table sweep) and is a footgun on a
rule-headed rel specifically.**

### F9 — no `effect_log` builtin

v5's closing query `? effect_log(id, kind, head, state, args, req_tx).`
(examples/gh-cache.dl:141) has no v6 counterpart — `5_diag.ts:48-59`'s builtin list has
no such rel, and the nearest thing (the `effect_cache` SQLite table `1_hosts.ts`
reads/writes) is not surfaced as a queryable rel at all — only the minted `__resp_h`
per-host rels are queryable (`GET /idb/__resp_fetch`, confirmed present in the
`bridgeOk.program.rels` list from Grading 1's response). Verdict: **inexpressible** —
no query gets you effect state (pending/done/error) across every host in one shot; you
would need one `__resp_h`/`__req_h` pair read per host, and neither exposes the
`effect_cache` state column at all.

## V5 gotchas — mapping

1. **`@async` fan-out rate-limit mass failure (jitter fix).** Does not apply the same
   way: v6 has no autonomous re-tick at all (F2), so there is no "one tick, many
   requests fire simultaneously" hazard the way v5 had it — v6's demand only expands
   when EDB facts change (an HTTP POST, a file_changed event), never on a wall-clock
   tick nobody asked for. The closer analog is a large `watch` list all becoming
   demandable in the SAME commit (e.g. bulk-inserting 500 `watch` rows in one
   `POST /edb/watch`), which WOULD fan out that many `__req_fetch` rows in one
   fixpoint round with no jitter/stagger mechanism in the language (nothing like v5's
   jitter fix exists here either) — but I did not test this at scale; noting it as an
   open, untested risk rather than a confirmed finding. Content-addressed dedupe
   (`salt_minting = content_addressed`) does NOT reduce the fan-out size for a genuinely
   distinct-per-endpoint batch — it only prevents re-firing the SAME identity twice, so
   it does not change the answer to this gotcha at all.
2. **term-extract rule cannot head the same rel as a derived rule (`pr_number ->
   change_log` split).** This exact hazard is a v5 engine constraint
   (`eval_extract_rules`/`rebuild_derived` ordering, CLAUDE.md's "One rel = one rule
   kind" law) that has no v6 analog to even TEST, because v6 has no term-extract
   operator at all (F3) — there is nothing to head a rel jointly with a derived rule in
   the first place. Not applicable, for a structural reason rather than because v6
   solved it.
3. **Content-addressed effect id means editing an effect body does not re-fire it.**
   Confirmed directly equivalent: v6's probe identity is `(host, identityCols,
   saltCols)` (`1_hosts.ts:436-440`), computed from REQUEST columns only — the shell
   TEMPLATE text itself is not part of the digest. Editing `sh fetch(...)`'s template
   body and reposting the program would NOT re-fire an already-answered `(ep,prev)`
   pair; the `effect_cache` row still matches on the old digest. This is the same
   behavior v5 has, for the same reason (identity is a function of request args, not
   of the effect's own definition).

## Cleanup

All test servers were killed after each run; `lsof -i :<port>` and
`ps aux | grep main.ts` were checked empty after the last one (port 17175, the
stratification-reject test). No process was left running at the end of this session.

## Diff scope

`git status --short` from `v6/dl/` at the end of this work shows exactly two files:
`v6/dl/fixtures/ghcacher.dl6` and `v6/dl/fixtures/ghcacher-findings.md` (this file). No
`src/`, `grammar/`, or `package.json` changes. `pnpm run typecheck` (`tsgo --noEmit`)
is clean (no TypeScript was touched, so this is expected, and confirmed rather than
assumed).
