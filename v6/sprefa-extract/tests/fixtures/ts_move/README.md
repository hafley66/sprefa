# ts_move fixture

The resolution cases `lang/ts_resolve.rs` is graded on. `vendor/` is copied to
`node_modules/` at test setup: this repository gitignores `node_modules` at any
depth (`.gitignore:107`), so the package cannot be committed under that name.

`src/entry/index.ts` is the file `tests/41_move_ts.rs` moves to
`src/deep/entry/index.ts`. Its five importers under `src/importers/` write the
five spellings that reach it, one per file:

| file | spec | what it pins |
|---|---|---|
| `relative.ts` | `'../entry/index.ts'` | the extension as written |
| `extensionless.ts` | `'../entry/index'` | no extension, `index` named |
| `directory.ts` | `'../entry'` | the directory form, `index` implied |
| `emitted.ts` | `'../entry/index.js'` | the `.js` written for a `.ts` |
| `alias.ts` | `'@app/entry'` | the tsconfig `paths` alias |

`src/deep/keep.ts` exists so the destination directory's ancestry already holds
a file: the alias re-spelling probes it before keeping the alias.
