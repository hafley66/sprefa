# Brief: sprefa-extract corpus battery, language = ts

Read `plans/extract-corpus-2026-08-28/COMMON.md` FIRST and follow it exactly.
Your lane name: `chore-extract-corpus-ts`.

## Your language and arm
- Language: **ts**, file glob `*.ts`.
- Arm you own (the ONLY src files you may edit): `v6/sprefa-extract/src/lang/ts.rs`, `ts_paths.rs`, `ts_rehome.rs`, `ts_rename.rs`, `ts_resolve.rs`
- Tests you may add: `v6/sprefa-extract/tests/*ts*.rs` and
  `v6/sprefa-extract/tests/fixtures/ts/corpus_*.ts`.

## Corpus (read-only, never modify)
`~/projects/instant/` and `~/projects/hafley-rxjs/` (source AND `node_modules`), plus `~/projects/sprefa/v6/tsv2`. Include `.tsx`, `.mts`, `.cts`, `.js`, `.mjs` in step 1 (record which extensions `source_for` accepts; a rejected extension is a finding with the roster line cited). Step 5: `scip-typescript` is installed (`~/.nvm/versions/node/v24.15.0/bin/scip-typescript`); run on `instant`, `hafley-rxjs`, `v6/tsv2`.

## Scratch dir for logs and TSVs before commit
`/private/tmp/claude-501/-Users-chrishafley-projects-sprefa/efe90f72-55fb-4958-b159-4982236661fe/scratchpad/ts` (create it).

Extra ts checks: `.d.ts` declaration files, `export * from`, `export { x as y }`, default exports, namespaces, decorators, overloads, `satisfies`, enums, JSX components as callees, path aliases from tsconfig `paths` (see `ts_paths.rs`), barrel re-exports across packages.

## Sample commands
```
X=$PWD/v6/sprefa-extract/target/release/extract
find <ROOT> -name '*.ts' -type f > /private/tmp/claude-501/-Users-chrishafley-projects-sprefa/efe90f72-55fb-4958-b159-4982236661fe/scratchpad/ts/files.txt
wc -l /private/tmp/claude-501/-Users-chrishafley-projects-sprefa/efe90f72-55fb-4958-b159-4982236661fe/scratchpad/ts/files.txt
nohup bash -c 'while read f; do s=$(date +%s%N); out=$(timeout 10 $X "$f" 2>/private/tmp/claude-501/-Users-chrishafley-projects-sprefa/efe90f72-55fb-4958-b159-4982236661fe/scratchpad/ts/err.tmp); rc=$?; e=$(( ($(date +%s%N)-s)/1000000 )); printf "%s\t%s\t%s\t%s\t%s\n" "$f" $rc $e $(printf "%s" "$out" | wc -l) "$(head -1 /private/tmp/claude-501/-Users-chrishafley-projects-sprefa/efe90f72-55fb-4958-b159-4982236661fe/scratchpad/ts/err.tmp)"; done < /private/tmp/claude-501/-Users-chrishafley-projects-sprefa/efe90f72-55fb-4958-b159-4982236661fe/scratchpad/ts/files.txt' > /private/tmp/claude-501/-Users-chrishafley-projects-sprefa/efe90f72-55fb-4958-b159-4982236661fe/scratchpad/ts/runs.tsv 2>&1 &
```
Adapt for parallelism (split files.txt into 8 chunks, one loop each).
