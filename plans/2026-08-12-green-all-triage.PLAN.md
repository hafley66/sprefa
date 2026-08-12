# 2026-08-12 green-all triage

Measured 2026-08-12 on base `154ae23c` in a fresh worktree, machine quiet
(all boop lanes 0.0-0.1% cpu). Each leg run 3 times, one at a time, never the
whole gate. See `.github/CI-KNOWN-RED.md` for the allowlist this updates.

## TOC

1. Verdict table
2. Real failures
3. Legs deleted from the ledger (green 3/3)
4. Flaky legs
5. Fix-these-first ranking

## 1. Verdict table

| leg | 3-run result | verdict | root cause (one line) | throw site |
|---|---|---|---|---|
| roundtrip | FAIL 3/3 | real | print_dl/parse_dl round-trip not variant-stable for the mutual-recursion fixture | `v6/prolog/compile/scripts/roundtrip.sh:132` |
| extraction-live | PASS 3/3 | stale (ledger) | `EXTRACTION LIVE HOLDS`; passes once the release extractor is present | none |
| lsp-diags | FAIL 3/3 | real | LSP client never receives both diagnostics for b.ts (driver.log stalls at READY); needs the v5 `dl` binary, then fails B1 deterministically | `v6/tsv2/scripts/lsp-diags.sh:266` |
| flagship | FAIL 3/3 | real | pinned corpus moved since the v5 golden: digest `b8d03946` now `8e3874d5`; golden must be regenerated | `v6/tsv2/scripts/flagship-callgraph.sh:287` |
| getting-started | FAIL 3/3 | real | engine error text changed to `rule-index unavailable:`, doc block 24 still prints the old `broken.dl6:4:` message | `v6/tsv2/scripts/getting-started.sh:224` |
| golden-flex | FAIL 3/3 | real | `json_object/2` stale `refused` excuse (now live) + `json_patch/2` unexercised; 69 registry constructs, 2 unaccounted | `v6/prolog/compile/scripts/golden_coverage.pl:174,178` |
| tsv2-test | FAIL 3/3 | real | sh host grid decode: a 0-row demand answers with rows; decoded per-demand counts `[0,1,2,3]` expected, `[1,2,2,3]` actual. (Needs `gen_emitted/` first, produced by `just sweep`.) | `v6/tsv2/tests/hostDecode.test.ts:144` |
| plunit | FAIL 3/3 | real | 6 unit-test failures now (ledger records 1); catalog_plane_rail + expression_inventory + rel_zero_arity + 3 json_merge_patch, incl. two `no_exception` | `v6/prolog/compile/test/plunit_tests.pl:1314,4561,5809,7684,7739,7743` |
| rtkq-golden | FAIL 3/3 | real | api_endpoint row order mismatch: engine emits updateUser-before-listUsers, order-sensitive golden expects listUsers-first (spans identical, not a corpus move) | `v6/tsv2/labs/1_rtkq-extraction-golden.ts:200` |
| compile-speed | FAIL 3/3 | real | `COMPILE_SPEED regressions=16 improvements=0` vs 2026-08-07 baseline; golden-flex lower +178%, emit +120% | `v6/prolog/compile/scripts/1_compile_speed.sh:248` |

## 2. Real failures

### lsp-diags

Verbatim:
```
PASS  phase B0  dl --lsp --diag-db initialized over real stdio ...
FAIL  phase B1: the real LSP client never received both diagnostics for b.ts: READY
```

Throw site: `v6/tsv2/scripts/lsp-diags.sh:266`
```
265      await_log_line "$WORK/driver.log" "PASS appeared" 30 \
266        || fail "phase B1: the real LSP client never received both diagnostics for b.ts: $(cat "$WORK/driver.log")"
267      say "PASS  phase B1  $(grep 'PASS appeared' "$WORK/driver.log")"
```

The driver (`lsp_diag_driver.py`) logs `READY`, then never logs `PASS appeared`; the awaited line never arrives within the 30s window and the leg exits 1. Phases A1-A4 (a.ts diag_v5 rows) pass and B0 initializes stdio, but the b.ts push never delivers both diagnostics to the LSP client. Fix not applied (read-only).

## 3. Legs deleted from the ledger (green 3/3)

| leg | 3/3 receipt |
|---|---|
| extraction-live | `EXTRACTION LIVE HOLDS`; 3/3 exit 0 (green only because the release extractor binary is present, per worktree setup; a fresh tree with no extractor is the build-step gap that originally reddened it) |

## 4. Flaky legs

(none yet)

## 5. Fix-these-first ranking

(ranking at end)

