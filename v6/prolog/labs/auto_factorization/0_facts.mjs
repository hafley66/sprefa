// Reads v6/DATAFLOW-ATLAS-flat-td.dot (READ ONLY) and writes the lab fact base.
//
// The .dot is the only checked-in projection carrying the full 421/809 node and
// edge set. Edge kind survives it only as far as the style string, and three
// style strings are shared by more than one kind, so the recovered kind is a
// CLASS. The classes that collapse are named in kindClass below.
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const labDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = join(labDir, '..', '..', '..', '..');
const dotPath = join(repoRoot, 'v6', 'DATAFLOW-ATLAS-flat-td.dot');
const outDir = join(labDir, 'out');

const STYLE_TO_CLASS = new Map([
  ['color="#93c5fd",arrowhead=none', 'resides'],
  ['color="#15803d"', 'calls'],
  ['color="#1d4ed8"', 'import'],
  ['color="#1d4ed8",style=dashed', 'import_unnamed'],
  ['color="#be123c"', 'statement'],
  ['color="#5eead4",arrowhead=none', 'annotation'],
  ['color="#b45309",penwidth=3', 'bridge'],
]);

// node id -> (plane, container path, symbol). Four shapes exist in the atlas:
//   ts:PATH#SYM   pl:PATH#SYM   sh:PATH   cli:NAME   ex:...   sql:TABLE
function splitNode(id) {
  const colon = id.indexOf(':');
  const prefix = id.slice(0, colon);
  const rest = id.slice(colon + 1);
  const hash = rest.indexOf('#');
  const plane = { ts: 'typescript', pl: 'prolog', sh: 'shell', cli: 'cli', ex: 'extractor', sql: 'sqlite' }[prefix];
  if (plane === undefined) throw new Error(`unknown node prefix: ${id}`);
  if (hash >= 0) return { plane, path: rest.slice(0, hash), symbol: rest.slice(hash + 1) };
  if (plane === 'shell') return { plane, path: rest, symbol: 'script' };
  return { plane, path: `<${plane}>`, symbol: rest };
}

const text = readFileSync(dotPath, 'utf8');
const nodes = new Map();
const edges = [];
for (const line of text.split('\n')) {
  const edgeMatch = /^\s*"([^"]+)" -> "([^"]+)" \[(.+)\];$/.exec(line);
  if (edgeMatch) {
    const kind = STYLE_TO_CLASS.get(edgeMatch[3]);
    if (kind === undefined) throw new Error(`unknown edge style: ${edgeMatch[3]}`);
    edges.push({ from: edgeMatch[1], to: edgeMatch[2], kind });
    continue;
  }
  const nodeMatch = /^\s{4}"([^"]+)" \[label=/.exec(line);
  if (nodeMatch) nodes.set(nodeMatch[1], splitNode(nodeMatch[1]));
}

for (const { from, to } of edges) {
  if (!nodes.has(from)) throw new Error(`edge endpoint absent from node set: ${from}`);
  if (!nodes.has(to)) throw new Error(`edge endpoint absent from node set: ${to}`);
}

mkdirSync(outDir, { recursive: true });
const nodeRows = [...nodes.entries()].sort((a, b) => (a[0] < b[0] ? -1 : 1));
writeFileSync(join(outDir, 'nodes.tsv'),
  nodeRows.map(([id, n]) => `${id}\t${n.plane}\t${n.path}\t${n.symbol}`).join('\n') + '\n');

const edgeRows = [...edges].sort((a, b) =>
  a.from < b.from ? -1 : a.from > b.from ? 1 : a.to < b.to ? -1 : a.to > b.to ? 1 : a.kind < b.kind ? -1 : 1);
writeFileSync(join(outDir, 'edges.tsv'),
  edgeRows.map((e) => `${e.from}\t${e.to}\t${e.kind}`).join('\t\n').replaceAll('\t\n', '\n') + '\n');

const perPlane = new Map();
for (const n of nodes.values()) perPlane.set(n.plane, (perPlane.get(n.plane) ?? 0) + 1);
const perKind = new Map();
for (const e of edges) perKind.set(e.kind, (perKind.get(e.kind) ?? 0) + 1);

console.log(`nodes\t${nodes.size}`);
console.log(`edges\t${edges.length}`);
for (const [k, v] of [...perPlane].sort()) console.log(`plane\t${k}\t${v}`);
for (const [k, v] of [...perKind].sort()) console.log(`kind\t${k}\t${v}`);
