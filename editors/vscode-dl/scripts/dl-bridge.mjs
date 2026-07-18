#!/usr/bin/env node
// HTTP -> dl daemon bridge, so media/flow-panel.html runs in any plain browser
// (or instant, or anything that can fetch) without VS Code. One POST /rpc per
// JSON-RPC request, forwarded to the singleton daemon's unix socket — which
// carries HTTP itself since the axum adoption arc, so the forward is a plain
// http.request over `socketPath`. Start the daemon first (`dl daemon start`),
// then:
//
//   node scripts/dl-bridge.mjs [--sock /path/to/daemon.sock] [--port 7379]
//   open media/flow-panel.html?dl=http://127.0.0.1:7379
//
// The default socket is the singleton home's (`$XDG_STATE_HOME/sprefa/
// daemon.sock`, falling back to `~/.local/state/sprefa/daemon.sock`). Name the
// target repo per-request via the JSON-RPC `params.root` envelope, the same
// way every dl client does.
//
// CORS is wide open on purpose: this binds 127.0.0.1 and serves your own
// local code graph.
import http from "node:http";
import os from "node:os";
import path from "node:path";
import process from "node:process";

const args = process.argv.slice(2);
function flag(name, dflt) {
  const i = args.indexOf("--" + name);
  return i >= 0 ? args[i + 1] : dflt;
}
const stateHome = process.env.XDG_STATE_HOME
  ? process.env.XDG_STATE_HOME
  : path.join(os.homedir(), ".local", "state");
const sock = flag("sock", path.join(stateHome, "sprefa", "daemon.sock"));
const port = Number(flag("port", "7379"));

/** One JSON-RPC exchange: POST /rpc over the daemon's unix socket (HTTP). */
function rpc(body) {
  return new Promise((resolve, reject) => {
    const req = http.request(
      {
        socketPath: sock,
        path: "/rpc",
        method: "POST",
        headers: { "content-type": "application/json" },
      },
      (res) => {
        let out = "";
        res.on("data", (d) => (out += d));
        res.on("end", () => resolve(out));
      },
    );
    req.on("error", reject);
    req.end(body);
  });
}

const server = http.createServer(async (req, res) => {
  res.setHeader("access-control-allow-origin", "*");
  res.setHeader("access-control-allow-headers", "content-type");
  if (req.method === "OPTIONS") { res.writeHead(204).end(); return; }
  if (req.method !== "POST" || req.url !== "/rpc") { res.writeHead(404).end(); return; }
  let body = "";
  req.on("data", (d) => (body += d));
  req.on("end", async () => {
    try {
      const out = await rpc(body);
      res.writeHead(200, { "content-type": "application/json" }).end(out);
    } catch (e) {
      res.writeHead(502, { "content-type": "application/json" }).end(JSON.stringify({
        jsonrpc: "2.0", id: null,
        error: { code: -32000, message: `daemon unreachable at ${sock}: ${e.message}` },
      }));
    }
  });
});
server.listen(port, "127.0.0.1", () => {
  console.log(`dl-bridge: http://127.0.0.1:${port}/rpc -> ${sock}`);
});
