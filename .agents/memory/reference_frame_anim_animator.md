---
name: reference_frame_anim_animator
description: "animated-explainer tool + global /animate command; markdown frames, magic-move code tween, zoom/pan d2 graphs"
metadata: 
  node_type: memory
  type: reference
  originSessionId: 05898624-2ffd-4c84-a405-bcc9fbf97e52
---

Bespoke animated-textbook tool. Step through frames; code panel tweens the token
delta between snapshots (shiki-magic-move), a d2 graph sits beside it with
zoom/pan and a draw-on animation.

**Where:** template at `~/projects/claude-research/frame-anim` (Vite+React). Global
command `~/.claude/commands/animate.md` (`/animate <topic>`) detects an existing app
(`./anim`, then `./v5/anim`) or scaffolds `./anim` from the template + npm install.
In sprefa the live app is `v5/anim` (committed; node_modules/generated gitignored).

**Authoring = markdown, no JSON, no commits.** `src/frames.md`: one `## ` heading
per frame, prose = narration (rendered as MARKDOWN via marked: lists/bold/links/
inline-code/quotes/tables), a fenced ```lang block = the code panel, a fenced
```d2 <name> block = the graph (`graph: <name>` to reuse). **Code and graph are
both optional** — a prose-only frame is a durable discussion note (layout goes
full-width/centered). The deck doubles as a session-discussion record. A Vite plugin (`bin/build-frames.mjs`)
compiles md -> `src/frames.json` + renders inline d2 to `public/*.svg` on every
save, so dev-server live-reload just works. `npm run dev` -> localhost:5173, arrow
keys step. `bin/from-git.mjs` turns a commit range into frames (snapshot=code,
message=narration). Needs the `d2` binary for graphs.

**Two graph layers** both supported: the data graph (nodes = things) and the
relation graph = predicate dependency graph (nodes = relations/types, edges = rule
deps, self-loop = recursion; SCCs of it = strata). Ties to [[project_v5_dl_engine]].

**Book/tree layout:** source can be single `src/frames.md` OR a `src/deck/` tree
of numbered chapter files (walked sorted; filename = chapter = breadcrumb). `o`
opens an outline overlay of the tree (jump by click). `[[slide]]` cross-links are
collected per frame -> the deck's own import/export graph. Press `m` for the MAP:
build-frames emits a `_map` d2 (chapters=containers, slides=nodes with `link:
"#idx"`, `[[links]]`=edges), rendered by the same kit+auto-color pipeline (content
cycles light up). Map nodes are `<a href="#idx">`; MapView highlights current
(.map-here) and click-jumps (verified). Outline=`o`, map=`m`, esc closes. sprefa
deck lives in `v5/anim/src/deck/{01-reachability,02-cycles,03-relations}.md`.

**Branch feat/frame-anim** (off feat/v5-lsp-diag, not pushed): baseline + 3
features committed. `code: ../src/foo.rs#L10-24 [as lang]` pulls a real-file span
at build (dedented, lang from ext) instead of pasting. `anchor: <token> -> <node>`
renders a hover chip lighting the matching graph node (by label) + code token
together (v1 of "point at code, watch graph"; real AST-resolution later). `npm run
check` = deck linter (broken [[links]], undefined graphs, missing code: files,
unknown kit classes, empty frames; nonzero exit). `![[slug#title]]` transcludes a
frame's graph+code. `src/glossary.md` (`term :: definition`) -> hover cards on
first occurrence per frame. `sql-graph <name> <db.sqlite>` fence runs a read-only SELECT (built-in
node:sqlite, zero deps) and renders rows as a graph through kit+auto-color — the
SEAM TO dl (repoint db path at dl's real kernel when ready; touches zero dl code).
Demo db: `npm run seed`. 7 commits total on feat/frame-anim. 3-opus brainstorm produced a
ranked-10 roadmap; NEXT big swing = live dl-query frames (render real call/reach/
SCC graphs from the engine, needs a build->SQLite-kernel bridge).

**Token-cheap graph authoring:** loops colour themselves (Tarjan in build-frames
tints SCC/self-loop nodes, one colour per cycle; opt out `# noautocolor`). A d2
"kit" `src/kit.d2` (classes: fn/relation/type/module/sink/dead/hub/ghost) is
prepended to every graph; tag nodes `helper.class: dead` instead of style blocks.
Vocabulary documented in the /animate command preamble. So the AI writes `a -> b`
+ a one-word class, no styling.

Gotchas learned: don't animate `transform` on the panzoom target (causes zoom
bounce) — animate an inner wrapper, fade opacity-only. d2 svgs have viewBox but no
width/height, so their flex wrapper needs a definite size or they collapse to 0x0.
No shebang in `build-frames.mjs` (esbuild bundles it via vite.config and chokes).
