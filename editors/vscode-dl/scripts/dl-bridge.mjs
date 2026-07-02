#!/usr/bin/env node
// HTTP -> dl daemon bridge, so media/flow-panel.html runs in any plain browser
// (or instant, or anything that can fetch) without VS Code. One POST /rpc per
// JSON-RPC request, forwarded Content-Length-framed over the daemon's unix
// socket at <root>/.dl/daemon.sock. Start the daemon first (`dl` in the repo,
// or let the LSP/daemon mode spawn it), then:
//
//   node scripts/dl-bridge.mjs --root ~/projects/foo [--port 7379]
//   open media/flow-panel.html?dl=http://127.0.0.1:7379
//
// CORS is wide open on purpose: this binds 127.0.0.1 and serves your own
// local code graph.
import http from "node:http";
import net from "node:net";
import path from "node:path";
import process from "node:process";

const args = process.argv.slice(2);
function flag(name, dflt) {
  const i = args.indexOf("--" + name);
  return i >= 0 ? args[i + 1] : dflt;
}
const root = path.resolve(flag("root", process.cwd()));
const port = Number(flag("port", "7379"));
const sock = path.join(root, ".dl", "daemon.sock");

/** One framed JSON-RPC exchange over a fresh unix-socket connection. */
function rpc(body) {
  return new Promise((resolve, reject) => {
    const c = net.createConnection(sock);
    let buf = Buffer.alloc(0);
    c.on("error", reject);
    c.on("connect", () => {
      c.write(`Content-Length: ${Buffer.byteLength(body)}\r\n\r\n${body}`);
    });
    c.on("data", (d) => {
      buf = Buffer.concat([buf, d]);
      const sep = buf.indexOf("\r\n\r\n");
      if (sep < 0) return;
      const header = buf.slice(0, sep).toString();
      const len = Number(/Content-Length:\s*(\d+)/i.exec(header)?.[1] ?? 0);
      if (buf.length >= sep + 4 + len) {
        c.end();
        resolve(buf.slice(sep + 4, sep + 4 + len).toString());
      }
    });
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
