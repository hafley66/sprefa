# Brief: sprefa-extract corpus battery, language = go

Read `plans/extract-corpus-2026-08-28/COMMON.md` FIRST and follow it exactly.
Your lane name: `chore-extract-corpus-go`.

## Your language and arm
- Language: **go**, file glob `*.go`.
- Arm you own (the ONLY src files you may edit): `v6/sprefa-extract/src/lang/go.rs`
- Tests you may add: `v6/sprefa-extract/tests/*go*.rs` and
  `v6/sprefa-extract/tests/fixtures/go/corpus_*.go`.

## Corpus (read-only, never modify)
`~/go/pkg/mod/` (331M). Step 1 over ALL `.go` files, including `_test.go`. Steps 3-4 per module dir (each `<mod>@<ver>` dir). Step 5 needs `scip-go`; it is NOT on PATH, so record the `scip_skip` row verbatim, try `go install github.com/sourcegraph/scip-go/cmd/scip-go@latest` into scratch GOBIN, and rerun on 3 modules if it installs.

## Scratch dir for logs and TSVs before commit
`/private/tmp/claude-501/-Users-chrishafley-projects-sprefa/efe90f72-55fb-4958-b159-4982236661fe/scratchpad/go` (create it).

Extra go checks: method receivers as definitions, interface method calls, embedded structs, generics (`[T any]`), `init()` functions, cgo files, build-tag files, dot imports, blank imports.

## Sample commands
```
X=$PWD/v6/sprefa-extract/target/release/extract
find <ROOT> -name '*.go' -type f > /private/tmp/claude-501/-Users-chrishafley-projects-sprefa/efe90f72-55fb-4958-b159-4982236661fe/scratchpad/go/files.txt
wc -l /private/tmp/claude-501/-Users-chrishafley-projects-sprefa/efe90f72-55fb-4958-b159-4982236661fe/scratchpad/go/files.txt
nohup bash -c 'while read f; do s=$(date +%s%N); out=$(timeout 10 $X "$f" 2>/private/tmp/claude-501/-Users-chrishafley-projects-sprefa/efe90f72-55fb-4958-b159-4982236661fe/scratchpad/go/err.tmp); rc=$?; e=$(( ($(date +%s%N)-s)/1000000 )); printf "%s\t%s\t%s\t%s\t%s\n" "$f" $rc $e $(printf "%s" "$out" | wc -l) "$(head -1 /private/tmp/claude-501/-Users-chrishafley-projects-sprefa/efe90f72-55fb-4958-b159-4982236661fe/scratchpad/go/err.tmp)"; done < /private/tmp/claude-501/-Users-chrishafley-projects-sprefa/efe90f72-55fb-4958-b159-4982236661fe/scratchpad/go/files.txt' > /private/tmp/claude-501/-Users-chrishafley-projects-sprefa/efe90f72-55fb-4958-b159-4982236661fe/scratchpad/go/runs.tsv 2>&1 &
```
Adapt for parallelism (split files.txt into 8 chunks, one loop each).
