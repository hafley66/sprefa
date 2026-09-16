# null-design-lowering

Base: `git merge --ff-only 35114ac3966908afca1c6dc4f891996f59faac4e` is your first action (origin/main at spawn). Fail = stop and hail. Branch `feature/null-design-lowering`. PR to main.

User decision, 2026-07-30 (`v6/prolog/conformance/rulings.pl` row `null_design`): "absence stays row absence; the consumer spells the default at the use site: one body operator, LEFT JOIN + coalesce in SQL, `?? default` in rx". User word 2026-08-23: "do the fix for null reads; whatever makes the language closer to idiomatic anything we lower to".

## The defect
`0_coalesce_expand.pl` (expansion phase 45) rewrites a rule with N `coalesce/2` goals into 2^N ordinary clauses (present arm: bare atom; absent arm: `not(...)` + `:=` default) BEFORE lowering. ghcache `page_response` (`v6/dl/ghcache/ghcache.dl6:500-509`, 6 coalesces) becomes 64 clauses: 64 recompute statements, 256 delta arms, 248 KB `insert_sql`, 414 KB of SQL for one head; `level_insert` there is 3.7 ms a call. Issue `delta-arm-subset-expansion` carries the measurement; PR #431 pinned arm count linear per clause.

## Deliverable
One clause, one LEFT JOIN per coalesce, `COALESCE(col, default)` in the projection. LEVEL bodies only. EDGE bodies (`<+`) keep the expander's `latest/1` split untouched (`conformance/fixtures/7_coalesce.pl` case d, and the expander header's reason: a bare atom in an edge body is a trigger).

1. Read first: `0_coalesce_expand.pl` whole (keep EVERY validation throw it has: `coalesce_not_top_level`, the derived-source rule, the type rule on the default; they move, they do not vanish), `lower.pl` body lowering for positive atoms and `not/1` (how a body item becomes a FROM/JOIN term and a delta arm), `level_positive_delta_arms/9`, the refcount support builder (`support_sql`), `registry.pl:63-72` (coalesce/2 row: `wrapper(rel_atom_default, expand(coalesce))`), `ARCH.pl:392-397`, `conformance/fixtures/7_coalesce.pl` (every case is the oracle's word and stays byte-identical), `docs/` anything named coalesce or null.
2. Design in the PR body before code (CLAUDE.md planning protocol: signatures, pseudo-code, lifetimes, storage then read/write order, uniqueness). State for a 2-coalesce level rule: the single SQL, its N+2 delta arms (one per positive item, and for each LEFT JOIN item: the arm when the optional rel GAINS a row and the arm when it LOSES one, both as set differences over the frontier), and the refcount support shape. Cite the DBSP identity you use for the outer join delta.
3. Registry: `coalesce/2` row becomes `wrapper(rel_atom_default, lower)`; the expander keeps only its validations (or they move to `analyze.pl`; say which and why).
4. Emit: `lower.pl` lowers a coalesce item to `LEFT JOIN <rel> ON <key eq>` + `COALESCE(...)`; delta arms and support per item 2; `emit_ts.pl` shares `lower.pl`: diff the whole committed TS corpus (`v6/prolog/compile/out/*.ts`), regenerate what moves, list every moved file.
5. Receipts, three runs each, in the PR body: `grade.sh byte-clean=340` (THE correctness net: conformance fixtures with coalesce are oracle-graded), `tests/fixtures/ghcache_ticklog_base.txt` byte-identical, `page_response` `insert_sql` 248 KB -> under 12 KB and arms 256 -> under 16, ghcache fold statements and wall from `DL_TRACE_SUMMARY` (baseline ~11,534 / ~235 ms; target 7,113 / 152 ms), `level_insert` us per call for page_response, a plunit test pinning "2 coalesces -> 1 clause, N+4 arms" on an inline rule, the `delta_arm_count` pin from #431 updated.
6. Ledger entry; `ARCH.pl:394` row text updated to the landed lowering (one line; the spring-cleaning lane owns the rest of that file, so touch ONLY that row and rebase onto its PR if it lands first); issue `delta-arm-subset-expansion` closed by the PR.

## You own
`v6/prolog/{0_coalesce_expand.pl,lower.pl,analyze.pl}`, `v6/prolog/compile/registry.pl` (one row), `v6/prolog/compile/test/plunit_tests.pl` (additive + the #431 pin), `v6/prolog/compile/out/*.ts` (regeneration), `v6/sprefa-engine-rs/tests/{one_tick_path.rs,ordered_statement_count.rs}` (caps), `docs/failure-modes.md`, `ARCH.pl` line 394 only.
Forbidden: `incremental.rs`, `sql.rs`, `driver.rs`, `run.rs`, `program.rs` (runtime lanes), `emit_rust.pl`, `emit_ts.pl`, `v6/dl/**`, every conformance fixture (if a fixture must change, the design is wrong: hail).

## Gates, all green before the PR, numbers in the PR body
```
cd v6/prolog/conformance && swipl -g go -t halt go.pl      # 444/0
cd v6 && just plunit                                        # 1082/0 + yours
bash v6/sprefa-engine-rs/grade.sh                           # graded=444 byte-clean=340
cd v6/sprefa-engine-rs && cargo test --workspace            # 163/0
bash v6/dl/ghcache/gate.sh                                  # ticks=14 pr_transition_open_merged=1
cd v6 && just ghcacher-rust                                 # goldens=6
cd v6/prolog && swipl -g go -t halt ARCH.pl                 # 7/0
```
Batteries in the background with `timeout`; never foreground-wait more than 10 s on one command. Commit per item; PUSH before you report; a result with nothing pushed is not a result.

## Style laws (CLAUDE.md)
No `eprintln!`; `tracing` only. Comments state only constraints the code cannot show. No em dashes. Banned words in prose and identifiers: provenance, substrate, load-bearing, regime, ground truth (say oracle), refusal, support (say refCount; the existing `support_sql` identifier stays as is). Language vocabulary: rxjs, prolog, SQL words only.

Done: `boop beep hail sprefa-coordinator --from null-design-lowering --body "PR #<n>: page_response bytes/arms before->after, fold stmts/wall, gate numbers"`; if refused, message the sprefa-* session over the cross-session socket. Blocked or a design fork: one line with the cited options, stop; the user decides.
