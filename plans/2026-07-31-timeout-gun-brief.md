# The timeout gun — brief (opus worktree, dispatch after the atlas merge)

Standing law (user 2026-07-31): every compute invocation in the toolchain
runs under a budget with a NAMED timeout failure. No open-ended grind
anywhere. A resource cliff is a named refusal, never a hang, never an
OOM death. Incidents this law answers: the devlog 35-minute hang, the
clock-check 9m40s/8GB grind that died as a stack overflow inside the
served compiler, 236 orphaned servers.

## The one pattern

bench-cli/bench.sh `run_capped` is the proven citizen: perl fork +
setpgrp + SIGALRM -> `kill -KILL -pgid`, exit 124. The perl one-liner
`alarm+exec` (tsv2_gen.sh) is the ANTI-pattern — it orphans the child
(measured, ledger row perl_alarm_orphan). Hoist run_capped into ONE
shared helper `v6/tools/run-capped.sh`; bench.sh sources it (its header
receipts stay); every other caller uses it.

## Muzzle list (each site: budget env var with default, named failure line)

1. **The served compile door** — 0_compile.ts spawns swipl for
   POST /program. Budget (default 60s): timeout answers the request
   with a named `compile_timeout` error body + exit of the swipl
   process group. A slow compile must never hang the POST or kill the
   server. Fail-first test: a program whose compile is artificially
   slowed (the clock-cliff fixture shape or a sleep goal) answers the
   named error within budget+ε, server stays alive, next POST works.
2. **Receipt scripts**: atlas.sh, self-map.sh, devlog.sh,
   text_door_receipt.sh, roundtrip.sh, sweep.sh legs, extraction-live,
   crawl-bench (the v5 leg too), goal-endurance, leak-soak, memory-soak,
   getting-started runner, files.sh — every swipl/node/dot/curl
   invocation wrapped. Per-script default generous (2-10x current
   measured wall, state each), env-overridable, failure line names the
   script, the leg, and the budget.
3. **graphviz dot** in atlas.sh — per-render budget.
4. **Engine-side**: one tick exceeding a budget (default off unless
   cheap via the existing DL_PERF_LOG seam) = out of scope for this
   lane if it needs engine changes; note it as a follow-up row instead.
   Do NOT touch engine internals here.
5. **justfile recipes** that call long tools directly — route through
   the helper where trivial.

## Receipts

- Fail-first: one planted-slow invocation per wrapped script class
  times out with the named line and exit 124; process tree verified
  DEAD after (pgrep on the group).
- The full battery (`just green` at minimum, plus atlas/self-map runs)
  passes UNCHANGED under the new budgets — a budget that trips on
  today's honest wall is a mis-set budget; measure first, set to
  headroom, state the measurement per site.
- tsv2_gen.sh's orphaning one-liner replaced; the perl_alarm_orphan
  ledger row closes.
- failure-modes.md gains the class entry (unbounded compute grind:
  incidents, law, rail = the shared helper + this sweep).

## Fences

- Worktree law: first action ff-only to the sha in the dispatch prompt;
  failure = STOP AND REPORT.
- Do NOT touch: engine tick internals, the compiler's .pl logic
  (wrapping its INVOCATION is in scope, its code is not), bench-cli
  beyond sourcing the hoisted helper, v5 src/**.
- pnpm install per package; never symlink outer node_modules.
- Style laws per CLAUDE.md. Commit per step `git commit -n`; no push.
