# Flash 0731 vs Opus: 5 tasks, 10 worktrees, same briefs

Worktrees at ~/projects/sprefa-lanes/<task>/<model>, all base 64908546, all
uncommitted, nothing deleted. Every lane has REPORT.md at its root (opus lanes:
relayed by coordinator; the agent harness blocked their own .md writes).
Coordinator verification level: reports read in full + spot commands; full gate
re-runs NOT yet done on any lane. Do not merge anything without that pass.

## Scoreboard

| task | flash | opus | verdict |
|---|---|---|---|
| t1 oracle min/max TEXT | shared-layer fix, caught the BRIEF's own term-shape error (oracle throws bare terms), parity tests, conformance 281 stable | found the class is 4 aggregates not 2, found compiler payload arg is an unbound var, TWO-layer fix (load-time shared + runtime value guard for undeclared cols), 5 tests, full gate battery | opus superset; flash genuinely good |
| t2 refCount rename | careful sweep, correct sense-split (unsupported-construct English untouched), byte-goldens held, 471 residuals classified | 25 files, 49 residuals categorized, refused to rename rulings.pl atoms (user-decision record), 3 findings: ledger's "rust identifiers" claim FALSE (all TS), manifest.json non-reproducible pre-existing, supportSql retirement = own arc; ran the only real cross-package proof (3x tsgo) | opus stronger; flash safe |
| t3 deaf watcher | could NOT reproduce (11 honest probes), added repeat()/retry() hardening on a wrong premise; hot-loop guard missing | DISPROVED the bug: 11 probes incl the real rail program all heard; read node's watch generator source; named the true cause of the receipts = coordinator harness used `bop run` (self-exits after 2s idle, BOP_RUN_IDLE_MS, bop.ts:165) as a forever-server; landed the missing real-fs regression test + 3 real hazards | opus decisively; the "bug" was coordinator error |
| t4 failure ledger | format-perfect entries (classes 39/40), gap table right | reproduced the orphan live, INVERTED the incident's stated mechanism (cap_self re-execs into its own pgroup so the outer cap kill can't reach it), 7 live rails exposed, found 2 scripts sharing port 17571, corrected fixture home to conformance corpus | opus found truth; flash transcribed the legend |
| t5 LSP exit hang | fixed adjacent contract defects (explicit exit handling, exit codes) but its own pre-fix hang test PASSED (never reproduced the named hang); missed the real wedge | stack-sampled the hang: background threads hold connection.sender clones, IoThreads::join can never return; loops were already correct; minimal detach fix + exit-code contract, 2 tests red pre-fix, lsp suite 33/0 | opus correct fix; flash partial credit |

## The pattern

- Flash is an excellent BRIEF-FOLLOWER: on well-specified mechanical/doc tasks
  (t1, t2, t4) its work is usable and careful, at a fraction of the cost. It
  fails exactly where the task premise is wrong or underspecified: it fixed a
  non-bug (t3) and the wrong layer (t5) rather than doubting the brief.
- Opus doubted premises by default and falsified THREE claims fed to it:
  the t4 incident mechanism, the t2 ledger claim, and the t3 defect itself
  (which was the coordinator's own harness misuse). Every opus lane produced
  evidence-grade receipts (stack samples, generator source reads, live
  reproductions).
- Two coordinator corrections on the record: (1) the "deaf watcher engine
  defect" filed twice on 2026-08-01 was `bop run` idle self-termination,
  operator error, no engine bug; the surviving real finding from that arc is
  cold-boot host-spawn cost (~1s/spawn). (2) briefs are the ceiling for flash;
  the t3/t5 briefs asserted wrong premises and flash inherited them.

## Merge candidates (user call, after coordinator gate re-runs)

1. opus t5 (LSP exit fix) and opus t1 (aggregate refusal): real defects, real
   fixes, receipts.
2. opus t3's watchRealSource.test.ts alone (the fix-less lane; the test is the
   value). Flash t3's repeat/retry is defensible hardening if wanted, needs a
   delay guard.
3. opus t2 rename (flash t2 is the same sweep, shallower residual analysis).
4. t4: opus entries (mechanism-true); also spawns follow-ups: cap_self pgroup
   rail, port-collision dedupe, 7 exposed rails.

## Follow-ups minted by the night

- bop run idle-exit vs rail-receipt harness: receipts for watch programs must
  use serve, or bop run needs a --forever flag (user call).
- cap_self process-group hole: rail design in opus t4 report.
- watch hazards: maxQueue silent drop, watch-error kills server, port 17571
  shared by two scripts.
- manifest.json reason-string nondeterminism (normalize in a sweep arc).
- justfile expect-comment staleness (271 vs 269 noted by both t1 lanes).
