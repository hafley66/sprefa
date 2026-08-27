# scip_move fixture

The corpus `tests/5_move_scip.rs` grades `move_scip::verify_import_refs` on. The
batch is one move, `src/util.ts` -> `src/moved/util.ts`, and it is never applied:
the test reads this tree in place and writes nothing.

`fixture.scip` is COMMITTED, because the check needs an index it did not build.
It is named `fixture.scip` rather than `index.scip` because `.gitignore:26`
ignores the latter at any depth. Rebuild it, from this directory:

```
scip-typescript index --output fixture.scip
```

| indexer | version | when |
|---|---|---|
| scip-typescript | 0.4.0 | 2026-08-27 |

The rebuilt bytes differ per machine: `Metadata.project_root` is an absolute
`file://` URL of whoever ran the indexer. Nothing reads that field — document
paths are relative to it and the check joins on those — so a rebuild is a
byte-different, behavior-identical index.

**That indexer sets no IMPORT role.** `dist/src/FileIndexer.js:80` and `:214` are
its only `symbol_roles` writes and both write `SymbolRole.Definition`; every one
of this index's occurrences carries roles in `{0, 1}`. What it does emit for an
import is an occurrence over the specifier literal whose symbol is the target
document's module symbol, and that is the row the check reads.

| file | spec | in the batch's scope because |
|---|---|---|
| `src/app.ts` | `"./util"` | the target moves |
| `src/deep/nested.ts` | `"../util"` | the target moves, one directory down |
| `src/unicode.ts` | `"./util"` | the target moves, and the literal sits past `🎌π日本語` on its own line: UTF-16 column 53, byte 62 |
| `src/util.ts` | `"./shared"` | the IMPORTER moves, so its own outgoing specifier counts |
| `src/unrelated.ts` | `"./other"` | NOT in scope: neither end moves. The index carries this occurrence and `Rehome for TsSource` answers with no ref for it, so an unscoped check would call it a miss |
