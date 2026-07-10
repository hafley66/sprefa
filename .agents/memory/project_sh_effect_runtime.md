---
name: project_sh_effect_runtime
description: sh/sh!/sh* template effect runtime on the temporal spine — Bloom @async/@stream over the rev/tx engine; ghcacher-into-dl arc
metadata: 
  node_type: memory
  type: project
  originSessionId: edd6a76b-d044-4536-b605-abe3adb0c58b
---

Branch `feat/temporal-next-async` (worktree `~/projects/sprefa-temporal`). Porting
ghcacher (gh-API polling cache: etag/304, normalized SQLite, change_log) INTO the
v5 dl engine via Bloom-style temporal modifiers riding the rev/tx spine. Plan:
`v5/plans/2026-06-29-sh-template-effect-runtime.md` (Status table near the bottom).

**The `sh` family** (parse-time bang/star is the only distinguisher; `ShellKind`):
`sh`=Read (cached, re-runnable), `sh!`=Mutate (exactly-once, idempotency-keyed),
`sh*`=Stream (long-lived, many rows). Decl: `sh name(p) -> (c: ty) = \`cmd {p}\`.`
Both effect surfaces lower to ONE `BodyItem::Effect{name,args,outs}`: head-response
(`resp(..) <-@async ..`) desugars in frontend; explicit body-effect
(`gh(repo,path) -> (status,body)`) parses direct. Identity split: `kind`=template
key (== head rel for head-response, the sh name for explicit), `head_rel`=rebuild
target; digest = blake3(head_rel∥kind∥args_json).

**LANDED this arc** (all on the branch, green it 323/0/3 + lib 164/0/1):
- Phase 1 a20f0f9/9377825/dcb2626 — sh decl + Effect body item + `check_effect`
  typecheck (codes: multiple-effects, effect-needs-async, unknown-sh, effect-arity,
  unused-hole, temporal-kind-mismatch). `sh` reservation is context-sensitive
  (`sh <ident|!|*>` is a decl, `sh(...)` stays a usable rel). Brace `{shell}` body
  form DEFERRED (lexer drops raw inter-token text; backtick form already multilines).
- Phase 3 7f7aef1 — `pending_effect` job-state machine: `state`
  (queued|running|done|failed) + `idem_key`. `sh!` exactly-once = atomic claim
  (UPDATE ... WHERE state='queued', changes()==1 wins); a crash-orphaned 'running'
  sh! is quarantined, never re-fired. sh re-runs. The pid-kill/timeout reconcile
  (Z3.10) is deferred to the Phase 4 tail (only long-lived children need it).
- Phase 4 6b504d1 — `@stream`/`sh*` runtime. `EffectExec::run_stream -> Vec<Vec<String>>`
  (each output LINE = one response row; ShellEffectExec splits stdout into
  lines×tab-slots via `split_tsv`, the @tsv/D-7 convention). `Engine::drain_streams`
  fans lines into N head rows, job stays 'running'. Cursor = ordinary @next fact.
  tick no longer bails on @stream; `async_rules` = @async ∪ @stream for emission.
- Phase 1b.2 7254a32 — `collect(x)` aggregate batch. Parses as
  `Term::Call{collect,[x]}`; gathers x across ALL body solutions, fires ONE request
  (`{ids}`="a,b,c" comma-joined), non-collected args must be constant (else loud
  bail). New `batch` col; drain fans the batch response through run_stream into N
  head rows under one id. The provider batch-by-id (gh graphql nodes(ids:), aws
  --instance-ids).

- Phase 1b.1 d469631 — TERM-form json/jsonp (the hybrid join+extract). JsonP/Json
  carry `src: Term` + `rev: Option<Term>` (Some=file form span-located unchanged,
  None=term form = bound str value). Parser disambiguates by jpath/pattern position
  (jpath = the Str literal followed by the non-Str out var; q:{} PathLit for json),
  so file+term forms coexist even with a Str rev. is_source EXCLUDES rev=None →
  routes to `eval_extract_rules`/`extract_rule_rows`: project the relational join
  to SQL (binds the content var via lower_body_projection), run run_data/run_pattern
  over each bound string, fan into head rows (Cmp post-filtered, val_of/eval_cmp
  reuse), one insert_rows per head rel. Runs AFTER the derived fixpoint (inputs
  populated), then a SECOND fixpoint pass when extraction moved (no feedback into
  inputs → converges in one extra pass; @recompute waiver in body). v1 limits: one
  extract op/rule, json format only for term sources, no span id, full-tick only
  (not --changed incremental). D-7 shell-jq route still covers no-engine-parse.

- effect_log builtin rel 93c4501 — the @async/@stream drain queue as a query rel
  (thin view over `pending_effect`, like refresh_daemon_rels): one row per
  distinct request (id, kind, head, state queued|running|done|failed, args JSON,
  req_tx). `? effect_log(...)` shows the queue live; a rule rails on state. The
  dl-native call log AND the parity surface vs ghcacher's `call_log`. Projected
  at TICK START (rebuild_async appends at tick end, daemon drains between ticks)
  → off-by-one for one-shot CLI (run shows PRIOR tick's queue), live under the
  daemon. Lazy (effect_rels_used gate). Test: effect_log_mirrors_the_drain_queue.
  NOTE: `str` brand is a HARD CLI error (tests tolerate as diag); use `text`.

**MERGED TO MAIN 2026-06-29** — the whole effect-runtime arc ff/merged into main
(`b673a07..f305497`, --no-ff merge commit; stale-base, conflicts in engine/ast/
frontend/typecheck all "both added a variant/arm", resolved keep-both). main is
the node2vec/flow arc + this. it 339/0/3, lib 165/0/1 post-merge. PUSHED.

**ghcacher PORT slice 1 — branch `feat/ghcacher-port` (off main, NOT merged):**
examples/gh-cache.dl + tests/it/gh_cache.rs (5e4888f). The etag/304 conditional-
cache loop AS a .dl program: @async conditional GET, request args carry the prior
etag (@next carry), term-form jsonp normalizes the 200 body, @next-accumulated
change_log. Cache = content-addressed pending_effect digest + etag carry; 304
carries old etag, lands no entity, change_log untouched (the parity-critical free
hit, proven hermetically w/ GhMock). KNOWN GAPS (next slices): (1) steady-state
re-poll-on-unchanged needs a CADENCE TOKEN in request args — the digest dedups an
unchanged (ep,etag) so it won't re-fire alone; every(N) gates first-emit but
doesn't vary the digest. (2) resp ACCUMULATES all responses, not latest-wins
upsert. (3) change_log is a value-SET, not the insert/update/delete event stream.

**RAN LIVE AGAINST REAL GITHUB (c8ebf46, branch) — utility verified, not mocked.**
gh_cache_live_against_github (#[ignore], needs gh-auth+network): drives gh-cache.dl
with a REAL ShellEffectExec, `gh api -i`. First poll 200 normalizes the live body
(stars=45058 cli/cli), carried etag re-poll returns a REAL 304, entity rels
populate. THE LIVE RUN FOUND THE GAP: the etag `W/"..."` interpolated into
`-H "If-None-Match: {prev}"` broke shell quoting (embedded `"`) → 200 not 304, cache
silently dead. FIX = ShellEffectExec exports each hole arg as an ENV VAR too; `$prev`
expands opaque values safely (shell never re-parses), `{k}` raw-substitution stays
for clean values. Generic primitive: metacharacter-safe value passing to effects.
WHAT'S LEFT THAT'S GENERIC (engine-ish, not just more .dl): (A) clock-as-fact for
re-poll cadence — THE keystone, digest dedups identical (ep,etag) so a poller needs
time in the args; (B) latest-wins/upsert reduction (argmax-by-tx) vs resp's accumulate;

**GAP A FINAL FORM = `clock(secs,bucket)` BUILTIN (dcac95f, MERGED+PUSHED main).**
The sexy answer to "do we need a new @ word": NO. The @ set is closed at the 3
Dedalus tick-relations (deductive=same tick, @next=+1 inductive, @async/@stream=later
async); cadence is plain @next, not a 4th modifier. `def` can't sugar it (body-atom
inline only, emits zero rules — agent-confirmed). The dl-native move: expose the time
bucket the engine ALREADY computes in refresh_every as a RELATION you join. New lazy
builtin `clock(secs, bucket)` = bucket now/secs per named period, PERSISTENT (present
every tick, unlike edge-triggered `every`). `poll(ep,prev,b) <- watch(ep), etag(ep,prev),
clock(300,b)` — the 4-rule poll_bucket counter collapses to ONE join; b advances once
per 300s → digest varies once per 300s → rate cap + re-poll trigger, no @next counter,
no startup-delay tick. Also kills one of the two --check @next stratification
false-flags. Impl mirrors `every` exactly (CLOCK_RELS/clock_rel_decls/clock_periods/
clock_rels_used/refresh_clock skip-if-unchanged/reserved guard/catalog/both tick
wirings); reuses now_secs (DL_NOW_SECS test override + shared tests/it/clock_lock.rs
mutex serializing time-injecting tests). Added via sprefa-v5-new-builtin-rel skill
checklist. it 343/0/4, lib 167/0/1, live re-verified (stars=45058, real 304).
Superseded the poll_bucket counter below (846bd2a, now removed from the example).

**(superseded) GAP A first cut — `poll_bucket` counter, pure .dl (846bd2a):**
Needed NO new primitive: a carried `poll_bucket(ep,n)` counter, seeded via negation
(`!poll_bucket` → 0, mirrors the etag carry), advanced by `every(N)` via @next
(`poll_bucket_next(ep, n+1) <- poll_bucket(ep,n), every(N)`; hold on `!every(N)`),
folded into the poll args. Digest now advances at most once per N sec → re-poll on
cadence AND between boundaries an unchanged (ep,etag,bucket) hashes identical →
INSERT-OR-IGNORE → ZERO new GitHub calls. every(300) = ≤12 conditional req/hr/ep vs
5000/hr limit; the N IS the rate knob. Dropped the every-gate on `resp` (the bucket
throttles via the id; a gate stalled the first poll until a boundary). Proven
network-free by driving every() via `_carry_meta` reset (test
repolls_once_per_cadence_bucket_and_is_silent_between: bucket 0 = exactly 2 calls
then silent across 10 re-ticks; each boundary = exactly 1 conditional re-poll) +
live-verified (stars=45058, real 304). GOTCHA: the static stratifier is NOT
@next-aware → `--check` false-flags the temporal carry as not-stratified (pre-existing,
the etag cycle did too); the tick engine runs it correctly. Suite green it 342/0/4,
lib 167/0/1. The cadence map (feedback_rx_operators): A=interval, DONE in .dl.
(C) pagination = recursive effect (Link header/next-cursor response feeds next request);
(D) general response-header capture (rate-limit X-RateLimit-Remaining, X-Poll-Interval,
Link) — today only status/etag/body are formatted out. NOT gaps (just write .dl):
more entity types = rel + one json rule each (PROVEN); change_log transitions = @next
prior-snapshot + set-diff.

**JSON→RELS NORMALIZER COMPLETE (af41e13, branch).** The model: every `rel` IS a
SQLite table (`rel_<name>`); the json/jsonp term-form extractors turn an API body
(a bound `text` col) into rows — adding a gh entity = declare a rel + ONE rule, no
Rust. The matcher gap that blocked real gh JSON: datapath.rs `walk_object`
multi-entry branch was LEAF-ONLY (every entry = Exact key + bare `$cap`), so a
pattern mixing a flat capture and a nested-object descent in one `{}` —
`{ number:$n, user:{ login:$a } }` — matched NOTHING (code comment called it
deferred "Step 5 continuation-passing"). FIXED with a frontier fold: each entry
walks its value sub-pattern (leaf/nested-object/array-spread/glob) through the
general walk_steps, threading binding-sets conjunctively; single-entry stays the
fast path. Now `json(body, q:[... { number:$num, title:$t, state:$s, user:{ login:$a } } ])`
normalizes a /pulls ARRAY into one pull_request row per element, sibling+nested
fields correlated. gh-cache.dl has the pull_request entity. Tests: datapath units
(mixed flat+nested, array-of-objects) + list_endpoint_body_normalizes e2e. Pattern
grammar: `[...P]` array spread, `{k:P,..}` object, `$cap`/`$_`/`**`/`re:`/glob keys.

**ENGINE BUG FIXED en route (aabe8a3, on the branch):** a carried @next rel that
MOVES now rebuilds the derived rules reading it. `load_carry` returns whether the
loaded rows differ from the live table's prior content (new `rel_content_digest`:
per-row blake3 over declared cols, XOR-folded — carry rows leave __src blank so
the old `rel_digest`/__src path can't see them); tick ORs it into `changed`.
Before: a derived rule over a carried-in rel FROZE at its first value (nothing
flipped `changed` → rebuild_derived skipped). Existing carry tests missed it
(their carried rel feeds a STABLE derived rel — acc_next over constant ping). The
gh-cache poll loop (carry etag → derived poll → new value/tick) is the first case
that drives it. Pattern for re-poll: include a varying token so the digest moves.

**ghcacher PARITY (mapped, harness not yet built):** ghcacher's observable state
= 3 layers — `poll_state`(etag, last_polled≠last_changed on 304), `change_log`
(append-only, event∈inserted|updated|deleted, id-monotonic), entity tables
(upsert by UNIQUE key, re-sync→0 new rows). Its `repo_event` INSERT OR IGNORE
keyed (repo_id,gh_id) + log_change-only-when-rows_affected>0 == the `sh!` claim.
Differential test design: ONE fake `gh` on PATH (replays recorded etag/304
fixtures by call #), TWO consumers (real ghcacher + a .dl port), diff the two
SQLite DBs on the 3 layers. Critical assertion: a 304 tick writes poll_state but
emits NO change_log row + skips entity upsert (proves the cache works). `EffectExec`
mock (MockExec/CountExec/StreamMock in tests/it/temporal_async.rs) covers the unit
layer; fake-gh covers e2e. effect_log is parity-comparable to call_log directly.

**EFFECT LOGGING + LEAK GC (main b163b39, 2026-06-30, in src/effect.rs after the
578f5fc engine-effect-extract):** observability + the ONE effect-table leak.
LOGGING via `DL_TRACE`: `spawn_stdout` debug-logs the rendered cmd (`preview()`
caps to one line) + result (exit/bytes); `drain_effects` info-logs per response
(kind, head, args=digest key, status=first response slot → 200-vs-304 cache hit
VISIBLE, like old ghcacher) + "draining effects n=N". GOTCHA: tracing writes to
STDOUT, `[daemon]` lines to stderr. RETENTION `gc_done_effects` (end of
drain_effects): a cadence-bucketed `sh` poll queues a fresh `done` row every
clock bucket forever (pending_effect rows were only ever UPDATEd queued→running→
done, NEVER deleted). Reclaim `done` READ rows older than DL_EFFECT_RETAIN_TICKS
(default 256); KEEP Mutate(sh!) done rows (= the exactly-once guard) + Stream
rows; safe because buckets only move forward (old bucketed Read can't recur) +
re-firing a Read is defined-harmless. Reads cur_tx from `_carry_meta` directly
(private `current_tx` lives in engine module, unreachable from effect.rs).
SQLite-maint context: SOURCE side well-managed (retract_paths/_prov orphan sweep,
digest prune, node2vec LRU); resp still ACCUMULATES (latest-wins argmax-by-tx
upsert still a gap); change_log append-only by design; _strings orphans linger
(known-harmless). @async drains ONLY under the daemon (poll_tick); --watch
doesn't; --db <file> required or daemon runs in-memory. Suite it 372/0/4 lib
167/0/1, live-verified (stars=45077, real 304).

**DEFERRED, with rationale (NOT just unbuilt):**
- Phase 5 native http — plan-sanctioned skip: `collect` (bulk graphql) removed its
  reason; curl/gh + etag/304-on-@next cover it. Delegate-wrapper, never a router.

Prior (other sessions/branch): Phase 0 parallel drain (204b9ba), Phase 2 every(N)
clock (6dceab6). Style: N+1 ban (collect-then-flush via insert_rows) holds
throughout; one-rel-one-rule-kind; the response rel is drain-only (bails if also
source/derived). NOT pushed to main (default-branch push needs Chris).
