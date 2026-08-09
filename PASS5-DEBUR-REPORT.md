lane boop5 pass 5 debur

## Gates

- `cd v6/boop && cargo test` — 29 passed (28 baseline + item 1's test)
- `cd v6/boop && cargo clippy -- -D warnings` — clean
- `tmux -L lanes ls | sort | md5` — identical before and after cargo test
  (`d049ba16b44797f2ad542b9283c06911`)

## Item 1 — kill the silent harness fallback

`dispatch --harness <name>` for an unregistered harness no longer falls back to
claude. Resolution moved from `harness_by_id` (which fell back to the first
registered adapter) to a new `resolve_dispatch_harness` in `src/main.rs:906`.
A named harness must resolve exactly; an unregistered name is a hard error that
bails with the requested id and the registered set:

```
unregistered harness `opencode`; registered harnesses: claude
```

`run_dispatch` (`src/main.rs:838`) resolves through it and takes the harness id
from the resolved adapter. `harness_by_id` remains for `hail`, which is out of
scope for this pass.

Test `dispatch_refuses_an_unregistered_harness` asserts the error names
`opencode` and lists the registered set (`src/main.rs:1423`).

## Item 2 — debur sweep over 88e2ff44..HEAD

Findings fixed (zero behavior change):

| # | location | fix |
|---|----------|-----|
| 1 | `src/worktree.rs` `init_repo` | removed dead `git rev-parse --show-toplevel` block whose output was discarded via `let _ = out;` |
| 2 | `src/main.rs:878` `run_dispatch` | dropped the unused `let _outcome =` binding on `adapter.send(...)` (value ignored) |
| 3 | `src/harness/claude.rs` tests | renamed single-letter test locals `s` -> `req`, `c` -> `caps` |

Reviewed, left as-is:

- `#![allow(dead_code)]` in `harness.rs` / `claude.rs` keep the facet-3 trait
  surface (default no-op methods plus the `capabilities`/`stop` overrides)
  compiling before the control-facet CLI verb lands. Those methods are never
  called in production yet; removing the allow would trip `-D warnings`, and
  deleting the overrides would lose facet surface. Intentional.
- Inline comment blocks in the surface are all ≤ 2 consecutive lines (comment
  budget holds).
- No unused imports, copy-paste helper candidates, or other dead code found in
  the four-commit surface.

## Note

`boop lane` (default harness `opencode`) routes through `run_dispatch`; with
item 1's strict resolver it now bails on `opencode` until the adapter lands in
pass 6, instead of silently running claude. This is the same fallback being
killed, surfaced through lane, not a separate behavior change.

## Commit

`boop: PASS — dispatch refuses an unregistered named harness; debur the four pass-5 commits`
