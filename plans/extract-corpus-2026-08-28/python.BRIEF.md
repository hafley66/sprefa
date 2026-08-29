# Brief: sprefa-extract corpus battery, language = python

Read `plans/extract-corpus-2026-08-28/COMMON.md` FIRST and follow it exactly.
Your lane name: `chore-extract-corpus-python`.

## Your language and arm
- Language: **python**, file glob `*.py`.
- Arm you own (the ONLY src files you may edit): `v6/sprefa-extract/src/lang/python/**`
- Tests you may add: `v6/sprefa-extract/tests/*python*.rs` and
  `v6/sprefa-extract/tests/fixtures/python/corpus_*.py`.

## Corpus (read-only, never modify)
`/opt/homebrew/opt/python@3.14/Frameworks/Python.framework/Versions/3.14/lib/python3.14/` (the CPython stdlib, includes `test/` and `site-packages`). Step 1 over ALL `.py` files. Steps 3-4 per package dir. Step 5: `scip-python` is installed (`~/.nvm/versions/node/v24.15.0/bin/scip-python`); run on 3 stdlib packages copied to scratch (`json`, `email`, `asyncio`).

## Scratch dir for logs and TSVs before commit
`/private/tmp/claude-501/-Users-chrishafley-projects-sprefa/efe90f72-55fb-4958-b159-4982236661fe/scratchpad/python` (create it).

Extra python checks: Python 2 syntax under `lib2to3`/`test/` (expected parse errors, count them separately), `match` statements, walrus, async def/await, decorators, `*args/**kwargs`, nested classes, `__init__.py` re-exports, relative imports (`from . import x`), `if TYPE_CHECKING:` imports, type annotations with `|`, PEP 695 `type X = ...` and `def f[T]()`. The arm merged this morning (PR #524): compare against `tests/fixtures/python/*.v5.jsonl` oracles for the shapes you find missing.

## Sample commands
```
X=$PWD/v6/sprefa-extract/target/release/extract
find <ROOT> -name '*.py' -type f > /private/tmp/claude-501/-Users-chrishafley-projects-sprefa/efe90f72-55fb-4958-b159-4982236661fe/scratchpad/python/files.txt
wc -l /private/tmp/claude-501/-Users-chrishafley-projects-sprefa/efe90f72-55fb-4958-b159-4982236661fe/scratchpad/python/files.txt
nohup bash -c 'while read f; do s=$(date +%s%N); out=$(timeout 10 $X "$f" 2>/private/tmp/claude-501/-Users-chrishafley-projects-sprefa/efe90f72-55fb-4958-b159-4982236661fe/scratchpad/python/err.tmp); rc=$?; e=$(( ($(date +%s%N)-s)/1000000 )); printf "%s\t%s\t%s\t%s\t%s\n" "$f" $rc $e $(printf "%s" "$out" | wc -l) "$(head -1 /private/tmp/claude-501/-Users-chrishafley-projects-sprefa/efe90f72-55fb-4958-b159-4982236661fe/scratchpad/python/err.tmp)"; done < /private/tmp/claude-501/-Users-chrishafley-projects-sprefa/efe90f72-55fb-4958-b159-4982236661fe/scratchpad/python/files.txt' > /private/tmp/claude-501/-Users-chrishafley-projects-sprefa/efe90f72-55fb-4958-b159-4982236661fe/scratchpad/python/runs.tsv 2>&1 &
```
Adapt for parallelism (split files.txt into 8 chunks, one loop each).
