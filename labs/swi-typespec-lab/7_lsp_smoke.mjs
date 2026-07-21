import { spawn } from "node:child_process";
import { readFile } from "node:fs/promises";
import process from "node:process";

const command = process.env.SOUP_LSP ?? "swipl";
const args = process.env.SOUP_LSP ? [] : ["-q", "-s", "6_lsp.pl", "-g", "main"];
const server = spawn(command, args, {
  cwd: new URL(".", import.meta.url),
  stdio: ["pipe", "pipe", "pipe"],
});

let buffer = Buffer.alloc(0);
const messages = [];
const waiters = [];

server.stdout.on("data", chunk => {
  buffer = Buffer.concat([buffer, chunk]);
  while (true) {
    const headerEnd = buffer.indexOf("\r\n\r\n");
    if (headerEnd < 0) return;
    const header = buffer.subarray(0, headerEnd).toString("ascii");
    const length = Number(/Content-Length: (\d+)/i.exec(header)?.[1]);
    if (!Number.isFinite(length) || buffer.length < headerEnd + 4 + length) return;
    const body = buffer.subarray(headerEnd + 4, headerEnd + 4 + length);
    buffer = buffer.subarray(headerEnd + 4 + length);
    const message = JSON.parse(body.toString("utf8"));
    messages.push(message);
    for (const waiter of [...waiters]) waiter();
  }
});

let stderr = "";
server.stderr.on("data", chunk => { stderr += chunk.toString(); });

function send(message) {
  const body = Buffer.from(JSON.stringify({ jsonrpc: "2.0", ...message }));
  server.stdin.write(`Content-Length: ${body.length}\r\n\r\n`);
  server.stdin.write(body);
}

function waitFor(predicate, timeout = 3000) {
  return new Promise((resolve, reject) => {
    const inspect = () => {
      const match = messages.find(predicate);
      if (match) {
        clearTimeout(timer);
        waiters.splice(waiters.indexOf(inspect), 1);
        resolve(match);
      }
    };
    const timer = setTimeout(() => {
      waiters.splice(waiters.indexOf(inspect), 1);
      reject(new Error(`timeout\nmessages=${JSON.stringify(messages, null, 2)}\nstderr=${stderr}`));
    }, timeout);
    waiters.push(inspect);
    inspect();
  });
}

const source = await readFile(new URL("schema.soup", import.meta.url), "utf8");
const uri = "file:///swi-typespec-lab/schema.soup";

send({ id: 1, method: "initialize", params: { capabilities: {} } });
const initialize = await waitFor(message => message.id === 1);

send({ method: "initialized", params: {} });
send({ method: "textDocument/didOpen", params: { textDocument: { uri, languageId: "soup", version: 1, text: source } } });
const diagnostics = await waitFor(message => message.method === "textDocument/publishDiagnostics");

const userIdOffset = source.indexOf("UserId", source.indexOf("id: UserId"));
const prefix = source.slice(0, userIdOffset);
const lines = prefix.split("\n");
const position = { line: lines.length - 1, character: [...lines.at(-1)].reduce((n, char) => n + (char.codePointAt(0) > 0xffff ? 2 : 1), 0) };

send({ id: 2, method: "textDocument/hover", params: { textDocument: { uri }, position } });
send({ id: 3, method: "textDocument/definition", params: { textDocument: { uri }, position } });
send({ id: 4, method: "textDocument/references", params: { textDocument: { uri }, position, context: { includeDeclaration: true } } });
send({ id: 5, method: "textDocument/completion", params: { textDocument: { uri }, position, context: { triggerKind: 1 } } });
send({ id: 6, method: "textDocument/documentSymbol", params: { textDocument: { uri } } });

const [hover, definition, references, completion, symbols] = await Promise.all(
  [2, 3, 4, 5, 6].map(id => waitFor(message => message.id === id)),
);

const brokenSource = `${source}\ntype Broken { missing: Missing; }\n`;
send({
  method: "textDocument/didChange",
  params: {
    textDocument: { uri, version: 2 },
    contentChanges: [{ text: brokenSource }],
  },
});
const changedDiagnostics = await waitFor(message =>
  message.method === "textDocument/publishDiagnostics" &&
  message.params.diagnostics.some(diagnostic => diagnostic.message.includes("Undefined type")),
);

send({ id: 7, method: "shutdown", params: null });
await waitFor(message => message.id === 7);
send({ method: "exit", params: null });
server.stdin.end();

const exitCode = await new Promise(resolve => server.on("exit", resolve));
if (exitCode !== 0) throw new Error(`server exited ${exitCode}\n${stderr}`);

const report = {
  server: initialize.result.serverInfo,
  diagnostics: diagnostics.params.diagnostics,
  changedDiagnostics: changedDiagnostics.params.diagnostics,
  hover: hover.result,
  definition: definition.result,
  referenceCount: references.result.length,
  completionLabels: completion.result.map(item => item.label),
  documentSymbols: symbols.result.map(symbol => symbol.name),
};

if (report.diagnostics.length !== 0) throw new Error(JSON.stringify(report, null, 2));
if (!report.hover?.contents?.value.includes("user_id")) throw new Error(JSON.stringify(report, null, 2));
if (!report.definition?.range) throw new Error(JSON.stringify(report, null, 2));
if (report.referenceCount < 2) throw new Error(JSON.stringify(report, null, 2));
if (!report.completionLabels.includes("user")) throw new Error(JSON.stringify(report, null, 2));

console.log(JSON.stringify(report, null, 2));
