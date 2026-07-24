#!/usr/bin/env node
// M5.5 --check reader: GET /idb/diag, print one JSON line per row, exit 2 if any
// row has severity === "error" (the rail signal), exit 0 otherwise, exit 1 on any
// fetch/parse failure. Plain node, no deps.
// Usage: DL_URL=http://localhost:7171 node scripts/check_diag.mjs

const dlUrl = process.env.DL_URL ?? 'http://localhost:7171';

try {
  const response = await fetch(`${dlUrl}/idb/diag`);
  const body = await response.json();
  const rows = body.rows;
  let hasError = false;
  for (const row of rows) {
    console.log(JSON.stringify(row));
    if (row.severity === 'error') hasError = true;
  }
  process.exit(hasError ? 2 : 0);
} catch (error) {
  console.error(`check_diag: could not read ${dlUrl}/idb/diag: ${error.message}`);
  process.exit(1);
}
