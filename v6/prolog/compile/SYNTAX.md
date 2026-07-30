# SYNTAX.md -- phase D parser surface (parse_dl.pl / print_dl.pl)

Contract: `plans/2026-07-28-tsv2-phase-d-parser-header.md`. This file is the
term-form-construct -> `.dl6` spelling -> grammar-authority mapping the
contract asks for, plus every gap finding with evidence.

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
| `pre/1` | `sample` | `refs_of_arg(1,pos,sampled)` | `wrapper(rel_atom,refuse(goal))` | `refused` |
| `now/1` | `time` | `no_refs` | `wrapper(expr,lower)` | `live` |
| `decode/2` | `guard` | `no_refs` | `wrapper(expr_pair,refuse(goal))` | `refused` |
| `json_each/2` | `guard` | `no_refs` | `wrapper(expr_pair,refuse(goal))` | `refused` |
| `true/0` | `guard` | `no_refs` | `word(lower)` | `live` |
| `:=/2` | `bind` | `no_refs` | `infix(lower)` | `live` |
| `is/2` | `bind` | `no_refs` | `infix(lower)` | `live` |
| `</2` | `guard` | `no_refs` | `infix(lower)` | `live` |
| `=</2` | `guard` | `no_refs` | `infix(lower)` | `live` |
| `>/2` | `guard` | `no_refs` | `infix(lower)` | `live` |
| `>=/2` | `guard` | `no_refs` | `infix(lower)` | `live` |
| `==/2` | `guard` | `no_refs` | `infix(lower)` | `live` |
| `\==/2` | `guard` | `no_refs` | `infix(lower)` | `live` |
| `count/1` | `aggregate` | `no_refs` | `head(lower)` | `live` |
| `sum/1` | `aggregate` | `no_refs` | `head(lower)` | `live` |
| `min/1` | `aggregate` | `no_refs` | `head(lower)` | `live` |
| `max/1` | `aggregate` | `no_refs` | `head(lower)` | `live` |
| `json_array/1` | `aggregate` | `no_refs` | `head(refuse(aggregate))` | `refused` |
| `json_object/2` | `aggregate` | `no_refs` | `head(refuse(aggregate))` | `refused` |
| `enum_decl/2` | `decl` | `no_refs` | `decl(enum_variants)` | `live` |
| `;/2` | `decl` | `no_refs` | `decl(enum_variant_separator)` | `live` |
| `col_type/3` | `decl` | `no_refs` | `decl(column_type)` | `live` |
| `type_decl/2` | `decl` | `no_refs` | `decl(struct_type)` | `live` |
| `set/0` | `decl` | `no_refs` | `decl(refuse(removed_word))` | `refused` |
| `match/2` | `sugar` | `no_refs` | `block(match_arms)` | `live` |
| `sh_decl/4` | `world` | `no_refs` | `decl(host_plan)` | `live` |
| `probe/4` | `world` | `no_refs` | `wrapper(host_probe,lower)` | `live` |
| `bind_decl/2` | `world` | `no_refs` | `decl(bind_plan)` | `live` |
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
| `bop check` | `<file.dl6>` | validate a program through the text door; no server boots. Exit 0 clean, 2 named-refusal findings, 1 broken (parse/compile error). |
| `bop load` | `<file.dl6> [--port <port>]` | POST a compiled program to an already-running bop serve; exit 1 if nothing is listening. |
| `bop q` | `<rel> [--port <port>] [--json]` | read one rel's current rows from a running bop serve. |
<!-- END GENERATED cli_command/3 TABLE -->

### Context status

| construct | level body | edge body |
|---|---|---|
| `latest/1` | refused as `latest_in_level_rule(Ref)` | live around ONE plain relation atom (sampled base-table read, never a trigger); wider arguments refused as `edge_body_with_latest(Body)` |
| `not/1` | live (NOT EXISTS), a guard nested inside it refused as `negated_guard_goal(Head, Goal)` | live around ONE plain relation atom; wider arguments refused as `edge_body_with_negation(Body)` |
| `now/1` | refused as `now_in_level_rule(Head, Goal)` -- compiler-only, the oracle solves it there | live around a plain VARIABLE (reads the emitted `__tick` counter); a non-variable argument refused as `edge_body_with_now(Body)` |
| `pre/1` | refused as `pre_in_level_rule(Ref)` | refused as `edge_body_needs_pre(Body)` -- the fold is occurrence-ordered and cross-arm; see the "pre" note in SCOREBOARD.md |
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
| shell host declaration | `sh name(in: type, ...) -> (out: type, ...) = \`template\`.` | `sh_decl(Name, Inputs, Outputs, template(Text))`; RX-H1 |
| host call | `name(inputs..., outputs...)` when `name` resolves to an `sh` signature | `probe(Name, IdentityInputs, Outputs, FreshnessSalts)`; RX-H2; registered positional metadata selects freshness inputs; an unresolved name remains an ordinary relation atom |
| bind declaration | `bind name(column: type, ...).` | `bind_decl(Name, Columns)`; RX-B1 |
| query | `? name(args).` | `query(RelAtom)`; RX-Q1 |
| mutation | `rel!(args)` | `unsupported_surface(mutation(Name/Arity))` |
| `true` / `false` as values | unavailable | bare identifiers remain variables in argument position |
| `null` | unavailable | no term-form mapping |

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
| `sh_decl(Name, Inputs, Outputs, template(Text))` | RX-H1: request rows group by witness, take one request, decode declared outputs, then commit an EDB arrival | emitted as a `hostPlans` data row carrying executor key `execution: "shell"`, or `"sprefa_extract"` for the established `extract(path, digest)` contract; the served runtime (`v6/tsv2/serve/1_hosts.ts`) runs the declaration template and commits the decoded response as an EDB arrival |
| `probe(Name, Inputs, Outputs, Salts)` | RX-H2: mint identity from host plus identity inputs, mint witness from identity plus compiler-registered freshness inputs, deduplicate by witness, then demand the host | lowers to `__host_demand_Name` SQL and a join with keyed EDB relation `__host_response_Name`; `Salts` is internal IR with no DL6 spelling |
| `bind_decl(interval, Columns)` | RX-B1: subscribe to the registered interval source while the program is active and commit each row as EDB | emitted as a `bindPlans` data row carrying `periods` (the integer literals the program's own rules read in the bind atom's first column) and `execution: "live_interval"`; schedule arrivals still grade phase 1, and the served runtime (`v6/tsv2/serve/2_binds.ts`) spins one rx `interval` per declared period |
| `query(RelAtom)` | RX-Q1: scan the current SQLite query plan and stream its rows | emitted as a `queryPlans` data row |
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
