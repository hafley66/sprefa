# pro4-smoke

Purpose: first drive of the `pro4` preset (deepseek-v4-pro-0813). Prove the
harness loop works end to end: read, run, write, commit.

## First action
`git merge --ff-only e70417d92480cd73c2855c8f49e53a22b86b1328`. If that fails,
STOP AND REPORT. Do not work around it.

## Task
1. Run: `jq -r '.fixtures | length' v6/prolog/compile/out/manifest.json`
   (if `.fixtures` is not the key, print the top-level keys with
   `jq 'keys' v6/prolog/compile/out/manifest.json` and count the per-fixture
   entries under whichever key holds them).
2. Run: `cd v6/prolog && swipl -g go -t halt ARCH.pl` and capture the final
   summary line.
3. Write both results to `PRO4-SMOKE.txt` at the worktree root, three lines:
   - `manifest_fixtures=<count>`
   - `arch_gate=<final line verbatim>`
   - `model=pro4 date=2026-08-13`
4. `git add PRO4-SMOKE.txt && git commit -m "pro4 smoke receipt"`

## Files you own
`PRO4-SMOKE.txt` only. Touch nothing else. No other file may be created,
edited, or deleted.

## Validation
`git log --oneline -1` shows your commit; `cat PRO4-SMOKE.txt` shows three
lines. Report both outputs verbatim as your final message.

## Style laws
- No comments beyond what is asked. No extra files. No summary documents.
- If any command fails, paste the exact error and stop.
