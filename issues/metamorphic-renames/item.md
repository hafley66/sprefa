---
created: 2026-08-15
updated: 2026-08-15
type: feature
reporter: fable
status: done
priority: normal
epic: bug-mining
labels:
- size:med
- area:testing
- bugmine
- pkg:prolog
- pkg:tsv2
closed: 2026-08-15
commits:
- hash: 763de17e
  summary: metamorphic-rename-pass
---

# Metamorphic testing: rename rels/vars/modules, expect identity modulo names

## Description

Rename every rel/var/module in a corpus program (including camelCase, __dunder, unicode-adjacent shapes), recompile, assert output identical modulo the rename map. Evidence: camelCase module mangling and the __dunder__ silently-dropped interface (both fixed in PR #262) were pure name-sensitivity bugs. Second metamorphic law worth a pass: split a rel into two + union, same rows.
