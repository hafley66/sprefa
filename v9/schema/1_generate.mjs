// Compiles 0_wire.tsp with the pinned emitters into a scratch dir, then writes (or with --check
// compares) the generated Rust under ../src/_9_wire/ and openapi.yaml here.
// src/_9_wire/ops.rs and src/_9_wire/mod.rs are hand-owned and not routed.
import { compile, NodeHost } from '@typespec/compiler';
import { cpSync, existsSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const args = process.argv.slice(2);
if (args.some(arg => arg !== '--check')) throw Error('usage: node 1_generate.mjs [--check]');
const check = args.includes('--check');
const here = dirname(fileURLToPath(import.meta.url));
const crate = join(here, '..');
const scratch = mkdtempSync(join(tmpdir(), 'v9-wire-'));

// scratch-relative output -> repo-relative target
const targets = [
  ['rust/src/_9_wire/models', 'src/_9_wire/models'],
  ['rust/src/_9_wire/ops_auto.rs', 'src/_9_wire/ops_auto.rs'],
  ['rust/src/_9_wire/http_auto.rs', 'src/_9_wire/http_auto.rs'],
  ['openapi/openapi.yaml', 'schema/openapi.yaml'],
];

try {
  const program = await compile(NodeHost, join(here, '0_wire.tsp'), {
    outputDir: scratch,
    emit: ['@hafley66/alloy-rs', '@typespec/openapi3'],
    options: {
      '@hafley66/alloy-rs': {
        'emitter-output-dir': join(scratch, 'rust'),
        routes: [
          { from: 'models', to: 'src/_9_wire/models' },
          { from: 'ops_auto.rs', to: 'src/_9_wire/ops_auto.rs' },
          { from: 'http_auto.rs', to: 'src/_9_wire/http_auto.rs' },
        ],
      },
      '@typespec/openapi3': { 'emitter-output-dir': join(scratch, 'openapi') },
    },
  });
  const errors = program.diagnostics.filter(d => d.severity === 'error');
  if (errors.length) throw Error(errors.map(d => `${d.code}: ${d.message}`).join('\n'));

  const files = root => statSync(root).isDirectory()
    ? readdirSync(root).sort().flatMap(name => files(join(root, name)))
    : [root];
  const stale = [];
  for (const [from, to] of targets) {
    const src = join(scratch, from);
    const dst = join(crate, to);
    const want = files(src).map(f => relative(src, f));
    const have = existsSync(dst) ? files(dst).map(f => relative(dst, f)) : [];
    for (const name of new Set([...want, ...have])) {
      const a = want.includes(name) ? readFileSync(join(src, name), 'utf8') : null;
      const b = have.includes(name) ? readFileSync(join(dst, name), 'utf8') : null;
      if (a !== b) stale.push(join(to, name));
    }
    if (!check) {
      rmSync(dst, { recursive: true, force: true });
      cpSync(src, dst, { recursive: true });
    }
  }
  if (check && stale.length) {
    throw Error(`stale generated files (run node schema/1_generate.mjs):\n  ${stale.join('\n  ')}`);
  }
  console.log(`V9_WIRE_OK ${check ? 'checked' : 'written'} stale=${stale.length}`);
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
