# BRIEF: every set rel gets `"__id" INTEGER PRIMARY KEY`, one shape, no exceptions

## Base
- Worktree of `/Users/chrishafley/projects/sprefa`. Base sha `154ae23c`.
- FIRST action: `git log --oneline -1`. Any other base = STOP AND REPORT.

## USER DECISION, 2026-08-12, verbatim
"TAKE THE CORRECT AND MOST CONSISTENT ONE EVEN IF IT MEANS MORE WORK"

The decision is made. You are implementing it, not re-opening it. Do not come
back with a fork asking which shape to use.

## One sentence
Today `lower.pl` emits set-rel tables in TWO shapes and the split is why a
zero-column rel has no table at all; collapse both onto the surrogate-key shape
the repo's own law already mandates.

## The two shapes today
| branch | `lower.pl` | shape |
|---|---|---|
| reference target (a declared type name) | :2101-2103 | `CREATE TABLE x ("__id" INTEGER PRIMARY KEY, <cols>, UNIQUE (<cols>))` |
| ordinary set rel | :2112-2114 | `CREATE TABLE x (<cols>, PRIMARY KEY (<cols>)) WITHOUT ROWID` |

The second is the outlier and it contradicts the repo's own standing law:

> **Surrogate keys law (user-set 2026-08-07, second interning incident)**: stored
> rels key on INTEGER ids; natural/composite TEXT keys live ONCE in a dictionary
> table; a composite TEXT PRIMARY KEY in emitted or hand DDL is a DEFECT.
> Measured: TEXT keys 1.7-2.0x slower on identical tables, every index copies
> the full key.

`PRIMARY KEY (<cols>)` over text columns IS the composite TEXT primary key that
law calls a defect. It is in the emitter.

## The target shape, one branch for both
```sql
CREATE TABLE "x" ("__id" INTEGER PRIMARY KEY, <cols>, UNIQUE (<cols>))
```
and at zero columns, with no trailing comma and no degenerate constraint:
```sql
CREATE TABLE "x" ("__id" INTEGER PRIMARY KEY)
```

`UNIQUE (<cols>)` carries the content identity that `PRIMARY KEY (<cols>)`
used to carry, so `INSERT OR IGNORE` and `ON CONFLICT` dedup keeps working.
At zero columns there is no content, so there is no dedup: a 0-ary rel is a
proposition and every arrival mints a row. State that in the plan doc.

## MANDATORY READS BEFORE YOU WRITE ANY DDL
`.claude/skills/sql-relational-design` and `.claude/skills/sqlite-costs`. Both
are required by CLAUDE.md before any schema, lowering, or DDL design. The
second one matters here: dropping `WITHOUT ROWID` changes the physical layout
of every set-rel table in the system and you must price it, not guess it.

## Files you own
| path | permission |
|---|---|
| `v6/prolog/lower.pl` | full |
| `v6/prolog/analyze.pl` | ONLY `set_rel_key_positions`-adjacent arity-0 work; see step 1 |
| `v6/prolog/0_generic_expand.pl` | the zero-column stop at :278-284 |
| `v6/prolog/conformance/fixtures/**` | add fixtures |
| `v6/prolog/compile/dl_view/**` | regenerated output only, never hand-edited |
| `plans/2026-08-12-uniform-surrogate-id.md` | create |

Forbidden: `v6/prolog/compile.pl` and `v6/prolog/compile/6_emit_dd_plan.pl`
(the `dd-plan-emitter-seam` lane owns both), `.github/**` (the triage lane owns
the ledger), `v6/bench-cli/**`, `v6/labs/**`, `chat_log/**`.

## Work, in this order. Commit after each step, with its gate output.

### 1. `lower.pl:867`, the third arity-0 landmine
`set_rel_key_positions` falls back to `length(Columns, Arity), numlist(1, Arity, KeyPositions)`.
`numlist(1, 0, _)` FAILS, so `lower_program` fails with no message. PR #206
fixed the same idiom at `analyze.pl:291` and `analyze.pl:529`; this is the
third and last one the coordinator found. Fix it the same way: arity 0 yields
`[]`.

Fail-first receipt, run it BEFORE the fix and paste the output:
```bash
printf 'rel zed().\nrel w(id: int).\n\nw(1) <- zed().\n' > /tmp/zed.dl6
bash v6/prolog/compile/scripts/compile_dl6.sh /tmp/zed.dl6 /tmp/zed.ts ; echo "rc=$?"
```
Today that is `rc=1` with NOTHING on stdout or stderr. That silence is itself
part of the defect; say whether your fix removes it.

### 2. Collapse the two DDL branches
One `format/3` for both, guarded on the empty column list. The declared-type
branch also emits a `CREATE TEMP VIEW ... __rendered` companion (:2107-2109);
keep that behavior attached to declared type names, not to the table shape.

### 3. Every reader of the old shape
`PRIMARY KEY (<cols>) WITHOUT ROWID` moving to `UNIQUE (<cols>)` changes what
`ON CONFLICT` targets and what `INSERT OR IGNORE` dedups against. Grep and fix
every site. Start from `set_arrival_sql_parts/4` at `lower.pl:2822-2841`, which
the coordinator already identified as the content-match site: the unkeyed arm
is `INSERT OR IGNORE INTO` + `''` conflict at :2822, the keyed arm builds
`ON CONFLICT (<key cols>)` at :2823-2841. There will be more. Find them by
running the gates, not by guessing.

### 4. Unblock the zero-column reference target
With steps 1-3 done, the stop at `0_generic_expand.pl:278-284` should no longer
be needed for the all-columns-moved-out case. Verify with probe C:
```
rel combo_move(name: text, url: text).
rel combo_pair(use_before: option(combo_move), use_after: option(combo_move)).
rel move_combo(id: int, normal: combo_pair).
```
Remove the stop ONLY if probe C compiles green AND a two-parent fixture proves
two distinct parents stay distinct rows. A previous lane correctly refused to
remove it without that fixture; removing a named stop without registration
working turns it into a silent wrong answer. If you cannot get there, LEAVE THE
STOP and say so.

### 5. Price it
`sqlite-costs` says what `WITHOUT ROWID` buys. Measure, do not assume:
`just dl6-bench` and `just perf-all`. Report the before and after numbers in
the plan doc. A slowdown is a finding to report, not a reason to abandon the
shape; the user chose consistency knowing it means more work.

## Gates. Every commit.
```bash
cd v6/tsv2 && bash scripts/sweep.sh     # RUN total=286 identical=283 wrong=0
cd v6 && just text-door                 # expect movement; see below
cd v6 && just conformance               # 392 PASS 0 FAIL today
cd v6 && just dd-grade                  # DD-GRADE HOLDS
cd v6/prolog && swipl -g go -t halt ARCH.pl
```
`text-door` byte-identity WILL move, because the emitted DDL changes on
purpose. That is the one gate allowed to move, and every moved byte must be a
DDL line you intended. Diff it and show the diff. `sweep`'s
`identical`/`wrong` counts must NOT move: the emitted programs must still
compute the same answers.

## KNOWN RED on main, not yours
- `just green-all` is red; `.github/CI-KNOWN-RED.md` allowlists 11 legs and a
  triage lane is rewriting it right now. Do not read it as truth and do not
  edit it.
- `roundtrip` fails on `mutual_recursion_matches_oracle`, `fail(not_variant)`.
- bench-cli's 3 red cells trace to `cases.json` pointing at the lossy
  `dl_view/` render. Not yours.

## Anti-cheat
| rule | why |
|---|---|
| `sweep` `identical` and `wrong` must not move | the shape changes, the answers do not |
| every `text-door` byte that moves is shown in a diff | otherwise a wrong DDL hides inside an expected change |
| the perf numbers come from `just dl6-bench` / `just perf-all`, before and after | the whole point of dropping `WITHOUT ROWID` is that it costs something |
| the stop at `0_generic_expand.pl:278-284` is removed ONLY with a two-parent distinctness fixture | a named stop removed without registration is a silent wrong answer |
| no fixture is widened to force a pass | |
| you do not touch `compile.pl` or `6_emit_dd_plan.pl` | another lane owns them |

## Worktree setup, before your first commit
```bash
mkdir -p v6/sprefa-extract/target/release
cp /Users/chrishafley/projects/sprefa/v6/sprefa-extract/target/release/extract \
   v6/sprefa-extract/target/release/extract
(cd v6/tsv2 && pnpm install)
(cd v6/sprefa-store/js && pnpm install)
```
`git commit -n` and `--no-verify` are FORBIDDEN.

## Rails
- Commit after each numbered step. Never spawn a subagent.
- The 10-second law applies to every command you run.

## Style laws, inline
- No em dashes. No `provenance`, `substrate`, `load-bearing`, `regime`.
- "refusal" banned in prose; unbuilt is "TODO" or "not built yet".
- No `here is`, `here's`, `below is`, `the following`, `clearly`, `obviously`.
- Comments state only constraints the code cannot show.
- dl variable names descriptive, never single-letter.
