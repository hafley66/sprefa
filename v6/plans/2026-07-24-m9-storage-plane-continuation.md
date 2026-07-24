# M9 storage-plane continuation — codex brief (2026-07-24)

Base: branch `codex/m9-storage-plane` (= dl/m9-core 7121c10b + dl/m9-before merged:
columnTypes flow landed, temp-join row matching landed, depth-ceiling regression green).
Everything below was specified by the M9-core session; execute mechanically. Where this
file and the code disagree, re-find by symbol name — line numbers go stale.

## Goal

Rewrite the dl runtime's fact-table plane from string-keyed rowid tables to
interned, typed, WITHOUT ROWID tables on the store's spine, with read views
keeping the http surface byte-identical. Steps 3-6 of the M9-core plan.

## The design (execute exactly)

1. **Base table per rel**: `relbase_<name>` minted via the store's
   `rels.create_rel_table` (v6/sprefa-store/js/src/engine/spine.ts, symbol
   `create_rel_table`): every column INTEGER affinity (int col = raw value;
   text col = `strings` dictionary id via `Store.intern`, or `-1` as the NULL
   sentinel — real ids are dense >= 0 so -1 is collision-free). PK = all
   columns, WITHOUT ROWID. Numeric NULLs are out of scope: guard with a thrown
   error + comment.
2. **Read view per rel**: `rel_<name>` — int columns passthrough; text columns
   `CASE WHEN c = -1 THEN NULL ELSE (SELECT content FROM strings WHERE string_id = c) END AS c`.
   `selectAll`/`selectAllTuples`, `/query`, and `diag_v5` read the VIEW so the
   surface stays text and byte-identical.
3. **Commit pipeline id-space** (v6/dl/src/3_runtime.ts): intern text -> id and
   `flush_strings` BEFORE `with_txn` (flush uses `executeMultiple`, which rolls
   back an open BEGIN — see the engine.ts header; strings are monotonic so a
   pre-txn flush is safe under rollback). Existence pre-check and retract go
   through the temp-table set-matching already landed (symbol
   `preCheckExistingKeys` / the `_row_match_candidates` helper) operating in id
   space. `rowDigest` stays computed over SURFACE values (relativized path,
   resolved text), not ids.
4. **Store attach in boot**: `await Store.open(db)` held in `RuntimeState`;
   source subjects seed from the views.
5. **Root-relative paths** (v6/dl/src/4_ingest.ts): `DL_ROOT` = server cwd
   (curl-session.sh runs the server from v6/dl). Read file bytes with the
   absolute path; emit rows with `path.relative(DL_ROOT, abs)`. `/idb` and
   `/query` handlers (v6/dl/src/6_http.ts) resolve back with
   `path.resolve(DL_ROOT, rel)` so http output is byte-identical after the
   existing TMPPATH normalization. `diag_v5` stays relative (the LSP wants it).
6. **Integer keys everywhere** (owner law: zero string foreign keys):
   `delta.rel TEXT` -> `rel_id INTEGER` (update the transcript script's
   `SELECT rel, ...` to resolve rel_id -> name so `__resp_sg|2|1` lines stay
   byte-identical); effect_cache digests become INTEGER with the host name
   folded into the digest (symbols `effectDigest`, DDL in 2_schema.ts, tests
   in tests/4_hosts.test.ts).
7. **Test plumbing**: `fakeBridgeOk` callers (tests/3_runtime.test.ts,
   tests/7_churn_stress.test.ts) pass real `columnTypeOverrides` for numeric
   columns.

## Hard laws

- Work ONLY inside this worktree. Do not touch v6/sprefa-store/js/src/** (the
  store is reused, never edited; if a store seam is genuinely missing, STOP
  that step and record it in the summary).
- Zero string foreign keys in any `relbase_*`/`delta`/`effect_cache` column.
- N+1 law: never a per-row statement; batch inserts, chunk IN-lists at 500.
- Banned words as identifiers or prose: provenance, substrate, load-bearing,
  regime, carry, port (TCP "port" for the http listener is allowed).
- rxjs style: composable pipes, named operators; no imperative rewrites of
  existing pipeline stages.
- File layout frozen: edit the named files; no new directories.
- Hermetic runs: every server/test boot uses a temp `DL_DB_PATH`; never write
  ~/.local/state.

## Gates (run, do not trust memory)

- `cd v6/dl && pnpm typecheck` clean.
- `cd v6/dl && pnpm test` — full suite green (64/64 at base; budget: max 8
  full runs).
- `pnpm -C v6/sprefa-store/js test` — 75/75, untouched store (max 2 runs).
- Schema proof in the summary: `.schema` excerpt for one fact rel + delta +
  effect_cache showing WITHOUT ROWID, all-INTEGER affinity, zero autoindexes,
  zero TEXT columns on relbase_*.
- `bash v6/dl/tests/golden/curl-session.sh` byte-identical, run TWICE. If the
  sandbox cannot bind localhost, SKIP this gate, run the rest, and flag the
  skip prominently in the summary — the reviewer runs it outside.

## Commit protocol

One step per commit, `git commit -n`, message prefix `v6/dl: M9-codex — `.
Do NOT push. Do NOT merge anywhere.

## Final summary shape

Per-step: commit sha + one-line receipt. Then: the schema proof excerpt, test
counts, curl gate status (pass/skipped-why), and an explicit list of any step
stopped under the laws with the reason.
