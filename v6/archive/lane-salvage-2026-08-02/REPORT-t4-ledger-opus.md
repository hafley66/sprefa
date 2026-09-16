# REPORT: failure-ledger entries 39 and 40

One file touched, uncommitted: `docs/failure-modes.md` (+169 lines). Two new classes
written in place before the rail gap table, plus one gap-table row each.

## What was written

- **Class 39** "Nested process-group cap: `cap_self` re-groups out from under the
  outer kill" (docs/failure-modes.md:1462-1548) + gap row 39 (:1667).
- **Class 40** "An aggregate emits no row for an empty group (the `coalesce` idiom
  nobody wrote down)" (:1550-1627) + gap row 40 (:1668).

Both follow the existing bullet spine (WHAT IT LOOKS LIKE / HOW IT BIT US / THE LAW /
THE RAIL / SAY THIS TO AN AGENT), the `--` dash convention entries 36 and 38 use, and
the doc's cite-or-say-unverified rule.

## Findings the entries rest on (all measured or cited, not assumed)

**Class 39, the mechanism, which the incident note did not have.** The hole is
not "a timeout gun orphans grandchildren" (class 38's rail closes exactly that).
It is that `cap_self` re-execs the calling script THROUGH `run_capped`
(v6/tools/run-capped.sh:78-92), whose child calls `setpgrp(0, 0)` (:51-64) -- so
a script carrying `cap_self` LEAVES the process group of any outer cap, and the
outer `kill -KILL -pgid` cannot reach it or anything it spawned. The re-entry
marker `cap_self` exports is per LABEL (:81-83) and an outer `run_capped` sets no
marker at all, so nesting is silent.

Reproduced both legs in scratch (nothing added to the tree, orphans cleaned up):

    inner.sh: source run-capped.sh; cap_self 120 innerlab; sleep 300 & ; sleep 300
    run_capped 3 bash ./inner.sh   -> outer exit=124
      ps: 53584 53548 53548 sleep 300      <- ALIVE
          53548     1 53548 bash ./inner.sh <- ALIVE, reparented to pid 1
      kill -9 -53548 took both.
    same script with the cap_self line REMOVED, same outer cap
      -> backgrounded child DEAD (class 38's own process-group receipt).

**Class 39, the rail candidate the task asked me to name.** Surveyed all 13
server-booting scripts in v6/tsv2/scripts/ (comment-rails, devlog, endurance,
flagship-flow, crawl-bench, extraction-live, flagship-callgraph, files,
lsp-diags, self-map, scip-families, v5-git-diags, v5-parity). They are one shape:
a fixed default port, `SERVER_PID=$!`, `trap stop_server EXIT`. None checks port
ownership; none reads the port back. Five of them carry `cap_self` (devlog,
crawl-bench, extraction-live, files, self-map), as do leak-soak and memory-soak,
so the escape applies to seven live rails today. Two rails already name the SAME
port: `TSV2_EXTRACTION_PORT` and `TSV2_SOAK_PORT` are both 17571
(extraction-live.sh:68, memory-soak.sh:26).

The cleanest existing pattern is on the TypeScript side, not in the scripts:
`startServed(port = 0)` + `served.port` + `reservePort()`
(v6/tsv2/tests/serveHelpers.ts:135-148), landed against bug
`hostdecode_hardcoded_port_collision` with a sabotage receipt that pins a
constant back and watches a test go red (v6/tsv2/tests/serveLifecycle.test.ts:
17-22, 49-54). `serve/main.ts` already reads `TSV2_PORT` (:18) and prints the
port it bound (:24), so porting it to the shell rails is mechanical. Cited as
rail candidate (b); (a) is the group-marker fix in `cap_self`.

**Class 40, the coverage gap is specific and was verified, not assumed.**
`coalesce/2` is live, ruled and graded, and v6/prolog/conformance/fixtures/
7_coalesce.pl carries eight fixtures -- but every source in them is an EDB rel or
a level view (`coalesce_over_derived_source` reads `heavy`, a `Kilos > 10`
filter, :103-122). None reads an aggregate-headed rel, which is the only shape
where the absent row is manufactured by the aggregate rather than by a missing
arrival. Nothing in v6/prolog/compile/SYNTAX.md (:86-101 coalesce, :138-148
aggregates) or v6/prolog/LANG.md mentions the empty group; grep for "count never
0" / "empty group" across docs/ and the prolog docs returns nothing. Folklore
confirmed.

Two independent labs, a day apart, both hit it: auto-factorization finding 9
(wrong modularity 0.0 vs -0.0278, plans/2026-07-31-auto-factorization-verdict.md
:958 and worked example :249-263) and csp-idioms finding W3 (semaphore grants
zero leases forever, compiles clean through both doors,
plans/2026-07-30-csp-idioms-verdict.md:97-111). The auto-factorization verdict
itself asks for the promotion and says the shape "today exists nowhere"
(:1031-1035).

## Facts I could NOT verify from the repo alone -- coordinator confirmation wanted

1. **The 60-second outer cap and the EADDRINUSE are from the session log only.**
   `chat_log/20260731.1.fable-parse-fix-comment-sweep-flatten-atlas-death.md:
   10,21,74` is UNTRACKED (it is not in this worktree; I read it from the main
   checkout). The entry cites it with that caveat and separates it explicitly
   from the code-and-reproduction half. If the file gets committed, the citation
   stands; if it never does, decide whether to keep the pointer or reword to
   "session record, uncommitted".
2. **What the 60-second cap actually was.** I could not determine whether the
   outer killer was `run_capped 60` around `just atlas` or a harness timeout that
   SIGKILLed the process. The entry states "a 60-second OUTER cap" and does not
   name the caller, because the mechanism is the same either way (both aim at the
   outer group or the outer process; neither reaches the inner group). If you
   know the exact invocation, it belongs in HOW IT BIT US.
3. **`atlas.sh` line numbers are from the deleted file.** All :NNN citations in
   class 39's HOW IT BIT US are against `2c08ea62^:v6/tsv2/scripts/atlas.sh`,
   recoverable by `git show`, since the file was scrapped in 2c08ea62. I made
   that explicit in the entry rather than citing a path that no longer resolves.
4. **Class 40's fixture home differs from the dispatch.** The task said to cite
   `v6/dl/fixtures/` naming conventions. The graded corpus for this construct is
   `v6/prolog/conformance/fixtures/7_coalesce.pl` (term fixtures, snake_case
   names, read by the oracle, the sweep and the text door), so the entry proposes
   `coalesce_fills_an_empty_aggregate_group` there and names the v6/dl/fixtures/
   kebab-case .dl6 convention (`coalesce-empty-group.dl6`) as the optional
   text-door companion. Flagging the swap rather than silently following the
   dispatch.
5. **Neither rail is implemented and neither fixture is written.** Both entries
   say "missing" / "fixture proposed" and both note the fail-pre-fix test still
   owed per the doc's own pipeline. Nothing in this lane changes code, and the
   class-39 reproduction lived in scratch and was cleaned up (both orphan pids
   killed).

## Numbering

Next free class numbers were 39 and 40 (the doc runs 1-38 with 25/33/34/37 absent
from the gap table, present as sections; I did not touch that). Gap-table rows
appended in numeric order after row 38.

## Commands over 10s

None. Every command in this lane returned in under a second except the two
reproduction runs, which are bounded by their own 3-second caps.

---

## The two entries, verbatim

```markdown
## 39. Nested process-group cap: `cap_self` re-groups out from under the outer kill

- WHAT IT LOOKS LIKE: an outer budget fires, prints its timeout line and exits
  124 -- and the served node engine the capped script backgrounded is still
  listening. Nothing reports it. The symptom arrives one run later as
  `EADDRINUSE` on the rail's own hardcoded port, and the quieter half is worse:
  the orphan ANSWERS on that port, so the next run's readiness probe accepts it
  and grades a whole receipt against a stale server and a stale db (class 35's
  squatter blindness, now reachable through a rail that believed it was capped).
- HOW IT BIT US (2026-07-31, atlas arc): `atlas.sh` booted the tsv2 server as a
  background child (`2c08ea62^:v6/tsv2/scripts/atlas.sh:251-255`) on the fixed
  default port 17811 (:187), with cleanup in the spawner only (`stop_server` +
  `trap stop_server EXIT`, :196-203) and a whole-script `cap_self 2400` on top
  (:166-167, whose own header names the backgrounded server as the reason the
  process-group cap is the honest one, :126-135). The run was made under a
  60-SECOND OUTER cap. The outer cap fired, the server survived holding 17811,
  and the next run of the rail died at boot on `EADDRINUSE`. The only record of
  the incident itself is the arc's session log, which named it UNFILED
  (chat_log/20260731.1.fable-parse-fix-comment-sweep-flatten-atlas-death.md:
  10,21,74 -- untracked when this entry was written); the 60-second outer cap and
  the EADDRINUSE come from there, and everything below is from the code and from
  a reproduction. `atlas.sh` itself was scrapped the same night (2c08ea62) and
  takes no fix with it, because every other served rail is shaped identically.
- THE MECHANISM, which is the inverse of what class 38's rail promises:
  `run_capped` forks a child that calls `setpgrp(0, 0)` before `exec`, then on
  SIGALRM kills that whole group (`kill("KILL", -$pid)`, v6/tools/run-capped.sh:
  51-64) -- one group, everything in it, which is exactly what makes the cap
  reach a backgrounded grandchild. `cap_self` then re-execs the calling script
  THROUGH `run_capped` (v6/tools/run-capped.sh:78-92), so the re-exec'd script
  calls `setpgrp` a second time and lands in a NEW group that the outer group's
  kill cannot address. Everything that script spawns -- the background server
  included -- inherits the inner group and survives with it. The re-entry marker
  `cap_self` exports is per LABEL (:81-83): it suppresses a second cap of the
  same name and knows nothing about an outer `run_capped`, which sets no marker
  at all.
- MEASURED, both legs, 2026-08-02 (lab reproduction in scratch, nothing added to
  the tree): a script that sources run-capped.sh, calls `cap_self 120 innerlab`,
  backgrounds `sleep 300` and then sleeps, run under `run_capped 3` -> outer
  `exit=124` and the backgrounded child is ALIVE, reparented to pid 1, sharing a
  pgid with the re-exec'd bash and killable only as that inner group
  (`ps -o pid,ppid,pgid`: `53584 53548 53548 sleep 300` beside
  `53548 1 53548 bash ./inner.sh`; `kill -9 -53548` took both). The SAME script
  with the `cap_self` line removed, same outer cap -> the backgrounded child is
  DEAD, which is class 38's own process-group receipt reproducing exactly.
  `cap_self` is the whole difference.
- THE LAW: a budget may narrow the process group it will be killed with, never
  mint a new one. A script installing a whole-script cap must first ask whether
  it is already inside one and decline to re-group if it is -- the OUTERMOST cap
  owns the group, and a nested cap that escapes it manufactures orphans out of
  the one mechanism whose entire purpose is not to. Second half, independent of
  the first: a rail that boots a server on a CONSTANT port cannot tell its own
  server from a squatter, so an orphan stays silent until a bind fails, and a
  bind failure is luckier than the alternative.
- THE RAIL: missing, two halves proposed.
  (a) group honesty in `cap_self`: set a label-INDEPENDENT marker in
  `run_capped` (pid + limit of the group that owns the run) and have `cap_self`
  return without re-exec when one is present, so the outer cap kills one group
  containing everything. Note the interaction with class 38's mis-set-budget
  lesson: under (a) a 60s outer cap correctly kills a rail whose honest wall is
  minutes, which is the right answer -- an outer budget below the inner one is a
  caller error that should be loud, not an orphan factory.
  (b) port fingerprint, and the repo already solved this ON THE TS SIDE: every
  served test used to name a constant (17521, 17531, ..., 17611) and collided as
  `EADDRINUSE` the moment two lanes ran one tree (bug
  `hostdecode_hardcoded_port_collision`); `startServed` now defaults to the
  ephemeral port 0 and callers read `served.port` back
  (v6/tsv2/tests/serveHelpers.ts:135-148), `reservePort()` supplies an address
  for receipts that need one NOT listening, and the sabotage receipt pins a
  constant back and watches the third test go red in under a millisecond
  (v6/tsv2/tests/serveLifecycle.test.ts:17-22, 49-54). The 13 server-booting
  shell scripts under v6/tsv2/scripts/ have no equivalent: each names a fixed
  default port, and two already name the SAME one -- `TSV2_EXTRACTION_PORT` and
  `TSV2_SOAK_PORT` are both 17571 (extraction-live.sh:68, memory-soak.sh:26).
  `serve/main.ts` reads `TSV2_PORT` (:18) and already prints the port it actually
  bound (`tsv2 serving on <port>`, :24), so `TSV2_PORT=0` plus reading that line
  back out of `server.log` is the mechanical port of the TS fix, and it makes
  "whose pid answers" moot rather than merely checkable.
  Fail-pre-fix test owed per the pipeline before either half counts; the
  reproduction above is its shape (assert the backgrounded pid is gone after the
  outer 124 -- red on today's code).
- SAY THIS TO AN AGENT: do not wrap a `cap_self` script in another cap and
  believe the outer one -- it kills a process group the inner script has already
  left, and the served engine goes on holding its port. If you run one under an
  outer budget anyway, check the rail's port with `lsof -i :<port>` when it
  returns, and know that the next run's `EADDRINUSE` is the FIRST notification
  you will get. When you write a new served rail, take the port from the server
  instead of naming one.

## 40. An aggregate emits no row for an empty group (the `coalesce` idiom nobody wrote down)

- WHAT IT LOOKS LIKE: a rule counts or sums per group and the answer comes back
  plausible and wrong. A group with zero members produces NO ROW from the
  aggregate, so its term leaves the formula entirely instead of entering it as 0.
  Nothing is refused, nothing is logged, and nothing in the tick log disagrees --
  the missing row is missing, so no delta and no final row can name it. The
  sharper arm: a rule that JOINS an aggregate against a threshold does not fire
  at all while the set is empty, and there the cost is the whole program rather
  than a term.
- HOW IT BIT US, twice, in two independent labs:
  1. auto-factorization (2026-07-31, finding 9,
     plans/2026-07-31-auto-factorization-verdict.md:958; worked example at
     :249-263): the first modularity draft read `und_internal_total` directly
     and returned 0.0 on the file axis where the referee (networkx) says -0.0278.
     Every group with zero internal edges dropped its own negative term, and on
     the file axis that is EVERY group, so the whole negative half of Q vanished.
     Wrong in the safe-looking direction -- the number stayed plausible. The fix
     was one rel, `und_internal_filled`, a `coalesce` against the GROUP rel
     (:255-261).
  2. csp-idioms (2026-07-30, finding W3,
     plans/2026-07-30-csp-idioms-verdict.md:97-111): a semaphore whose gate
     compares `count()` against a limit grants ZERO leases, permanently.
     `count()` over the empty set yields no row, so `latest(held_count(...))`
     matches nothing before the first grant, and because nothing is granted the
     set stays empty forever. It compiles clean through BOTH doors (`bop check`
     exit 0), runs to completion, and the tick log carries only the `acquire`
     arrivals. No diagnostic anywhere. The verdict prices the standing cost: a
     `not(held_count(_))` base case is a +1 rule tax on every aggregate compared
     to a threshold (:109-111).
  Both are language-design-review finding A11 ("count never 0") landing on real
  programs, in labs a day apart, with no fixture between them that could have
  warned either.
- THE LAW: an idiom that grading cannot see is not documented by being known.
  Empty-group absence is invisible to tick-log grading AND to final-state
  grading -- the same shape as the retention and `keep(count)` gaps this ledger
  already carries -- so the only thing that can hold it is a fixture written on
  purpose. Every construct whose failure mode is a MISSING row owes one; without
  it the next lab rediscovers the class at the price of a plausible wrong number,
  which is the most expensive kind.
- THE RAIL: missing; fixture proposed, and the spelling it would pin already
  exists. `coalesce(agg_rel(Group, Total), 0)` derived over the group rel is the
  fix both labs converged on, and `coalesce/2` is live, ruled and graded
  (`null_design = get_else_use_site_never_storage`; surface row
  v6/prolog/compile/registry.pl:69; expander v6/prolog/0_coalesce_expand.pl) --
  but not for this. v6/prolog/conformance/fixtures/7_coalesce.pl carries eight
  fixtures and every source is an EDB rel or a level view
  (`coalesce_over_derived_source` reads `heavy`, a filter, :103-122); NONE reads
  an aggregate-headed rel, which is the only shape where the absent row is
  manufactured by the aggregate itself rather than by a missing arrival. The
  fixture owed follows that file's own snake_case naming
  (`coalesce_defaults_the_absent_row`,
  `coalesce_default_returns_when_source_retracts`,
  `coalesce_over_derived_source`, `coalesce_in_edge_body_samples`):
  `coalesce_fills_an_empty_aggregate_group` in 7_coalesce.pl -- a group rel with
  three groups, an aggregate deriving rows for one, final state carrying 0 for
  the other two, and a delta leg where a populated group EMPTIES and its term
  returns to 0, which is the retraction flip :77-92 already grades for a plain
  source. The auto-factorization verdict asks for exactly this promotion and says
  the shape "today exists nowhere" (:1031-1035). Doc half: SYNTAX.md's coalesce
  paragraph states the total-read semantics and never mentions aggregates
  (v6/prolog/compile/SYNTAX.md:86-101), and the generated aggregate rows
  (:138-148) say nothing about the empty group; one sentence in each is where the
  idiom stops being folklore. If a text-door program is wanted beside the term
  fixture, v6/dl/fixtures/ is kebab-case .dl6 (clock-swr-demo.dl6, diag-rail.dl6,
  door-handwritten.dl6), so `coalesce-empty-group.dl6` -- but the graded corpus
  the sweep and both doors read is the conformance file, and that is where the
  coverage gap is. The honest fix the verdict names beyond all of it is a CHECK:
  an aggregate feeding an arithmetic expression over a group rel must have a
  filled source (auto-factorization verdict :1054-1057), which is a rail rather
  than a fixture, and is unowned.
- SAY THIS TO AN AGENT: `count`/`sum`/`min`/`max` never emit a row for a group
  with no members, so any formula where an empty group still owes a term must put
  the term back by hand -- derive over the GROUP rel and wrap the aggregate as
  `coalesce(agg(Group, Value), 0)`. And if you are comparing an aggregate against
  a threshold, the empty case is not "0 vs threshold", it is NO ROW, so the rule
  does not fire at all: that one costs a whole program rather than a term, and it
  needs a `not(agg(...))` base clause instead of a default.
```

## The two gap-table rows, verbatim

```markdown
| 39 | nested `cap_self` re-groups out from under the outer kill (orphaned server squats its port) | missing | (a) label-independent group marker so `cap_self` declines to re-exec inside an existing cap (v6/tools/run-capped.sh:78-92), fail-pre-fix receipt = an outer `run_capped` 124 leaves no backgrounded child; (b) `TSV2_PORT=0` plus reading the bound port back off the server's own `tsv2 serving on <port>` line (v6/tsv2/serve/main.ts:18,24) across the 13 fixed-port shell rails in v6/tsv2/scripts/ — the mechanical port of the TS-side fix already shipped as `startServed(port = 0)` (v6/tsv2/tests/serveHelpers.ts:135-148); today two of those rails even share 17571 (extraction-live.sh:68, memory-soak.sh:26) |
| 40 | aggregate emits no row for an empty group (the `coalesce` empty-group idiom) | missing | fixture `coalesce_fills_an_empty_aggregate_group` in v6/prolog/conformance/fixtures/7_coalesce.pl (all eight existing sources are EDB rels or level views, none aggregate-headed), plus one sentence each on the coalesce paragraph and the aggregate rows of v6/prolog/compile/SYNTAX.md (:86-101, :138-148); the honest rail beyond both is a check that an aggregate feeding an arithmetic expression over a group rel has a filled source (plans/2026-07-31-auto-factorization-verdict.md:1054-1057), unowned |
```
