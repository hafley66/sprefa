# rtkq-golden-green (size:small, EXPLICIT FIX)

FIRST ACTION: `git merge --ff-only 046cbc510804671d2441aca36536bbd310eef485`. Failure = STOP AND REPORT.
Read CLAUDE.md at repo root.

BUG (already root-caused, .github/CI-KNOWN-RED.md:29): `just rtkq-golden` fails
at v6/tsv2/labs/1_rtkq-extraction-golden.ts:200-area — the `/idb/api_endpoint`
assertion expects `listUsers` before `updateUser`, the served rows arrive
`updateUser` first. Spans identical, not a corpus move. dl rel rows are SETS;
an order-sensitive `assert.deepEqual` on `rows` is the defect. The allowlist
entry is CI-KNOWN-RED.md:29 (table row) + :48 (`allow: rtkq-golden`).

THE FIX (exact, nothing else):
1. In 1_rtkq-extraction-golden.ts add ONE local helper:
   `const sorted = (rows: unknown[][]) => [...rows].sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)));`
2. At EVERY multi-row `/idb/...` rows assertion in this file, compare
   `sorted(actual.rows)` against `sorted([...expected])`. Single-row
   assertions stay as they are.
3. Remove the rtkq-golden row from .github/CI-KNOWN-RED.md (both the table row
   and the `allow: rtkq-golden` line).

FILES YOU OWN: v6/tsv2/labs/1_rtkq-extraction-golden.ts,
.github/CI-KNOWN-RED.md (those two lines only).
FORBIDDEN: everything else, especially the extractor, runtime, and emitters —
row emit order is not contractual and you do NOT chase it.

VALIDATION: `cd v6 && just rtkq-golden` green TWICE, outputs pasted. Then run
it a third time; three green runs total (this golden was flaky by order, so
three).

COMMIT plain, COMMENT_RAIL_IDLE_MS=3000, never pipe a commit, commit ONLY in
your worktree (`pwd` before every git commit).
Report: three green receipts, the two allowlist lines removed.
