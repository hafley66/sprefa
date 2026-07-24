#!/usr/bin/env node
/**
 * scripts/ingest_corpus.mjs — scaled-corpus ingest driver (M9-before measurement
 * harness, EngineStorageLaw pin, tasks.d.ts).
 *
 * Usage: node --experimental-transform-types scripts/ingest_corpus.mjs <corpus-dir> <db-path>
 * (The --experimental-transform-types flag is required because this file imports the
 * package's own .ts sources directly across the src/ boundary; package.json's "test"
 * and "serve" scripts carry the identical flag for the same reason.)
 *
 * Boots ONE DlRuntime in-process (no http, no daemon) against fixtures/sg-rail.dl,
 * bridged with the real builtin decls (5_diag.ts's spineDecls/diagDecl — the same
 * source of truth src/6_http.ts boots against, not this package's local
 * spineDeclsLocal test-only builder). Then walks <corpus-dir> recursively for every
 * *.ts file (nothing is skipped, INCLUDING node_modules — the corpus itself may live
 * inside a node_modules directory, as the M9-before rxjs baseline does) and calls
 * ingestFile once per file.
 *
 * Deliberately NO HostRunner is constructed here (src/1_hosts.ts's HostRunner is a
 * separate class 6_http.ts instantiates and .start()s explicitly — DlRuntime.boot
 * alone never creates or runs one). sg-rail.dl's `sh sg(...)` host effect therefore
 * never fires: this harness measures ingest's spine-rel storage (file/node/edge/sig/
 * site/const/span_line), not host-effect cost. Wiring a HostRunner here would spawn
 * one ast-grep subprocess per demand row — hundreds to thousands of processes for a
 * corpus this size — which is not what M9-before measures and would dominate the
 * wall clock with unrelated cost.
 *
 * A file whose extract exits nonzero (ingestFile rejects) is logged and skipped, not
 * fatal: some .ts files in the wild (declaration-only stubs, unusual encodings) are
 * genuine edge cases for the extract binary, and the run must finish regardless.
 */
import fs from "node:fs";
import path from "node:path";

import { bridge } from "../src/0_ast_bridge.ts";
import { DlRuntime } from "../src/3_runtime.ts";
import { builtinDecls, DIAG_V5_VIEW_SQL } from "../src/5_diag.ts";
import { ingestFile } from "../src/4_ingest.ts";

const PROGRESS_EVERY = 50;

/** Recursive *.ts walk. Skips nothing (not even node_modules/.git) by design — see
 *  file header: the corpus may itself live inside a node_modules tree. */
function walkTsFiles(rootDir) {
  const out = [];
  const stack = [rootDir];
  while (stack.length > 0) {
    const dir = stack.pop();
    let entries;
    try {
      entries = fs.readdirSync(dir, { withFileTypes: true });
    } catch (err) {
      console.error(`[ingest_corpus] cannot read directory ${dir}: ${err instanceof Error ? err.message : String(err)}`);
      continue;
    }
    for (const entry of entries) {
      const fullPath = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        stack.push(fullPath);
      } else if (entry.isFile() && fullPath.endsWith(".ts")) {
        out.push(fullPath);
      }
    }
  }
  out.sort(); // deterministic ingest order run-to-run, independent of readdir order.
  return out;
}

function percentile(sortedAscending, p) {
  if (sortedAscending.length === 0) return 0;
  const idx = Math.min(sortedAscending.length - 1, Math.floor((p / 100) * sortedAscending.length));
  return sortedAscending[idx];
}

function round2(n) {
  return Math.round(n * 100) / 100;
}

async function main() {
  const [, , corpusDir, dbPath] = process.argv;
  if (!corpusDir || !dbPath) {
    console.error("usage: node --experimental-transform-types scripts/ingest_corpus.mjs <corpus-dir> <db-path>");
    process.exit(1);
  }

  const fixturePath = path.join(import.meta.dirname, "..", "fixtures", "sg-rail.dl");
  const dlText = fs.readFileSync(fixturePath, "utf8");
  const bridgeResult = bridge(dlText, builtinDecls);
  if (bridgeResult.kind === "err") {
    console.error("[ingest_corpus] bridge failed on fixtures/sg-rail.dl:", JSON.stringify(bridgeResult.diags, null, 2));
    process.exit(1);
  }

  const rt = await DlRuntime.boot({ dbPath, bridge: bridgeResult, extraDdl: [DIAG_V5_VIEW_SQL] });

  const files = walkTsFiles(corpusDir);
  console.log(`[ingest_corpus] found ${files.length} .ts files under ${corpusDir}`);

  const perFileMs = [];
  let failedCount = 0;
  const startAll = performance.now();

  for (let i = 0; i < files.length; i++) {
    const filePath = files[i];
    const start = performance.now();
    try {
      await ingestFile(rt, filePath);
      perFileMs.push(performance.now() - start);
    } catch (err) {
      failedCount += 1;
      console.error(`[ingest_corpus] skipping (extract failed) ${filePath}: ${err instanceof Error ? err.message : String(err)}`);
    }

    if ((i + 1) % PROGRESS_EVERY === 0) {
      console.log(`[ingest_corpus] ${i + 1}/${files.length} files processed (${failedCount} failed so far)`);
    }
  }

  const totalMs = performance.now() - startAll;
  await rt.dispose();

  const sortedMs = [...perFileMs].sort((a, b) => a - b);
  const summary = {
    files: files.length,
    ingested: perFileMs.length,
    failed: failedCount,
    total_ms: round2(totalMs),
    ms_per_file_p50: round2(percentile(sortedMs, 50)),
    ms_per_file_p95: round2(percentile(sortedMs, 95)),
  };
  console.log(JSON.stringify(summary));
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
