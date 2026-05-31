# frame-anim

An animated, scroll-free way to explain code. You write plain markdown; it renders
as a deck you step through with arrow keys. Each step animates: code tweens token by
token, graphs draw themselves on, file trees slide. It's meant for building intuition
about algorithms, data models, and architecture — a bespoke animated textbook you can
spin up per topic.

> Status: built fast in one long session, **lightly tested**. The build pipeline,
> code/graph/fs rendering, and live-reload all work and have screenshots to prove it.
> Treat the edges (mobile, odd inputs, large decks) as unproven. See [Caveats](#caveats).

## Quick start

```bash
cd v5/anim
npm install
npm run seed       # one-time: makes a demo SQLite DB for the sql-graph example
npm run dev        # open the printed http://localhost:5173
```

Arrow keys to step. `o` = outline, `m` = map (see [Navigation](#navigation)).

## The idea in one minute

- A **deck** is a folder of markdown chapter files: `src/deck/01-foo.md`, `02-bar.md`, …
  The filesystem is the table of contents. (A single `src/frames.md` also works.)
- Each `## ` heading is one **frame** — one idea.
- A frame has prose (the narration) and, optionally, a **left panel** (a code block)
  and a **right panel** (a graph or a file tree). Any of them can be omitted; a
  prose-only frame is a fine discussion note.
- Stepping between frames **animates** the panels. The one rule behind every
  animation: things with the same key slide to their new place, new things fade in,
  gone things fade out. Code keys on tokens, graphs key on node ids, file trees key
  on paths.

That's the whole model: *a frame is a moment, a panel is a lens, stepping is time.*

## Authoring reference

Everything is one markdown file per chapter. Here is every piece you can put in a
frame.

### Heading and narration

```markdown
## the title of this frame

Prose here is the narration. It is real markdown: **bold**, lists, `inline code`,
[links](https://example.com), tables, > blockquotes.
```

Two extras in narration:

- `[[other-chapter]]` or `[[#frame title]]` — a cross-link. These build the deck's
  own link graph (see the `m` map).
- Glossary terms (defined in `src/glossary.md` as `term :: definition`) get a hover
  card on their first appearance in each frame, automatically.

### Code panel (left)

A fenced block with a language:

````markdown
```rust
fn main() { run(); }
```
````

Keep consecutive frames' code blocks **similar** — the animator tweens the
difference, so small deltas read as motion.

Instead of pasting, pull from a real file (it never drifts):

```markdown
code: ../src/scc.rs#L63-71 as rust
```

### Graph panel (right) — hand-drawn

A [d2](https://d2lang.com) diagram. Name it so later frames can reuse it:

````markdown
```d2 callgraph
main -> run
run -> parse
parse -> lex
lex -> run
```
````

Reuse it in a later frame with `graph: callgraph`.

Two things happen automatically:

- **Cycles color themselves.** A Tarjan pass tints any node in a loop. You never
  style a loop by hand. (Opt out with `# noautocolor` in the block.)
- **A vocabulary** is prepended so you tag nodes instead of styling them:

  | tag | meaning | look |
  |---|---|---|
  | `node.class: fn` | a function/value | light box |
  | `node.class: relation` | a relation/table | blue cylinder |
  | `node.class: type` | a type | green hexagon |
  | `node.class: module` | a file/module | purple page |
  | `node.class: sink` | terminal, calls nothing | yellow |
  | `node.class: dead` | defined, never used | dashed/dim |
  | `node.class: hub` | important | thick stroke |
  | `node.class: ghost` | de-emphasized | faded |

### Graph panel — from a database

Render the result of a SQL query as a graph. Opens the DB **read-only**; the tool
never runs anything, it just reads:

````markdown
```sql-graph callgraph data/callgraph.sqlite
SELECT caller, callee FROM call_edge
```
````

Two columns become edges, a third becomes the edge label, one column becomes bare
nodes. This is the seam to a real engine: point the path at any SQLite database.

### File-tree panel (right)

Renders like a file explorer; animates between frames (files slide in, the tree
reflows on a move):

````markdown
```fs
Cargo.toml
src/main.rs
src/scc.rs +        # + added, ~ changed, * focus
```
````

### Bind code to the graph

Make a hover chip that lights a graph node and the matching code token together:

```markdown
anchor: reaches -> reaches
```

### Reuse a whole panel

Pull another frame's graph and code in without re-authoring:

```markdown
![[03-relations#now zoom out: the relations themselves form a graph]]
```

## Navigation

| key | action |
|---|---|
| `→` / space | next frame |
| `←` | previous frame |
| `o` | outline — the deck tree, click a slide to jump |
| `m` | map — the deck's own link graph (chapters as boxes, slides as nodes, `[[links]]` as edges); click a node to jump |
| esc | close outline/map |
| scroll / drag | zoom / pan a graph |

## Commands

| command | what |
|---|---|
| `npm run dev` | live-reloading dev server |
| `npm run build` | production build to `dist/` |
| `npm run check` | lint the deck — broken `[[links]]`, undefined graphs, missing `code:` files, unknown tags, empty frames. Exits nonzero on errors. |
| `npm run seed` | create the demo SQLite DB |
| `npm run frames -- <range> <path> [lang]` | turn a git commit range into frames (snapshot = code, message = narration) |

## How it's wired

```
src/deck/*.md  ──build──▶  src/frames.json   ──▶  the React app
   │  (bin/build-frames.mjs)                        (src/Frames.jsx)
   ├─ ```d2``` / ```sql-graph``` ──▶ d2 ──▶ public/*.svg
   ├─ ```fs``` ──▶ frame.fs
   ├─ code: spans ──▶ read real files
   └─ glossary.md ──▶ glossary.json
```

A Vite plugin reruns the build on every save, so editing a chapter file
live-reloads the deck. Rendering uses [shiki-magic-move](https://github.com/shikijs/shiki-magic-move)
for the code tween, [d2](https://d2lang.com) for graphs, and a small keyed-FLIP
routine for the file tree. SQLite is read via Node's built-in `node:sqlite` (no
dependency).

## Caveats

- **Lightly tested.** Most features have a screenshot but not a test suite. Large
  decks, deep folder nesting, unusual SQL shapes, and the leave-animation on the fs
  lens are unproven.
- `d2` must be on PATH for graphs to render. Without it, frames still show; graph
  panels are just empty.
- Generated files (`frames.json`, `glossary.json`, `public/*.svg`, `graphs/*.d2`)
  are gitignored and rebuilt; the demo `data/callgraph.sqlite` is committed as a
  fixture.
- The `sql-graph` demo points at a local fixture DB. There is no live engine wired
  in yet — that's intentional.

## Where it's heading

The architecture is "one cursor, many lenses": a frame is a cursor `{path, line,
ref, …}` and each panel is a lens that reads it and animates by the same keyed-FLIP
rule. Code, graph, and filesystem are the three lenses today. Planned lenses: a git
history scrubber, an editor-style snippet view, and graph group collapse/expand for
gradually revealed concept maps. Because the `sql-graph` seam already reads any
SQLite DB, a lens can be fed real data (git log, a file index, code-analysis output)
the day that data lands in a database.
