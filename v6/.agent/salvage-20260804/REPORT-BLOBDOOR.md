# REPORT-BLOBDOOR

`extract query` gains `--digest <oid>` (source = `git cat-file blob <oid>`), the
cst/ast host passes the digest, and comment-prod.dl6's stated worktree-read
deviation is deleted.

## TOC

- Changes (file:line)
- Gate receipts
- Live receipt
- Deviations

## Changes (file:line)

| file | change |
| --- | --- |
| `v6/sprefa-extract/src/0_query.rs:20` | `--digest: Option<String>` flag on QueryCli |
| `v6/sprefa-extract/src/0_query.rs:31` | run reads via `source_bytes(&path, digest)` |
| `v6/sprefa-extract/src/0_query.rs:52` | `source_bytes`: digest present = `cat_blob`, absent = path read (as before) |
| `v6/sprefa-extract/src/0_query.rs:60` | `cat_blob`: `git cat-file blob <oid>` in process CWD; nonzero git exit = `Err` (printed once, exit 2) |
| `v6/sprefa-extract/tests/9_query_cli.rs:117` | `query_with_digest_reads_the_staged_blob`: hash fixture into temp repo, digest output == path output |
| `v6/sprefa-extract/tests/9_query_cli.rs:139` | `query_bad_digest_exits_two_with_one_line_stderr`: bogus oid -> exit 2, one stderr line |
| `v6/prolog/0_ast_expand.pl:190` | minted template -> `--digest {digest}`, `: {digest};` no-op removed |
| `v6/prolog/compile/test/plunit_tests.pl:2899` | expansion expectation matches new template |
| `v6/dl/fixtures/comment-prod.dl6` | header deviation paragraph (old lines 6-11) deleted; rest byte-identical |

Existing tests untouched; existing plunit expectations untouched except the one
template string; comment-prod.dl6 edits confined to the header paragraph.

## Gate receipts (verbatim)

```bash
cd v6/sprefa-extract && cargo build --offline --release --features cli --bin extract && cargo test --offline --release --features cli
```

```
Finished `release` profile [optimized] target(s) in 7.37s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s   [9_query_cli]
(all binaries green, exit 0)
```

```bash
cd ../.. && cd v6 && just conformance && just text-door && just plunit
```

```
just conformance: PASS (all cases)
just text-door:  TEXT_DOOR compiled=418 byte_identical=418 failures=0
just plunit:     345 passed; 0 failed (PLUNIT_EXIT=0)
```

Pre-change battery stated as conformance PASS, text-door 418/418, plunit 345/345
(all unchanged).

## Live receipt

Fixture: `blobdoor_receipt_check.rs` with 3 `//` prose lines, staged in the
worktree repo. Head 3-line finding; exit 2.

```bash
COMMENT_RAIL_PROGRAM=$PWD/v6/dl/fixtures/comment-prod.dl6 \
DL_EXTRACT_BIN=$PWD/v6/sprefa-extract/target/release/extract \
bash v6/tsv2/scripts/comment-budget-rail.sh
```

```
COMMENT BUDGET VIOLATION (max 2 consecutive comment lines in new code):
blobdoor_receipt_check.rs:1-3 (3 comment lines)
Repo law: comments state only constraints the code cannot show.
Fix: delete the narrative, keep at most 2 lines, or carry '@comment-ok: <reason>' if a scanner-backed waiver truly applies.
RAIL_EXIT=2
```

Then 10 more comment lines appended to the worktree file WITHOUT restaging
(staged blob still 3 lines; `AM` status):

```
COMMENT BUDGET VIOLATION (max 2 consecutive comment lines in new code):
blobdoor_receipt_check.rs:1-3 (3 comment lines)
Repo law: comments state only constraints the code cannot show.
Fix: delete the narrative, keep at most 2 lines, or carry '@comment-ok: <reason>' if a scanner-backed waiver truly applies.
RAIL_EXIT=2
```

Same 3-line finding despite the unstaged 14-line worktree edit: cst's minted host
now reads the STAGED BLOB via `--digest`, not the worktree path. Fixture removed
afterward, worktree left clean.

## Deviations

None. The change matches the brief. Two operational notes, not code deviations:

- The live-receipt JS server (`v6/tsv2/serve/main.ts`) could not boot on the
  first attempt: the `sprefa-store-engine` workspace deps were not installed
  (`v6/sprefa-store/js/node_modules` absent; the `"--digest"` harness brief said
  node_modules pre-seeded). Ran `pnpm install --offline --frozen-lockfile` in
  `v6/sprefa-store/js` (resolved from the pnpm store cache, ~7s) to provision
  `rxjs`, `@libsql/client`, `better-sqlite3`. Untracked, gitignored, no source
  change.
- An initial fixture named `blobdoor-receipt-fixture.rs` was dropped by the
  exemption rail (`*fixture*` in the name); recreated as
  `blobdoor_receipt_check.rs` before the receipt above.
