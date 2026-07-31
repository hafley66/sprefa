// A SECOND fact plane, REGEX-shaped and said so: file-level TypeScript import
// edges for the packages the atlas does not cover (v6/dl/src). The atlas plane
// is symbol-level and comes from the fixed extractor; this one is file-level
// and comes from `from "..."` specifier text.
//
// The reason this is admissible: it is CROSS-GRADED. Over the overlap set (the
// tsv2 files the atlas already covers) the regex plane's file-level import
// edges are compared against the atlas's extractor-derived ones, and the
// disagreement is printed. A plane that disagrees there is not to be trusted
// where the atlas is silent.
import { readFileSync, writeFileSync, readdirSync, statSync, existsSync } from 'node:fs';
import { dirname, join, resolve, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const labDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(join(labDir, '..', '..', '..', '..'));
const outDir = join(labDir, 'out');

const ROOTS = ['v6/dl/src', 'v6/tsv2/cli', 'v6/tsv2/serve', 'v6/tsv2/runtime', 'v6/tsv2/gen_emitted'];

function walk(dir, acc) {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    // v6/tsv2/runtime/runtime is a dangling symlink; a broken link is not a file.
    let stat; try { stat = statSync(full); } catch { continue; }
    if (stat.isDirectory()) { walk(full, acc); continue; }
    if (entry.endsWith('.ts') && !entry.endsWith('.d.ts')) acc.push(full);
  }
  return acc;
}

const files = [];
for (const root of ROOTS) {
  const abs = join(repoRoot, root);
  if (existsSync(abs)) walk(abs, files);
}

const known = new Set(files.map((f) => relative(repoRoot, f)));
const SPECIFIER = /(?:from|import)\s*\(?\s*["']([^"']+)["']/g;

const edges = [];
const externals = new Map();
for (const file of files) {
  const rel = relative(repoRoot, file);
  const text = readFileSync(file, 'utf8');
  const seen = new Set();
  for (const match of text.matchAll(SPECIFIER)) {
    const spec = match[1];
    if (spec.startsWith('.')) {
      let target = relative(repoRoot, resolve(dirname(file), spec));
      if (!known.has(target) && known.has(target + '.ts')) target += '.ts';
      if (!known.has(target) && known.has(join(target, 'index.ts'))) target = join(target, 'index.ts');
      if (!known.has(target)) { externals.set(spec, (externals.get(spec) ?? 0) + 1); continue; }
      if (target === rel || seen.has(target)) continue;
      seen.add(target);
      edges.push({ from: rel, to: target });
    } else {
      externals.set(spec, (externals.get(spec) ?? 0) + 1);
    }
  }
}

edges.sort((a, b) => (a.from < b.from ? -1 : a.from > b.from ? 1 : a.to < b.to ? -1 : 1));
writeFileSync(join(outDir, 'ts_file_imports.tsv'), edges.map((e) => `${e.from}\t${e.to}`).join('\n') + '\n');

// Cross-grade against the atlas plane, over the files BOTH planes see.
const atlasNodes = readFileSync(join(outDir, 'nodes.tsv'), 'utf8').trim().split('\n')
  .map((l) => l.split('\t'));
const nodePath = new Map(atlasNodes.map(([id, , path]) => [id, path]));
const atlasFiles = new Set(atlasNodes.filter(([, plane]) => plane === 'typescript').map((r) => r[2]));

const atlasImports = new Set();
for (const line of readFileSync(join(outDir, 'edges.tsv'), 'utf8').trim().split('\n')) {
  const [from, to, kind] = line.split('\t');
  if (kind !== 'import' && kind !== 'import_unnamed') continue;
  const a = nodePath.get(from), b = nodePath.get(to);
  if (a !== b) atlasImports.add(`${a}\t${b}`);
}
const overlap = (e) => atlasFiles.has(e.from) && atlasFiles.has(e.to);
const regexImports = new Set(edges.filter(overlap).map((e) => `${e.from}\t${e.to}`));
const atlasOverlap = new Set([...atlasImports].filter((k) => {
  const [a, b] = k.split('\t');
  return atlasFiles.has(a) && atlasFiles.has(b);
}));

const onlyRegex = [...regexImports].filter((k) => !atlasOverlap.has(k)).sort();
const onlyAtlas = [...atlasOverlap].filter((k) => !regexImports.has(k)).sort();

console.log(`ts_files\t${files.length}`);
console.log(`file_import_edges\t${edges.length}`);
console.log(`external_specifiers\t${externals.size}`);
console.log(`crossgrade_overlap_atlas\t${atlasOverlap.size}`);
console.log(`crossgrade_overlap_regex\t${regexImports.size}`);
console.log(`crossgrade_only_regex\t${onlyRegex.length}`);
console.log(`crossgrade_only_atlas\t${onlyAtlas.length}`);
for (const k of onlyRegex) console.log(`  only_regex\t${k}`);
for (const k of onlyAtlas) console.log(`  only_atlas\t${k}`);
