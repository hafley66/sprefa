# graphviz_demo — import graph from a sprefa pipeline

Three-line `imports.sprf` extracts every `use MOD::ITEM` in the v3 server
crate. A small awk wrapper (`to_dot.sh`) folds the cursor stream into a
graphviz `.dot`.

## Run

```bash
# from repo root
v3/target/debug/sprefa-run \
  v3/crates/server/fixtures/graphviz_demo/imports.sprf \
  --root . 2>/dev/null \
  | v3/crates/server/fixtures/graphviz_demo/to_dot.sh \
  > /tmp/imports.dot

dot -Tsvg /tmp/imports.dot -o /tmp/imports.svg
dot -Tpng /tmp/imports.dot -o /tmp/imports.png
```

`BASENAME=0` on `to_dot.sh` keeps full paths instead of basenames.

## Pipeline shape

```
fs(glob(v3/crates/server/src/**/*.rs))
  > ast[rust](use ${MOD?}::${ITEM?})
  > print(:edge);
```

Each row binds `${MOD}` and `${ITEM}` against the `use foo::bar`
pattern; `print(:edge)` labels the cursor stream so `sprefa-run`'s
output formatter prints one line per match.

## Why a shell wrapper

In-language emission (`render[plain](...) > write_file(graph.dot)`)
needs `write_file` to support an append mode (or a stream-concat sink
op) so per-cursor lines accumulate into one file instead of clobbering
each other. The shell wrapper sidesteps that — sprefa does the
extraction; `awk` does the line-formatting; `dot` does the layout.

The cleaner in-language version is tracked under `sprefa-4m7.7`
(mutation effects + render). Likely shape:

```
str(`digraph imports {\n`) > write_file(graph.dot, :replace);
fs(glob(...)) > ast[rust](use ${MOD?}::${ITEM?})
  > render[plain](`  "${fs}" -> "${MOD}";\n`)
  > write_file(graph.dot, :append);
str(`}\n`) > write_file(graph.dot, :append);
```

That depends on:

1. `write_file` learning a `mode` arg (`:replace` / `:append`).
2. Backtick template strings supporting cursor-field reads (`${fs}`).
3. (optional) A snapshot-only drain on `tag?` so multi-pipe accumulation
   via `tag(:edge, $F, $T)` followed by `tag?(:edge, ${F?}, ${T?})`
   actually terminates.
