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

