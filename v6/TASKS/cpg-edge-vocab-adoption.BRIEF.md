# cpg-edge-vocab-adoption

Issue: `issues/cpg-edge-vocab-adoption/item.md`. User decision (2026-08-16):
adopt Joern's edge vocabulary as a REFERENCE ENUMERATION in the prolog tree.
Documentation only. Nothing may ever consult or import it.

## First action

```bash
git merge --ff-only 79aeed4fa1ede8bb5522fc032ca61f11a4961587
```

Failure = STOP AND REPORT. That sha carries your one source of truth:
`plans/2026-08-16-cpg-spec-research.REPORT.md`.

## Deliverable

ONE new file: `v6/prolog/cpg_edge_vocab.pl`. You own nothing else. Forbidden:
every existing file, especially `v6/prolog/compile/**`, `v6/prolog/ARCH.pl`,
the report, the anchor doc.

The file is a standalone prolog fact module:

1. `cpg_edge(Name, Semantic, JoernCite, OurInterface).` — one row per edge
   kind in report section 1a. All 34. `Name` lowercase atom (`ast`, `cfg`,
   `reaching_def`, ...). `Semantic` = the report's one-line semantic, atom or
   string. `JoernCite` = the report's cite atom, e.g. `'Ast.scala:423'`.
   `OurInterface` = how it would interface with sprefa-extract families, one of:
   - `existing(RelName)` — an extract rel already carries this meaning. Derive
     the mapping from the report and the anchor doc
     `plans/2026-08-16-joern-cpg-striking-distance.md` (five-of-seven table);
     e.g. AST -> the Cst parent edge, CALL -> call edges, REF -> resolve/ref
     rels, EVAL_TYPE -> TypeF, REACHING_DEF -> DfF direct edges. Name the
     actual rel/family from `v6/sprefa-extract/src/types.rs` (read it; do not
     invent names).
   - `planned(Card)` — covered by a filed card: `cfg_edge`, `cdg_edge`
     (cards `cpg-cfg-cdg-first-plane` family). Use for CFG, CDG, DOMINATE,
     POST_DOMINATE, CONDITION and the structured-body edges.
   - `none` — no analog and none planned (e.g. TAGGED_BY).
2. `cpg_node(Name, Semantic, JoernCite).` — one row per node kind in report
   section 1b.
3. Header comment, short: the reference-only law ("enumeration for humans and
   design discussion; consulting this from any compile or runtime code is a
   defect"), and See links: the report path, the anchor doc path,
   `https://cpg.joern.io`.
4. `go/0`: asserts exactly 34 `cpg_edge` rows, every row's cite non-empty and
   matching `\w+\.scala:\d+`, every `OurInterface` one of the three shapes,
   `cpg_node` rows nonzero; prints one PASS line with both counts.
   `swipl -g go -t halt v6/prolog/cpg_edge_vocab.pl` must exit 0.

## Receipts in the PR body

- gate output line;
- `grep -rn "cpg_edge_vocab" v6/ --include="*.pl" | grep -v cpg_edge_vocab.pl`
  output EMPTY (proof nothing consults it);
- row count `grep -c "^cpg_edge(" v6/prolog/cpg_edge_vocab.pl` = 34.

## Style laws

Banned words in prose and identifiers: provenance, substrate, load-bearing,
regime. "refusal" banned in prose. Comment budget: constraints only, no
narrative. Descriptive variable names. No numbers copied into CLAUDE.md.

## Landing

Commit on your branch, push, `gh pr create` with the receipts above. Do not
merge. Lanes never spawn subagents.
