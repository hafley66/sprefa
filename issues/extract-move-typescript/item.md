---
created: 2026-08-25
updated: 2026-08-25
type: task
reporter: hafley66
status: open
priority: normal
epic: extract-astgrep-soopy
labels:
- pkg:extract
---

# extract move for TypeScript: corpus walk, path resolution, batch list

## Description

extract move is prolog-only by construction (0_move.rs walks .pl/.plt and resolves prolog specs). lang/ts.rs already emits Specifier rows. Missing: a .ts/.tsx corpus walk, TS path resolution (extensionless, index.ts, package.json exports, tsconfig paths), a second language arm in rules/move_specifier.yml, and a batch form (--list old<TAB>new) staging every move in one soopy transaction. First real corpus: hafley-rxjs grapht layout (38 files into 4 folders, done by hand in another session).
