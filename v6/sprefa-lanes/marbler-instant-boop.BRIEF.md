# BRIEF: plan replacing instant's agent views with marbler over boop

Planning only. Write ZERO implementation code. One task-breakdown doc plus its
unga twin. You are working in a worktree of `~/projects/instant`.

## Base
Confirm the base with `git log --oneline -1` before your first commit.

## The ask, in the user's words

> "is boop ready to integrate with marbler in ~/projects/hafley-rxjs/packages/marbler?
> can we get a pro4 on that to plan a breakdown of tasks to get that into instant
> to replace the cmd+shift+period external shells/subagents view in instant bc
> holy shit ~/projects/instant and its subagents from bus literally never worked."

Two jobs, in this order:
1. **Answer "is boop ready"** with evidence, not a yes.
2. **A task breakdown** to put marbler into instant in place of the current view.

And one thing you must do BEFORE either: find out why the existing thing never
worked. The user says it "literally never worked". Rebuilding on top of an
unexplained failure repeats it. Diagnose first, in writing.

## The three codebases. Measured, verify each.

| repo | fact |
|---|---|
| `~/projects/instant` | `src/plugins/harnessTrace/` is 5539 lines across bus, live, mail, strip, tree, waterfall, viewerTab, leg, join, liveState, each with a test |
| | `src/boopAgents.ts`, 507 lines, "Shells out to the boop binary", hardcoded path at line 8 to `/Users/chrishafley/projects/claude-research/bin/boop`, fails at line 425 if lanes arrive without a pstree projection |
| | `src/plugins/cass/1_SwarmPanel.tsx`, 92 lines |
| | the keybinding: `src/main.ts:219`, `$mod+Shift+Period` bound to `term.strip`, titled "Toggle Relations Strip" |
| | biggest views: `HarnessTracePanel.tsx` 282, `InTabStrip.tsx` 305 |
| `~/projects/hafley-rxjs/packages/marbler` | `@hafley66/marbler`, built on `@hafley66/grid` and `@hafley66/signals` |
| | tabular columns are DOM rows; PixiJS renders the viewport-clipped waterfall and the density overview |
| | the overview owns a shared time viewport: cursor-anchored wheel zoom, horizontal pan, live-follow, fit on double click |
| | **phase and event counts do not change the DOM node count**, which is the property that matters for thousands of agent turns |
| | key files: `0_types.ts`, `1_model.ts`, `0a_TimeViewport.ts`, `1a_WaterfallPixi.tsx`, `1b_TimeNavigatorPixi.tsx`, `2_Marbler.tsx` |
| boop store | plain SQLite at `~/.agent/boop.db`, ~306 MB, queried through `boop db "<sql>"` |
| | tables: `agent_session`, `agent_turn`, `agent_touch`, `agent_cmd`, `agent_fetch`, `agent_skill`, `agent_span`, `agent_edge`, `agent_usage`, `agent_live`, `agent_live_span`, `agent_pr`, plus `dict_*` |
| | **new as of PR #217**: `agent_trace`, `agent_trace_span`, `agent_lane` (goal text, brief path, brief body at spawn), `markdown_cache` |
| | 2767 sessions, 382,497 turns; 1556 sessions (56.2%) backfilled into 162 traces, the rest deliberately unattached |

## Job 0: why did it never work

Read the existing `harnessTrace` plugin and `boopAgents.ts` and determine what
actually breaks. Candidate causes to check, and there will be others:

- the hardcoded boop path at `boopAgents.ts:8`
- the hard failure at `:425` when lanes lack a pstree projection
- whether it polls, and how often, and what that costs against a 306 MB db
- whether the "bus" it reads is the ndjson mailbox at `~/.agent/mail/bus.ndjson`
  or the SQLite store, and whether those two ever disagreed
- whether subagent rows were ever produced at all. Note that until PR #217 there
  was no trace identity, so a subagent fan-out had no durable parent grouping,
  which is a strong candidate for "subagents never worked"

Write the diagnosis as a short section with citations. If the cause is that the
data simply did not exist until now, say that plainly, because it changes the
plan from "fix" to "build on new ground".

## Job 1: is boop ready

Answer with a data-shape table: what a marbler waterfall needs per row and per
event, and whether boop can supply it today.

marbler's own model is in `0_types.ts` and `1_model.ts`. Read them and state the
shape it consumes rather than guessing. Then map:

| marbler needs | boop has | gap |
|---|---|---|

Cover at minimum: a lane or track identity, a time axis, discrete events with
timestamps, a phase or state per span, nesting or parentage, and labels. Note
that `agent_turn` has a `ts`, `agent_span` and `agent_live_span` exist, and
`agent_edge` carries parent-child with `first_ts`/`last_ts`.

Say explicitly whether the answer is yes, yes-with-gaps, or no, and list the
gaps as boop work items.

## Job 2: the task breakdown

The deliverable. A numbered task list where each task has: what it does, which
files it touches, what proves it done, and a rough size. Order by dependency.

Decide and justify:
- what of the 5539-line `harnessTrace` plugin is REPLACED by marbler, what is
  KEPT because marbler does not cover it (mail, tree, bus parsing), and what is
  DELETED
- how marbler is consumed: a workspace dependency, a published package, or
  vendored. `@hafley66/grid` and `@hafley66/signals` come with it; say what that
  means for instant's dependency tree
- how data reaches the view: keep shelling out to `boop`, read the SQLite file
  directly, or a new `boop` subcommand that emits marbler-shaped JSON. Price all
  three. Note the standing law that boop never reinvents SQL and `boop db` is
  the query surface
- whether `$mod+Shift+Period` keeps its current binding and title, or the view
  is renamed
- live updates: how the panel learns about new turns. Polling interval, a
  watcher, or a boop-side push. Respect the 10-second law and the standing rule
  that nothing seizes the machine

## Anti-cheat
- do not plan a rebuild before writing the diagnosis
- cite `file:line` for every claim about existing code
- no task without a proof-of-done
- the unga twin is required
- do not write implementation code

## Deliverables, exactly two files, in the instant worktree
1. `plans/2026-08-12-marbler-instant-boop.PLAN.md`, table of contents first
2. `plans/2026-08-12-marbler-instant-boop.PLAN.visual.human.unga.md`

Create the `plans/` directory if instant does not have one.

## File ownership
YOURS: the two plan docs. Everything else in every repo is READ ONLY.

## Style laws
- No em dashes. Banned in prose and identifiers: `provenance`, `substrate`,
  `load-bearing`, `regime`.
- "refusal" banned in prose; say TODO or not built yet.
- No sycophancy, no negative parallelism ("not X, Y").
- Tables, lists and mermaid over prose. Docs open with a table of contents.
- `signal` is the name of the user's library; never use the word loosely.
