---
created: 2026-08-25
updated: 2026-08-25
type: improvement
reporter: hafley66
status: testing
priority: high
epic: extract-astgrep-soopy
labels:
- pkg:extract
---

# extract move dry run 0.12 s -> 0.35 s after the YAML rule path

## Description

Measured on the installed binary from origin/main 209d8ed70, real tree, registry.pl dry run x3: 0.71 (cold) / 0.35 / 0.35 s; user CPU 0.63 s. Before arc C (#466 binary): 0.12 s wall, 0.28 s user. The rule path (rules/move_specifier.yml with stopBy end walks + Op::every + fact matchers over every prolog file) roughly doubles CPU. Byte-identical output, so this is cost only. Levers to measure with DL_TRACE_SUMMARY / debug spans: run the rule only on files the memchr prescan admits (the prescan from #465 may have been bypassed), restrict the rule's kind set via potential_kinds so ast-grep prunes before stopBy, preload the fact set once (confirm), and parse on EXTRACT_POOL (confirm the pool path survived the rewrite). Target: back to the 0.12 s band, byte-identical.
