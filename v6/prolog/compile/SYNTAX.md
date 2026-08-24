# SYNTAX.md -- phase D parser surface (parse_dl.pl / print_dl.pl)

Contract: `plans/2026-07-28-tsv2-phase-d-parser-header.md`. This file is the
term-form-construct -> `.dl6` spelling -> grammar-authority mapping the
contract asks for, plus every gap finding with evidence.

## Generic interface bounds

Generic parameters use interface applications:

```dl6
interface json_encodable(Format).
rel box(T: json_encodable(any))(value: T).
rel text_box(T: json_encodable(text))(value: T).
```

`T` is the implementing type. `any` is a wildcard for one complete
interface argument. Bare `T: comparable` remains a zero-argument bound.
Bounds check interface name, arity, and argument patterns. Structural compiler
proofs are erased before runtime lowering, while ordered arguments remain in
type metadata. Relation declarations carry compiler annotations through
ordinary type applications such as `key(int)`.

## Ruling that reframes this whole document (relayed mid-flight)

`v6/dl/grammar/dl.langium` was always a stopgap. Effective now, **the
prolog DCG in `parse_dl.pl` is the CANONICAL parser of the language**;
dl.langium is demoted to a reference for surface SPELLING only (so the two
existing real programs, `v6/dl/fixtures/ghcacher.dl6` and `conformance.dl6`,
keep parsing), not a permanent authority. Every row below marked EXT is
therefore not "waiting on a grammar change" -- the grammar will not change to
catch up. This parser's accepted surface **is** the language definition
after merge. The GAP rows (dl.langium has a construct this term form cannot
express, or vice versa) are still named honestly, with grammar line
evidence, because that evidence is exactly the phase D gap report the user
asked for.

## The central superseding decision: bare identifier = variable, always

`dl.langium`'s `Var` rule (`name=ID`, `dl.langium:153-154`) makes **every**
bare identifier a variable -- there is no unquoted-atom-literal production
anywhere in the grammar (`ArgTerm := Var | Literal | Wildcard`,
`dl.langium:150-151`; `Literal := StrLit | IntLit | BoolLit | NullLit`,
`dl.langium:165-166` -- no `AtomLit`). That is provably correct for the two
real `.dl6` files: grepped both, neither ever writes a bareword atom constant;
every constant is a quoted string (`"repos/cli/cli"`) or an int (`200`).

The term-form corpus needs the opposite in places: a bareword
constant-tag match is a real, critical construct:
`fixtures/state_machine.pl`'s `phase(Endpoint, fetching)` matches the exact
atom `fetching`, not a fresh variable. Since this parser is now canonical
for **both** surfaces, it resolves the tension with one rule instead of two
dialects: **a bare identifier is always a variable; an atom-literal constant
is always single-quoted** (`'fetching'`, `'idle'`, `'none'`), a string is
always double-quoted (`"eprintln-exceeded"`, matching `StrLit` exactly).
This costs the real files nothing (they never wrote an unquoted atom
constant to begin with) and lets the term-form corpus's constant-tag
matches round-trip exactly. `print_dl.pl` always quotes atom literals for
this reason -- never Prolog's own `~q` "quote only if necessary" -- see
`parse_dl.pl`'s and `print_dl.pl`'s module headers for the full argument.

## Construct table

Generated from `registry.pl` by `1_emit_registry_docs.pl`. The row order is
the compiler inventory order. Edit the registry, then run the emitter. The
status column labels the registry surface: `live` rows have compiler wiring,
while `refused` and `reserved` rows name refusal-only surface. Context-specific
theorems can refuse a live row; those cases are listed immediately below.

`latest/1` is live around one plain relation atom in an edge body. Its
sampled atom reads the current base table and never becomes a trigger.
Level-rule use remains `latest_in_level_rule`; wider edge arguments remain
`edge_body_with_latest`.

`finalize/1` is live in an edge body only, and the update arm plus the
pairwise idiom are both spelled with it:

    changed(Key, Old, New) <+ finalize(reading(Key, Old)), reading(Key, New).

WHAT IT PAIRS, stated because the answer is cadence-dependent and both doors
agree on it: a departure is a next-tick occurrence, so the second atom reads
the relation as it stands ONE TICK AFTER the replace that produced the
departure. `TICK-MODEL.md` section 2 writes the arm as `(dS)- at t JOIN S at
t` and section 3 grades `finalize` at +1; those two lines together are this
behavior. Values 10, 14, 9 arriving on consecutive ticks pair as (10, 9) and
(14, 9); the same program over a source that idles for one tick after each
change pairs adjacent values, (10, 14) and (14, 9). Fixtures
`pairwise_reads_state_at_the_departure_tick` and
`pairwise_pairs_adjacent_values_when_the_source_idles` pin both cadences, and
`engine_core.pl` records the two probes taken while grading them: `latest(...)`
around the second atom changes nothing, and the one same-tick candidate,
`pre(...)` beside a bare arrival trigger, pairs every value with itself. rx
`pairwise()` keeps the previous value inside the operator, so it has no
equivalent cadence sensitivity; this idiom keeps it in a delta that has to
survive a tick before anything reads it.

`seq/1` is live in an edge-body value bind. An atom argument gives one global
ordinal stream; a variable argument gives one stream per value. The shared
expander mints a keyed cursor relation and emits the four-rule cursor block
used by both doors. The cursor rows remain visible in the tick log. Level-rule
use remains `seq_in_level_rule`.

`coalesce(rel_atom(Bound..., Out), Default)` is the TOTAL read (ruling
`null_design = get_else_use_site_never_storage`). `Out` binds from the matching
row when one exists and from `Default` when none does, so the tuple survives
either way -- the outer-join effect, spelled at the use site by the consumer
that wants it. Null never enters storage or the type system; absence stays row
absence. Exactly one variable in the atom may be unbound by the rest of the
body, and `Default` must be a literal value of that column's own type.

It is SUGAR: `0_coalesce_expand.pl` rewrites one such rule into two ordinary
clauses -- the read, and `not(...)` plus a `:=` of the default -- before either
door sees the program, so it inherits the shipped incremental delta path, the
negation path's retraction flip and the naive referee rather than adding a
lowering. In an EDGE body the read arm is `latest(...)`, because a bare atom
there is a trigger and an optional lookup must not become a firing source.
Refusals: `coalesce_no_output`, `coalesce_multiple_outputs`,
`coalesce_output_not_column`, `coalesce_default_not_literal`,
`coalesce_source_not_rel_atom`, `coalesce_not_top_level`, `coalesce_in_head`.

<!-- BEGIN GENERATED surface/5 TABLE -->
| signature | axis | analyze role | lower role | status (writable surface) |
|---|---|---|---|---|
| `latest/1` | `sample` | `refs_of_arg(1,pos,sampled)` | `wrapper(rel_atom,lower)` | `live` |
| `finalize/1` | `time` | `refs_of_arg(1,pos,trigger)` | `wrapper(rel_atom,lower)` | `live` |
| `next/1` | `time` | `splice_bare` | `wrapper(rel_atom,lower)` | `live` |
| `combine/variadic` | `join` | `splice_bare` | `wrapper(atom_list,lower)` | `live` |
| `zip/2` | `join` | `splice_bare` | `wrapper(atom_list,refuse(functor))` | `reserved` |
| `unsubscribe/1` | `time` | `refs_of_arg(1,pos,trigger)` | `wrapper(rel_atom,refuse(lifecycle))` | `reserved` |
| `complete/1` | `time` | `refs_of_arg(1,pos,trigger)` | `wrapper(rel_atom,refuse(lifecycle))` | `reserved` |
| `subscribe/1` | `time` | `refs_of_arg(1,pos,trigger)` | `wrapper(rel_atom,refuse(lifecycle))` | `reserved` |
| `error/1` | `time` | `refs_of_arg(1,pos,trigger)` | `wrapper(rel_atom,refuse(lifecycle))` | `reserved` |
| `not/1` | `sign` | `arm(neg)` | `wrapper(body_item,lower)` | `live` |
| `coalesce/2` | `sugar` | `refs_of_arg(1,pos,sampled)` | `wrapper(rel_atom_default,expand(coalesce))` | `live` |
| `pre/1` | `sample` | `refs_of_arg(1,pos,sampled)` | `wrapper(rel_atom,lower)` | `live` |
| `pre/2` | `sample` | `refs_of_arg(1,pos,sampled)` | `wrapper(rel_atom_default,lower)` | `live` |
| `seq/1` | `sugar` | `no_refs` | `wrapper(expr,expand(seq))` | `live` |
| `now/1` | `time` | `no_refs` | `wrapper(expr,lower)` | `live` |
| `decode/2` | `guard` | `no_refs` | `wrapper(expr_pair,lower)` | `live` |
| `json_each/2` | `guard` | `no_refs` | `wrapper(expr_pair,refuse(goal))` | `refused` |
| `{}/1` | `json` | `no_refs` | `value(json_object_shape)` | `live` |
| `{}/0` | `json` | `no_refs` | `value(json_empty_object)` | `live` |
| `spread/1` | `json` | `no_refs` | `value(json_array_spread)` | `live` |
| `$/1` | `json` | `no_refs` | `value(json_hole)` | `live` |
| `**/0` | `json` | `no_refs` | `value(json_descent)` | `live` |
| `tagged_brace/1` | `json` | `no_refs` | `value(refuse(tagged_brace_reserved))` | `reserved` |
| `true/0` | `guard` | `no_refs` | `word(lower)` | `live` |
| `:=/2` | `bind` | `no_refs` | `infix(lower)` | `live` |
| `is/2` | `bind` | `no_refs` | `infix(lower)` | `live` |
| `</2` | `guard` | `no_refs` | `infix(lower)` | `live` |
| `=</2` | `guard` | `no_refs` | `infix(lower)` | `live` |
| `>/2` | `guard` | `no_refs` | `infix(lower)` | `live` |
| `>=/2` | `guard` | `no_refs` | `infix(lower)` | `live` |
| `==/2` | `guard` | `no_refs` | `infix(lower)` | `live` |
| `\==/2` | `guard` | `no_refs` | `infix(lower)` | `live` |
| `=:=/2` | `guard` | `no_refs` | `infix(lower)` | `live` |
| `=\=/2` | `guard` | `no_refs` | `infix(lower)` | `live` |
| `regexp/2` | `guard` | `no_refs` | `wrapper(expr_pair,lower)` | `live` |
| `count/1` | `aggregate` | `no_refs` | `head(lower)` | `live` |
| `sum/1` | `aggregate` | `no_refs` | `head(lower)` | `live` |
| `min/1` | `aggregate` | `no_refs` | `head(lower)` | `live` |
| `max/1` | `aggregate` | `no_refs` | `head(lower)` | `live` |
| `avg/1` | `aggregate` | `no_refs` | `head(lower)` | `live` |
| `json_array/1` | `aggregate` | `no_refs` | `head(refuse(aggregate))` | `refused` |
| `json_object/2` | `aggregate` | `no_refs` | `head(lower)` | `live` |
| `json_group_array/1` | `aggregate` | `no_refs` | `head(lower)` | `live` |
| `json_group_array/2` | `aggregate` | `no_refs` | `head(lower)` | `live` |
| `group_concat/2` | `aggregate` | `no_refs` | `head(lower)` | `live` |
| `group_concat/3` | `aggregate` | `no_refs` | `head(lower)` | `live` |
| `group_concat/1` | `aggregate` | `no_refs` | `head(lower)` | `live` |
| `enum_decl/2` | `decl` | `no_refs` | `decl(enum_variants)` | `live` |
| `;/2` | `decl` | `no_refs` | `decl(enum_variant_separator)` | `live` |
| `col_type/3` | `decl` | `no_refs` | `decl(column_type)` | `live` |
| `type_decl/2` | `decl` | `no_refs` | `decl(struct_type)` | `live` |
| `set/0` | `decl` | `no_refs` | `decl(refuse(removed_word))` | `refused` |
| `scan/variadic` | `world` | `no_refs` | `goal(refuse(removed_word))` | `reserved` |
| `match/2` | `sugar` | `no_refs` | `block(match_arms)` | `live` |
| `sh_decl/4` | `world` | `no_refs` | `decl(host_plan)` | `live` |
| `arrival_identity/2` | `world` | `no_refs` | `decl(arrival_identity)` | `live` |
| `probe/4` | `world` | `no_refs` | `wrapper(host_probe,lower)` | `live` |
| `query/1` | `read` | `no_refs` | `decl(query_plan)` | `live` |
| `ts_query/1` | `world` | `no_refs` | `value(tree_sitter_query)` | `live` |
| `sg_pattern/3` | `world` | `no_refs` | `value(refuse(slot_sg_metavariable_semantics))` | `refused` |
<!-- END GENERATED surface/5 TABLE -->

## CLI ("the bop")

Generated from `registry.pl`'s `cli_command/3` rows by `1_emit_registry_docs.pl`,
the same emitter, a second table. `v6/tsv2/cli/bop.ts` wires the identical five
verbs through commander; the row order here is the registry's own order.
`run` and `check` boot the served tsv2 engine **in-process** (server-calls-
itself, no daemon concept); `serve` is the long-running entry `run`/`check`
each start privately and tear down on exit. Exit codes are `check`'s own
contract, applied consistently wherever `run`/`load` hit the same compile
door: 0 clean, 2 named-refusal findings (`unsupported_construct` and its
sibling throw shapes -- see `scripts/bop_check.pl`'s own header), 1 broken
(a program that does not parse, or any other uncaught fault).

<!-- BEGIN GENERATED cli_command/3 TABLE -->
| verb | args | summary |
|---|---|---|
| `bop serve` | `[--port <port>] [--db <url>]` | boot the served tsv2 engine and keep it running (exactly serve/main.ts). |
| `bop run` | `<file.dl6> [--ticks <n>] [--port <port>]` | compile + load a program on an in-process ephemeral server, stream ticks to stdout until quiescent or --ticks fires, then shut down cleanly. |
| `bop check` | `<file.dl6>` | validate a program through the text door; no server boots. Exit 0 clean, 2 named-unsupported construct findings, 1 broken (parse/compile error). |
| `bop load` | `<file.dl6> [--port <port>]` | POST a compiled program to an already-running bop serve; exit 1 if nothing is listening. |
| `bop q` | `<rel> [--port <port>] [--json]` | read one rel's current rows from a running bop serve. |
| `bop stats` | `[--port <port>]` | read process and SQLite storage statistics from a running bop serve. |
| `bop ticks` | `[--port <port>]` | stream served tick events from a running bop serve until interrupted. |
<!-- END GENERATED cli_command/3 TABLE -->

### Context status

| construct | level body | edge body |
|---|---|---|
| `latest/1` | refused as `latest_in_level_rule(Ref)` | live around ONE plain relation atom (sampled base-table read, never a trigger); wider arguments refused as `edge_body_with_latest(Body)` |
| `not/1` | live (NOT EXISTS), a guard nested inside it refused as `negated_guard_goal(Head, Goal)` | live around ONE plain relation atom; wider arguments refused as `edge_body_with_negation(Body)` |
| `now/1` | refused as `now_in_level_rule(Head, Goal)` -- compiler-only, the oracle solves it there | live around a plain VARIABLE (reads the emitted `__tick` counter); a non-variable argument refused as `edge_body_with_now(Body)` |
| `pre/1` | refused as `pre_in_level_rule(Ref)` | live around one plain relation atom; the occurrence-ordered keyed fold reads the evolving row between edge occurrences |
| `seq/1` | refused as `seq_in_level_rule` | live only as the ordinal value bind in an edge rule; other placements are refused |
| comparisons, `:=`, `is` | live (WHERE / SELECT expressions) | live, same three compilers, folded after the positive atoms |

### Core grammar and input aliases

These rows describe syntax outside the registered body and aggregate
construct inventory.

| term-form shape | `.dl6` spelling | parser treatment |
|---|---|---|
| `kind(Ref, log)` | `log` after columns | declaration modifier |
| `col_type(Ref, Column, Type)` | `Column: int` / `Column: text` | typed declaration entry; source order is preserved |
| `type_decl(Name, [col(Column, Type), ...])` | `rel name(column: type, ...).` referenced from another column type | relation-valued row; values are storage-plane dictionary rows keyed on canonical content |
| `col_type(Ref, Column, TypeName)` | `Column: span` | ref column; stores the dictionary id, renders the value at the boundary |
| removed `set` word | `set` after columns | `unsupported_surface(removed_word(set))` |
| `keep(Ref, all\|count(N))` | `keep(all)` / `keep(count(N))` | declaration modifier |
| `keyed(Ref, Positions)` | `key(P, P, ...)` | declaration modifier |
| `(Head <- Body)` | `Head <- Body.` | level rule |
| `(Head <+ Body)` | `Head <+ Body.` | edge rule |
| `match(Source, ((Head <- Guards) ; (Head <+ Guards)))` | `match Source ( ; Guards \|-> Head ; Guards \|+> Head )` | retained sugar; optional first `;`; left-to-right arms become one ordinary rule each |
| bare fact | `Head.` | body becomes registered `true/0` |
| bare positive relation | `name(args)` | trigger relation |
| comparison alias `<=` | input only | maps to registered `=</2` |
| comparison alias `!=` | input only | maps to registered `\==/2` |
| comparison alias `=` | input only | maps to registered `==/2` |
| arithmetic `+ - * / mod` | infix with precedence-preserving parentheses | expression grammar |
| `concat([e1, e2, ...])` | same call shape | general compound expression |
| `'{}'(Pairs)` | `{key: value, ...}` | braces expression |
| list | `[e1, e2, ...]` | list expression |
| wildcard | `_` | fresh anonymous variable |
| named variable | bare identifier | file-wide variable identity |
| atom constant | `'text'` | always single-quoted |
| string | `"text"` | SWI string |
| integer | `123` / `-123` | integer |
| named args | `col: val` | resolved to declared positional order |
| body named args with omitted columns | `rel(first: Value)` | omitted declared columns become fresh anonymous variables; RX relation projection |
| partial named head | `head(first: Value) <- ...` | `unsupported_surface(partial_head(Name/Arity))` |
| executor module import | `use soopy.` / `use soopy as sy.` | `use_mod(Family)` / `use_mod(Family, Alias)`; the family is an `arrival_executor/2` name's first `__` segment. The file's declaration of an exported leaf (`rel files(...)` bare, `rel sy.files(...)` aliased) is renamed to the registry's `soopy__files` in decls, rules and queries, so all four spellings emit one program. An unrostered family is `unsupported_construct(unknown_executor_module(Family))`; two used families claiming one leaf is `ambiguous_executor_leaf(Leaf, Families)`; ruling executor_modules_use_import |
| arrival rel declaration | `rel name(in: type, ...) -> (out: type, ...) key(P, ...).` | `sh_decl(Name, Inputs, Outputs, template(""))` plus `arrival_identity(Name, Positions)` from `key(..)`; RX-H1; ruling arrival_arrow_spelling; the `sh` and `bind` keywords answer `unsupported_surface(removed_word(..))` |
| host call | `name(inputs..., outputs...)` when `name` resolves to an arrival signature | `probe(Name, IdentityInputs, Outputs, FreshnessSalts)`; RX-H2; `key(..)` positions (falling back to registered contracts) select freshness inputs; an unresolved name remains an ordinary relation atom |
| query | `? name(args).` | `query(RelAtom)`; RX-Q1 |
| ordered query | `? name(args) order by col [asc\|desc], ... .` | `query(RelAtom, order([order_col(Position, Direction), ...]))`; RX-Q2; `asc` is the unwritten direction; a column the query's args do not name is `dl_parse_error(order_column_unknown(Name, Column), _)` |
| mutation | `rel!(args)` | `unsupported_surface(mutation(Name/Arity))` |
| `true` / `false` as values | unavailable | bare identifiers remain variables in argument position |
| `null` | unavailable | no term-form mapping |

### The json plane

`json` is a column type, and the brace grammar is one grammar with two roles:
the LITERAL is the PATTERN minus holes. Which lowering a brace pattern gets is
decided by the SOURCE COLUMN'S DECLARED TYPE, never by the pattern -- a
declared struct becomes a dictionary join, a `json` column becomes json1 SQL.

| term-form shape | `.dl6` spelling | role | ruling |
|---|---|---|---|
| `'{}'(Pairs)` | `{key: value, ...}` | object literal / open object pattern | `json5_subset = unquoted_keys_only` |
| `'{}'` (arity 0) | `{}` | empty object; matches any object, binds nothing | term-door agreement |
| `spread(Pattern)` | `[... pattern]` | array fan-out, one row per element | the gh-cache flagship |
| `$(Var)` as a KEY | `$name` | key capture; the key is data | `json_key_hole_marker = dollar` |
| `$(Var)` as a VALUE | `$name` | alias for the bare variable (text door) | same ruling |
| `'**'` as a KEY | `**` | descent at any depth, root included | `descent_depth_cap = uncapped` |
| quoted key | `{'name': v}` / `{"name": v}` | literal label, never a hole | `string_quote = both_parse` |
| `json_list(T)` | `tags: json_list(text)` | typed view over the json array carrier | `list_spelling = list_of_type` |
| `Var : Type` as a VALUE | `{stars: Stars: int}` | typed capture: bind AND require the json type | `decl_column_spelling`, one level down |
| tagged brace | `Tag{...}` / `_{...}` | `unsupported_construct(tagged_brace_reserved(Tag))` | reserved |

A TYPED CAPTURE is the value-plane counterpart of a column's colon type, and
it exists because an untyped hole has no type to give: `json_extract` carries
no declared column type, so `lower.pl` types a bare hole `text` and a json
number cannot reach an `int` column through an undeclared intermediate rel
(`unsupported_construct(edge_head_column_type_mismatch(total/2,2,text,int))`).
`:` is 600 xfy in SWI, so `stars: Stars: int` already reads as
`:(stars, :(Stars, int))` and the term door needs no new shape.

The type is a MATCHER, not a cast, and it is checked on both doors --
`json_type(<path>) = 'integer'` in the emitted WHERE, `integer/1` in
`body.pl:json_capture_type/2`. A value of the wrong json type contributes no
row, exactly as an absent key does; SQL cannot raise a named refusal from a
WHERE clause, so a throwing oracle would disagree with a filtering emitter on
every such document. The program-level mistake -- a `text` capture feeding an
`int` column -- stays loud at compile time through
`edge_head_column_type_mismatch`.

Live capture types: `int`, `float`, `text`, one per json1 `json_type` answer.
Anything else, `bool` included, is `json_capture_type_unknown(Type)`. `bool` is
refused rather than defined because a top-level json `true` DOCUMENT was
measured degrading to the integer `1` through the real emitted arrival
statement (json_flex card C4), so its storage is an open card.

A json column's ARRIVAL is its document TEXT, on every door
(`serve/4_http.ts`: "a json document arrives as its text"). The schedule entry
for `rel event(payload: json)` is the JSON STRING `"{\"repo\":\"cli\"}"`, not a
raw JSON object; `compile/scripts/dl6_oracle.pl` parses that text into the
oracle's own json terms so the two doors read one spelling.

Bareness is the literal marker on the KEY plane and quoting is the literal
marker on the VALUE plane, and that is forced rather than chosen: JSON5 permits
unquoted keys and forbids unquoted string values, so the value slot is free for
dl6 variables and the key slot is not. Every key-axis production is therefore
PATTERN-ONLY, forever -- constructing an object with a computed key is
`json_object(Key, Value)`, never a brace literal.

NOT taken out of JSON5: trailing commas, and `#` comments inside a brace.

`Tag{...}` is reserved by measurement, not preference. SWI reads `_{a: 1}` and
`point{x: 1}` as DICTS, a term shape `{}`/1 can never unify with, so the term
door could never agree with a text door that read them as json. The refusal
also keeps the spelling free for the stated future use of `{` beyond json.

Cost, in joins: an exact key at any depth is 0 (one accumulated `json_extract`
path); array spread, key capture and key wildcard are 1 (`json_each`); `**` is
1 (`json_tree`, whose `fullkey` rides the same join). Statement counts stay
flat per rule.

### Legacy surface: parsed, then refused

These spellings remain in `parse_dl.pl` because current `.dl6` files use
them. The parser retains the declaration shape and returns the named finding;
the compiler does not treat the resulting declaration as writable surface.

| spelling | retained parser shape | finding |
|---|---|---|
| `rel(N) Name(...)` | ordinary `rel Name(...)` declaration plus retention value `N` | `unsupported_surface(retention_marker(Ref, N))` |
| `Key(text)` / `Min(int)` / `Max(int)` | ordinary column position with wrapper type omitted from the term declaration | `unsupported_surface(column_type_wrapper(Ref, Column, Wrapper))` |

### World term lowering rows

| term | rx lowering | phase-1 compiler result |
|---|---|---|
| `sh_decl(Name, Inputs, Outputs, template(""))` | RX-H1: request rows group by witness, take one request, decode declared outputs, then commit an EDB arrival | emitted as a `hostPlans` data row whose `execution` is the registry's `arrival_executor/2` slash path (`/soopy/files`, `/extract/records`, ...) or the `shell` sentinel for a replay-only feeder; the Rust runtime links every rostered name in-process (hosts.rs LINKED_EXECUTORS). `Name` is the `__` join a program reaches through `use soopy.` plus a bare `files`, through `soopy.files`, or through `/soopy/files` |
| `probe(Name, Inputs, Outputs, Salts)` | RX-H2: mint identity from host plus identity inputs, mint witness from identity plus compiler-registered freshness inputs, deduplicate by witness, then demand the host | lowers to `__host_demand_Name` SQL and a join with keyed EDB relation `__host_response_Name`; `Salts` is internal IR with no DL6 spelling |
| `query(RelAtom)` | RX-Q1: scan the current SQLite query plan and stream its rows | emitted as a `queryPlans` data row |
| `query(RelAtom, order(OrderCols))` | RX-Q2: RX-Q1 then `sortBy` the named columns, each with its direction | the same `queryPlans` row plus an `ORDER BY` clause on the rel's `final_select` cursor, and nowhere else; an order index rides the rel's DDL only when the ordered read hits the base table |
| `ts_query(Patterns)` | RX-TS1: group file demand by content and query identity, run the compiled tree-sitter query, then commit EDB rows | value compiles to query text; phase-2 host execution is named `unsupported_host_execution_phase_2(tree_sitter_query)` |
| `sg_pattern(language(Language), source(Text), captures(Names))` | RX-SG1: group file demand by content and pattern identity, run ast-grep, then commit EDB rows | retained as a separate pattern family; current compiler refusal is `unmapped_feature(slot_sg_metavariable_semantics, Term)` |

Host declarations and calls contain one ordinary positional input list. Exact
compiler registry rows can mark selected positions as witness freshness inputs;
local shell declarations default every position to identity. The printer
reconstructs the ordinary input order from the same metadata.

File and content hosts use the current worktree when no revision is present.
A pinned revision is written as a marked argument or a sibling host. There is
no required source atom.

## Round-trip design note (why decl lines are exact, not fallback-merged)

G1 is a `=@=` variant check over `prog(Decls, Rules)`, and `=@=` over a LIST
is position- and content-exact: `[a, b] =@= [b, a]` is false, and
`[kind(r,log), keep(r,all)] =@= [kind(r,log)]` is false even though
`decl_keep/3`'s own fallback makes them mean the same thing at analysis
time. `print_dl.pl` therefore reproduces the LITERAL `kind`/`keep`/`keyed`/
`col_type` entries a ref has in the original `Decls` list, in their original relative
order -- never `rel_kind/3`/`decl_keep/3`/`decl_key/3`'s fallback-merged
view, and never a synthesized decl line for a ref that has zero entries
(the extreme case: `expressions.pl`'s fixtures all have `Decls = []` even
though their rules reference many rels -- the printed `.dl6` text correctly
shows zero decl lines for those, with the rule text alone still revealing
every ref's name, arity, and column names via `analyze.pl:rel_columns/5`).

## Grades (from `scripts/roundtrip.sh`, regenerate to reproduce)

- **G1**: 136 / 136 fixtures round-trip (`parse_dl(print_dl(Term)) =@= Term`
  for every `fixture/5` in `v6/prolog/conformance/fixtures/*.pl`).
- **G2**: both real files parse without error.
  - `ghcacher.dl6`: Decls 19, Rules 9, Queries 2, 0 findings. The selected
    host declarations, probes, and queries are first-class `program/3` terms.
  - `conformance.dl6`: Decls 29, Rules 28, 0 findings (the named/positional
    mix resolves silently, per the construct table above).
- **G3**: `v6/prolog/conformance/go.pl`, 136 pass / 0 fail.

## What `dl_view/*.dl6` is

Every fixture in the 136-fixture corpus, printed as `.dl6` text by this
parser's own printer, committed under `v6/prolog/compile/dl_view/`. This is
the "language you can see" deliverable: inspect any file there to read a
conformance fixture's PROGRAM (not its test scaffolding -- `Initial`,
`Schedule`, and `Expectations` are deliberately not printed, since they are
harness concepts, not part of `prog(Decls, Rules)`) as ordinary source text
instead of a Prolog term. Regenerate via `scripts/roundtrip.sh` (G1's run
writes every file as a side effect).
