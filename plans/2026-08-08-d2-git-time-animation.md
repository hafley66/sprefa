# d2 boards animated through git time (user idea, 2026-08-08)

HOME NOTE (user, end of session): these ideas belong to ~/projects/instant
(daily driver; anim is v4 of its own journey; rect-vs-canvas layout and
state mgmt already labbed there; user's own rxjs lazy proxy signals =
rxjs + signal + immer style). Canonical copy:
instant/plans/2026-08-08-canvas-state-ideas.md. This file stays as the
sprefa-side record because half the pins (dl6 CLI emitters, user-mind
queries, entity=rowid) ARE sprefa: "oh yea this is sprefa lol."

## Shell pipe into panel state (user, same conversation)

A shell query/pipe step that takes any command's output and routes it into
the STATE CALL for the panel it controls: `... | into panel:tasks` writes
the panel's store slot, panel re-renders reactively. The pipe is just
another writer to the central store; panels subscribe. With the dl6-CLI
idea above, the pipe target is a source rel and the panel is a derived-rel
view — the shell becomes one more ingest door.

User's spec, written down verbatim-in-spirit before any research:

## The idea

1. **Git is the timeline.** The .d2 files (task-queue, interning-machine) are
   already version-controlled; each commit is a board version V_i at time T_i.
   The diff between V_i and V_i+1 is a known set of node adds/removes/relabels
   and edge adds/removes.
2. **Staggered entry.** Animating V_i -> V_i+1: surviving nodes first, then new
   nodes animate in, then new edges, staggered.
3. **Force-fade displacement for entry position.** A new node does not pop in at
   its final place. Run a force algorithm (fading out over the transition) to
   pick where the node STARTS, then it pulls toward its position in the target
   d2 layout. The d2 layout (dagre/elk) stays the ground truth for where things
   end; the force pass only shapes the journey.
4. **Attachment knowledge.** If object O_i moved, and a new object is linked to
   O_i, the animation should know that: the new node's entry origin derives from
   its anchor's position/motion (enter from the thing it attaches to).
5. **Pipeline question.** How does d2 go to svg (where do node ids + x/y live in
   the svg), and how do d2 + svg get into cytoscape, where the user has deep
   prior work ("tons of vibing on cytoscape").

## What 60 seconds of recon already found in ~/projects/anim

- anim's core animation rule IS item 2's contract: "things with the same key
  slide to their new place, new things fade in, gone things fade out. Graphs
  key on node ids." (README.md)
- `bin/from-git.mjs` (npm run frames): frames built FROM GIT — smells like
  item 1 already has a door. Accuracy unverified.
- `bin/render-d2.sh` (npm run graphs): a d2 rendering path already exists.
- Frames hold ` ```d2 name ` fenced blocks as right-panel graphs.
- README/AGENTS accuracy vs code: UNVERIFIED — two flash4 lanes dispatched
  (A: anim entrypoint + README-accuracy audit; B: d2->svg position extraction
  + cytoscape mapping study). Their reports land beside this doc.

## The live-canvas loop (user, same conversation)

The end shape: a cute svg / web-tech rendering backed by a SERVER, and the AI
CALLS THE SERVER (user correction, same conversation: "ai calls the server") —
an amendment API (add node, link edge, relabel, remove), not a file the AI
edits and a watcher picks up. The server owns the canonical d2/jsoncanvas
state, versions it (git or its own log), and pushes the diff to the renderer,
which animates it in. So the AI-facing surface is tool calls against the
server; the d2 file is the server's storage format, git is the memory, the
animation is how a human absorbs the delta. User position:
"d2 is easily the best diagramming language." jsoncanvas (Obsidian's open
canvas JSON) is the named alternative live format — it carries explicit x/y
per node, so it trades d2's auto-layout for free position stability; worth one
line in the comparison when the lane reports land.

## Addressable groups + entity links (user, same conversation)

Every group/set of nodes on the board is ADDRESSABLE, so a link can point at
an entity, and an entity is just a row id. That is: board nodes/groups carry
the same surrogate integer ids the sprefa store keys on (the interning/
surrogate-keys law), so "link these two things" is an edge between row ids,
resolvable across boards, versions, and the store itself. d2's dotted paths
already give every node and container an address (lane B proved they survive
into the svg); the entity layer maps those addresses to row ids.

## Textbox cwd inference (user, same conversation)

Automate reading the board's code textboxes and the cwd each command implies.
A command whose cwd differs from the repo root is a fact worth surfacing: the
board should explain that the thing runs from a process root, and when a
system has MULTIPLE process roots, those roots get drawn on the canvas one
day (a root is a first-class node; commands hang off their root). Today's
queue board already carries `cd v6/tsv2 && ...` strings inside example
boxes — that is the raw material; the parse is mechanical.

## Canvas UI for instant + anim (user, same conversation)

~/projects/instant and ~/projects/anim already run react + dockview (panel
docking) + xyflow (node canvas). Wanted: a canvas UI layer there too, with
json-canvas as a candidate document format. Wiring model: any process output
is pipeable into the canvas at any time; declarative event rules ("when this
event happens do this") drive it; one render target is an auto table in a
dockview panel. This is the sprefa shape restated for UI: event stream ->
derived state -> view, where a view can be a canvas node OR an auto table.
"json-photoshop" question answered in chat 2026-08-08: no standard exists;
nearest neighbors are Polotno (JSON design docs), Fabric.js/Konva canvas JSON,
Excalidraw/tldraw JSON (vector scenes), OpenRaster .ora (layered raster,
XML+PNG zip), ag-psd (PSD <-> JS objects).

## Legend as enforced schema (user, same conversation)

The board is the state surface for BOTH the AI and other users. The legend
stops being decoration: it renders as a sticky/popover widget,
expand/collapse. The law: any new semantic mapping (a class, a color, a
shape meaning) REQUIRES a new legend entry before use — the renderer refuses
or flags an unregistered class. d2 already has the raw material: the
`classes:` block IS the machine-readable legend; the widget is generated
from it, and the rail is "no class used that classes: does not declare."
Same shape as the engine's registry.pl surface table and the magic-rel ban:
vocabulary lives in ONE declared place, use without declaration is a defect.
Side effect: AI amendments through the server (see amendment API above)
must ship the legend entry in the same call when they mint a new category —
the schema grows only through the front door.

## Tabs carry cwd identity, past process death (user, same conversation)

Every tab/session node on the board knows its cwd — including sessions whose
process is gone. The source already exists: the bus registry
(~/.agent/mail/registry.json) records agent -> {sessionId, harness, tmux,
cwd} at dispatch time, so cwd is a durable fact about the lane, never a
live-process probe. Board consequence: a tab node's cwd is part of its
identity; two tabs with different cwds are different process roots (ties to
the textbox-cwd inference idea above — the roots get drawn one day).

## Session-fact extractor: the bus + AI analytics as one rust crate (user)

The observation: everyone keeps rebuilding the same local-AI read tools —
the bus (instant/scripts/bus.ts + ~/.agent/mail ndjson/registry), transcript
search (cass), opencode.db readers (sessions/tokens/cost), claude-code
session jsonl readers, chat_log tooling, tmux ctx capture. Wanted: ONE
"fucking aces" rust library with sprefa-extract's exact virtues — per-source
pure readers, JSONL facts out, no daemon, frozen boundary law (index what is
written; programs interpret). Sources it would cover: bus ndjson, registry
json, opencode sqlite, claude jsonl transcripts, tmux panes. Then bus-the-CLI
and every analytics view become thin programs over facts, same as .dl
programs over extract facts.
GATE before any lane: the build-vs-buy law. Research candidates first
(existing session-log parsers, sqlite readers, agent-observability tools) and
write the candidate-by-candidate analysis; the bespoke crate has to earn it.

REVISION (user, same conversation): or skip the hand crate — write it in
dl6, and grow dl6 until a whole CLI compiles OUT of it. The emitter family
becomes rust (clap), ts, bash; order ts -> rust first. The dl6 program is
the spec: source rel decls = the readers, derived rels = the analytics,
declared query/command surface = subcommands and flags, and the emitter
packages the lot as a real CLI binary. This nests inside the existing
roadmap instead of beside it: emit_ts.pl is the ts emitter today, P1-C
already grows a one-shot CLI, and the endgame is per-program rust codegen —
CLI emission is that same endgame pointed at a user-facing binary instead of
a fixpoint executor. The session-fact readers then live as dl6 source rels
(bus ndjson, registry, opencode.db, claude jsonl), and "everyone's local AI
tool" becomes one .dl6 file per tool.

## User-mind as a query (user, same conversation)

The ability for an agent (or another user) to know someone's context on
demand: what they looked at historically, their interactions, previous
turns, in which session and cwd. The reframe: "get user mind" is JUST A
QUERY over user-context facts sitting in one central store (a db). No
special memory system — the session-fact readers above (bus, registry,
opencode.db, claude jsonl, tmux) land everything in the store, and dl6 is
the turnkey layer: derived rels like recent_focus(user, file, tick),
turn_in(session, cwd, ts), looked_at(user, path, count) answer "what is
Chris thinking about" the same way in_dir answers path facts. Cross-session
resume, "ask the other session what it did," and the board's tab nodes all
become the same query surface. Privacy edge is real and named: whose
transcripts enter the store and who may query them is a decision, not a
default.

## Lane verdicts (pass 1, flash4, 2026-08-08; coordinator spot-verified)

Full reports: REPORT-D2ANIM-A.md (anim repo root) and REPORT-D2ANIM-B.md
(sprefa repo root).

- **Node ids ARE recoverable from d2's svg**: every node/edge `<g>` carries
  the d2 dotted path base64-encoded as its first class token
  (`djUucm9vdHM=` = `v5.roots`; edges encode `(src -> dst)[i]`). Geometry =
  the child `<rect>`; container nesting = decode the dotted path (svg
  g-hierarchy is flat). Undocumented convention, d2 0.7.1; no machine-readable
  layout export exists, svg is the only coordinate carrier.
- **dagre beats elk for tween stability**: appending a sibling left every
  unchanged node pixel-identical under dagre; elk shifted the whole rank
  +50px. Both stable on linear append.
- **anim has most of the machinery, not the glue**: cytoscape (AtlasPanel +
  headless CssGraph), node-id-keyed enter/move/exit primitive
  (core/transition.ts) wired only to atlas cone-focus, FLIP keyed rows,
  from-git.mjs (git history -> frames, but code snapshots only, not d2
  boards). The static d2 graph panel does NOT do keyed animation — it swaps
  innerHTML and fades whole-svg (Frames.tsx:472), so README's "graphs key on
  node ids" claim is wrong today; also stale: `npm run seed` doesn't exist,
  README says Frames.jsx (it's .tsx).
- **Missing glue = the project**: per git version, render d2 -> decode ids +
  rect centers -> cytoscape elements with preset positions -> diff versions
  by data.id -> transitionViews-style enter/keep/exit with fcose as the
  entry-displacement force, preset as the target. task-queue.d2 has 2
  committed versions today (V0 3b371691, V1 549c9acc: +v5 container 5 nodes,
  +v6note, -anytime.v5w, s1 relabel).
- Coordinates must never be identity: pivot on decoded ids, normalize for
  viewBox offset (legend rects go negative), re-derive per version.

## Open design questions for the lanes to inform, user to decide

- Does anim animate d2 by re-parsing the d2 source (node ids from the d2 AST)
  or by diffing rendered svgs? (determines whether d2 layout coords are
  available as tween targets)
- d2 --layout=elk vs dagre coordinate stability across small graph edits
  (layout jitter would fight rule 3's "pull toward the target layout").
- cytoscape as the runtime (user's prior art) vs anim's existing svg tweener:
  cytoscape has force layouts built in (cose/fcose) which gives rule 3's force
  pass for free; the question is importing d2's final positions as a preset
  layout and animating preset<->fcose.
