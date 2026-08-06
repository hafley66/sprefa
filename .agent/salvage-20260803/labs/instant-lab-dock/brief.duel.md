# Lane: harness-trace panel lab (instant)

Worktree: /Users/chrishafley/projects/instant-lab-trace-fable, branch lab/harness-trace-fable,
base 0e4e01734fd983f157dfbc49b2454e803aa4557b. FIRST action:
`git merge --ff-only 0e4e0173` — failure = STOP, write REPORT.md saying so.
SECOND action: `corepack pnpm@10.12.4 install --prefer-offline`. If it errors or
hangs past 5 minutes, STOP and report; do not try alternative install mechanisms.

Read CONTRACT.md at the worktree root. It is the spec: row model, filter rule,
data sources, repo laws, gates. Implement exactly that. If reality deviates from
the contract or this brief, STOP and report the deviation in REPORT.md; do not
improvise. Do NOT commit. Do NOT write outside this worktree. NEVER run
`just dev` (the owner's live instance is running; `just dev-safe` is the only
sanctioned live run).

## Build order

1. Rust: extend src-tauri/src/harness.rs with one command harness_trace_rows
   returning structured rows (serde struct mirroring HarnessTraceRow minus
   from/why), reusing the four existing readers' path logic. Register in
   ipc/commands.json, regenerate (corepack pnpm@10.12.4 run api:generate).
   Follow the file's existing error style (no unwrap on IO paths).
2. Claude-subagent filter: inspect real files under ~/.claude/projects/ (read
   only) to find the subagent/sidechain marker; implement the filter in the
   rust reader; record the marker evidence (file path + the line that proves
   it) in REPORT.md. No reliable marker = leave sessions unfiltered, STOP that
   sub-goal, report what was tried.
3. Frontend: src/plugins/harnessTrace/index.ts (registerPlugin) +
   src/plugins/harnessTrace/HarnessTracePanel.tsx (TreeTable panel, column defs
   copying TMUX_COLUMNS shape at src/tablepanels.tsx:56, bridge-free: the
   panel may invoke the generated command directly like the cass plugin does).
   One line added in src/main.ts beside registerCassPlugin().
4. Mail-ledger join (frontend or rust, your call, state which and why in one
   sentence): parse ~/.agent/mail/*.ndjson if present; join envelopes to rows
   (to -> session via registry.json when present); enrich from/why. Missing
   dir = zero enrichment, zero errors.
5. fs-watch leg: claimFsWatch on ~/.agent/mail when the dir exists; refresh
   rows on event. Release the claim on panel dispose.
6. Gates: `just check`, `just build`, `just cargo-check` (+ `just test` if
   touched code has coverage). Full receipts (command, exit code, tail of
   output) in REPORT.md.

## Sabotage receipts (mandatory, like every lab)

- S1: temporarily rename one harness session dir (e.g. point the reader at a
  nonexistent HOME via env in a unit test, not the real HOME) and show the
  panel data path yields an empty list for that harness, not a crash.
- S2: feed one malformed NDJSON line into a temp mail file used by a unit
  test; show the parser skips it and keeps the rest. Real ~/.agent/mail is
  read-only for you.

## REPORT.md required sections

1. Base + install receipts. 2. What was built, file list with line counts.
3. Subagent-marker evidence (or the STOP record). 4. Gate receipts.
5. Sabotage receipts. 6. Deviations from CONTRACT.md (empty section if none).

Style: repo laws in CONTRACT.md bind. Comments state only constraints the code
cannot show. No em dashes. Never the words provenance, substrate, load-bearing,
regime. Descriptive variable names.
