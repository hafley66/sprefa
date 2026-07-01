# Anim in the repo: dl relations → animated graphs → node-editor ports

Date: 2026-06-11. Follows plans/2026-06-06-graph-viz-atlas.md. Everything in
"verified" sections below was compiled locally with d2 0.7.1; probe files in
/tmp/typeviz/probe/.

## What landed today (working, in examples/)

| file | what it proves |
|---|---|
| examples/typegraph-anim.dl + .d2 | 3-frame cumulative reveal of the type graph; `comment` markers inside d2 `steps:` boards, three gen splice rules batch into one write; `d2 --animate-interval=1200` renders an animated SVG |
| examples/typeports.dl + .d2 | node-editor shape: hub structs as `sql_table` nodes, one port row per outgoing edge (kind shown as the row type), wires leave the exact port row |

Engine fix that unblocked it: splices accumulate across ALL gen rules per tick
(`Splices` in engine.rs), one bottom-up write per file. Test:
tests/gen_op.rs two_gen_rules_splice_one_file_in_one_write.

## Verified d2 surface reachable from gen's one-line-per-row grain

Everything below compiles as FLAT dotted lines, so plain gen rows cover it.
No nesting needed beyond the hand-written skeleton.

| d2 feature | flat form | dl source |
|---|---|---|
| node | `{f}.shape: sql_table` | any unary rel |
| port row | `{f}.{t}: {k}` | edge rel with kind |
| port-anchored wire | `{f}.{t} -> {t}` | edge rel |
| marching-ants edge | `({a} -> {b})[0].style.animated: true` | per-edge styling, indexed flat form works |
| class assignment | `{f}.class: hot` | tier/threshold rels |
| frame membership | rows spliced into a `steps:`/`scenarios:` board | one marker pair per frame |

Boards: `steps:` inherit cumulatively (reveal animations), `scenarios:` each
inherit only the base (state A vs state B, e.g. WORK vs HEAD). Both keep node
positions stable across frames, which is the mental-map requirement from the
atlas plan. `layers:` are separate drill-down pages.

## Frame axes expressible in dl today

- threshold tiers over an aggregate (`n >= 5` / `4` / `3`) — typegraph-anim
- rev: WORK vs HEAD vs pinned rev (`scan` first arg) → scenarios boards
- membership in any derived rel (reaches() from a seed, SCC, diag severity)
- anything `comment` can mark: one marker pair per frame, no count limit

## Gaps (the honest list)

1. **No int arithmetic in heads** → no BFS-depth frames, no "step = prev + 1".
   This is the single blocker between "tier reveal" and "algorithm walk".
2. **File-form gen, same path, two rules** = last write wins silently (splice
   form is batched now; file form is not unioned).
3. **No header/footer in file form** → valid JSON unreachable (markers can't
   live in JSON); blocks direct d3/observable export. NDJSON or a .js skeleton
   with markers works today.
4. **No string escaping hook in templates** → `::` names collide with d2 label
   syntax; demos filter to plain idents instead of quoting.
5. d2 sql_table rows have no left/right (in/out) side control — see ports plan.

## Plan: bring anim into the repo proper

- [x] examples/typegraph-anim.dl, examples/typeports.dl (this session)
- [ ] `just viz` or a tiny `tools/viz.sh`: run a .dl then d2-render its targets
      to /tmp/typeviz/ (dl writes only inside the root by design; the render
      step stays external and explicit)
- [ ] scenarios demo: WORK-vs-HEAD type-graph diff as a 2-board animation
      (type_edge_rev already exists; this is the cheapest NEW capability)
- [ ] arithmetic in heads (`n + 1`, `n - 1` over int columns) — unlocks
      depth/step frames; SQLite does the math, lower.rs emits the expression;
      stratification already guards recursion
- [ ] gen file-form union across rules (mirror the Splices batching) + an
      optional header/footer pair: `gen("p.json", "[", "{row}", "]") <- ...`
      → valid JSON for d3-force / observable
- [ ] `{var:d2}` / `{var:json}` template escape modifiers (quote-and-escape per
      target language) so `::` names stop being filtered out

## Node-editor ports (for later, designed not built)

Goal: a node is currently a label; show structured interfaces — in-ports and
out-ports as rows, wires landing on the exact row, like a node editor.

Relation schema (target-agnostic, lives in .dl):

    port(node, port, dir, ty)     dir ∈ in|out
    wire(src_node, src_port, dst_node, dst_port)

typeports.dl already emits the degenerate version (every port is an out-port,
dst_port is the whole node). Full version per target:

1. **d2 sql_table** (today): rows render in declaration order; emit in-ports
   first, then out-ports — visual grouping, no side control. Wire form
   `a.p1 -> b.p2` anchors both ends to rows. Good enough for readability,
   not true left-in/right-out alignment.
2. **graphviz record/HTML-table ports** (cheap add): `node [label="<p1> x|<p2> y"]`
   + `a:p1:e -> b:p2:w` compass points give REAL side alignment (out leaves
   east, in enters west). Same flat one-line-per-row grain; gen can target a
   .dot skeleton identically. No board/steps animation though — frames =
   one .dot per frame + `gifsicle`/imagemagick, or svg post-class toggling.
3. **JSON → a JS node editor** (litegraph/rete/svelvet) once file-form grows
   header/footer: nodes+ports+wires as data, the editor does drag/zoom/anim.
   This is the "have fun with dl data" endgame: dl is the source of truth,
   the browser is the lens.

Sequencing: 1 is done in spirit (typeports), 2 is a one-evening demo, 3 waits
on the file-form JSON gap above.

## Steering haiku with this (the cursed-blade note)

Sketch only, full plan in plans/2026-06-11-haiku-blade.md: `cmd` is already a
per-file UDF with exactly the right cache key (file hash, rule text) — point it
at `claude -p --model claude-haiku-4-5-20251001` and a haiku call becomes a
cached, deterministic-enough fact source; diag rails verify the output shape;
gen splices accepted output back into files; `--check` stays write-free so the
enforcement rail never triggers model calls it can't cache.
