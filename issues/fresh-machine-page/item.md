---
created: 2026-08-19
updated: 2026-08-19
type: task
status: open
priority: high
epic: productionize-rust-door
size: S
blocked_by: ['@dl6c-saved-state', '@dl6-build-single-binary']
---

# fresh-machine.md: zero to running program, tested on a temp HOME

## Description

## Description
No page takes a stranger from zero to a running program. `README.md`, `v6/GETTING-STARTED.md`, `compile/SYNTAX.md`, `docs/hosts-are-arrivals.md` exist; none is tested on a clean machine.
## Acceptance Criteria
- [ ] `docs/fresh-machine.md`: install dl6c, write `hello.dl6` (one base rel, one rule, one `sh`), `dl6 build`, run, `curl --unix-socket` a rel. Under 80 lines, every command copy-pastable.
- [ ] A lane runs it with `HOME=$(mktemp -d)` and no sprefa checkout on PATH except the two installed binaries; the transcript of that run is attached to this card.
- [ ] Opens with a TOC; tables and code only, no prose paragraphs longer than two lines.
