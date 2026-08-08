# REPORT-D2ANIM-B (lane d2anim-b, pass 1 of 2, 2026-08-08)

Pass 1 of 2: d2 -> svg position extraction + cytoscape mapping study. No recommendation on pipeline choice (Q5) — the user decides. Scope: `plans/2026-08-08-task-queue.d2` (primary), `plans/2026-08-08-interning-machine.d2` (confirmation), to serve the plan `plans/2026-08-08-d2-git-time-animation.md`.

`d2 version` = 0.7.1. All scratch under `/tmp/d2anim-b/`.

## Findings, one sentence each

1. d2 node ids ARE recoverable from the svg: every node/container/edge `<g>` carries its d2 dotted path as a **base64 class name** (e.g. `class="djUucm9vdHM= word"` decodes to `v5.roots`). x/y/width/height live in a `<rect>` inside a nested `<g class="shape">`. Edges are `<path d="M...C...">` Bezier curves whose endpoints are raw coordinates, but the edge's own class name encodes `(src -> dst)[i]`.
2. There is NO machine-readable layout export in d2 0.7.1. Outputs are svg/png/pdf/pptx/gif/txt only; svg is the only carrier of coordinates. `d2 layout dagre|elk` exposes only spacing flags.
3. Layout jitter test: for the append-a-sibling case (toy2) **dagre kept every unchanged sibling pixel-identical** and only recentered the parent; **elk moved the entire unchanged rank down by 50px**. dagre is the more position-stable engine in the tested scenarios.
4. cytoscape mapping is direct: d2 dotted path -> `data.id`, dotted prefix -> `data.parent`, base64 path yields both; svg rect center -> preset `position:{x,y}`; edge class `(a -> b)[0]` -> `data.source/target`.
5. Real animation input: the committed file has exactly 2 versions; the newer commit (549c9acc) adds the `v5` container (5 new word nodes) and `v6note`, drops `anytime.v5w`, and rescopes `s1`.

---

## Q1. SVG anatomy (file:line receipts from `plans/2026-08-08-task-queue.svg`)

### Node id recovery

Only two `<id>` attributes exist in the whole file — the root svg and a single marker:

```
grep -o 'id="[^"]*"' plans/2026-08-08-task-queue.svg | sort -u
id="d2-903638344"
id="mk-d2-903638344-3488378134"
```
No `data-*`, no per-node `id`. Node identity is NOT carried in any attribute named for it.

Instead every node/edge is a `<g>` whose first class token is the **base64 of the d2 dotted path**, followed by the d2 shape class:

```
<g class="djUucm9vdHM= word"><g class="shape" ><rect x="3322.000000" y="163.000000"
  width="403.000000" height="159.000000" .../></g><text ...>orphan db roots: rm 3, frees 1.86GB</text></g>
```
(REPORT file:892/line 894 region.) Decoding:

```
echo "djUucm9vdHM=" | base64 -d   => v5.roots
echo "czcuZGQ="      | base64 -d   => s7.dd
echo "djZub3Rl"      | base64 -d   => v6note
```

So the answer to "is the d2 source node id recoverable": **yes — via base64-decoded class token**. The label `<text>` is present but the id is separate and reliable.

### x/y/width/height

Shape geometry is a `<rect>` with x/y/width/height in the `<g class="shape">` child of each node's `<g>`:

```
<g class="bGVnZW5kLmE= word"><g class="shape" ><rect x="5359.0" y="-199.0" width="190.0" height="61.0" .../></g>
<text x="5454.0" y="-163.0" ...>your word</text></g>
```
(legend.a, file line 894.) Container labels use `<text x= y=>` separately; box geometry is the `<rect>`. Note legend coords are negative (svg has a mask/viewBox offset, pad `20` here).

### Edges

Each edge is a `<g>` with base64 class `(src -> dst)[i]` and a `<path d="M ... C ...">` cubic Bezier plus an arrow marker-end. Endpoints are raw coordinates only; the id link lives in the class name.

```
<g class="KHMxLm1lcmdlIC0mZ3Q7IHMyLmRlZmVjdHMpWzBd" >   // "(s1.merge -> s2.defects)[0]"
  <marker id="mk-...-3488378134" ...orient="auto"...><polygon .../></marker>
  <path d="M 714.5 301.0 C 714.5 419.0 ... C 714.5 605.799988 714.5 629.0"
        class="connection stroke-B1" marker-end="url(#mk-...)" mask="url(#d2-...)" /></g>
```
(file line 966 region.) Decoding the class:
```
echo "KHMyLmRlZmVjdHMgLSZndDsgczMuaWMpWzBd" | base64 -d  => (s2.defects -> s3.ic)[0]
```

Edge with a **label** adds a `<text>` inside the same edge `<g>`:
```
<g class="KHM1LmJlbmNoIC0mZ3Q7IHM2LnAxYilbMF0=">  // (s5.bench -> s6.p1b)[0]
  <path d="M 714.5 2474.0 C ..."/>
  <text x="715.0" y="2588.0" class="text-italic fill-N2" style="text-anchor:middle;font-size:16px">tsv2-first gate opens</text></g>
```
So edge start point = source node bottom-center (`M 714.5 301.0` for `s1.merge` whose rect is `x=586 w=258 -> cx 714.5`), edge end = target top-center. Edge **topology and ids are recoverable**; coordinates are how they're drawn.

### Nested containers

Containers (s1..s7, v5, anytime, v6note, legend) are bare base64 `<g>` (no shape-class suffix) with their own border rect + label text:

```
<g class="czM="><g class="shape" ><rect x="0.0" y="1018.0" width="1405.0" height="254.0" .../></g>
<text x="702.5" y="1005.0" ...>3. remaining interning lanes (two-pass each)</text></g>
```
The SVG **g-hierarchy is flat** — child nodes are siblings of the container `<g>`, not nested inside it. Nesting is only implied by the dotted path in the class name:

- task-queue: `czMuaWM=` = `s3.ic`, `czMuaWs=` = `s3.ik` (children of `s3`).
- interning-machine depth 3: `czIuZGlyZWN0LmRkbA==` = `s2.direct.ddl` (child of `s2.direct`).

```
grep -o 'class="[A-Za-z0-9+/=_]* [a-z]*"' im.svg | ... 
class="czIuZGlyZWN0LmRkbA== code"    # s2.direct.ddl
class="czUucHJpemUubm90ZQ== prizebox" # s5.prize.note
```

To reconstruct the parent/child tree: decode the dotted path, use all-but-last segment as the parent chain. This is a parse-your-own step, not something the svg element nesting gives you.

---

## Q2. Machine-readable layout / engine jitter

### Is there any non-svg positional export?

No. `d2 --help` output formats are svg/png/pdf/pptx/gif/txt; only svg carries shape/edge coordinates (txt is ascii-art, no coords).

```
d2 layout   # engine list — both engines, spacing flags only, no JSON/graph dump
dagre (bundled) ... Flags: --dagre-nodesep, --dagre-edgesep
elk  (bundled) ... Flags: --elk-algorithm, --elk-nodeNodeBetweenLayers, --elk-padding, ...
```
`d2 fmt` formats the .d2 source (whitespace), not positions. No `--json`, no `--output-format` graph option, no geometry env var. svg is the single carrier of layout.

### Jitter experiment (toy under /tmp/d2anim-b)

Toy 1 `toy.d2` `a->b->c->d`, render with one node added (`d->e`). Both dagre and elk were **pixel-identical** for unchanged `a,b,c,d` (linear chain, both engines stable).

| node | dagre vA (x,y) | dagre vB (x,y) | elk vA (x,y) | elk vB (x,y) |
|---|---|---|---|---|
| a | 1,0 | 1,0 | 12,12 | 12,12 |
| b | 1,166 | 1,166 | 12,148 | 12,148 |
| c | 1,332 | 1,332 | 12,284 | 12,284 |
| d | 0,498 | 0,498 | 12,420 | 12,420 |

Toy 2 `toy2.d2` fan-out `t->x1 x2 x3`, add `t->x4`. **This is the decisive jitter case.**

| node | dagre vA (x,y) | dagre vB (x,y) | elk vA (x,y) | elk vB (x,y) |
|---|---|---|---|---|
| t (parent) | 128,0 | 189,0 | 65,12 | 86,12 (w 120->160) |
| x1 | 0,166 | 0,166 | 12,158 | 12,**208** |
| x2 | 122,166 | 122,166 | 94,158 | 94,**208** |
| x3 | 244,166 | 244,166 | 176,158 | 176,**208** |
| x4 | — | 366,166(new) | — | 258,**208**(new) |

- **dagre**: adding `x4` left `x1,x2,x3` at identical absolute coords; only the parent `t` nudged right (+61) to recenter.
- **elk**: adding `x4` translated the ENTIRE unchanged rank down by +50 (158->208) and grew `t` wider; no unchanged node kept its place.

Toy 3 mid-rank insertion `t->a,t->c` then `t->a,t->b,t->c` (b inserted between). Both engines push the later sibling right to make room; `a` stays fixed in both:

| node | dagre vA x | dagre vB x | elk vA x | elk vB x |
|---|---|---|---|---|
| t | 58 | 114 | 35 | 51 |
| a | 0 | 0 | 12 | 12 |
| b | — | 113 | — | 85 |
| c | 113 | 226 | 85 | 158 |

**Verdict on stability (data, not recommendation):** in the tested append-a-sibling and mid-insertion cases, **dagre** is the more position-stable engine — unchanged siblings kept identical coordinates (toy2), while elk shifted the whole rank. Both engines are fully stable on pure linear append (toy1). Note the asymmetry matters to the idea doc rule 3 ("pull toward target layout"): if d2 is the ground-truth target, dagre minimises how much of the UNCHANGED graph relocates after a one-node edit.

---

## Q3. Version timeline available today

Two committed versions of `plans/2026-08-08-task-queue.d2`:

| V_i | sha | subject |
|---|---|---|
| V0 (oldest) | 3b371691 | plans: task queue drawn with per-stage examples |
| V1 (newest) | 549c9acc | plans: queue updated post-merge, v5 words expanded, v6 expansion note, endgame approved |

Both dated 2026-08-07 (short format).

Top-level keys per version (`git show <sha>:... | grep -oE '^[a-z0-9_]+:'`):

```
V0  vars direction classes legend s1 s2 s3 s4 s5 s6 s7 anytime
V1  vars direction classes legend s1 s2 s3 s4 s5 s6 s7 anytime v5 v6note
```

Node-level change, newest vs oldest (full leaf id sets differ):

| change | V0 | V1 |
|---|---|---|
| `s1` label | `"1. one word each"` | `"1. DONE 2026-08-08"` |
| `s1.merge`, `s1.cull` class | `word` | `side` (+ relabelled content) |
| `anytime.v5w` | present | **removed** |
| `v5` container + `v5.roots .port .lazy .fsize .dmatch` | absent | **added** |
| `v6note` (markdown note, shape:rectangle) | absent | **added** |

`git show 549c9acc:plans/2026-08-08-task-queue.d2 | head -40` shows the `s1` rewrite is the first visible node delta after the (unchanged) `vars/classes/legend` preamble; the tail of the diff is the wholly-new `v5` block + `v6note`. So the real animation input the user wants resolves to: **add 5 v5-nodes + 1 v6note, drop anytime.v5w, relabel s1 + change its two children's class from word to side** — across 2 snapshots. That is the entire available timeline today; there is no deeper history (`git log --follow` shows exactly these 2 commits).

---

## Q4. Cytoscape mapping sketch

### Mapping table

| d2 concept | d2 carrier in svg | cytoscape elements JSON field |
|---|---|---|
| node | `<g class="<b64 path> <shapeclass>">`; decode path = id | `data.id` = decoded path |
| node label | `<text>` child | `data.label` |
| container / nesting | flattened `<g class="<b64 dotted path>">`; parent = all-but-last path segment | `data.parent` = parent node `data.id` (compound) |
| node position | `<rect x y width height>` -> center (x+w/2, y+h/2) | `position: {x, y}` fed to a **preset** layout |
| edge | decode `(src -> dst)[i]` from class name | `data.source` / `data.target` (= node ids) |
| edge label | `<text>` inside edge `<g>` | `data.label` |
| class / style | second class token (`word/lane/gatebox/...`) + `stroke`/`fill` attrs on rect | `data.shape`/`data.style` (or a class map) |

Where svg coords feed cytoscape: cytoscape's **preset layout** takes `elements[i].position.{x,y}` verbatim — those x/y come straight from the svg `<rect>` center. So the d2 (dagre/elk) final positions become the cytoscape preset; then cytoscape's built-in **force layouts (fcose/cose)** run on top to shape the entry-displacement journey (idea doc rule 3), animating preset <-> fcose. The svg `<rect>` center is the only transform needed (no decode, just arithmetic).

### Worked 5-node example (toy.d2 `a->b->c->d`, dagre), side by side

d2 svg (from `/tmp/d2anim-b/toy-dagre-a.svg`), `a->b->c->d`:

```
node  rect(x,y,w,h)      center(x,y)
a     (1,0,53,66)        (27.5, 33)
b     (1,166,53,66)      (27.5,199)
c     (1,332,53,66)      (27.5,365)
d     (0,498,54,66)      (27,531)
edges: (a->b)[0] (b->c)[0] (c->d)[0]
```

cytoscape elements JSON:

```json
{
  "nodes": [
    { "data": { "id": "a", "label": "a" }, "position": { "x": 27.5, "y": 33 } },
    { "data": { "id": "b", "label": "b" }, "position": { "x": 27.5, "y": 199 } },
    { "data": { "id": "c", "label": "c" }, "position": { "x": 27.5, "y": 365 } },
    { "data": { "id": "d", "label": "d" }, "position": { "x": 27, "y": 531 } }
  ],
  "edges": [
    { "data": { "id": "a->b", "source": "a", "target": "b" } },
    { "data": { "id": "b->c", "source": "b", "target": "c" } },
    { "data": { "id": "c->d", "source": "c", "target": "d" } }
  ]
}
```

Nested example from the real board: `s3.ic` svg class `czMuaWM=` -> `data.id="s3.ic"`, `data.parent="s3"` (container `czM=` = `s3`); position from its `<rect x="30" y="1083" ...>`.

---

## Q5. Parse-vs-scrape candidate pipelines (evidence only, no recommendation)

| # | pipeline | yields | misses / list of what it does NOT give | fragility notes |
|---|---|---|---|---|
| A | Render each git version -> parse svg `<g>` base64 class + `<rect>` + findByClass `<path>` | id (decoded), label, container via dotted path, x/y/w/h per node, edge src/dst from `(s->t)[i]` | — | Depends on d2's base64-class-by-convention (undocumented); breaks if d2 changes the class scheme. Salt prefix (`d2-903638344`) changes per compile but does NOT affect node-class decode. |
| B | Parse the **d2 source** for topology (nodes/edges/nesting) + render svg for coords only | stable topology independent of svg internals; coords from svg rects | coords only for whatever d2 chose to draw; layout-derivable labels; no explicit edge-endpoint ids in svg (edges keyed by src/dst coords + class) | Topology parser is on you (`go-d2parser` exists but not installed here); svg base64 still needed for coords. Most robust to svg-scheme changes on the topology side. |
| C | Graph a single render across edits: same as A but re-run per commit and diff decoded id sets | same as A per commit | none beyond A | Requires one render per V_i; render is fast (~45ms dagre here) but each svg is large with inlined base64 fonts (dominates bytes, not layout extraction). |
| D | `d2 fmt` / any native export | nothing positional | — | `d2 fmt` reformats source only; there is **no** machine-readable layout export in 0.7.1 (Q2). Dead end for coords. |

Cross-cutting fragility for all svg-based routes:
- Coordinates are absolute, not normalized; svg has a viewBox offset / `pad` and negative legend coords (`y="-199"`), so normalize before comparing across versions.
- Edge endpoint identity must come from the decoded edge class name (`(a->b)[0]`), NOT from the `<path>` endpoints, which are coordinates-only.
- Layout jitter (Q2, toy2) means the same logical node id can have different x/y between versions even with nothing else changing — pivot animation on `data.id`, never on coordinates.

---

## Appendix

- Plan context read: `plans/2026-08-08-d2-git-time-animation.md` (59 lines; questions 1,2,5 map to its pipeline and engine questions; Q4 to its cytoscape rule-3 requirement).
- All experiments reproducible under `/tmp/d2anim-b/`.
- No deviation from the brief was encountered; this report is plain findings with receipts.
