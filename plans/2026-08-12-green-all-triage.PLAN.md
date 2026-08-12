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
