# INVENTORY.md

## Method

Read `v6/prolog/compile/SYNTAX.md` (the authority on spelling). Read the 397
files under `dl_view/`. Each construct row shows the `.dl6` spelling and the
count of files that use it, measured with `grep -rl <pattern> .` run from
`dl_view/`. "files" means the 397-file corpus.

## Corpus oracle

| item | value | evidence |
|---|---|---|
| files | 397 | `ls dl_view \| wc -l` |
| words | 8955 | brief |
| rel declarations | 308 files | `grep -rlE '^\s*rel '` |
| level rules (`<-`) | 249 files | `grep -rlE '<-'` |
| edge rules (`<+`) | 100 files | `grep -rlE '<\+'` |
| bare facts (`Head.`) | 53 files | `grep -rlE '\w\.$'` |

## Construct table

Count = number of the 397 files whose text contains the spelling.

| construct | `.dl6` spelling | files |
|---|---|---|
| rel declaration | `rel name(col: type, ...)` | 308 |
| typed column | `colname: text` / `: int` / `: bool` / `: float` / `: json` | see below |
| applied column type | `option(int)`, `list(text)`, `json_list(text)`, `option(list(...))` | see below |
| struct ref type | `at: span`, `at: plot` | 115 |
| module-path rel name | `orchard.tree(...)`, `orchard.north.tree(...)` | 17 |
| enum decl | `rel result(ok(v: text) ; error(m: text))` | 14 |
| retention `log` | `rel x(...) log` | 119 |
| retention `keep(all)` | `keep(all)` | 115 |
| `keyed` | `key(1, 2)` | 101 |
| `spann`/ref variant type | `: spann` | 3 |
| level rule | `Head <- Body.` | 249 |
| edge rule | `Head <+ Body.` | 100 |
| bare fact | `Head.` | 53 |
| `not(...)` negation | `not(atom)` | 36 |
| `pre(...)` sample | `pre(rel(...))` | 31 |
| `decode(E, {pattern})` | `decode(Body, {k: v})` | 44 |
| `json_each` | `json_each(Body, Item)` | 4 |
| `now(Var)` | `now(Tick)` | 7 |
| `next(...)` | `next(...)` | 2 |
| `seq(Var)` | `Ordinal := seq('q')` | 2 |
| `finalize(...)` | `finalize(rel(...))` | (in edge fixtures) |
| `coalesce(atom, literal)` | `coalesce(latest_commit(N,C), 'absent')` | 8 |
| `match` block | `match Source ( ; G |-> H ; G |+> H )` | 4 |
| `regexp(E, P)` | `regexp(...)` | 7 |
| `rtrim`/`replace` builtin | `Dir := rtrim(File, replace(File, '/', ''))` | 2 |
| `concat(...)` | `concat([...])` | (several) |
| comparison `==` | `Status == 200` | 14 |
| comparison `\==` | `\==` | 14 |
| comparison `=<` | `WaiverLine =< LineNumber` | 11 |
| comparison `=:=` | `=:=` | 1 |
| comparison `=\=` | `=\=` | 14 |
| comparison `>=`/`>` | `Count >= 2` | 17 / 46 |
| comparison `<` | in `<+`/`<-` (excluded) | 0 standalone |
| bind `:=` | `Next := SoFar + 1` | 42 |
| bind `is` | `X is E` | 0 (only in string) |
| arithmetic `+ - * / mod` | `Total + 1`, `Value * 2` | 66/156/2/4/1 |
| float literal | `1.5`, `-0.0` | 2 files as source |
| negative int | `-1` | 1 |
| atom constant | `'warning'`, `'none'`, `'z)z'` | many |
| string | `"text"`, `"eprintln at "` | many |
| backslash in atom | `'digit \\d here'` | 1 |
| braces object | `{key: value}` | 44 |
| braces value positions | value = var/term, key = bare/maybe quoted | 44 |
| typed capture | `{stars: Stars: int}` | several |
| key capture `$` | `{$Key: Value}` | 10 |
| descent `**` | `{**: {...}}` | 6 |
| array spread `[...` | `[... {n: Item}]`, `[... Tag]` | 5 |
| empty object `{}` | `decode(Value, {})` | 1 |
| quoted key | `{'name': v}` | (json plane) |
| list literal | `['eprintln at ', Path, ...]` | many |
| wildcard | `_`, `_Kind` | many |
| sh decl | `sh name(in...)->(out...)= \`tpl\`` | 10 |
| bind decl | `bind name(col: type, ...)` | 3 |
| query | `? rel(args).` | 6 |
| ts_query value | `ts_query([group(node(...))])` | 1 (native) |
| higher-order head | `demanded(fetch_of(Endpoint), Endpoint)` | several |
| head arithmetic | `union_size(L, R, Ls + Rs - Shared)` | several |

## Column type spectrum (from `grep -rhoE ': +[a-zA-Z_.]+(\([^)]*\))?'`)

| type | files |
|---|---|
| `text` | 377 |
| `int` | 341 |
| `json` | 50 |
| `span` | 22 |
| `float` | 20 |
| `fpath` | 18 |
| `repo` | 16 |
| `file` | 16 |
| `option(...)` | ~16 |
| `bool` | 6 |
| `json_list(text)` | 4 |
| `list(...)` | ~10 |
| `spann` | 3 |
| other single names (`Value`, `Name`, `node`, `view`...) | misc |

## Findings so far (no code yet)

1. Column-type slot is a full term grammar, not one token: `option(list_entity_dense_sequence(fighter_summary))` is a nested applied type. The decl arg grammar (name or `name: type`) must recurse.
2. Enum variants share the decl-arg shape: a name with a parenthesised, colon-typed field list, joined by `;` at the top level only.
3. Value/term grammar must be fully general: head args and body exprs are arbitrary term trees (calls, lists, braces, vars, atoms, strings, ints, floats, infix operators).
4. Dotted module-path names appear in both declaration and atom heads/bodies.
5. Operators are plentiful (comparison, bind, arithmetic); the body parser needs precedence handling so `WaiverLine >= LineNumber - 1` groups the subtraction first.
6. Round-trip is term equality after re-parse, so my printer only has to re-emit what my parser produced deterministically; it disposes of any reference-text expectation.
