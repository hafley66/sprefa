# Viewport facts: "which N turns is the user looking at right now" as a db query

Design only. No code written. Companion: `2026-08-09-viewport-facts-design.visual.human.unga.md`.

Composes with, does not fork: `TMUX-VISIBILITY.md` (pane-level interval table),
`SELF-ID-AND-STATUS.md` (current-cache + span-history pattern), `QUERY-SURFACE.md`
(every fact keys on session or session+turn).

## Table of contents

1. [The fact already exists and dies every frame](#1-the-fact-already-exists-and-dies-every-frame)
2. [Receipts measured today](#2-receipts-measured-today)
3. [Schema](#3-schema)
4. [Source A: cooperating viewers](#4-source-a-cooperating-viewers)
5. [Source B: raw tmux panes](#5-source-b-raw-tmux-panes)
6. [The shared-key problem](#6-the-shared-key-problem)
7. [Content-kind detection: three candidates, measured](#7-content-kind-detection-three-candidates-measured)
8. [The payoff query](#8-the-payoff-query)
9. [The instant retrofit](#9-the-instant-retrofit)
10. [Hook-time composition](#10-hook-time-composition)
11. [Writer concurrency](#11-writer-concurrency)
12. [Open questions for the user](#12-open-questions-for-the-user)

---

## 1. The fact already exists and dies every frame

`instant` computes exactly the wanted answer, for exactly the diagram case, once
per paint, and drops it on the floor.

`/Users/chrishafley/projects/instant/src/0_terminalDiagrams.ts`:

| line | what is in scope |
|---|---|
| 521 | `const viewportTop = this.term.buffer.active.viewportY` |
| 522 | `const viewportEnd = viewportTop + this.term.rows - 1` |
| 525-529 | `messages: AiMessage[]`, each carrying `editor` + `session_id` + `id` |
| 530-531 | `locateMessageDiagrams(...)` returns fences with `start`, `end`, `messageId` |
| 539-541 | `fences.filter(f => f.end >= viewportTop && f.start <= viewportEnd)` |

Line 539-541 IS "a d2/mermaid diagram is on screen right now". It is a local
`const`. `messageId` is declared at `:27`, written at `:127`, and read nowhere in
the repo. `paint()` returns and the fact is gone.

Two more discard sites in the same app:

| site | computed | discarded at |
|---|---|---|
| `/Users/chrishafley/projects/instant/src/favorites.ts:193-205` | `lo`/`hi`, the exact first and last screen row of one turn block | `:206-210`, collapsed to a joined string, then the turn identity is recovered by fuzzy word-coverage search at `:230` |
| `/Users/chrishafley/projects/instant/src/treetable.tsx:366` | `items[0].index` / `items.at(-1).index` over `TurnNode` rows carrying real `AiMessage` values | used only to slice `renderRow` |

So the design is not "reconstruct the viewport". It is "stop throwing it away,
give it a table".

---

## 2. Receipts measured today

Live db `~/.agent/boop.db`, 114,634,752 bytes, SQLite 3.43.2, `page_size` 4096,
`journal_mode` **delete**.

| measure | value |
|---|---|
| `agent_turn` rows | 197,662 |
| `agent_session` rows | 1,372 (harness `claude` only, 1374 by join) |
| `sum(length(said))` | 34,166,669 bytes; avg 173, max 155,793 |
| turns with empty `said` | 159,205 = **80.4%** (tool 103,359 + system 754 + assistant-with-no-text 55,092) |
| turns with text | 38,857 (user 9,029 + assistant 29,828) |
| full-table `LIKE '%```mermaid%'`, warm | 40-60 ms |
| full-table mermaid OR d2, warm | 80-90 ms |
| viewport-scoped scan (`session_id=? AND turn BETWEEN ? AND ?`) | `SEARCH agent_turn USING PRIMARY KEY`, 0.00 s |
| ```` ```mermaid ```` fences | 169 turns / 56 sessions |
| ```` ```d2 ```` fences | 42 turns / 31 sessions |
| `~~~mermaid` fences | 0 |
| `%mermaid%` with no fence | 162 turns (the false-positive set a naive match would take) |
| any ```` ``` ```` fence | 2,444 turns / 664 sessions |
| single-row UPSERT, journal_mode=delete | 0.274 ms |
| single-row UPSERT, journal_mode=wal | 0.051 ms (5.4x) |

Schema shape confirmed from `~/projects/sprefa-lanes/boop/v6/boop/src/ident.rs:907-1046`
and `sqlite3 .schema`: `agent_turn(session_id, turn, ts, role_id, said)
PRIMARY KEY (session_id, turn) WITHOUT ROWID`. No `uuid` column anywhere;
`grep -n uuid ident.rs` returns nothing.

Language surface confirmed: `regexp/2` is the registered pattern guard
(`v6/dl/fixtures/*.dl6`, `conformance/fixtures/9_regexp.pl`); `>=`, `<=`, `!=`,
`>` all appear in green fixtures; a backtick inside a double-quoted dl6 string is
an ordinary character, verified by running `parse_dl:quoted_chars/4` on
`"```mermaid"` and getting `'```mermaid'` back with empty remainder
(`v6/prolog/compile/parse_dl.pl:499-512`; the backtick branch at `:1008` only
opens a shell template).

---

## 3. Schema

Surrogate INTEGER keys, natural keys interned once, booleans as INTEGER 0/1, per
`.claude/skills/sql-relational-design/SKILL.md`.

```sql
-- Dictionaries, same shape as every dict_* in ident.rs SCHEMA.
CREATE TABLE IF NOT EXISTS dict_viewer       (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_viewer_kind  (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_content_kind (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_msg          (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);

-- A surface: one scrollable region showing one session at a time.
-- dict_viewer.value is an opaque identity string the viewer picks and keeps
-- stable across paints, e.g. "instant:term:win3" or "instant:tree:tab7".
CREATE TABLE IF NOT EXISTS agent_viewer (
  viewer_id    INTEGER PRIMARY KEY,   -- dict_viewer
  kind_id      INTEGER NOT NULL,      -- dict_viewer_kind
  pid          INTEGER,
  tmux_pane_id INTEGER,               -- dict_pane (TMUX-VISIBILITY), NULL for a GUI surface
  hello_ts     INTEGER NOT NULL,
  bye_ts       INTEGER
);

-- Current viewport. One row per surface, UPSERT. This is the hook-time read.
-- Mirrors agent_live: a current-state cache with no history.
CREATE TABLE IF NOT EXISTS agent_viewport (
  viewer_id   INTEGER PRIMARY KEY,
  session_id  INTEGER NOT NULL,
  first_turn  INTEGER NOT NULL,
  last_turn   INTEGER NOT NULL,
  focused     INTEGER NOT NULL DEFAULT 1,
  observed_ts INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_viewport_session ON agent_viewport(session_id);

-- Dwell history. One row per resting position, NOT per scroll tick.
-- Same interval shape as agent_visible and agent_live_span, so one fold
-- implementation closes all three.
CREATE TABLE IF NOT EXISTS agent_viewport_span (
  viewer_id  INTEGER NOT NULL,
  from_ts    INTEGER NOT NULL,
  to_ts      INTEGER,
  session_id INTEGER NOT NULL,
  first_turn INTEGER NOT NULL,
  last_turn  INTEGER NOT NULL,
  focused    INTEGER NOT NULL,
  PRIMARY KEY (viewer_id, from_ts)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS idx_viewport_span_open ON agent_viewport_span(to_ts) WHERE to_ts IS NULL;
CREATE INDEX IF NOT EXISTS idx_viewport_span_sess ON agent_viewport_span(session_id, from_ts);

-- Content kinds per turn, derived at ingest. Extensible without a column per kind.
CREATE TABLE IF NOT EXISTS agent_turn_kind (
  session_id INTEGER NOT NULL,
  turn       INTEGER NOT NULL,
  kind_id    INTEGER NOT NULL,
  PRIMARY KEY (session_id, turn, kind_id)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS idx_turn_kind_bykind ON agent_turn_kind(kind_id, session_id, turn);

-- The key every existing viewer already holds. See section 6.
ALTER TABLE agent_turn ADD COLUMN msg_id INTEGER;   -- dict_msg, NULL before backfill
CREATE INDEX IF NOT EXISTS idx_turn_msg ON agent_turn(msg_id);
```

`SCHEMA_VERSION` at `ident.rs:24` goes 3 -> 4.

Why `agent_turn_kind` as a junction and not `agent_turn.has_diagram INTEGER`:
one column per content kind does not scale past the first kind, and the junction
is 209 rows against 197,662. Measured on a copy of the live db: the flag-column
backfill is 286 ms and the partial index is 13 ms; the junction build is 107 ms
total for both kinds. The junction wins on shape and does not lose on cost.

---

## 4. Source A: cooperating viewers

The viewer already computed the viewport. It pushes. Nobody reconstructs.

### The push contract

```
boop db viewport put --viewer <id> --session <s> --first <turn> --last <turn> [--focused 0|1]
boop db viewport bye --viewer <id>
```

REST twin, matching the `beep`/`db` split already stated in `TMUX-VISIBILITY.md` §12:

| clap | method + path |
|---|---|
| `boop db viewport put` | `PUT /viewports/{viewer}` |
| `boop db viewport list [--at T]` | `GET /viewports` |
| `boop db viewport now` | `GET /viewports?open=true` |
| `boop db viewport bye` | `DELETE /viewports/{viewer}` |

`db`, not `beep`: it writes rows into the store, it does not act on the world.

### Retention: both tables, and the reason is not a hedge

`SELF-ID-AND-STATUS.md` §8 already set this pattern for liveness: `agent_live` is
the current-state cache, `agent_live_span` is the history, and the write happens
only when an observation differs from the cache. Viewport takes the identical
pair for three reasons.

1. **The hook read must be sub-100ms** (`TMUX-VISIBILITY.md` §11 constraint 1).
   `agent_viewport` is a single-row primary-key probe per open surface, and there
   are single digits of those. Scanning a span table for the open interval is a
   partial-index probe, which is also fast, but the cache keeps the hot path a
   point read.
2. **"Right now" and "at 14:32" are different questions and both get asked.**
   The status resource already ships a 10-minute window; a viewport that only
   knows `now` cannot answer "was the user watching when the lane died", which is
   the question that makes the table worth having.
3. **One fold serves three tables.** `agent_visible`, `agent_live_span` and
   `agent_viewport_span` all close an open interval when the next observation
   disagrees, and all three are read with
   `from_ts <= T AND (to_ts IS NULL OR to_ts > T)`.

### The coalescing law, and why the span table records dwells

`instant`'s paint is debounced 1000 ms on activity and 80 ms on scroll
(`0_terminalDiagrams.ts:381-395`). A scroll burst therefore produces up to 12.5
genuinely-different viewports per second. Logging every one gives roughly
25,000 rows on an active day, about 1.5 MB, 10 MB at a 7-day retention. That is
affordable and it is also noise: transit positions are not "what the user was
looking at".

Rule: the fold opens a span only when a viewport tuple has held for
`min_dwell_ms` (default 2000). Transit tuples update `agent_viewport` and never
reach `agent_viewport_span`. Daily span volume drops to the number of resting
positions, order hundreds. Retention then becomes a prune of closed spans older
than N days (default 7) on whatever tick already prunes, and the prune is a
convenience rather than a requirement.

The emit side carries the same rule, so the wire is quiet too: the viewer emits
only when `(session, first_turn, last_turn, focused)` changes, trailing-debounced
250 ms.

### Staleness

A `agent_viewport` row that has not been updated for 5 minutes does not mean the
screen went blank; it means the viewer stopped repainting because nothing moved.
So staleness is not invisibility. Three explicit signals instead:

| signal | written by | meaning |
|---|---|---|
| `focused = 0` | viewer, on blur | the surface exists and is not the one being read |
| `agent_viewer.bye_ts` | viewer, on close | the surface is gone |
| `agent_viewer.pid` unreachable | boop's existing liveness check | the viewer died without saying goodbye |

A reader that wants certainty joins `agent_viewer` and demands `bye_ts IS NULL`
plus a live pid.

---

## 5. Source B: raw tmux panes

**Verdict: out of scope for turn resolution. In scope at pane granularity, where
`agent_visible` already answers it.**

The instruction was to be direct if the accurate answer is "declare it out of
scope". It is, and the obstacles are measured rather than assumed.

### What actually blocks it

| # | obstacle | measurement or citation |
|---|---|---|
| 1 | **80.4% of turns carry no text to match.** 159,205 of 197,662 rows have `length(said)=0`: every tool row (103,359), every system row (754), and 55,092 assistant rows. The screen is dominated by tool-call rendering whose content lives in `agent_touch` / `agent_cmd`, never in `said`. | measured today, `~/.agent/boop.db` |
| 2 | **The TUI renders markdown; it does not print `said`.** Captured live from pane `%1` at 98x63: a markdown pipe table rendered as U+250C box drawing with the cell text reflowed to fit column widths. Substring matching against the markdown source fails on every line of that block. | `tmux capture-pane -p -t sprefa`, run today |
| 3 | **Chrome lines are not turns.** The same capture carries `✻ Cooked for 55s`, `✽ Computing… (8s · thinking with high effort)`, two horizontal rules, and a status line `glm-5.2[1m] ctx 9% of 1000k 93747in 1606out`. A matcher must classify these out, and they are version-specific TUI strings. | same capture |
| 4 | **`capture-pane` without `-M` reads the grid, not the mode screen.** A pane scrolled back in copy-mode captures the wrong region unless `#{pane_in_mode}` and `#{scroll_position}` are read and compensated. | `TMUX-VISIBILITY.md` §4, from `cmd-capture-pane.c` |
| 5 | **Wrapping is width-dependent and the capture is post-wrap.** Re-wrapping `said` at `#{pane_width}` to compare means reimplementing the harness's own markdown renderer plus its wrap rules, which is the thing that changes every harness release. | structural |
| 6 | **The harness elides long output.** On-screen text is a lossy projection with no inverse. | structural |

### The confidence ladder, most exact first

| rung | source | resolves | confidence |
|---|---|---|---|
| 1 | cooperating viewer pushes `(first_turn, last_turn)` | exact range | exact, it is the layout itself |
| 2 | harness hook reports the transcript tail plus pane height | `last_turn` exact while the user is following live; `first_turn` estimated | exact at the bottom, estimated at the top, blind while scrolled back |
| 3 | `agent_visible` pane interval (`TMUX-VISIBILITY.md` §9) | which SESSION was on screen, never which turn | exact at session granularity |
| 4 | `capture-pane` text matched against `said` | nothing reliable | rejected, obstacles 1-6 |

Rung 2 is worth naming because it is nearly free and nobody has proposed it: a
`PostToolUse` hook already receives `session_id` and `transcript_path`, so
"the transcript is at line N and the user has not scrolled" is a one-line push
that covers the common case of watching a lane run. It is a follow-on, not part
of this design.

Rung 4 is where the effort would go and it is the rung that cannot be made
correct. Declaring it out of scope costs nothing that rungs 1-3 do not already
cover.

---

## 6. The shared-key problem

This is the one thing that must be decided before any code, and it is not
obvious from the outside.

Three different ordinals exist for "a message", and no two agree:

| system | identifier | granularity | source |
|---|---|---|---|
| boop | `agent_turn.turn` | one per CONTENT BLOCK; a single assistant response mints one row for its text block plus one per `tool_use` block | `ident.rs:749-773` |
| instant | `AiMessage.seq` | one per JSONL LINE | `/Users/chrishafley/projects/instant/src-tauri/src/ledger.rs:521` `read_claude`, `seq` = line index |
| instant | `AiMessage.id` | the transcript record `uuid` | same reader; `locator` = `claude:/path/session.jsonl#L42` |

`instant` has zero knowledge of boop: `grep -rn "boop|agent_turn|\.agent/boop"`
over the whole repo returns nothing. It parses jsonl itself, which is the fifth
copy of that parser the `QUERY-SURFACE.md` replacement map already flags.

So a viewport row keyed on boop `turn` ordinals cannot be written by instant
today. Two ways out.

**Option 1: instant adopts `agent_turn` as its message source.** This is what the
replacement map already prescribes ("message side panel | own jsonl parser (one
of 5 copies) | agent_turn"). It gives ordinals for free. It is blocked today
because boop reads `claude` only (`v6/boop/src/harness/` contains exactly
`claude.rs`; all 1,374 sessions are harness `claude`) while instant reads claude,
codex, kimi and opencode (`ledger.rs:163,223,521,658`). Adopting boop would blind
instant on three harnesses.

**Option 2: boop stores the transcript record uuid.** One dict, one nullable
column, one index (section 3). The uuid is the only identifier BOTH sides already
hold. instant pushes uuids; boop resolves uuid -> `(session_id, turn)` with one
index probe per endpoint at write time. Backfill needs a re-ingest from cursor 0,
or the column fills forward only and viewport rows land for new turns first.

Recommendation: **option 2 now, option 1 as it becomes true per harness.** Option
2 is additive, does not block on boop growing three harness readers, and the
uuid column is worth having on its own (it is the join key to
`file-history-delta`, `pr-link`, and every other native record type).

The push contract therefore accepts either spelling:

```
--first-turn N --last-turn N        # a caller that already speaks boop ordinals
--first-msg UUID --last-msg UUID    # a caller that speaks transcript records
```

and stores ordinals either way. A push whose uuids do not resolve is rejected
with a named reason rather than stored as a guess, matching the rung discipline
in `SELF-ID-AND-STATUS.md` §3.

Coverage note that falls out of this: `dict_session` interns any string, so a
viewport row for a codex or opencode session stores fine and joins to zero turns.
"viewport rows whose session has no `agent_turn` rows" is then a one-query
coverage metric rather than a silent hole.

---

## 7. Content-kind detection: three candidates, measured

Build-vs-buy applies. All three measured on a copy of the live db.

| candidate | build cost | query cost | disk | verdict |
|---|---|---|---|---|
| **A. LIKE at query time, viewport-scoped** | zero | `SEARCH agent_turn USING PRIMARY KEY`, ~60 rows, 0.00 s | zero | **ships the payoff query with no schema change at all** |
| **B. LIKE at query time, whole corpus** | zero | 80-90 ms warm over 197,662 rows / 34 MB | zero | fine for a report, a real tax at 4 reads/sec |
| **C. `agent_turn_kind` junction, derived at ingest** | 107 ms one-time backfill for both kinds; per-turn cost at sync is one `str::contains` | index probe on `idx_turn_kind_bykind`, 209 rows total | +32 KB | **the corpus query, and the only shape that extends past one kind** |
| **D. FTS5 over `said`** | 755 ms build, 38,780 docs | 0.00 s for `MATCH 'mermaid'`, but 325 hits against 169 real fences, so a LIKE confirm is still needed | **+57.6 MB** (db 114.6 MB -> 172.3 MB, 168% of the text it indexes) | **rejected for this question.** Also blocked as external-content: `agent_turn` is `WITHOUT ROWID`, so `INSERT INTO fts(rowid,...) SELECT rowid` fails with `no such column: rowid` and needs a synthetic id. FTS5 earns its 57 MB only if open-ended transcript search becomes a feature, which is a different ask. |

Decision: **A for the on-screen query, C for the corpus query, and C is what
ships** because "which sessions ever drew a diagram" is asked as often as "what
is on screen".

Detection rule at ingest, stated precisely so it is testable:

```
kind 'mermaid'  <=  said contains "```mermaid"
kind 'd2'       <=  said contains "```d2"
```

Measured discrimination: 169 + 42 fence hits; 162 turns mention "mermaid"
without a fence and are correctly excluded; 0 turns use `~~~` fences. The rule
runs on 19.6% of rows (the ones with text) and skips the rest on a length check.

---

## 8. The payoff query

### SQL, current viewport

```sql
SELECT viewer.value        AS viewer,
       session.value       AS session,
       turn_kind.turn      AS turn,
       kind.value          AS diagram_kind,
       viewport.first_turn,
       viewport.last_turn,
       viewport.observed_ts
FROM agent_viewport AS viewport
JOIN agent_turn_kind AS turn_kind
       ON turn_kind.session_id = viewport.session_id
      AND turn_kind.turn BETWEEN viewport.first_turn AND viewport.last_turn
JOIN dict_content_kind AS kind    ON kind.id    = turn_kind.kind_id
JOIN dict_session      AS session ON session.id = viewport.session_id
JOIN dict_viewer       AS viewer  ON viewer.id  = viewport.viewer_id
JOIN agent_viewer      AS surface ON surface.viewer_id = viewport.viewer_id
WHERE viewport.focused = 1
  AND surface.bye_ts IS NULL
  AND kind.value IN ('mermaid', 'd2');
```

Cost: `agent_viewport` holds one row per open surface (single digits). For each,
a primary-key range probe into `agent_turn_kind`, which is `WITHOUT ROWID` and
209 rows. Sub-millisecond. The analogous probe into the 197,662-row
`agent_turn` measured `SEARCH ... USING PRIMARY KEY` at 0.00 s.

### SQL, at an arbitrary time T

```sql
FROM agent_viewport_span AS span
JOIN agent_turn_kind AS turn_kind
       ON turn_kind.session_id = span.session_id
      AND turn_kind.turn BETWEEN span.first_turn AND span.last_turn
WHERE span.from_ts <= :t AND (span.to_ts IS NULL OR span.to_ts > :t)
```

Same predicate as `agent_visible` and `agent_live_span`.

### SQL, no junction table (candidate A)

```sql
JOIN agent_turn AS turn
       ON turn.session_id = viewport.session_id
      AND turn.turn BETWEEN viewport.first_turn AND viewport.last_turn
WHERE turn.said LIKE '%```mermaid%' OR turn.said LIKE '%```d2%'
```

Measured plan: `SEARCH agent_turn USING PRIMARY KEY (session_id=? AND turn>? AND
turn<?)`, 0.00 s. The junction is not needed for THIS query; it is needed for the
corpus query, which is 80-90 ms without it.

### .dl6

```
% EDB, projected by boop; the same rels QUERY-SURFACE.md declares.
rel agent_turn(session: text, turn: int, ts: int, role: text, said: text).
rel agent_viewport(viewer: text, session: text, first_turn: int, last_turn: int,
                   focused: int, observed_ts: int).

% Content kind. Two rules unioning into one derived head; each carries its kind
% as a literal so the head keeps a column the regexp cannot produce.
rel diagram_turn(session: text, turn: int, kind: text).
diagram_turn(session, turn, "mermaid") <-
    agent_turn(session, turn, _, _, said), regexp(said, "```mermaid").
diagram_turn(session, turn, "d2") <-
    agent_turn(session, turn, _, _, said), regexp(said, "```d2").

% The payoff.
rel on_screen_diagram(viewer: text, session: text, turn: int, kind: text).
on_screen_diagram(viewer, session, turn, kind) <-
    agent_viewport(viewer, session, first_turn, last_turn, 1, _),
    diagram_turn(session, turn, kind),
    turn >= first_turn, turn <= last_turn.

? on_screen_diagram(viewer, session, turn, kind).
```

Three surface checks made against the real door rather than a comment header:

- `regexp/2` is the registered pattern guard, green in
  `conformance/fixtures/9_regexp.pl` and used across `v6/dl/fixtures/*.dl6`.
- A backtick inside a double-quoted string lexes as an ordinary character:
  `parse_dl:quoted_chars(0'", "```mermaid\"", Out, Rest)` returns
  `'```mermaid'` with `Rest = []`. The backtick branch at `parse_dl.pl:1008`
  only opens a shell template.
- A literal in a body argument position is green (`clock_bucket(2, bucket)`,
  `v6/dl/fixtures/pr-size.dl6:22`), so `agent_viewport(..., 1, _)` is spelled
  correctly.

One law to re-check before landing, not to assume: two rules sharing the derived
head `diagram_turn` is ordinary datalog union. The 2026-08-08 measurement that
found a duplicated `grade_tag(401,ripe)` row was about wiring a DERIVED rel as a
REFERENCE TARGET, which is a different wiring. Confirm against
`compile/out/manifest.json` rather than against this paragraph.

### Pure-rxjs lowering

```ts
// agent_viewport: a current-state cache, latest per surface, with the
// coalescing law expressed as operators rather than as a rule in prose.
const viewport$: Observable<Viewport> = viewportEvent$.pipe(
  groupBy((event) => event.viewer),
  mergeMap((perViewer) =>
    perViewer.pipe(
      distinctUntilChanged(
        (before, after) =>
          before.session === after.session &&
          before.firstTurn === after.firstTurn &&
          before.lastTurn === after.lastTurn &&
          before.focused === after.focused,
      ),
      debounceTime(250),
    ),
  ),
  shareReplay({ bufferSize: 1, refCount: true }),
);

// agent_viewport_span: the dwell fold. debounceTime(minDwellMs) after the
// distinct check is exactly "held still for 2 seconds", so transit tuples never
// open a span.
const viewportSpan$ = viewport$.pipe(
  groupBy((viewport) => viewport.viewer),
  mergeMap((perViewer) =>
    perViewer.pipe(
      debounceTime(2000),
      scan(
        (open, resting) => ({ closed: open.current, current: resting }),
        { closed: null as Viewport | null, current: null as Viewport | null },
      ),
    ),
  ),
);

// diagram_turn: a stateless expansion over the turn stream, one emission per
// (turn, kind) pair.
const diagramTurn$ = turn$.pipe(
  mergeMap((turn) =>
    from(
      (["mermaid", "d2"] as const)
        .filter((kind) => turn.said.includes("```" + kind))
        .map((kind) => ({ session: turn.session, turn: turn.turn, kind })),
    ),
  ),
);

// on_screen_diagram: the range join. The index is keyed by session because the
// viewport names one session, which is the same access path the SQL primary-key
// range probe takes.
const onScreenDiagram$ = viewport$.pipe(
  filter((viewport) => viewport.focused === 1),
  withLatestFrom(diagramTurnsBySession$),   // Map<string, DiagramTurn[]>
  mergeMap(([viewport, bySession]) =>
    from(
      (bySession.get(viewport.session) ?? [])
        .filter((d) => d.turn >= viewport.firstTurn && d.turn <= viewport.lastTurn)
        .map((d) => ({ viewer: viewport.viewer, ...d })),
    ),
  ),
);
```

`distinctUntilChanged` is the emit-on-change rule, `debounceTime(250)` is the
wire quiet, `debounceTime(2000)` after it is the dwell rule, and `scan` closing
the previous value is the interval fold. The rx spelling and the SQL spelling
agree on every one of those, which is the check that the schema is not inventing
behaviour the stream cannot express.

---

## 9. The instant retrofit

**Emit point: `/Users/chrishafley/projects/instant/src/0_terminalDiagrams.ts`,
`TerminalDiagramOverlay.paint()`, in the window between line 541 (visible fences
computed) and line 542 (render begins).**

Why that function and no other: it is the only place where the message set, the
session identity, the first visible row, the last visible row, and the row-to-
message map are all simultaneously in scope, on a cadence that already fires on
exactly the right events.

| need | variable already in scope | line |
|---|---|---|
| message set | `messages: AiMessage[] \| null` | 525-529 |
| session identity | `messages[i].editor` + `.session_id` | `state.ts:109-110` |
| message identity | `messages[i].id` (transcript uuid), `.seq` | `state.ts:110-111` |
| first visible row | `viewportTop = this.term.buffer.active.viewportY` | 521 |
| last visible row | `viewportEnd = viewportTop + this.term.rows - 1` | 522 |
| row -> message | `locateMessageDiagrams(...)` fences carrying `start`, `end`, `messageId` | 530-531 |
| visibility filter | `fences.filter(f => f.end >= viewportTop && f.start <= viewportEnd)` | 539-541 |

Cadence: `scheduleFrame()` (`:466`) is driven by `onWriteParsed` (`:411`),
`onScroll` (`:425`), `onResize` (`:429`) and `viewportScrolled()` (`:437`),
debounced 1000 ms on activity and 80 ms on scroll (`:381-395`). That is the
correct trigger set for a viewport fact with no new plumbing.

### What is missing, and it is one function

Today the row-span computation is scoped to fenced diagram code. A viewport fact
needs a row span for messages with no diagram, to find the FIRST and LAST visible
message rather than the first and last visible fence.

Two ways to get it, both near-copies of code that exists:

1. Generalize `locateMessageDiagrams` (`:84`) into `locateMessages(term,
   messages)`. `logicalLines(term, from, through)` (`:60`) already returns
   `{ text, start, end }` with real buffer rows; swap the anchor source from
   `normalizedDiagramLines(diagram.code)` (`:100`) to the message text. The
   uniqueness guard at `:104` (`anchorOwners.get(text)?.size === 1`) already
   handles two turns sharing a line.
2. Reuse the bullet walk that already works on real claude and opencode output:
   `rowSignature()` (`favorites.ts:157`, bullet set at `:151`) plus the walk at
   `:193-205` segments the viewport into turn blocks and produces `lo`/`hi`.
   Those two values are computed and discarded at `:206`. Feed each block through
   `searchTurns()` (`:230`) to name it.

Path 2 is the better bet for TUI-rendered prose because it segments on the
harness's own turn bullets instead of matching text through the renderer, and it
recovers a value the code already computes and throws away.

### Where the row goes

instant already carries `rusqlite` (`src-tauri/src/activity.rs:35`,
`src-tauri/src/favorites.rs:16`) and `favorites.db` is already keyed on
`(editor, session_id, message_id)`, which is the exact identity tuple a viewport
fact needs. So the write is one more `Connection` and one `PUT`-shaped UPSERT.
Three landing options in preference order:

| option | mechanism | cost |
|---|---|---|
| 1 | a new `#[tauri::command] viewport_put` next to `activity_log`, writing `agent_viewport` + `agent_viewport_span` in `~/.agent/boop.db` | one command, one generated binding via `ipc/commands.json`, zero new processes |
| 2 | `POST /viewports` to `boop serve` | blocked on the undecided daemon question (`TMUX-VISIBILITY.md` §13) |
| 3 | `logLine(JSON.stringify({kind:"viewport",...}))` via `src/core.ts:147` | zero infrastructure, but the line sink is a 2 MB ring log, not a queryable table; useful as a one-day proof, not as the design |

Option 1. Section 11 is its precondition.

### Runner-up emit point

`/Users/chrishafley/projects/instant/src/treetable.tsx:366`, inside the
`if (virtual)` branch: `items[0].index` and `items[items.length-1].index` are
exact with zero derivation, and `modelRows[it.index].original` is a `TurnNode`
carrying a real `AiMessage`. It needs only an `onVisibleRangeChange` prop
threaded from `sessionSidebar.tsx`. It is the runner-up because it describes the
sidebar tree rather than the terminal the user is reading, and it should ship as
a SECOND `dict_viewer` row (`instant:tree:<tab>`) rather than instead of the
first. Two surfaces, two rows, one table, which is why the primary key is the
surface and not the app.

---

## 10. Hook-time composition

`TMUX-VISIBILITY.md` §11 designs the hook read as `boop db visible --now` plus a
`capture-pane`, returning pane text as `additionalContext`. Viewport facts turn
that capture into a join.

The hook receives `session_id` on stdin. With viewport rows present, the
auto-context read becomes: find the focused surface, take its `(session_id,
first_turn, last_turn)`, and select those `agent_turn` rows by primary-key range.
The result is the ACTUAL transcript text the user is reading, at full fidelity,
in the harness's own message boundaries, rather than a post-wrap post-markdown
screen capture that has to be un-rendered. It is also the answer to "what is the
user reading in ANOTHER session", which a capture of the current pane cannot
reach at all. `agent_visible` still answers which pane and therefore which
session is on screen when no cooperating viewer exists, so the two tables stack:
`agent_visible` narrows to a session, `agent_viewport` narrows to turns within
it, and the capture path stays as the rung-3 fallback rather than the mechanism.

---

## 11. Writer concurrency

`~/.agent/boop.db` is `journal_mode = delete` today, measured. Under a rollback
journal every write takes an EXCLUSIVE lock on the whole 114 MB file and blocks
every reader for the duration. Adding a second writer process to that is the
avoidable class of defect.

Measured on a copy: 200 single-row UPSERTs took 54.8 ms in delete mode (0.274 ms
each) and 10.2 ms in WAL (0.051 ms each), a 5.4x difference. Absolute throughput
is not the problem at 4 writes per second; the lock class is. WAL removes the
reader-blocking entirely and it is one pragma at open.

Precondition before any second writer: **`PRAGMA journal_mode=WAL` on
`~/.agent/boop.db`**, set where the connection is opened in `ident.rs`. If that
is refused for a reason not visible here, the fallback is a separate
`~/.agent/boop-viewport.db` that readers `ATTACH`, which keeps the lock domains
disjoint at the price of a second file and cross-database joins.

---

## 12. Open questions for the user

1. **uuid column on `agent_turn`, yes or no.** Section 6 recommends it as the
   only key instant and boop both already hold. It costs one dict, one nullable
   column, one index, and a re-ingest from cursor 0 to backfill. The alternative
   is instant switching its message source to `agent_turn`, which blinds it on
   codex, kimi and opencode until boop grows those readers.
2. **WAL on `~/.agent/boop.db`.** Section 11. Required before instant writes.
3. **`min_dwell_ms` default.** 2000 ms proposed. It is the knob that decides
   whether the span table records where the eye rested or where it passed
   through.
4. **Does a viewport row survive a restart?** Proposal: no. `agent_viewport` is a
   cache, a viewer says hello on start and bye on close, and a row with a dead
   pid is aged out. Stated because the alternative (rows persist and lie after a
   crash) is a real failure mode, not because it is close.
5. **Second surface, or one row per app?** Proposal: the primary key is the
   SURFACE, so instant's terminal and its sidebar tree are two rows. Confirm,
   because it decides the `dict_viewer` naming convention.
6. **Which content kinds ship in `dict_content_kind`.** Measured today: mermaid
   169, d2 42, any fence 2,444 across 664 sessions. `code`, `sql`, `table` are
   the obvious next three and cost nothing extra at ingest.
7. **Rung 2 (harness hook reports the transcript tail).** Section 5 names it as
   nearly free and covering the follow-along case with no viewer at all. Worth a
   separate arc, or fold it in.
</content>
</invoke>
