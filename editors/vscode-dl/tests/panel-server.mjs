#!/usr/bin/env node
// Static host for the e2e suite: serves media/flow-panel.html with the
// DL_CYTOSCAPE marker filled in the same way extension.ts fills it for the
// VS Code webview (a <script src> tag pointing at the vendored
// cytoscape.min.js). This is an in-memory transform of the response bytes --
// media/flow-panel.html on disk is never modified. The DL_HOST marker is
// left untouched, so the panel's own browser-bridge dlHost fallback kicks in
// (media/flow-panel.html's `if (!window.dlHost)` branch), pointed at the
// fixture bridge via the page's `?dl=<url>` query param.
import http from "node:http";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const mediaDir = path.join(here, "..", "media");
const panelPath = path.join(mediaDir, "flow-panel.html");
const cytoscapePath = path.join(mediaDir, "cytoscape.min.js");

export function renderPanel() {
  const html = fs.readFileSync(panelPath, "utf8");
  return html.replace("<!-- DL_CYTOSCAPE -->", '<script src="/cytoscape.min.js"></script>');
}

export function createServer() {
  return http.createServer((req, res) => {
    const url = (req.url || "/").split("?")[0];
    if (url === "/" || url === "/flow-panel.html") {
      res.writeHead(200, { "content-type": "text/html; charset=utf-8" }).end(renderPanel());
      return;
    }
    if (url === "/cytoscape.min.js") {
      res.writeHead(200, { "content-type": "application/javascript" })
        .end(fs.readFileSync(cytoscapePath));
      return;
    }
    res.writeHead(404).end();
  });
}

// CLI entry: `node tests/panel-server.mjs [--port 7382]`
if (import.meta.url === `file://${process.argv[1]}`) {
  const args = process.argv.slice(2);
  const portIdx = args.indexOf("--port");
  const port = Number(portIdx >= 0 ? args[portIdx + 1] : 7382);
  const server = createServer();
  server.listen(port, "127.0.0.1", () => {
    console.log(`panel-server: http://127.0.0.1:${port}/flow-panel.html`);
  });
}
