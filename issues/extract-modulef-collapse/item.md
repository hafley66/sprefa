---
created: 2026-08-16
updated: 2026-08-17
type: task
status: done
priority: normal
epic: extract-port-closeout
labels:
- pkg:extract
closed: 2026-08-17
closed_by: modulef-driver
---

# ModuleF is collapsed: fork C, decided and shipped

## Description

## needs-chris

Design decision on the plane roster. No lane.

## Description

`ModuleF` is written down as a sketch and commented out. The resolution half
was folded into SCIP namespace edges and the binding half into aux metadata,
by an agent, and the file itself flags the call for human review.

## Receipts

| fact | receipt |
|---|---|
| the collapse, the sketch, and the flag | `v6/sprefa-extract/src/types.rs:629-645` ("PENDING - collapsed; not yet a family ... ADDENDUM 4a RULING: phase-1 specifier rows live in `CallFAux.specifiers` ... ModuleF stays collapsed. Flagged for human review; revival stays possible") |
| the plane roster still lists it | `v6/sprefa-extract/src/types.rs:17-19` |
| the resolve surface deliberately declares no arm | `v6/sprefa-extract/src/types.rs:1084-1088` |
| where the binding half actually lives | `v6/sprefa-extract/src/types.rs:497-503` (`CallFAux.specifiers`) |
| what v5 spends on the plane instead | `src/engine/family/mod.rs:397-408` — ten relations, incl. four `module_binding` shapes and `crate_edge` |

## The consequence, measured

@extract-module-plane-non-ts is blocked behind this in shape though not in
sequence: the per-language specifier emission is the same work either way, but
where the resolved edges LAND differs. Today a cross-file import edge is a
`file_edge` record (`src/schema.rs:36`) with a symbol count and no kind, where
v5 distinguishes import / edge / unresolved / binding / crate_edge.

## Forks, decided by nobody

| fork | shape |
|---|---|
| A. keep collapsed | specifiers stay `CallFAux` aux; `file_edge` stays the only module-level output; v5's five distinctions never come back |
| B. revive `ModuleF` | the sketch at `types.rs:637-645` becomes real; a fifth plane, its own Resolve arm, its own wire records |
| C. middle | no new family, but the `file_edge` record grows a `kind` column carrying v5's import/unresolved/binding distinction |

## Comments

### 2026-08-16T19:52:24Z · @chris

Deferred 2026-08-16: no call yet. User wants the port census absorbed first; the fold stands meanwhile. extract-module-plane-non-ts holds with it.

### 2026-08-17T12:55:29Z · @modulef-driver

FORK C LANDED 2026-08-17 by modulef-driver, on Chris's word: no new family, and the module-level distinctions come back on the WIRE, language-neutral.

WHAT SHIPPED (v6/sprefa-extract):

| record | columns | filler |
|---|---|---|
| file_edge | src_path, dst_path, kind, symbols | --deps fills kind from the SpecifierKind that bound the crossing; --scip-deps fills `unknown` |
| file_unresolved | src_path, module, reason | --deps only; reason = the deps.rs Policy slug (node_modules_boundary, absolute_path, relative_unresolved). v5 name: module_unresolved |
| package_edge | src_manifest, dst_manifest, kind | new --package-deps flag over Cargo.toml / package.json / go.mod. v5 name: crate_edge, which was Cargo-only and keyed on crate NAMES |
| specifier | +imported | the one v5 module_binding column the row lacked: `import {inner as outer}` and a default import's `default` |

THE EDGE KEY IS NOW (src, dst, kind). One pair carries one row per import form; the per-pair total is the sum over kinds, and the reverse (recovering forms from a summed row) was impossible. Measured on the fixture: `./lib/bare` named + namespace, `./lib/util.ts` named + default + reexport.

ROTTED RECEIPTS in this card, corrected: the collapse block is types.rs:1027-1041 (this card cited :629-645), the plane roster is types.rs:13-18 (cited :17-19), the CallFAux specifier home is types.rs:570-585 (cited :497-503). All three now say collapsed by decision (fork C) and the "flagged for human review" wording is deleted.

package_edge DIVERGES FROM v5 ON PURPOSE: v5's crate_edge was crate-name to crate-name, which needs a second dictionary to reach a file. The path key is the key file_edge already uses, so the two grains join directly.

--scip-deps CANNOT fill kind and the schema says so: an index records resolved occurrences, never the import statement that bound the name. `unknown` is the index's property, not a gap in the fold.

## Resolution

### 2026-08-17T12:55:36Z · @modulef-driver

Fork C, chosen by Chris 2026-08-17 and implemented in the same day: file_edge gains kind, file_unresolved and package_edge are new records, specifier gains imported. ModuleF stays collapsed and the sketch stays as the revival shape. Receipts in the note above.
