---
created: 2026-08-21
updated: 2026-08-21
type: improvement
reporter: chris
assignee: chris
status: open
priority: normal
epic: cheap-fast-analysis
---

# engine: soopy::discover once per repository, not once per directory

## Description

selfdoc run: discover ran 27 times for 3.02s on a cold tree, once per directory, all resolving to one repository, plus 4 x 265ms soopy_files enumerations. Memo by repository root after the first hit: walk up from the directory and stop at a known root. hosts.rs repository_root.
