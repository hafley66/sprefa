# v8 design review (two independent reviewers, one brief)

## TOC
1. What you are reviewing and the one output file
2. Base and first action
3. The question, stated without a thumb on the scale
4. Sources, in reading order
5. The cross-reference: what Chris said, from the boop store
6. What the report must contain
7. Style laws

## 1. What you are reviewing
`v8/` is a Rust compiler and evaluator for a datalog-over-code language (`.dl7` surface today). The stated goal for it is the core of the relational application that `v6/` is: a reactive engine where rules derive tables, outside processes feed tables, and effects (HTTP, filesystem, timers, subprocesses) are expressed relationally. Whether the `.dl7` surface, the v7 prolog compiler, or the v7 oracle fixtures survive is NOT a constraint; treat them as history that may be dropped. Review the design as it stands and the design conversation that produced it.

Output: exactly one file, `plans/v8/2026-09-14-v8-design-review.<your lane suffix>.md` (`astra` or `fable`, the coordinator names it in the goal line). Change nothing else. Write no code.

## 2. Base and first action
- Base sha `cf6326e741499c0a7204f1ec65272af75c408cda` (`origin/main`).
- Branch `review/v8-design-<suffix>-20260914`, worktree under `/Users/chrishafley/projects/sprefa-wt/`.
- FIRST command: `git merge --ff-only cf6326e741499c0a7204f1ec65272af75c408cda`.

## 3. The question
Over the last month the effects design went through these spellings, each retracted or replaced: `Effect`/`Clock`/`Policy` keywords; `mode`; `host` keyword; `(Host (* ...))` as a node annotation with `Key` columns as inputs; `want` rows; `effect` rows with a rule column; `effect` rows with a cons-list pattern and a `free` atom; and finally `effect` rows as the language's own partial-application term, with no annotation at all and the runner naming what it serves (`dl8 eval --serve`). Chris's summary of his own position: "in dl7 we thought to just say fuck this and let any rel be settable from outside" and "I'm about done fighting an AI on compiler design for the 8th time".

Answer, with evidence, all three:
1. Is the current design (section 4, the last two documents) the simplest thing that meets the v6-core goal, or is it still carrying ceremony? Name every construct you would delete or add, with the file:line that would change.
2. Across the month's transcript, who introduced each piece of complexity and who removed it? Build the table. Score each turn of the design conversation on one axis: did it add a construct, remove one, or rename one. Then say plainly whether the pattern is "the human keeps asking for less and the assistant keeps adding", "the human changes the goal and the assistant follows", "both", or something else. This is the "vector calc" Chris asked for: direction and magnitude of each side's pushes, from the record, not from politeness.
3. Where the assistant over-complicated, say what a simpler answer would have been at that turn. Where the human was inconsistent or moved the goal, cite the two messages that conflict. Neither side is protected.

## 4. Sources, in reading order
| path | what |
|---|---|
| `chat_log/20260913.1.dl8-shrink-effects-as-mode-history-reports.md` | first effects design and the shrink lane |
| `chat_log/20260913.2.dl8-host-annotation-want-rows-share-macro-self-host.md` | Host-as-annotation design, `want` rows, `share` as rules, self-host stages |
| `chat_log/20260914.0.dl8-night-lanes-store-host-effect-extract-move.md` | the night's lanes, the five store decisions, findings |
| `plans/v8/2026-09-14-v8-host-effect.brief.md` | the Host-annotation implementation brief (superseded) |
| `plans/v8/2026-09-14-v8-effect-demand.brief.md` | the current design: effects without Host |
| `plans/v8/2026-09-14-v8-store.PLAN.brief.md` and, on branch `plan/v8-store-20260914`, `plans/v8/2026-09-14-v8-store.PLAN.md` section 15 | persistence plan and Chris's five decisions |
| `plans/history/2026-09-13-how-i-steered.md`, `plans/history/README.md` | Chris's own account of steering v0 to v7 |
| `v8/src/` | the compiler: `_0_read` to `_8_driver`, kernel `_6_eval`; 16k lines; read `_6_eval/_5_evaluate.rs` (`demand`, `positive_solutions`) and `_6_eval/_4_kernel.rs` fully, the rest by module doc |
| `v7/prelude/*.dl7` | the language's own prelude: constructors (`Key`, `Partial`, `Pick`), `keyed_edge`, type algebra |
| `v6/README.md`, `v6/sprefa-engine-rs/src/{incremental.rs,hosts.rs}`, `v6/tsv2/goldens/ghcacher_tick_golden/0_ghcacher_clock_golden.dl6` | what the v6 core does with hosts and demand rows (`__demand_`) |
| PRs #739, #740, #741 (being rewritten), #742, #743 on hafley66/sprefa | the open work; read bodies with `gh pr view <n> --json body -q .body` |
| `CLAUDE.md` | the standing laws, including "Lang design happens with Chris in the room" |

## 5. The cross-reference: what Chris said
The boop store is plain SQLite at `~/.agent/boop.db`; `boop db "<sql>"` runs a query and prints JSON lines. Timestamps are epoch milliseconds. Schema you need:
```
agent_turn(session_id, turn, ts, role_id, said, cwd_id)
dict_role(id, value)            -- 'user', 'assistant', 'tool', 'system', 'developer'
agent_session(session_id, harness_id, nickname, cwd_id, branch_id, started_ts)
dict_cwd(id, value)
```
Start from:
```sql
select t.ts, s.session_id, t.turn, substr(t.said,1,400) as said
from agent_turn t
join dict_role r on r.id = t.role_id
join agent_session s on s.session_id = t.session_id
left join dict_cwd c on c.id = s.cwd_id
where r.value = 'user'
  and c.value like '%sprefa%'
  and t.ts > (strftime('%s','now') - 30*86400) * 1000
  and length(t.said) > 20
  and t.said not like '<%'
order by t.ts;
```
That returns on the order of 16,000 rows; most are hook injections and command echoes. Filter to design talk with `and (t.said like '%host%' or t.said like '%effect%' or t.said like '%want%' or t.said like '%key%' or t.said like '%demand%' or t.said like '%dl8%' or t.said like '%v8%' or t.said like '%prolog%' or t.said like '%rel%')` and read the assistant turn that follows each (same session, turn + 1, role assistant). Quote messages by timestamp and session id. Every claim about who said what carries such a citation. Chris's messages are lower-case, profane, and short; do not clean them up when quoting.

## 6. What the report must contain, in this order, with a TOC
1. Verdict in five lines: simplest-or-not, who pushed which way, the one change you would make first.
2. The construct ledger: one row per construct (Effect, Clock, Policy, mode, host, Host annotation, Key-as-mode, want, effect+rule, effect+cons+free, effect+partial application, `--serve`, hosted diagnostics, `kernel_arity` fallback, `edge_ref` keys, `Fold`, `term_lt`, table naming, and any you find), columns: introduced by (cite), retracted by (cite), still alive, your call keep/cut/rename.
3. The turn-by-turn vector table for the effects thread: timestamp, who, add/remove/rename, what, and a one-line reading.
4. The v6-core gap: what the v6 runtime does that v8 cannot yet, from `incremental.rs` and `hosts.rs`, as a table of capability, v6 site, v8 state, effort.
5. Where the assistant over-complicated (cite turn, give the simpler answer).
6. Where the human moved the goal or contradicted himself (cite both messages).
7. What the reviewer would do next week, three items max, each with the file it touches.
8. Every query you ran, verbatim, and its row count.

## 7. Style laws
No em dashes. No sycophancy. No hedging paragraphs. Banned words: provenance, substrate, load-bearing, regime, honest, ground as a verb, refusal, "ground truth". Tables over prose; a paragraph over three sentences is a defect. Numbers only from commands you ran. Quote Chris verbatim. Do not soften a finding against either side. Commit the one file, push, open a PR to `main` titled `review(v8): design review, <suffix>`, body = section 1 verbatim, ending with `🤖 Generated with [Claude Code](https://claude.com/claude-code)`. Then `boop beep --no-wait --as <your-lane-name> sprefa-coordinator "design review <suffix>: PR #<n>"`. Never merge, never spawn subagents, never edit anything but your one file.
