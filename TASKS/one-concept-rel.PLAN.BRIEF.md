# Brief (PLAN lane): one concept, a rel with external arrivals

Issue: `@one-concept-rel` (epic `@cheap-fast-analysis`). Base sha: printed by the spawner; FIRST ACTION
`git merge --ff-only <that sha>`; failure = stop and report. Never spawn subagents. Deliver through a
GitHub PR against `main` carrying TWO docs (lab protocol): `plans/2026-08-21-one-rel-with-arrivals.PLAN.md`
(receipts, citations, for the auditor) and `plans/2026-08-21-one-rel-with-arrivals.PLAN.visual.human.unga.md`
(plain words, diagrams, zero citations, for Chris). A plan without the second doc is undelivered. The PR
body ends with `Refs-Issue: @one-concept-rel`. THIS LANE WRITES NO CODE beyond the two docs and, at most,
a probe fixture under `v6/prolog/conformance/fixtures/` that shows today's behaviour and is NOT wired in.

## The user's design statement (2026-08-21, verbatim intent)
"did we get rid of the idea of bind and host and sh, because it's all rels that can have external arrivals."
Read it as the decision: `sh` declarations, `bind` declarations and the host machinery are one thing, a
rel whose rows arrive from an executor instead of from a rule. The template string is dead text since the
shell executor was deleted (main `d011ddc77`): every host answers through a linked Rust executor chosen by
an adapter row. No new keywords. `rel` syntax plus the existing `->` demand-to-response arrow is the
budget; `order by`, spread, aggregates exist; nothing else gets invented.

## Laws in force
- tsv2 paused: Rust door only; `emit_ts.pl` output for unchanged programs stays byte-identical in any plan.
- Zero shell in the engine. The banned words: "ground truth" in any form (say oracle). No em dashes.
  Banned in prose and identifiers: provenance, substrate, load-bearing, regime, refusal, honest(ly),
  ground* as a verb, support (say refCount). Vocabulary: rxjs, prolog, SQL words only.
- Every command wraps `timeout`. Nothing foreground over 10s. Batteries in the background, polled.
- Every `.dl6` snippet carries its pure-rxjs lowering as a comment.
- Planning protocol (user's global CLAUDE.md): type signatures first, pseudo-code body as a comment under
  the signature, instance lifetimes for each type that holds state, storage layout then reads and writes
  then uniqueness conditions. The four layers may disagree; say where.

## Read first (cite file:line in PLAN.md for every claim)
`CLAUDE.md`; `v6/prolog/compile/parse_dl_dcg.pl` (`sh_head//2`, `sh_decl_stmt//1` ~:994-1010,
`bind_decl_stmt`, `dotted_path//1`, `query_stmt//1`); `v6/prolog/compile/registry.pl:330-480`
(`host_input_contract/3` keyed on host NAMES, `host_output_contract`, `scip_namespace_host/3`,
`host_input_roles/3` default all-identity, `toml_json`); `v6/prolog/1_host_expand.pl` (`prepare_program/5`:
how a host declaration becomes `__host_demand_<n>` and `__host_response_<n>` rels, identity vs freshness
roles, witness digests, `query_decl/3`); `v6/prolog/2_subscribe.pl`, `v6/prolog/3_clock_check.pl`
(`bind interval`, `bind watch`: the clock and watcher forms; `program_uses_tick`); `v6/prolog/lower.pl`
(every `sh_decl(` and `bind_decl(` consumer: `grep -n "sh_decl\|bind_decl" v6/prolog/*.pl
v6/prolog/compile/*.pl`, 12 files today); `v6/prolog/emit_rust.pl` (`host_plans`, `bind_plans`,
`queries`); `v6/sprefa-engine-rs/src/hosts.rs` (`HostLiveRunner::collect`, `executor_for`, adapter rows,
`HostRow`, `select_columns`, `carries_every_column`, `FixtureExecutor`), `src/types.rs`
(`HostPlanData`, `HostAdapterRow`, `IHostExecutor`), `src/driver.rs:126` (the one construction site);
`v6/dl/deadcode/dead-module-rail.dl6` + `.adapters.json`, `v6/dl/deadcode/receiver-rail.dl6`,
`v6/dl/ghcacher/**` if present (the consumers that must keep compiling); `v6/prolog/conformance/rulings.pl`
(decisions already taken about hosts, bind, tick); `v6/prolog/compile/out/manifest.json` (grep every
fixture whose reason names `host`, `bind`, `sh_decl`, `tick`; count them); `docs/failure-modes.md`
entries 51-59 (host seam incidents).

## Deliverables (both docs)
1. Inventory table: every construct today (`sh` decl, `bind interval`, `bind watch`, host demand rel,
   host response rel, adapter row, `host_input_contract` row, `host_output_contract`, template string,
   `--arrive` seeds, schedule `__host_response_*` scripted rows) with: where it is parsed, where it is
   lowered, where the runtime reads it, how many corpus programs use it (manifest counts), what it
   carries that the others do not.
2. The collapse: ONE declaration form, written as type signatures first. Candidate to evaluate and
   improve, never to rubber-stamp: `rel extract(path: text, digest: text) -> (record: text, family: text,
   callee: text).` meaning "rows of the right side arrive from an executor keyed by the rel name when rows
   of the left side exist"; a rel with no arrow and no rules is a seed rel (rows arrive from `--arrive` or
   a schedule); `bind interval(300)` becomes a rel whose arrivals come from the clock executor; `bind
   watch(glob)` becomes a rel whose arrivals come from the watcher executor (soopy). For each: the demand
   identity vs freshness roles (today per-host in `registry.pl`; propose where they live when the name
   is no longer special: a column annotation that exists? the adapter row? state the smallest), the
   executor binding (adapter row keyed by rel name; the registry row disappears or becomes
   executor-declared), the witness digest, the response projection by column presence.
3. Migration table: every consumer site (file:line) and what changes; every corpus fixture that moves
   bucket and why; the `emit_ts.pl` byte-identity argument (paused door: the old forms keep parsing as
   sugar that desugars to the new one at parse time, so nothing below the parser changes shape, or state
   why that is wrong).
4. Instance lifetimes and storage: what the engine holds per arrival rel across ticks (claim-once by
   witness, memo per directory, per-repo `GitBatch`), where it lives today, where it lives after.
5. Risks with a probe each: a program declaring both the old and the new spelling; a host name colliding
   with a module path; a rel with an arrow AND rules; the `fixture` executor's constant rows; the TS door.
6. Three-step lane plan with file ownership and gates (plunit 1041, conformance 439/0, grade.sh 439/335
   rc=0, oracle-rustc, oracle-knip, the dead-module rail 0/16/0 on hafley-rs), each step landing green.
7. The visual doc: a TOC, one mermaid `flowchart LR` of the arrival path before and one after (24 shapes
   max per board), one table of "word today -> word after", one step trace of a demand row becoming
   response rows through the executor with real values from the dead-module rail (path, digest, witness).

## Gates this lane runs (read-only plan; still run them so the numbers in the doc are yours)
`timeout 600 bash -c 'cd v6 && just plunit'`, `timeout 600 bash -c 'cd v6/prolog/conformance && swipl -g go
-t halt go.pl'`, `timeout 120 bash v6/dl/deadcode/dead-module-rail.sh ~/projects/hafley-rs
'crates/*/src/*.rs'`. Paste the three lines.

## File ownership
YOURS: the two plan docs, the optional unwired probe fixture. FORBIDDEN: everything else. Requests go in
the PR body. Commit messages imperative, ending `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`,
last paragraph `Refs-Issue: @one-concept-rel`.
