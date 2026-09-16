// test-panel-rollup.mjs — unit test for the BOM subtree rollup in
// applyCollapse (media/flow-panel.html). The renderer pure functions live
// inline in the HTML; this harness extracts applyCollapse + its helper
// dirsWithFilteredContent by brace-matching and exercises the rollup
// accumulator on hand-built rows, no DOM. Run: node scripts/test-panel-rollup.mjs
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const html = readFileSync(join(here, '..', 'media', 'flow-panel.html'), 'utf8');

// Pull a `function NAME(...) { ... }` out of the source by matching braces from
// the first `{` after the signature.
function extractFn(src, name) {
  const at = src.indexOf('function ' + name + '(');
  if (at < 0) throw new Error('function not found: ' + name);
  const open = src.indexOf('{', at);
  let depth = 0;
  for (let i = open; i < src.length; i++) {
    const c = src[i];
    if (c === '{') depth++;
    else if (c === '}') { depth--; if (depth === 0) return src.slice(at, i + 1); }
  }
  throw new Error('unbalanced braces for ' + name);
}

const source = extractFn(html, 'dirsWithFilteredContent') + '\n'
  + extractFn(html, 'applyCollapse') + '\n'
  + 'return { applyCollapse, dirsWithFilteredContent };';
// eslint-disable-next-line no-new-func
const { applyCollapse } = new Function(source)();

let failures = 0;
function eq(actual, expected, msg) {
  const a = JSON.stringify(actual), e = JSON.stringify(expected);
  if (a === e) { console.log('  ok   ' + msg); return; }
  failures++;
  console.log('  FAIL ' + msg + '\n       expected ' + e + '\n       actual   ' + a);
}

// A small assembly: one file dir `f`, one owner `A` (2 members), members m1/m2.
// nums = [members, fanIn, fanOut, weight].
const N = (id, depth, nums, hasChildren) =>
  ({ type: 'node', depth, id, key: id, kind: 'x', label: id, nums, hasChildren });
const D = (key, depth) => ({ type: 'dir', depth, key, label: key, hasChildren: true });

const rows = [
  D('f', 0),
  N('A', 1, [2, 5, 1, 10], true),
  N('A::m1', 2, [0, 3, 0, 4], false),
  N('A::m2', 2, [0, 1, 2, 4], false),
];

console.log('rollup: no collapse -> empty map');
{
  const { rollup, visibleRows } = applyCollapse(rows, new Set(), [], null, null);
  eq(rollup.size, 0, 'nothing folded, nothing rolled up');
  eq(visibleRows.length, 4, 'all four rows visible');
}

console.log('rollup: collapse owner A -> members fold onto A');
{
  const { rollup, visibleRows } = applyCollapse(rows, new Set(['A']), [], null, null);
  // visible: [f (idx0), A (idx1)]; m1/m2 fold under A (visible index 1).
  eq(visibleRows.map(r => r.id ?? r.key), ['f', 'A'], 'A collapsed, members hidden');
  eq(rollup.get(1), [0, 4, 2], 'A rollup = m1+m2 [members, fanIn, fanOut]');
  eq(rollup.has(0), false, 'the file dir has no folded descendants of its own');
}

console.log('rollup: collapse the file dir f -> whole subtree folds onto f');
{
  const { rollup, visibleRows } = applyCollapse(rows, new Set(['f']), [], null, null);
  eq(visibleRows.map(r => r.id ?? r.key), ['f'], 'only the dir row is visible');
  // A + m1 + m2 = [2+0+0, 5+3+1, 1+0+2] = [2, 9, 3]
  eq(rollup.get(0), [2, 9, 3], 'f rollup = A+m1+m2 subtree total');
}

console.log('rollup: rows without a numeric band never roll up');
{
  const plain = [
    D('f', 0),
    { type: 'node', depth: 1, id: 'A', key: 'A', kind: 'x', label: 'A', hasChildren: true },
    { type: 'node', depth: 2, id: 'A::m', key: 'A::m', kind: 'x', label: 'A::m' },
  ];
  const { rollup } = applyCollapse(plain, new Set(['A']), [], null, null);
  eq(rollup.size, 0, 'no nums -> rollup stays empty (non-bom presets)');
}

if (failures) { console.log('\n' + failures + ' failure(s)'); process.exit(1); }
console.log('\nall rollup tests passed');
