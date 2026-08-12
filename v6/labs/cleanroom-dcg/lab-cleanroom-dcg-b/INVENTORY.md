# INVENTORY.md — construct -> spelling -> corpus count

Source: `v6/prolog/compile/SYNTAX.md` + `v6/prolog/compile/dl_view/*.dl6`
(397 files, 8955 words). Counts are files using the construct, via `grep -rl`
over the corpus. Only the canonical printed spelling appears in `dl_view`; the
alias spellings (`<=`, `!=`, `=`) are input-only and do not occur there.

## Declarations

| construct | `.dl6` spelling | files |
|---|---|---|
| rel declaration, untyped cols | `rel name(arg, arg, ...).` | 308 |
| keyed decl | `rel name(col...) key(1).` / `key(1,2).` | 101 |
| log modifier | `log` after columns | 114 |
| keep all | `keep(all)` | 112 |
| keep count | `keep(count(2))` | 6 |
| typed column, builtin | `col: int` / `text` / `float` / `json` / `bool` | 209 |
| custom type ref (struct/enum) | `col: span`, `col: fpath`, `col: repo` ... | 44 |
| compound type | `items: list(text)`, `payloads: json_list(json)`, `entries: option(list(text))` | 22 (option), 6 (list), 9 (json_list) |
| enum / variant decl | `rel body(page(view: int) ; redirect(to: text)).`, `none()` | 14 |
| sh host decl | `` sh name(in: type,...) -> (out: type,...) = `tpl`. `` | 10 |
| bind decl | `bind interval(period: int, bucket: int).` | 3 |
| query decl | `? name(args).` | 6 |

## Rules

| construct | `.dl6` spelling | files |
|---|---|---|
| level rule | `Head <- Body.` | 249 |
| edge rule | `Head <+ Body.` | 100 |
| match | `match Source ( ; G1 |-> H1 ; G2 |+> H2 ).` | 4 |
| bare fact | `Head.` (empty body) | 0 |

## Body items

| construct | `.dl6` spelling | files |
|---|---|---|
| plain relation call | `source(Text)` | ubiquitous |
| nested compound arg | `fresh(Tag, Body)`, `error(_)` | 30 (wildcard arg) |
| not | `not(total(Repo, _Prev))` | 36 |
| pre | `pre(log_text(Channel, SoFar))` | 31 |
| latest | `latest(subscriber(Client))` | 22 |
| finalize | `finalize(ev(Ordinal, Payload))` | 6 |
| now | `now(Tick)` | 7 |
| seq | `seq('ping')` / `seq('q')` | 2 |
| coalesce | `coalesce(latest_commit(Name, Commit), 'absent')` | 8 |
| decode | `decode(Body, {stars: N})` | 44 |
| regexp | `regexp(Text, "^a.c$")` | 7 |
| json_each | `json_each(Body, Item)` | 4 |
| bind `:=` | `Next := Prev + Stars` | 42 |
| bind `is` | `X is expr` | 0 |

## Comparison ops

| op | files |
|---|---|
| `==` | 14 |
| `\==` | 5 |
| `>=` | 17 |
| `>` | 39 |
| `<` | 12 |
| `=<` | 11 |
| `=:=` | 1 |
| `=:=` (arith) | 1 |

## Expressions

| construct | `.dl6` spelling | files |
|---|---|---|
| variable (bare id) | `Name` | ubiquitous |
| wildcard | `_` | 30 |
| single-quoted atom | `'warning'`, `'idle'`, `'none'`, `'absent'` | many |
| double-quoted string | `"unwrap-budget"`, `"^a.c$"` | many |
| integer / negative | `200`, `-1` | many |
| float | `0.2`, `0.30000000000000004` | 2 |
| list literal | `[e1, e2, ...]` | few (concat) |
| concat | `concat([Total, " non-test unwraps..."])` | 23 |
| arithmetic in head | `sum(Stars)`, `min(Stars)`, `Value + 0.2` | several |
| arithmetic in bind | `Prev + Stars`, `At + 1`, `Total - 1` | 42 |
| arithmetic `* / mod` | `Shared * 100 / Union`, `Numerator mod Denominator` | 1-2 |

## JSON braces (values and patterns)

| construct | `.dl6` spelling | files |
|---|---|---|
| object literal | `{repo: Name}`, `{stars: 4, name: Name}` | many |
| empty object | `{}` | few |
| key capture | `{$Key: Value}` | 6 |
| descent `**` | `{**: {leaf: Leaf}}` | 6 |
| array spread | `[... {n: Item}]` | 5 |
| typed capture | `{repo: Repo: text, stars: Stars: int}` | 4 |

## Notable spelling laws applied

1. A bare identifier is ALWAYS a variable; atom-literal constants are single-
   quoted (`'warning'`); strings are double-quoted. Seen in
   `unwrap_aggregate_and_interpolation.dl6` and `async_state_machine...`.
2. Relation/decl names, keywords (`rel`, `log`, `keep`, `key`, `sh`, `bind`,
   `match`, `not`, `pre`, `latest`, `finalize`, `now`, `seq`, `decode`,
   `regexp`, `json_each`, `coalesce`, `concat`) are lowercase bare identifiers
   in their respective non-value positions.
3. Less-equal prints as `=<`, not-equal as `\==`.
4. One file compares to bare `true` (`Enabled == true`): per law 1 that is a
   variable, and it round-trips as one.
5. `ts_query(...)` native term (`native_ts_query_term.dl6`) is deeply nested
   expr of atoms, strings, lists, compound calls; no exotic tokens.
6. sh template text is raw between backticks (`` `head -1 {path}` ``).
