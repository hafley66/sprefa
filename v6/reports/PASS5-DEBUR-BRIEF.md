# LANE boop5 — pass 5 DEBUR (pass 2 of 2). Scope is SMALL. No new features.

Worktree /Users/chrishafley/projects/sprefa-lanes/boop, branch lane/boop.
If reality deviates, STOP and hail.

EXPLICITLY OUT OF SCOPE (do not touch): opencode/kimi/codex/ccz/pi adapters,
the synthesised done-event, any new capability. Those are pass 6, not debur.

## Item 1 — kill the silent harness fallback (the one behavior change)

`dispatch --harness <name>` for an UNREGISTERED harness currently falls back
to claude and succeeds. Make it a hard error: exit nonzero, message naming the
requested harness and the registered set. A named harness resolving to a
different harness is a capability lie. Test:
`dispatch_refuses_an_unregistered_harness` asserting the error names
"opencode" and lists the registered set.

## Item 2 — debur sweep over the four pass-5 commits ONLY

git diff 88e2ff44..HEAD is your surface. Sweep for: dead code, unused
imports, comment-budget violations (max 2 consecutive lines, constraints
only), single-letter names, copy-paste blocks that should be one helper.
Fix what you find; list each fix in the report. Zero behavior changes
beyond item 1.

## Gates (verbatim in report)

    cd v6/boop && cargo test                      # 29+ passed (28 + item 1's)
    cd v6/boop && cargo clippy -- -D warnings
    tmux -L lanes ls | sort | md5                 # before AND after cargo test, identical

## Deliverable

PASS5-DEBUR-REPORT.md, first line `lane boop5 pass 5 debur`. Commit prefix
`boop:`. Then exactly ONE of:

    bus hail --to fable-main --kind result --body "boop5-debur done: <one line>"
    bus hail --to fable-main --kind result --body "boop5-debur BLOCKED: <one line>"
