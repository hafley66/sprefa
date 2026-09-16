# dl flow panel test harness

Two layers: vitest unit tests over standalone JS, and a playwright e2e suite
that drives the real `media/flow-panel.html` in a browser against a hermetic
fixture instead of a live `dl` daemon.

## Bug found while building this (FIXED): SVGElement has no `offsetTop`

The windowing math in `media/flow-panel.html` read
`document.getElementById('gutterLeft').offsetTop` to resolve the toolbar's
live height, but `#gutterLeft` is an `<svg>` and `offsetTop` belongs to the
CSSOM View `HTMLElement` mixin, which `SVGElement` does not implement
(verified `false` on Chromium 149 and Chrome 150). The math computed `NaN`
bounds and the list view (the default view) materialized zero rows in any
Chromium-family browser, webview included.

Fixed by reading `#listRows` (a div in normal flow whose `offsetTop` resolves
to the same `--bar-h` value) at all three sites. This suite now exercises the
real code path with no shim; the polyfill that originally worked around it
was removed from `tests/e2e/fixtures.ts` when the fix landed. If the list
view ever renders empty again, check those `offsetTop` reads first.

## Running

From `editors/vscode-dl/`:

```sh
npm install                    # once, also pulls @playwright/test + vitest
npx playwright install chromium  # once, downloads the browser binary

npm test                       # vitest unit tests
npm run test:e2e               # playwright e2e + snapshots
npm run test:all               # both
```

`npm run test:e2e` boots two throwaway HTTP servers itself (see
`playwright.config.ts`'s `webServer` array) on fixed ports:

- `127.0.0.1:7381` — `tests/fixture-bridge.mjs`, the canned data source.
- `127.0.0.1:7382` — `tests/panel-server.mjs`, serves the panel HTML.

Playwright tears both down when the run ends. If a previous run left one
stuck, `lsof -i :7381` / `:7382` and kill it.

## Updating snapshots

```sh
npm run test:e2e -- --update-snapshots
```

Snapshots live under `tests/e2e/snapshots/`. Only regenerate them
deliberately — a snapshot diff is the point, not a nuisance to clear.

## Fixture bridge design

`media/flow-panel.html` is host-agnostic: when it doesn't find
`window.dlHost` (the VS Code webview injects one), it falls back to POSTing
JSON-RPC `{method:"query_sql"}` to an HTTP bridge at `?dl=<url>` (see
`scripts/dl-bridge.mjs`, which forwards to a real `dl --daemon` over a unix
socket).

`tests/fixture-bridge.mjs` speaks that exact wire shape but answers from an
in-memory table map (`TABLES` in the file) instead of forwarding anywhere.
Two `_node`/`_edge` pairs are canned:

- `rel_type_entity` / `rel_type_link` — the panel's default "Type graph
  (all)" preset, and also picked up as the builtin `type` layer
  (`BUILTIN_LAYERS` in flow-panel.html, since it doesn't follow the
  `_node`/`_edge` naming convention).
- `rel_demo_node` / `rel_demo_edge` — a plain discovered layer pair, with
  data disjoint from the type entities so a test can tell which layer's
  rows rendered after toggling a checkbox.

The `resolveQuery(sql)` export pattern-matches the handful of SQL shapes the
panel actually sends — schema discovery (`sqlite_master` scans), `PRAGMA
table_info`, and row selects against a known table (including `UNION ALL`
across tables) — and answers from the canned rows. SQL it doesn't recognize
logs one line to stderr (`[fixture-bridge] unmatched SQL: ...`) and returns
an empty row set; the panel already treats an empty result as "nothing to
draw" and a missing table as "nothing loaded yet", so this degrades quietly
rather than breaking the page. `resolveQuery` is exported standalone (no
HTTP) specifically so it has real vitest coverage
(`tests/unit/fixture-bridge.test.ts`) without a server in the loop.

This route (a) over (b) a real `dl` daemon on a fixture repo, because the
panel's query surface is small and stable enough to enumerate by hand, and a
canned map makes the whole suite hermetic — no `dl` binary on PATH, no
daemon socket, no filesystem scan, no flake from either.

`tests/panel-server.mjs` serves `media/flow-panel.html` unmodified from disk
except for one in-memory string replace: the `<!-- DL_CYTOSCAPE -->` marker
becomes a `<script src="/cytoscape.min.js">` tag, mirroring what
`extension.ts` does for the real VS Code webview. The `<!-- DL_HOST -->`
marker is left alone, so the panel's own fallback `dlHost` (pointed at the
fixture bridge via `?dl=`) is what the tests exercise — this is real
production code, not a test-only stub of the panel's host contract.

## Snapshot notes

`toHaveScreenshot` covers two DOM-only views:

- **list view** — the default view on a fresh page (no `dl-flow-view` in
  localStorage), showing the default "Type graph (all)" preset's rows.
- **trace view** — pin `build_widget` (the caller in the canned
  `rel_type_link` edge `build_widget -[uses]-> Widget`), click the trace
  proto chip; a deterministic 2-row forward slice.

The canvas/cytoscape graph view is **not** snapshotted. Cytoscape's
force-directed layout (`cy.layout(...)`) settles to slightly different node
positions from run to run even with `reducedMotion: 'reduce'` set in
`playwright.config.ts` — that's real layout nondeterminism, not a rendering
artifact `toHaveScreenshot`'s pixel diff can absorb cleanly. If a future
change needs canvas coverage, prefer asserting on `cy.nodes().length` /
`cy.edges().length` / class membership via `page.evaluate` over a raster
diff, or accept a wide `maxDiffPixelRatio` (the config already sets 0.02 as
headroom, in case a canvas snapshot gets added later) and expect occasional
manual re-baselines.

Both snapshot tests call `settleForSnapshot()` before shooting: it turns off
the "hover card" proto toggle (async content keyed on mouse position — a
source of run-to-run diff unrelated to the thing under test) and parks the
mouse in empty toolbar space. Baselines were generated, then the suite was
run twice back to back to confirm the second run passes clean with no
regeneration. No flake was hit once the hover card was excluded — it was
the first thing tried and immediately suspect (async, mouse-position-keyed,
literally designed to change based on nothing the test controls), so it
never made it into a snapshot to begin with.

## Follow-up: extracting the panel script

`media/flow-panel.html` is one file with an inline `<script>` — intentional,
host-agnostic-by-design (see the comment at the top of the file), and owned
by another arc, so this pass doesn't touch it. That means vitest can't unit
test panel internals (the collapse rollup in `applyCollapse`, `buildRows`,
`layerNodeSql`/`layerEdgeSql`'s column-binding, `computeTraceRows`'s BFS,
...) directly — only through a real page via playwright.

If/when the script is extracted into an ES module (even just the pure
functions, leaving DOM wiring inline), those functions get real vitest
coverage for free, and the same module becomes reusable outside this
extension — e.g. the `atlas-db`-style rel table reads/graph algorithms in
~/projects/instant, which currently would have to reimplement the same
column-binding and collapse-rollup logic from scratch.
