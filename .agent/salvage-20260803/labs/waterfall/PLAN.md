# PLAN — session waterfall (devtools-network style) for the in-tab strip

Worktree      : instant-lab-waterfall
Branch        : lab/waterfall-plan
Base          : 75dc33f (verified `git log --oneline -1` = 75dc33f)
Scope         : research + plan only. No src edits, no commits, no subagents.
Deliverables  : this file + PLAN.visual.human.unga.md
State         : a next session implements from this plan.

## Reality deviation receipts (recorded, no improvisation)

- The brief cited `activity.rs:8787` as the tiny_http precedent. On disk
  `src-tauri/src/activity.rs` is 837 lines total. The server is `spawn_server`
  at `src-tauri/src/activity.rs:320` (thread + `Server::http(("127.0.0.1",
  INGEST_PORT))`); `INGEST_PORT` is 8787 (`activity.rs:22` line, value 8787).
  The 8787 is the PORT, not a line. The server precedent stands, port value
  correct, line citation wrong.
- The brief assumed the strip is a single presentational state. It is a view
  union already: the table (`TreeTable`) is replaced by `MailPreview` while a
  `mail-preview` view is the router's top (`InTabStrip.tsx:156`). The waterfall
  slots into that same switch, which is where "how the waterfall coexists with
  the existing table + router" resolves.
- Codex and kimi session *lists* are stubbed empty in `ledger.rs`
  (`list_ai_sessions`, `ledger.rs:721-723`) but their per-session *reads* work
  (`read_codex` `ledger.rs:166`, `read_kimi` `ledger.rs:226`, wired through
  `read_ai_messages` `ledger.rs:698-699`). The waterfall reads per-session, so
  codex/kimi ticks are available even though their browse lists are not.

---

# PART 1 — library research (build-vs-buy, candidate by candidate)

Method: facts pulled with `npm view <pkg>` (registry on 2026-08-03) and the
npm downloads API. Anything not verified on this machine is marked UNKNOWN.
Weekly download figures are point values for the week ending 2026-08-02.

Existing deps that shrink candidates' cost before we add anything:
- `@tanstack/react-table` ^8.21.3 and `@tanstack/react-virtual` 3.14
  (`package.json:26-28`) — the canonical grid stack; the sessions table below
  the waterfall must reuse `TreeTable` (`src/treetable.tsx`, `TreeTableProps`
  at `treetable.tsx:57`), per `AGENTS.md` "No bespoke list/table UIs".
- `rxjs` ^7.8.2, `react-resizable-panels` ^3.0.6, `vega-embed` ^7.1.0
  (`package.json:42,49`) exist but none provide a brush or a timeline.
- `vega-embed` (vega-lite) is interesting for static charts but has no free
  interactive brush primitive and no row-of-bars-with-vertical-tick-marks idiom
  without a custom signing module; researched, rejected below.

No d3, no visx, no vis-timeline, no observable plot is installed today.

Candidate summary table:

| # | candidate | license | last publish | wk downloads | unpacked | brush/overview free? |
|---|-----------|---------|--------------|--------------|----------|----------------------|
| 1 | Chrome devtools-frontend (PerfUI / network waterfall) | BSD-3 (Chromium headers) | n/a (monorepo, main) | n/a | n/a (repo) | yes (OverviewGrid) but fused to SDK |
| 2 | vis-timeline 8.5.2 | Apache-2.0 OR MIT | 2026-07-15 | 245,542 | 77,580,919 | yes |
| 3 | react-calendar-timeline 0.30.0-beta.4 | MIT | 2026-07-24 | 156,259 | 1,769,071 | yes |
| 4 | @visx/brush 4.0.0 (+scale,shape) | MIT | 2026-06-11 | 251,581 | 116,006 | yes |
| 5 | d3-brush 3.0.0 + d3-scale 4.0.2 | ISC | 2022-06-14 | 17,129,718 | 66,555 | yes (d3-brush) |
| 6 | @observablehq/plot 0.6.17 | ISC | 2026-04-06 | 607,508 | 1,526,486 | no (no brush gesture) |
| 7 | react-chrono 3.3.3 | MIT | 2025-12-16 | 37,505 | 1,508,228 | no (vertical) |

## 1. Chrome devtools-frontend (the user's first question)

Can we reuse the network visualizer itself? License-wise yes: every source file
is Chromium, BSD-3-Clause (headers read "Copyright ... The Chromium Authors ...
use of this source code is governed by a BSD-style license"). On-paper
extractable. Realistically no, and the receipts:

- The network waterfall lives in
  `front_end/panels/network/NetworkWaterfallColumn.ts` (26,100 bytes) + its
  data model `NetworkDataGridNode.ts` (75,954) + the log host
  `NetworkLogView.ts` (125,387), all in `front_end/panels/network/`.
  `NetworkWaterfallColumn` paints against `Sdk.NetworkRequest` objects and
  `UIUtils` helpers; there is no published npm split, only the monorepo via
  `BUILD.gn` (`front_end/panels/network/BUILD.gn`).
- The overview/brush concept the user actually wants is the Timeline/Perf
  "overview" which is `front_end/ui/legacy/components/perf_ui/OverviewGrid.ts`
  (28,654 bytes) + `TimelineOverviewPane.ts` (22,475) + `ChartViewport.ts`
  (20,030) + `TimelineGrid.ts` (10,894). These are `legacy/components`, not
  React; they render into a DevTools-hosted DOM shell.
- Coupling fan: `perf_ui` imports `Root.Runtime` (their module loader),
  `Platform`, `Common.Settings`, `UIUtils`, and the whole `models/trace`
  `TraceEngine` for data. `FlameChart.ts` in the same package is 183,971 bytes.
  Vendoring even `OverviewGrid` drags in DevTools' runtime, settings, and unit
  system, which are not separable npm packages. There is no maintained app-embed
  of these outside of DevTools itself.

Verdict: reuse in spirit, not substance. We copy the interaction model (one
brush overview on top, a time-axis-constrained row list below) and render it
ourselves at ~80 svg lines. This is the stated reason the rest of part 1 exists.

## 2. vis-timeline

Apache-2.0/MIT, 245k/wk, actively publishing (2026-07-15). Purpose-built
horizontal timeline with groups (rows), items (bars), and a built-in
"StackSubgroups"/item range. Fully-featured: it would render the session bars
and even has an overview/range slide via `rangechange`. BUT:
- unpacked size 77.5 MB (by far the largest candidate); pulls vis-data/vis-util
  and its own imperative API (construct on a DOM node, call `.setItems`), which
  fights the React host and the repo's "colocated consistency" 0_/pure split.
- It draws its own grid + scroll chrome; embedding one scroller inside the
  strip's constrained 240px shell duplicates the existing TreeTable + virtual
  stack instead of reusing it.
- Integration cost (a second rendering paradigm + 77 MB dep) is not repaid by
  anything the thin waterfall needs.

Verdict: loses on weight + non-React API vs the need.

## 3. react-calendar-timeline

MIT, 156k/wk, but the latest published version is still `0.30.0-beta.4`
(published 2026-07-24 is a maintainer re-release of a package that has sat in
pre-1.0 beta for years; many long-open issues). Has rows/groups, items, a
`resizeDetector`, and time-based click handlers. Loses on:
- 0.30 pre-1.0 status = higher maintenance risk than raw d3.
- It owns the whole table+axis; we already have the table via TreeTable and only
  want the chart strip.

Verdict: loses (beta, heavier than needed, owns our table).

## 4. @visx

MIT, 251k/wk. `@visx/brush`, `@visx/scale`, `@visx/shape` are thin React
wrappers around d3-scale/d3-brush/d3-shape (their 4.0.0 release, 2026-06-11).
Pros: idiomatic React (jsx props, no manual lifecycle). Cons:
- it is a wrapper layer over exactly what we'd otherwise import raw; every
  version of all sibling @visx packages must align, so the dep graph is 3+
  aligned packages for what d3-brush + d3-scale give in two.
- The waterfall bar/tick layout is still ours either way; @visx saves almost
  nothing on that surface.

Verdict: acceptable, but raw d3-scale + d3-brush is the same power at half the
candidate surface and zero version-alignment. Not chosen.

## 5. d3 (d3-brush + d3-scale) over hand-rolled SVG  <- RECOMMENDED

ISC, d3-brush at 17.1M/wk (the de-facto standard), last published 2022/2023
(which reads as *stable*, these are finished primitives). Together 240 KB
unpacked.
- What we need is exactly a linear/UTC time scale (d3-scale) and a drag-a-
  rectangle gesture (d3-brush). Both are the reference implementations; the
  user preference "never write our own for a common-shaped problem (scale,
  brush)" points straight here. d3-scale is world-authoritative linear
  interpolation + tick formatting; d3-brush is the canonical min/max window
  gesture with correct pointer math across edge cases we would otherwise
  re-derive and bug-test.
- Brush/overview:" comes free (that is literally what d3-brush renders: a
  selected window rect over a full-domain strip, with drag/resize + drag the
  selected window's left/right edges).
- The only bespoke surface is the SVG row layout: one `<rect>` bar per session
  (left=x(start), width=x(end)-x(start)), one `<circle>`/mark per tick
  (cx=x(ts), cy=row midline, fill/shape by type), a background grid, and the
  brush overlay on top. ~80 lines of JSX we control and unit-test via pure
  projection functions.
- Works headless for vitest: d3-scale runs in node (no DOM), so the pure
  projection module is testable exactly like the rest of the 0_ modules.

Verdict: chosen for the chart. d3-brush + d3-scale only, not the d3 aggregate
bundle.

## 6. @observablehq/plot

ISC, 607k/wk, actively shipping (2026-04). Excellent declarative static
charts, and `plot.plot()` returns an svg we could mark up with an overlay.
But Plot is a drawing library, not an interaction library: there is no
brush/range gesture primitive; "no brush" is the honest answer. We would have
to hand-roll the brush anyway on top of Plot's output, at which point Plot
contributes only the axis (d3-scale already does that) while adding a 1.5 MB
dependency and a second markup model.

Verdict: loses (no brush, redundant axis, heavier).

## 7. react-chrono / other timeline libs found

react-chrono (3.3.3, MIT, 37k/wk) is a *vertical* item timeline (cards down a
spine), the opposite orientation of a session row-bar waterfall, and its items
are DOM cards, not time-scaled bars. Not a fit. No other candidate surfaced by
the npm search that beats d3-brush+d3-scale for "one brush + row bars + ticks"
under a grid table.

Verdict: loses (orientation + DOM cards).

## Recommendation (prices; the user rules)

Add **d3-scale** and **d3-brush** (ISC, ~240 KB unpacked total, ~66 KB
`d3-brush` + ~174 KB `d3-scale`), render the waterfall as hand-rolled SVG on
top of them in a new `4_Waterfall.tsx`, and reuse `TreeTable` unchanged for
the sessions table below. Reject the Chrome frontend (coupled support code,
not a packaged embed), vis-timeline (77 MB + own chrome/API), 
react-calendar-timeline (0.30 beta), Observable Plot (no brush), react-chrono
(vertical). @visx is a defensible alternative but adds an aligned multi-package
graph for no capability gain over raw d3 here.

---

# PART 2 — the plan

Order follows the user's protocol: types, pseudocode, lifetimes, storage, e2e,
ladder. Every claim about existing code carries a `file:line` receipt.

## 2.1 Type signatures first (`0_types.ts` additions)

All new types go in `src/plugins/harnessTrace/0_types.ts` (the package's types
file, precedent: every I-interface there), below the existing
`ITermStripEntry` block at `0_types.ts:252-254`. The strip policy's
`external()` gate drops done/dead rows (`0_strip.ts:67-74`); the waterfall is
history, so it must see those rows. Add the entry field first:

```ts
// A terminal's persisted strip entry. Absent (null) = never toggled on this
// terminal; present = the user summoned/dismissed it or flipped Show active.
// showActive absent = default-checked (today's going-on view).
export interface ITermStripEntry {
  open: boolean;
  showActive?: boolean;   // default true when absent; false = history waterfall
}
```

Mirror the field onto the persisted `TermStripState` in `src/state.ts:138-140`
(open + showActive) and read/write it with `store.get().termStrip[sid]` the same
way `InTabStrip.tsx:68-72` already does. `state.ts` owns the persisted shape;
`0_types.ts` owns the strip's reading of it, matching the existing split.

```ts
// One session's bar in the waterfall: start -> last activity (or now for live).
export interface ISessionSpan {
  id: string;        // session id (== AgentSessionNode.id)
  harness: Harness;
  start: number;     // unix ms
  end: number;       // unix ms (lastActivity; now for live/dead-present)
}

// One message/event tick on a session's bar, colored/shaped by type.
export type TickType = "user" | "assistant" | "tool" | "reasoning" | "dispatch";

export interface ISessionTick {
  sessionId: string;
  ts: number;        // unix ms
  type: TickType;
  preview: string;   // short text for title tooltip
}

// The brush window. Both unix ms. start <= end. A range is always within the
// overview domain; the table keeps only sessions whose span intersects it.
export interface IWaterfallRange {
  start: number;
  end: number;
}

// The full domain the overview strip spans (min session start .. max activity
// or now), padded so the brush has breathing room.
export interface IWaterfallDomain {
  start: number;
  end: number;
}
```

## 2.2 New pure module `0_waterfall.ts`

New file in the same package dir. Pure (d3-scale runs headless in node), no
React, no store import — so vitest covers it in the node environment the other
0_ modules use (`vitest.config.ts`, `include: src/**/*.test.ts`). One concern
per file (`AGENTS.md` "One panel per file", 0_/pure + component split).

```ts
// signatures (bodies as comments under each, per the brief)

export function toSpan(node: AgentSessionNode, nowMs: number): ISessionSpan;
//   start = Date.parse(node.ts)||nowMs fallback; end = Date.parse(node.lastActivity)||nowMs;
//   end = Math.max(end, start); live statuses render end = nowMs so the bar breathes.

export function tickType(msg: AiMessage): TickType;
//   candidate = msg.subtype?.includes("tool") ? "tool"
//             : msg.subtype === "reasoning" ? "reasoning"
//             : msg.role === "user" ? "user"
//             : msg.subtype /* codex uses subtype for tool result names */
//   fallback "assistant". Coerce the AiMessage rust shape from src/harness.ts.

export function tickFrom(msg: AiMessage): ISessionTick;
//   { sessionId: msg.session_id, ts: msg.ts, type: tickType(msg), preview: msg.preview }

export function sessionSpans(nodes: AgentSessionNode[], nowMs: number): ISessionSpan[];
//   nodes.map(toSpan) — spans for every node, including done/dead history.

export function domainOf(spans: ISessionSpan[], nowMs: number): IWaterfallDomain;
//   pad = max(1ms, (maxEnd - minStart) * 0.02)
//   { start: minStart - pad, end: Math.max(maxEnd, nowMs) + pad }

export function defaultRange(domain: IWaterfallDomain): IWaterfallRange;
//   whole domain by default; the checkbox default (Show active) never shows the
//   brush — only history mode does, and its first paint defaults to full domain.

export function spansInRange(spans: ISessionSpan[], r: IWaterfallRange): ISessionSpan[];
//   filter spans whose [start,end] intersects [r.start,r.end].

export function ticksInRange(ticks: ISessionTick[], r: IWaterfallRange): ISessionTick[];
//   filter ticks with r.start <= ts <= r.end.

export function visibleSessionIds(spans: ISessionSpan[], r: IWaterfallRange): Set<string>;
//   spansInRange(...).map(span.id)
```

## 2.3 Component `4_Waterfall.tsx`

New panel (one file). Renders, in history mode, top-to-bottom:

1. A brush overview strip (full domain, one thin row of mini session bars so
   the brush has something to drag on; d3-brush selection = `IWaterfallRange`).
2. The waterfall svg: one row per session, a `<rect>` from start to end and a
   `<circle>` per visible tick (cx from the scale, cy centered on the row,
   fill/shape by `TickType`).
3. Below it, the `TreeTable` reusing `COLUMNS` from `DockStripShared.tsx:18`
   constrained to sessions whose span intersects the range (rows only, no tree
   in history mode: the waterfall flattens to one bar per session).

Props:

```tsx
export interface WaterfallProps {
  nodes: AgentSessionNode[];                            // full in-scope node set (history not just live)
  events: ReadonlyMap<string, ISessionTick[]>;          // lazy per-session cache (see lifetimes)
  nowMs: number;                                        // clock, injected for deterministic tests
  onOpen: (sessionId: string) => void;                  // row click: openSession + push view
  onLayout: () => void;                                 // term slot refit, same as InTabStrip today
}
export function Waterfall({ nodes, events, nowMs, onOpen, onLayout }: WaterfallProps): JSX.Element;
```

Pseudocode:
```
spans = sessionSpans(nodes, nowMs)
domain = domainOf(spans, nowMs)
range, setRange = useState(() => defaultRange(domain))          // brush window (see lifetimes)
if spans changed, clamp range into domain (cheap reconcile in render)
x = scaleUtc().domain([range.start, range.end]).range([0, plotW])   // d3-scale, zoomed detail zone
visible = visibleSessionIds(spans, range)
rows = nodes.filter(n => visible.has(n.id))                        // feeds TreeTable, keeps it small
return <>
  <BrushOverview domain xSpans={spans} range={range} onRange={setRange} />   // d3-brush
  <svg waterfall>
    {spans.map(s => <g key={s.id}>
        <rect x=x(s.start) width=x(s.end)-x(s.start) .../>
        {(events.get(s.id) ?? []).filter(ticksInRange).map(t => <circle cx=x(t.ts) fill=TYPE_FILL[t.type] .../>)}
      </g>)}
  </svg>
  <TreeTable columns={COLUMNS} data={rows} virtual controls onRowClick={onRowClick} />
</>
```

`BrushOverview`: a host component owning the d3-brush instance (ref + effect),
so the d3 imperative API stays in one place; the pure projection/range math
lives in `0_waterfall.ts` and is unit-tested.

## 2.4 Where it plugs into InTabStrip

`InTabStrip.tsx` already renders `MailPreview` instead of the table when the
router top is `mail-preview` (`InTabStrip.tsx:156-183`). Add the same branch:
read `showActive` from the entry (`showActive: false`) => render `<Waterfall/>`
with the full node set (not the `external()` going-on subset) and `onOpen`
doing what `onRowClick` does today (`InTabStrip.tsx:113-117`). When `showActive`
is true (default) render exactly today's table (`external()` + `TreeTable`) —
unchanged for every existing test.

The act-bar gains the "Show active" checkbox (`InTabStrip.tsx:129-155`). Default
checked (absent field = true). Unchecking writes `showActive: false` into
`store.termStrip[sid]` (persisted) and calls `onLayout()` so the term refits.

Router coexistence: history mode is not a router view; it is the strip's
current presentational state like the table is today. Clicking a waterfall row
still `termViewRouter.push(sid, {kind:"agent-session"})` (`InTabStrip.tsx:116`),
which swaps the waterfall for the agent view; back pops to the waterfall, which
re-renders from the still-held range/events. No new view kinds in
`TermViewAny` (`0_types.ts:234`).

## 2.5 Storage layout, reads/writes, uniqueness

The waterfall has exactly two data planes:

1. Session spans — already served. `harness_trace_rows()` returns one
   `HarnessTraceRow` per session with `ts` (start) and `last_activity`
   (`src-tauri/src/harness.rs:466-472`, row struct `harness.rs:154-168`, all
   four readers `trace_claude/opencode/codex/kimi` `harness.rs:313-452`),
   enriched frontend-side by the mail ledger through `useAgentTree`
   (`DockStripShared.tsx:151-177`) + `toAgentNodes` / `resolveDispatchParents`
   (`0_tree.ts:22-71`). The strip already eats this as `nodes`.

2. Message ticks — the exact seam, per the brief's "name the exact seam for
   message-level events per harness":
   `read_ai_messages(editor, session_id, cwd, after_seq)` at
   `src-tauri/src/ledger.rs:684-701`, which returns `AiMessage[]` giving
   `ts` (unix ms), `role`, `subtype` (tool/reasoning/) and `session_id`
   (`ledger.rs:65-78`) for all four harnesses:
   - claude: `read_claude` `ledger.rs:498` (jsonl, uuid id, timestamp ts)
   - opencode: `read_opencode` `ledger.rs:635` (sqlite `message` table)
   - codex: `read_codex` `ledger.rs:166` (rollout jsonl)
   - kimi: `read_kimi` `ledger.rs:226` (wire.jsonl)
   The frontend already wraps it per-harness in
   `src/harness.ts:38-83` (`harnessAdapter(id).read(sessionId, cwd, afterSeq)`).
   The harness row's `harness` field maps straight to the editor arg; its
   `cwd` (tildified) is expanded via `getHomeDir()` (`core.ts:24`) before the
   claude call, matching how `claude_dir` drops `/` for the path
   (`ledger.rs:133-140`).

   **It only loads a session's messages, never all history up front** — the
   lazy law. The waterfall calls `read_ai_messages` per session whose span
   intersects the *current* range, and caches the result (see lifetimes). A
   session outside the range costs one Map lookup, zero IPC.

   New rust command needed? **No, for the incremental path.** `read_ai_messages`
   is already the correct per-session, per-harness read and the waterfall
   touches only a subset of sessions. A bulk command
   `harness_events(sessions: Vec<(editor, sessionId, cwd)>) -> map` that fans
   out in one IPC round is the later optimization if N IPC round-trips prove
   slow; it is not needed to land the feature. Standing ruling (server/rust
   land) is honored either way because the reads are already rust-side
   readers, not frontend file IO. `scripts/livespawn.ts` proves the same
   sources read headlessly for the gate (`livespawn.ts:204-209` jsonl,
   `livespawn.ts:254-263` mail files).

   Alternative tick source considered: the `~/.agent/mail/*.ndjson` bus
   envelopes (`IMailMessage` with `from_timestamp` + `kind`,
   `0_types.ts:107-126`; read via `loadMailLedger` `HarnessTracePanel.tsx:138`)
   give dispatch/ack ticks. They enrich but do not replace transcript turns:
   the plan keeps transcript ticks primary and may overlay dispatch ticks later
   as `TickType "dispatch"`.

Writes: none. The waterfall reads harness stores (rust, read-only) and the mail
dir (frontend, read-only). No new on-disk state.

Uniqueness: tick id = `AiMessage.id` within a session (`ledger.rs` comment
"stable identity", `ledger.rs:69-70`); the cache is keyed by `sessionId`, and
`read_ai_messages` returns oldest-first, so re-reading a session with the same
`after_seq` never duplicates.

The checkbox state is the one written field, through the existing store:
`store.set({ termStrip: {...store.get().termStrip, [sid]: entry} })`
(colocated with `toggleTermStripFor`, `InTabStrip.tsx:41-44`).

## 2.6 Instance lifetimes

- **Range brush state** (`IWaterfallRange`): a `useState` inside `Waterfall`.
  Created on first history render = `defaultRange(domain)`; mutated only by
  d3-brush callbacks; dropped on unmount when `showActive` flips back to true
  or the terminal's strip closes. Not persisted (a transient UI selection, like
  `scope` in `InTabStrip.tsx:53`).
- **Checkbox `showActive`**: per-terminal, persisted, lives beside `open` in
  `store.termStrip[sid]` (`state.ts:262,475`). Same lifetime as `ITermStripEntry`
  today: survives hotkey toggles and reloads, keyed by `sid`. Subscribed with the
  existing `store.subscribe(..., ["termStrip"])` (`InTabStrip.tsx:69-72`) — no
  new manual `.subscribe()`.
- **Loaded event windows** (the `Map<sessionId, ISessionTick[]>` cache): held in
  a `useRef` owned by the `Waterfall` host, so it is created when history mode
  first mounts and dropped when that mount unmounts (matching the "claim
  released on panel dispose" precedent, `DockStripPanel.tsx:39-57`). Entries are
  filled lazily on first intersection and never globally prefetched. Because the
  strip is capped at 240px and the event cache is per-terminal, an unmount mid-
  session releases the memory.

## 2.7 E2E gate shape (playwright.stripsub precedent)

Copy the strip-in-tab gate shape:
- New host page `e2e/waterfall.tsx` modeled on `e2e/dock-strip-in-tab.tsx`
  (register harness-trace plugin, `setHomeDir`, `store.set` sessions, mount, add
  a term panel + strip via `addTermPanel`).
- New spec `e2e/waterfall.spec.ts` modeled on `e2e/dock-strip-in-tab.spec.ts`,
  which seeds via `__instantE2eNativeResults` (`dock-strip-in-tab.spec.ts:34-53`)
  — add a `read_ai_messages` stub returning seeded `AiMessage`-shaped ticks for
  the fixture sessions alongside `harness_trace_rows` + `list_dir` + `read_text`.
  Freeze the clock with `page.clock.setFixedTime` (same reason the strip gate
  does, `dock-strip-in-tab.spec.ts:33`).
- New `playwright.waterfall.config.ts` on its own port + own dev server, exactly
  like `playwright.stripsub.config.ts` (port 4197, `reuseExistingServer: false`),
  so sibling worktrees' vite instances never serve our sources.
- Assertions: default state = checkbox checked, today's going-on table visible,
  no `.waterfall` svg. Uncheck => `.waterfall` visible, brush rect present, one
  bar per seeded session, ticks with the expected per-type fills, the table
  below shows only sessions whose span intersects the range. Drag the brush to
  a sub-range and assert the table row set shrinks and out-of-range ticks
  disappear. A raw screenshot to `test-results/waterfall-*.png` either way.
  `claimFsWatch` no-ops under `?e2e=1` (`fsWatch.ts:15`), so no live leg noise.

## 2.8 Step ladder (each step lands green alone)

1. **Types + pure module.** Add `ISessionSpan`/`ISessionTick`/`IWaterfallRange`/
   `IWaterfallDomain` + `ITermStripEntry.showActive` in `0_types.ts`; add
   `0_waterfall.ts` with all pure fns; add `0_waterfall.test.ts`. Green: vitest.
   No UI change; `harness_trace_rows` render path untouched.
2. **Checkbox + persistence, no behavior change.** Add the "Show active"
   checkbox to the act-bar, persist `showActive` in `termStrip[sid]`, and make
   the render branch on it — but history mode initially renders the *same*
   table (placeholder). Green: existing `e2e/dock-strip-in-tab.spec.ts` still
   passes (default checked = old behavior bit-for-bit) + a small vitest for the
   entry toggle.
3. **Waterfall render.** Add `4_Waterfall.tsx` (brush overview + bars + ticks +
   constrained `TreeTable`) and the lazy `read_ai_messages` cache. Green: new
   `e2e/waterfall.spec.ts` static case (checkbox off, seeded events, bars/ticks
   drawn, table constrained) + screenshot.
4. **Brush interaction.** Wire d3-brush selection -> `setRange` -> re-filter
   table + ticks. Green: `e2e/waterfall.spec.ts` drag case (drag brush, assert
   row set shrinks, out-of-range ticks hidden). d3-brush isolated in one host
   component so the pure math stays tested at step 1.
5. **Live leg + polish.** Re-run `read_ai_messages` for ranges still in view on
   fs-watch/refresh; empty-state strings; `onLayout` refit on range drag. Green:
   full `just check` (`api:check`, `tsc --noEmit`), `just build`,
   `just cargo-check` (no rust change expected, stays green), vitest suite, and
   the two playwright gates (strip-in-tab + waterfall). Extension unaffected
   (`just ext-build` if any generated api drift).

Dependencies for the ladder: `pnpm add d3-scale d3-brush` (ISC) plus `@types/d3`
devDeps if needed. No other dep.

## Files touched (next session)
- `src/plugins/harnessTrace/0_types.ts` (types + entry field)
- `src/plugins/harnessTrace/0_waterfall.ts` (new, pure) + `.test.ts` (new)
- `src/plugins/harnessTrace/4_Waterfall.tsx` (new)
- `src/plugins/harnessTrace/InTabStrip.tsx` (checkbox + branch)
- `src/state.ts` (`TermStripState.showActive`)
- `package.json` (+ d3-scale, d3-brush)
- `e2e/waterfall.tsx` + `e2e/waterfall.spec.ts` + `playwright.waterfall.config.ts` (new)
- No src-tauri change required for the incremental path.
