#!/usr/bin/env node
// Hermetic stand-in for scripts/dl-bridge.mjs. Serves the same POST /rpc
// JSON-RPC {method:"query_sql"} shape the panel's fallback dlHost speaks
// (media/flow-panel.html, the `if (!window.dlHost)` branch), answered from a
// canned in-memory table map instead of forwarding to a live dl daemon over
// a unix socket. This is what lets the e2e suite drive flow-panel.html with
// no dl binary and no daemon running.
//
//   node tests/fixture-bridge.mjs --port 7381
//   open .../media/flow-panel.html?dl=http://127.0.0.1:7381
//
// The resolveQuery() matcher below is exported standalone (no HTTP) so
// vitest can exercise the SQL pattern matching without spinning up a server.
import http from "node:http";

// ── canned schema ────────────────────────────────────────────────────────
// Two _node/_edge pairs:
//  - rel_type_entity / rel_type_link: the panel's default "typeAll" preset
//    (DEFAULT_NODES/DEFAULT_EDGES), also picked up as the builtin "type"
//    layer (BUILTIN_LAYERS in flow-panel.html).
//  - rel_demo_node / rel_demo_edge: a plain discovered _node/_edge pair with
//    data distinct from the type entities, so a test can tell which layer
//    rendered after toggling a checkbox.
const TABLES = {
  rel_type_entity: {
    cols: ["sym", "name", "kind", "file", "line"],
    rows: [
      ["demo::src/widget.rs::struct::Widget", "Widget", "struct", "src/widget.rs", 10],
      ["demo::src/widget.rs::fn::build_widget", "build_widget", "fn", "src/widget.rs", 24],
    ],
  },
  rel_type_link: {
    cols: ["src", "dst", "kind"],
    rows: [
      ["demo::src/widget.rs::fn::build_widget", "demo::src/widget.rs::struct::Widget", "uses"],
    ],
  },
  rel_demo_node: {
    cols: ["id", "label", "kind", "file", "line"],
    rows: [
      ["demo-a", "Demo Node A", "widget", "src/demo.rs", 5],
      ["demo-b", "Demo Node B", "widget", "src/demo.rs", 12],
    ],
  },
  rel_demo_edge: {
    cols: ["src", "dst", "kind"],
    rows: [["demo-a", "demo-b", "flow"]],
  },
};

const NODE_TABLES = Object.keys(TABLES).filter((t) => t.endsWith("_node"));

function pragmaTableInfo(table) {
  const def = TABLES[table];
  if (!def) return [];
  // sqlite PRAGMA table_info columns: cid, name, type, notnull, dflt_value, pk
  return def.cols.map((name, i) => [i, name, "TEXT", 0, null, 0]);
}

function fullRows(table) {
  const def = TABLES[table];
  return def ? def.rows.map((r) => r.slice()) : [];
}

/**
 * Pattern-match a SQL string against the shapes flow-panel.html actually
 * sends (schema discovery, PRAGMA introspection, preset/layer row queries)
 * and answer from the canned TABLES map above. Unknown SQL logs to stderr
 * and returns an empty row set rather than throwing -- the panel already
 * treats a missing table as "nothing loaded yet" and an empty result as
 * "nothing to draw", both fine defaults for SQL this fixture doesn't model
 * (e.g. rel_op_reach_fn, queried best-effort for the hover-card gateways
 * line).
 */
export function resolveQuery(sql) {
  const s = String(sql || "").trim();

  // PRAGMA table_info(table) -- check first, "table_info(rel_type_entity)"
  // would otherwise also satisfy the generic FROM-table fallback below.
  const pragma = /PRAGMA\s+table_info\(\s*"?([a-z0-9_]+)"?\s*\)/i.exec(s);
  if (pragma) return pragmaTableInfo(pragma[1]);

  if (/FROM\s+sqlite_master/i.test(s)) {
    // layer discovery's first pass: rel_*_node tables only
    if (/LIKE\s*'rel_%_node'/i.test(s)) return NODE_TABLES.map((t) => [t]);
    // refreshExistingRels(): every rel_ table
    if (/LIKE\s*'rel_%'/i.test(s)) return Object.keys(TABLES).map((t) => [t]);
    // builtin-layer existence count: name IN ('a','b')
    const nameIn = /name\s+IN\s*\(([^)]+)\)/i.exec(s);
    if (nameIn) {
      const wanted = nameIn[1].split(",").map((t) => t.trim().replace(/'/g, ""));
      return [[wanted.filter((t) => TABLES[t]).length]];
    }
    // edge-table existence check: name='rel_x_edge'
    const nameEq = /name\s*=\s*'([^']+)'/i.exec(s);
    if (nameEq) return TABLES[nameEq[1]] ? [[nameEq[1]]] : [];
    return [];
  }

  // row queries: union every table this SQL reads FROM, in appearance order
  // (covers both a single-table preset and a UNION ALL across layers).
  const tables = [...s.matchAll(/FROM\s+"?([a-z0-9_]+)"?/gi)]
    .map((m) => m[1])
    .filter((t) => TABLES[t]);
  if (tables.length) return tables.flatMap(fullRows);

  process.stderr.write(`[fixture-bridge] unmatched SQL: ${s}\n`);
  return [];
}

export function createServer() {
  return http.createServer((req, res) => {
    res.setHeader("access-control-allow-origin", "*");
    res.setHeader("access-control-allow-headers", "content-type");
    if (req.method === "OPTIONS") { res.writeHead(204).end(); return; }
    if (req.method !== "POST" || req.url !== "/rpc") { res.writeHead(404).end(); return; }
    let body = "";
    req.on("data", (d) => (body += d));
    req.on("end", () => {
      let msg;
      try { msg = JSON.parse(body); } catch (e) { res.writeHead(400).end(); return; }
      const sql = msg?.params?.sql ?? "";
      const rows = resolveQuery(sql);
      res.writeHead(200, { "content-type": "application/json" }).end(JSON.stringify({
        jsonrpc: "2.0", id: msg.id ?? null,
        result: { rows },
      }));
    });
  });
}

// CLI entry: `node tests/fixture-bridge.mjs [--port 7381]`
if (import.meta.url === `file://${process.argv[1]}`) {
  const args = process.argv.slice(2);
  const portIdx = args.indexOf("--port");
  const port = Number(portIdx >= 0 ? args[portIdx + 1] : 7381);
  const server = createServer();
  server.listen(port, "127.0.0.1", () => {
    console.log(`fixture-bridge: http://127.0.0.1:${port}/rpc`);
  });
}
