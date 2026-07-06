---
name: sprf-flow-panel-graph-viewer
description: Primed context for the dl flow panel (editors/vscode-dl/media/flow-panel.html). The panel is a standalone-capable graph viewer with schema-driven layer discovery. Load when editing the panel, adding view modes, wiring graph layers, or modifying the host seam.
---

# dl flow panel graph viewer

## What the panel is

A single HTML file (`editors/vscode-dl/media/flow-panel.html`, ~3100 lines)
that renders a node/edge graph from SQL queries against the running dl
daemon's SQLite. Three view modes (same data, different layout):

- **list** (default): fs-tree. Nodes grouped by `file` column into a segment
  trie, compacted GitHub-style. Edges drawn as bracket arcs in gutters.
- **canvas**: force-directed graph. Nodes positioned by edges.
- **trace**: flat forward slice from pinned node(s).

All three use the same node/edge rows, pin set, and legend filter.

## Host seam (standalone capability)

The ONLY host coupling is `window.dlHost` (`flow-panel.html:565-585`):

```js
window.dlHost = { query(sql, params), hover(files), open(file, line) }
```

Three methods. VSCode injects one impl via the `DL_HOST` marker. A plain
browser falls through to the HTTP bridge:
```js
window.dlHost = {
  async query(sql, params = []) {
    const res = await fetch(endpoint + '/rpc', { method: 'POST', ... });
    ...
  },
  hover(files) { /* no-op in browser */ },
  open(file, line) { console.log('open', file + ':' + line); },
};
```

`endpoint` from `?dl=<url>` query param or `localStorage['dl-endpoint']`,
default `http://127.0.0.1:7379`. Backed by `scripts/dl-bridge.mjs`.

No other VSCode API touches the panel. SVG rendering, tree builder, layout,
kind filtering — all vanilla JS. Going standalone = ship the HTML + bridge.

## Two view sources: presets + layers

### Presets (`PRESETS` object, `:666`)

Curated named views with hand-written SQL. Each entry: `{nodes: SQL, edges: SQL}`.
Selected via the `presetSel` dropdown. Some need joins, UNIONs, CASE
expressions (e.g. `marks` joins 4 rels; `diff` adds a diff-class column).
These stay for views that name-binding can't express.

Key presets: `typeAll` (raw `type_entity`+`type_link`), `typeCurated`
(`panel_node`), `typeRefs`, `typeNeighbors`, `prGraph`, `marks`,
`enumMatches`, `madge`, `memberFlow`, `diff`.

### Layers (`discoverLayers()`, `:804`)

Schema-driven. Discovers `rel_X_node`/`rel_X_edge` pairs from `sqlite_master`,
introspects columns via `PRAGMA table_info`, binds by name to renderer roles.
Toggleable checkboxes; active layers UNION ALL into a composed graph.

The `_node`/`_edge` convention + name binding = the turnkey surface. Write
`.dl` declaring `foo_node(sym,name,kind,file,line)` + `foo_edge(src,dst,kind)`,
click `↻`, layer appears.

Built-in pairs that don't follow the convention are in `BUILTIN_LAYERS`
(`:797`):
```js
const BUILTIN_LAYERS = [
  { name: 'type', nodeTable: 'rel_type_entity', edgeTable: 'rel_type_link' },
];
```

## Column-name binding (`:755-790`)

Node binding constants:
```js
const NODE_ID_COLS = ['id', 'sym'];
const NODE_LABEL_COLS = ['label', 'name'];
const NODE_KIND_COLS = ['kind'];
```

Edge binding constants:
```js
const EDGE_SRC_COLS = ['src', 'source', 'a'];
const EDGE_DST_COLS = ['dst', 'dest', 'target', 'b'];
const EDGE_KIND_COLS = ['kind', 'type'];
```

`pickCol(cols, candidates, fallback)` returns the first matching column name
(quoted), or the fallback. Positional fallback (`cols[0]`, `cols[1]`) handles
unconventional names.

`layerNodeSql(table, cols)` generates:
```sql
SELECT "sym", "name", "kind", <file-expr>, "line", "parent" FROM rel_X_node LIMIT 600
```

The `<file-expr>` is a repo-prefix CASE when id is `sym` and `file` exists
(extracts repo slug from `::`-delimited sym), otherwise the raw `file`/`path`
column, or NULL.

`layerEdgeSql(table, cols)` generates:
```sql
SELECT "src", "dst", "kind" FROM rel_X_edge LIMIT 1200
```

## State management

| variable         | type          | purpose                              |
|------------------|---------------|--------------------------------------|
| `currentPreset`  | string        | active view source: preset name, `'layers'`, or `'custom'` |
| `layers`         | array         | discovered layer objects `{name, nodeTable, edgeTable, nodeCols, edgeCols}` |
| `activeLayers`   | Set<string>   | toggled-on layer names               |
| `pinned`         | Set<string>   | node ids pinned by click             |
| `kindFilter`     | Set<string>   | legend-selected kinds (empty = all)  |
| `viewMode`       | string        | `'list'` or `'canvas'`               |

Persistence (localStorage, key `dl-flow-preset`):
- preset mode: `{preset: 'typeAll'}`
- custom mode: `{preset: 'custom', nodes: '...', edges: '...'}`
- layers mode: `{preset: 'layers', layers: ['type', 'member']}`

`restorePreset()` (`:984`) restores on load. `savePreset()` (`:3070`)
persists on every change.

## Init sequence (`:3112-3114`)

```js
applyView();                                          // set list/canvas CSS
discoverLayers();                                     // async, fire-and-forget
if (currentPreset !== 'layers') run();                // skip if layers mode
```

`discoverLayers()` calls `runLayers()` at the end if `currentPreset === 'layers'`
and there are active layers. Otherwise the restored preset runs via `run()`.

## Key functions

| function          | location  | what it does                            |
|-------------------|-----------|-----------------------------------------|
| `discoverLayers`  | `:804`    | scan schema, build layer list, render   |
| `renderLayerList` | `:852`    | render checkbox chips                   |
| `runLayers`       | `:880`    | compose UNION ALL, query, render        |
| `layerNodeSql`    | `:771`    | generate name-bound node SELECT         |
| `layerEdgeSql`    | `:786`    | generate name-bound edge SELECT         |
| `run`             | `:996`    | execute preset/custom SQL, render       |
| `render`          | `:1100+`  | build canvas nodes/edges + list rows    |
| `showError`       | `:977`    | error banner with missing-rel hint      |

## How to add a new view feature

1. **New graph layer**: declare `_node`/`_edge` rels in `.dl`. Done.
2. **New preset**: add to `PRESETS` object + `<option>` in `presetSel`.
3. **New view mode**: add to the `viewMode` toggle, add a render path in
   `render()`, add CSS. The list/canvas/trace modes share the same node/edge
   data.
4. **New built-in alias**: add to `BUILTIN_LAYERS`.
5. **New column role**: add a binding constant + slot in `layerNodeSql`.

## Column shapes verified against cache.db

| rel               | columns                                      |
|-------------------|----------------------------------------------|
| `type_entity`     | `repo, sym, name, kind, parent, file, line`  |
| `type_link`       | `src, dst, kind`                             |
| `panel_node`      | `sym, name, kind, file, line`                |
| `member_node`     | `sym, name, kind, file, line, parent`        |
| `module_node`     | `path, kind`                                 |
| `graph_node`      | `id, label, kind`                            |
| `bare_node`       | `repo, sym, kind`                            |
| `call_node`       | `id, callee, file, line`                     |
| `df_node`         | `id, kind, var, fn, file, line`              |

All include a trailing `__src` (provenance sentinel, ignored by binding).

## Build verification

```bash
cd editors/vscode-dl && npx tsc --noEmit   # exit 0 = clean
```

The panel is vanilla JS in HTML (no TS), but the extension wrapper (`src/extension.ts`)
is TS. tsc verifies the extension compiles. The panel itself has no compile
step — open it in a browser or the VSCode webview to test.

## daemon RPC seam

The daemon's `query_sql` RPC (`src/daemon.rs:1250`) accepts arbitrary SQL and
passes it to `eng.query_sql(sql, &params)` — pure SQLite passthrough. This is
what `host.query()` calls (via the LSP `dl/query` proxy at `src/lsp.rs:365`,
or via the HTTP bridge's `query_sql` method). `PRAGMA table_info` and
`sqlite_master` queries work through this path.

The daemon socket lives at `~/.local/state/sprefa/daemon.sock` (macOS) or
`<root>/.dl/daemon.sock` (if path is short enough). The HTTP bridge listens
on `127.0.0.1:7379`.
