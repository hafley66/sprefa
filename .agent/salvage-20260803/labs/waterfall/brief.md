# brief: PLAN ONLY — session waterfall (devtools-network style) for the strip

You are a research + planning lane. NO implementation this session; a
next session implements from your plan. If reality deviates from this
brief, STOP and write what you saw into PLAN.md; do not improvise.

Bounds: worktree /Users/chrishafley/projects/instant-lab-waterfall,
branch lab/waterfall-plan at 75dc33f (verify: `git log --oneline -1`
must show 75dc33f; any other base = STOP AND REPORT). Write ONLY
PLAN.md and PLAN.visual.human.unga.md at the worktree root. Read
anything in this worktree. No src edits, no commits, no other
worktrees, no subagents.

## The ask (user's words, verbatim, 2026-08-03)
"can we reuse the network tools visualizer somehow from chrome or a
react component emulating it? i want to show a session scroller where
it shows when session started and message and type when as ticks like
in a network request viz waterfall thingy, thne we have top scroller
that defines the range so the table stays small, it only shows if we
unclick the default selected 'Show active' states"

Decode: the in-tab strip (src/plugins/harnessTrace/InTabStrip.tsx)
currently shows only going-on shells (live/idle; 0_strip.ts external()).
Add a "Show active" checkbox, DEFAULT CHECKED = exactly today's view.
UNCHECKING enters history mode: a devtools-network-style waterfall —
one row per session (bar from session start to last activity), message
events on the row as ticks colored/shaped by type, a top overview
scroller (brush) that defines the visible time range, and the table
below constrained to sessions intersecting that range so it stays small.

## Part 1 (MANDATORY, before any design): library research
Build-vs-buy law: never "write our own" without candidate-by-candidate
written analysis. No one-line dismissals. For EACH candidate: what it
gives, license, npm package + weekly downloads + last publish, bundle
cost, how it renders (svg/canvas/dom), whether the brush/overview part
comes free, and the concrete integration sketch or the concrete reason
it loses. Verify facts with `npm view <pkg>` (network reads are fine);
mark anything unverifiable UNKNOWN.

Candidates to cover at minimum:
1. Chrome devtools frontend itself (the user's first question): the
   devtools-frontend repo's PerfUI/TimelineOverviewPane / network
   waterfall components — are they extractable? License, coupling,
   realistic verdict with receipts (repo paths), not a shrug.
2. vis-timeline
3. react-calendar-timeline
4. @visx (brush + shape primitives)
5. d3 (d3-brush + d3-scale) over hand-rolled svg
6. Observable Plot
7. react-chrono or any timeline lib you find that fits better
Existing deps first: read package.json — anything already installed
(e.g. @tanstack/react-table is there) that shrinks a candidate's cost
counts in its favor. State the recommendation as a recommendation with
prices; the user rules.

## Part 2: the plan, in this exact layered order (user's protocol)
1. TYPE SIGNATURES FIRST: every new interface in
   src/plugins/harnessTrace/0_types.ts style (I prefix, declared in the
   package's types file), every new pure function signature, the
   component props. Data types: ISessionSpan (session id, harness,
   start, last), ISessionEvent (ts, type, sessionId) or better names.
2. PSEUDO-CODE bodies as comments under each signature.
3. INSTANCE LIFETIMES for each stateful thing: the range brush state,
   the checkbox state (per-terminal like ITermStripEntry? say which),
   the loaded event windows, when each is created/dropped.
4. STORAGE LAYOUT + reads/writes + uniqueness: where events come from —
   read the existing seams first: src-tauri/src/harness.rs
   (harness_trace_rows returns sessions only), scripts/livespawn.ts
   readers, ~/.agent/mail/bus.ndjson envelopes, opencode.db message
   tables, claude jsonl transcripts. The plan must name the exact seam
   for message-level events per harness and whether a new rust command
   is needed (the standing ruling prefers server/rust land, tiny_http
   precedent activity.rs:8787). Lazy law: message events load per
   visible range/session, never all history up front.
Also: how the waterfall coexists with the existing table + router
(3_router.ts view stack), the e2e gate shape (playwright.stripsub
precedent, seeded rows, screenshot), and a step ladder where each step
lands green (vitest + e2e) on its own.

## Deliverables
- PLAN.md: part 1 research table + writeups, then part 2 layers, with
  file:line receipts for every claim about existing code.
- PLAN.visual.human.unga.md: plain words, ascii mock of the waterfall +
  brush + table, zero citations, zero code blocks.
A plan without the unga doc is undelivered.

## Style laws
Banned words: provenance, substrate, load-bearing, regime. Comment
budget: constraints only. Interfaces I-prefixed in 0_types.ts. One
manual .subscribe() per app stays main.ts. Async stays rxjs-or-promise
at the invoke seam per file precedent; in-memory list work is plain
arrays. Colocated consistency: follow harnessTrace's existing 0_/pure +
component split.
