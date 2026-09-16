# REPORT: spine catalog emitter (type-IR MVP, steps a+b)

Lane `lab/duel-catalog-flash`, base `9557daf2`. Spec: CONTRACT.md (binding),
grounding: PLAN2.md, live spine.rs / spine.ts / types.ts, emitter precedents
`1_emit_registry_docs.pl` and `2_emit_cli_inventory.pl`.

## First action

```
$ git merge --ff-only 9557daf2
Already up to date.
exit 0
```
Base was already HEAD. Proceeded.

## Deliverables (all present in this worktree, no commits made)

| # | file | status |
|---|---|---|
| 1 | `v6/prolog/compile/3a_spine_schema_facts.pl` | new: table/2, table_symbol/2, column/6, 9 tables / 37 columns |
| 2 | `v6/prolog/compile/3_emit_spine_schema.pl` | new: module `emit_spine_schema`, `emit_spine_schema/0`, `rows_ts_text/2` |
| 3 | `v6/sprefa-store/js/src/engine/spine.ts` + `types.ts` | edited: marker zones added, interfaces untouched |
| 4 | `v6/tsv2/tests/spineSchema.test.ts` | new: staleness gate, pattern `bopCommandInventory.test.ts:52-59` |
| 5 | `REPORT.md` | this file |

## Transcription receipts

The three sources cross-checked per table; count 9 tables / 37 columns.

| receipt | claim | verdict |
|---|---|---|
| spine.ts:62-102 | 7 row interfaces (StringsRow..FileBytesRow), column order and TS types as emitted | CONFIRMED byte-for-byte via emitter diff |
| types.ts:80-95 | NodeRow/EdgeRow interfaces | CONFIRMED byte-for-byte via emitter diff |
| spine.rs:314-473 | entity columns, pk sets (auto_increment=false composite pks), WITHOUT ROWID flags (revs_files, file_bytes, edge) | CONFIRMED against facts |
| table count | 9 (strings, repos, roots, repo_revs, files, revs_files, file_bytes, node, edge) | CONFIRMED (spine.ts table_names:105, spine.rs:386-399) |

Column tally: strings 2, repos 4, roots 3, repo_revs 6, files 4, revs_files 3,
file_bytes 4, node 7, edge 4 = 37.

## Gates (exact commands and outputs)

### Gate 1: emitter loads

```
$ swipl -q -l v6/prolog/compile/3_emit_spine_schema.pl -g halt
exit 0
```
Clean, no stderr.

### Gate 2: byte-equality

Proven two ways: (a) direct diff of `rows_ts_text` output against the pre-marker
file bodies, (b) `emit_spine_schema/0` is idempotent (regenerated files are
byte-identical to the pre-run files). Both empty.

```
$ swipl -q -l v6/prolog/compile/3_emit_spine_schema.pl \
    -g "emit_spine_schema:rows_ts_text(spine,T),format('~s',[T])" -g halt \
  | diff <(sed -n '63,102p' v6/sprefa-store/js/src/engine/spine.ts) -
(empty diff) SPINE OK

$ ... rows_ts_text(types,T) ... | diff <(sed -n '80,95p' v6/sprefa-store/js/src/engine/types.ts) -
(empty diff) TYPES OK

$ cp spine.ts types.ts to /tmp, swipl -g emit_spine_schema -g halt, diff against originals
(empty diff) IDEMPOTENT OK
```

### Gate 3: staleness test (same runner as bopCommandInventory)

Runner: `node --test --experimental-transform-types` from `v6/tsv2` (matches
`package.json` scripts.test and the scoped run of bopCommandInventory).

```
$ node --test --experimental-transform-types tests/spineSchema.test.ts tests/bopCommandInventory.test.ts
✔ registry.pl cli_command/3 and cli/bop.ts's commander verbs name the same set
✔ generated CLI and HTTP inventory is current with canonical Prolog facts
✔ spine.ts marker zone is current with canonical Prolog facts
✔ types.ts marker zone is current with canonical Prolog facts
ℹ tests 4
ℹ pass 4
ℹ fail 0
```

### Gate 4: plunit untouched

```
$ cd v6/prolog && swipl -g go -t halt ARCH.pl < /dev/null > /dev/null 2>&1
exit 0  (0 ERROR lines on stderr)
```

### Gate 5: tsc (tsv2/store typescript still compiles)

```
$ cd v6/sprefa-store/js && node_modules/.bin/tsgo --noEmit
exit 0

$ cd v6/tsv2 && node_modules/.bin/tsgo --noEmit -p tsconfig.json
exit 0
```

## Deviations

None of the STOP conditions triggered. Two recorded notes, neither a contract
violation:

1. `table/2` arity spec. This SWI (10.0.2) declares `table` as an `fx`
   operator (1150), so the bare arity spec `table/2` in a module/use_module
   export list fails to parse ("Operator expected"). The facts are still
   `table(Name, WithoutRowid)`; only the export/import list writes it as
   `'table'/2`. Same for the fact predicate `column/6`, which needs no quote.
2. `tsgo` was not preinstalled (no `node_modules`). Ran
   `pnpm install --prefer-offline` in `v6/sprefa-store/js` and `v6/tsv2` to
   obtain `@typescript/native-preview` (the `tsgo` bin) and the store/tsv2
   dependencies. Installs live inside this worktree; nothing was written
   outside it.

## Style

No em dashes. The words provenance, substrate, load-bearing, regime, and
subagent never used. Marker begin/end comments are the only added comments
(constraint the code cannot show).
