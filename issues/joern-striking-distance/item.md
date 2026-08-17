---
created: 2026-08-16
updated: 2026-08-16
type: epic
status: open
priority: high
labels:
- pkg:extract
---

# CPG parity: cfg/cdg planes + walk rules put Joern-class analysis in dl6

## Description

## Comments

### 2026-08-16T17:27:07Z · @fable

Anchor analysis: plans/2026-08-16-joern-cpg-striking-distance.md (edge-color distance table, node dictionary, kind_role generic CFG, CDG as post-dominance, walks as recursive rels, build-vs-buy candidates, 4 forks). Companion research in flight: plans/2026-08-16-extract-generic-typesystems.PLAN.md (extract-closeout driver). Related epics: extract-port-closeout, bug-mining.

## Decisions

### 2026-08-17T00:39:43Z · @chris

CDG fork DEFERRED by user 2026-08-16 ('yield on cdg spark, not priority 2'). When it wakes, coordinator preference on record: rust pass in extract via petgraph::algo::dominators rather than dl6 rules (stratification for post-dominance unverified).

### 2026-08-17T00:42:42Z · @chris

Stop 4 DECIDED 2026-08-16: type info is a GRAPH (queryable type rels plane), never a checker-shaped SYSTEM. Tier-1 type propagation lives as a rust pass in sprefa-extract, same pattern as FlowF/CfgF feeding queryable planes.

