#!/usr/bin/env node
// M5.3 slice-golden capture harness: spawns the v5 `dl` binary in
// `--lsp --diag-db <path>` mode (NO engine boot), speaks minimal LSP over
// stdio (Content-Length framing), and prints one JSON line per
// `textDocument/publishDiagnostics` notification received:
//   {"uri": ..., "diagnosticCount": N, "diagnostics": [...]}
// Plain node, no npm deps. Usage: node lsp_capture.mjs <diag-db-sqlite-path>

import { spawn } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const diagDbPath = process.argv[2];
if (!diagDbPath) {
  console.error('usage: node lsp_capture.mjs <diag-db-sqlite-path>');
  process.exit(1);
}

const dlBin = process.env.DL_BIN || path.join(scriptDir, '..', '..', '..', 'target', 'debug', 'dl');
const captureSeconds = Number(process.env.CAPTURE_SECONDS || 30);

const child = spawn(dlBin, ['--lsp', '--diag-db', diagDbPath], { stdio: ['pipe', 'pipe', 'inherit'] });

let nextId = 1;
function send(message) {
  const body = JSON.stringify(message);
  child.stdin.write(`Content-Length: ${Buffer.byteLength(body, 'utf8')}\r\n\r\n${body}`);
}
function sendRequest(method, params) {
  const id = nextId++;
  send({ jsonrpc: '2.0', id, method, params });
  return id;
}
function sendNotification(method, params) {
  send({ jsonrpc: '2.0', method, params });
}

// Content-Length-framed message parser over the child's stdout stream.
let inbox = Buffer.alloc(0);
child.stdout.on('data', (chunk) => {
  inbox = Buffer.concat([inbox, chunk]);
  for (;;) {
    const headerEnd = inbox.indexOf('\r\n\r\n');
    if (headerEnd === -1) return;
    const header = inbox.subarray(0, headerEnd).toString('utf8');
    const lengthMatch = /Content-Length: (\d+)/i.exec(header);
    if (!lengthMatch) { inbox = inbox.subarray(headerEnd + 4); continue; }
    const contentLength = Number(lengthMatch[1]);
    const bodyStart = headerEnd + 4;
    if (inbox.length < bodyStart + contentLength) return; // wait for the rest
    const body = inbox.subarray(bodyStart, bodyStart + contentLength).toString('utf8');
    inbox = inbox.subarray(bodyStart + contentLength);
    onMessage(JSON.parse(body));
  }
});

const initializeId = sendRequest('initialize', {
  processId: process.pid,
  rootUri: `file://${process.cwd()}`,
  capabilities: {},
});

function onMessage(message) {
  if (message.id === initializeId && Object.prototype.hasOwnProperty.call(message, 'result')) {
    sendNotification('initialized', {});
    return;
  }
  if (message.method === 'textDocument/publishDiagnostics') {
    const { uri, diagnostics } = message.params;
    console.log(JSON.stringify({ uri, diagnosticCount: diagnostics.length, diagnostics }));
  }
}

let shuttingDown = false;
function shutdown() {
  if (shuttingDown) return;
  shuttingDown = true;
  try { child.kill('SIGTERM'); } catch { /* already gone */ }
  process.exit(0);
}
process.on('SIGINT', shutdown);
process.on('SIGTERM', shutdown);
setTimeout(shutdown, captureSeconds * 1000);
child.on('exit', () => process.exit(0));
