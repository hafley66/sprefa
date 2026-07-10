---
name: sprefa-flow-panel-layers
description: Schema-driven graph layer discovery for the dl flow panel. The _node/_edge naming convention plus column-name binding lets any .dl-declared rel pair show up as a toggleable graph layer with no preset edit and no SQL. Load when wiring new graph views, editing flow-panel.html, or designing .dl rels for panel rendering.
---

# Flow panel schema-driven layer discovery

## The contract: _node / _edge

Any dl rel ending in `_node` that has a sibling ending in `_edge` is a graph
layer. The panel discovers them automatically from the daemon's SQLite schema.

```
rel foo_node(sym: text, name: text, kind: text, file: file, line: int).
rel foo_edge(src: text, dst: text, kind: text).
```

Daemon hot-reloads the `.dl` file. Click `↻` in the panel (or reload). The
layer `foo` appears as a checkbox. Toggle it. Nodes + edges render. No preset
edit, no SQL, no recompile.

## Column-name binding

The panel reads column names via `PRAGMA table_info(rel_X_node)` and binds
them to renderer roles **by name, not position**. The dl declaration IS the
view spec.

| dl column name     | panel role                          |
|--------------------|-------------------------------------|
| `id` / `sym`       | node identity (merge key across composed layers) |
| `label` / `name`   | display text                        |
| `kind`             | legend chip, color, filter          |
| `file` / `path`    | fs-tree grouping (list view)        |
| `line`             | in-file ordering, click-to-jump     |
| `parent`           | nesting (member under owner)        |

Edge columns: `src`/`source`/`a` slot 0, `dst`/`dest`/`target`/`b` slot 1,
`kind`/`type` slot 2. Positional fallback (`cols[0]`, `cols[1]`) handles
unconventional names (`caller`/`callee`, `from`/`to`).

If the id column is `sym` AND a `file` column exists, the panel applies a
repo-prefix CASE (`substr(sym, 1, instr(sym, '::') - 1) || '/' || file`) so
the fs-tree groups by repo slug. This is the type-graph convention where syms
carry `::repo::` segments.

## Built-in alias table

Engine built-ins (`type_entity`, `type_link`, `call_edge`, `created`) predate
the panel and don't follow the `_node`/`_edge` convention. The discovery has
an explicit alias table for known graph-shaped built-in pairs:

```js
const BUILTIN_LAYERS = [
  { name: 'type', nodeTable: 'rel_type_entity', edgeTable: 'rel_type_link' },
];
```

Add more pairs here as needed. The column binding is the same — PRAGMA
introspection + name-to-role mapping.

## How discovery works

1. `SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'rel_%_node'`
2. For each `rel_X_node`, check `rel_X_edge` exists. Skip if not.
3. `PRAGMA table_info(rel_X_node)` and `PRAGMA table_info(rel_X_edge)` for column names.
4. Also scan `BUILTIN_LAYERS` for non-convention pairs.
5. Sort by layer name. Render as checkbox chips.

All queries go through `host.query(sql)` which is the `dl/query` LSP RPC
(`src/lsp.rs:365`, `eng.query_sql` — pure SQL passthrough to SQLite).

## Presets vs layers

**Presets** (`PRESETS` object, `flow-panel.html:666`) are curated named views
with hand-written SQL. Some need joins, UNIONs, or CASE expressions that
name-binding can't express (e.g. `marks` joins `rel_mark` + `rel_mark_selector`
+ `rel_mark_call` + `rel_mark_flow`). Presets stay for these complex views.

**Layers** are the turnkey surface. Any `_node`/`_edge` pair. Toggle multiple
to compose (UNION ALL, nodes merge by id). The composed SQL appears in the
nodes/edges input boxes — editable, which flips to Custom mode.

## Existing _node/_edge pairs

| layer name       | node rel                    | edge rel                    | source               |
|------------------|-----------------------------|-----------------------------|----------------------|
| `type`           | `rel_type_entity`           | `rel_type_link`             | built-in (alias)     |
| `panel`          | `rel_panel_node`            | `rel_panel_edge`            | .dl/flow-panel.dl    |
| `type_refs`      | `rel_type_refs_node`        | `rel_type_refs_edge`        | .dl/flow-panel.dl    |
| `type_neighbor`  | `rel_type_neighbor_node`    | `rel_type_neighbor_edge`    | .dl/flow-panel.dl    |
| `member`         | `rel_member_node`           | `rel_member_edge`           | .dl/flow-panel.dl    |
| `module`         | `rel_module_node`           | `rel_module_edge`           | .dl/flow-panel.dl    |
| `graph`          | `rel_graph_node`            | `rel_graph_edge`            | .dl/git-graph.dl     |
| `bare`           | `rel_bare_node`             | `rel_bare_edge`             | .dl/graph-diff.dl    |
| `call`           | `rel_call_node`             | `rel_call_edge`             | .dl/flow-panel.dl    |
| `df`             | `rel_df_node`               | `rel_df_edge`               | .dl/flow-panel.dl    |

## Adding a new graph layer

1. Declare the rel pair in a `.dl` file:
   ```
   rel myview_node(sym: text, name: text, kind: text, file: file, line: int).
   rel myview_edge(src: text, dst: text, kind: text).
   ```
2. Populate them with rules.
3. Save. Daemon hot-reloads.
4. Click `↻` in the panel. Layer `myview` appears.

Column names drive the binding. If you need `parent` nesting, add a `parent`
column. If you don't have file paths, omit `file`/`path` and nodes render flat
(no fs-tree grouping).

## Host seam (standalone capability)

The panel's only host coupling is `window.dlHost` (`flow-panel.html:565-585`):

```js
window.dlHost = { query(sql, params), hover(files), open(file, line) }
```

VSCode injects one implementation. A plain browser falls through to the HTTP
bridge (`fetch` to `http://127.0.0.1:7379/rpc`, backed by
`scripts/dl-bridge.mjs`). No other VSCode API touches the panel. SVG
rendering, tree builder, layout — all vanilla JS. Going standalone = ship the
HTML + bridge script, point at the daemon's JSON-RPC socket.
