---
created: 2026-08-27
updated: 2026-08-27
type: feature
assignee: chris
status: open
priority: normal
epic: extract-move-parity
labels: [extract, refactor, scip]
---

# extract move: verify Rehome import refs against a SCIP index

## Description

When a fresh SCIP index exists, cross-check every ImportRef a Rehome impl produced against SCIP Occurrence rows with the IMPORT role; report refs SCIP knows that the impl missed and refs the impl produced that SCIP does not know. Library + tests first, no CLI flag (0_move.rs is owned by other lanes).

## The IMPORT role does not exist in practice

The brief's rule — "SCIP Occurrence rows with the IMPORT role" — reads ZERO rows
against scip-typescript 0.4.0. That indexer writes `symbol_roles` at exactly two
sites, `dist/src/FileIndexer.js:80` and `:214`, and both write
`SymbolRole.Definition` (0x1). A fresh index of `tests/fixtures/scip_move`
carries roles in `{0, 1}` across all 30 occurrences and not one IMPORT bit.

What that indexer DOES emit for an import is an occurrence covering the
specifier literal, quotes included, whose symbol is the target document's module
symbol:

| document | range | symbol | roles |
|---|---|---|---|
| `src/app.ts` | 23..31 = `"./util"` | `... src/\`util.ts\`/` | 0 |
| `src/util.ts` | 0..0 | `... src/\`util.ts\`/` | 1 = DEFINITION |

So the index names the target itself, with no indexer-specific string parsing:
look the occurrence's symbol up in the definition table and read off the
document that defines it.

`move_scip.rs` reads an occurrence as import-shaped when `roles` carries IMPORT
**or** its byte range is a quoted string literal in the document. The role arm
stays for indexers that do set it; the literal arm is what answers today.

## Scope is symmetric or the report is noise

A `Rehome` impl answers for the BATCH: `lang/ts_rehome.rs:176` drops a relative
specifier whose last segment cannot name a moved file. An index carries every
import in the corpus. Comparing them unscoped calls every unmoved-to-unmoved
import a miss — measured, with the scope clause deleted `src/unrelated.ts`
`"./other"` reports as `missed_by_impl`.

Both sides therefore pass one predicate: importer and target are corpus files
with index documents, and one of the two is in `cx.moved()`.

## Not built here

- No CLI flag. `src/0_move.rs` is owned by the shootout lanes.
- Rust. `use_path` refs are identifier occurrences, not literals, and several
  per path; that is a different match shape and a separate arc.
- `path_literal` and `manifest_target` refs. An index has no document for a
  `.mmd` data file or for `package.json`, so the indexed-document filter drops
  them rather than calling them disagreements.
