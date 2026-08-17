---
created: 2026-08-16
updated: 2026-08-16
type: feature
status: done
priority: high
epic: joern-striking-distance
labels:
- pkg:extract,size:med
blocked_by: ['@cpg-spec-research']
closed: 2026-08-16
commits:
- hash: cb191e035609e971da63bd2363a1b780c2cc4b71
  summary: CfgF fifth family, kind_role x4 langs, kotlin keyword read
---

# cfg_edge plane + generic CDG derivation, first implementation

## Description

## Comments

### 2026-08-16T17:27:07Z · @fable

Beginning implementation, blocked on fork 1 of plans/2026-08-16-joern-cpg-striking-distance.md section 7 (new CfgF family vs rels in DfF plane; consistency law leans family, Chris decides). Scope once unblocked: (a) kind_role fact rows for rust + go; (b) one generic cfg builder over CstF output (sibling-sequence + branch/loop/jump rules), emitting span-keyed cfg_edge rows on the wire; (c) CDG as post-dominance over cfg_edge (fork 4 decides engine-side dl6 vs rust pass); (d) receipts: a fixture function whose cfg/cdg row set is asserted exactly, both doors if lowered to dl6. Depends on cpg-spec-research for the vocabulary and the kind census.

### 2026-08-16T21:29:41Z · @coordinator

Shipped vocab (flag for Chris, deltas from anchor): node kinds entry/exit/stmt/branch/loop/jump/ret; edge kinds next/arm/jump/exit (arm replaces branch_true/false — CST does not label arms; loop back edges ride next). Branch-target rows (match_arm, *_case, when_entry, else_clause) deliberately role-free, receipts in src/cfg.rs header. CDG remains open on fork 4 (dl6 rules vs rust pass); goto/labelled-break unresolved. cpg-walk-rules now unblocked.


## Decision (user 2026-08-16)

CfgF is a NEW family (own `Family` impl, own edge kinds), never rows in DfF.
Edge-kind naming is ours; Joern's 34-kind vocabulary is reference-only
(`v6/prolog/cpg_edge_vocab.pl`, in flight). kind_role rows are hand-authored;
kotlin's `jump_expression` needs a leading-keyword read (research REPORT sec 4).
