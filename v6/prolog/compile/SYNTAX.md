# SYNTAX.md -- phase D parser surface (parse_dl.pl / print_dl.pl)

Contract: `plans/2026-07-28-tsv2-phase-d-parser-header.md`. This file is the
term-form-construct -> `.dl` spelling -> grammar-authority mapping the
contract asks for, plus every gap finding with evidence.

## Ruling that reframes this whole document (relayed mid-flight)

`v6/dl/grammar/dl.langium` was always a stopgap. Effective now, **the
prolog DCG in `parse_dl.pl` is the CANONICAL parser of the language**;
dl.langium is demoted to a reference for surface SPELLING only (so the two
existing real programs, `v6/dl/fixtures/ghcacher.dl` and `conformance.dl`,
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
real `.dl` files: grepped both, neither ever writes a bareword atom constant;
every constant is a quoted string (`"repos/cli/cli"`) or an int (`200`).

The 112-fixture term-form corpus needs the opposite in places: a bareword
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
the compiler inventory order. Edit the registry, then run the emitter.

<!-- BEGIN GENERATED surface/5 TABLE -->
| signature | axis | analyze role | lower role | status |
|---|---|---|---|---|
| `latest/1` | `sample` | `refs_of_arg(1,pos,sampled)` | `wrapper(rel_atom,lower)` | `live` |
| `finalize/1` | `time` | `refs_of_arg(1,pos,trigger)` | `wrapper(rel_atom,refuse(goal))` | `refused` |
| `next/1` | `time` | `splice_bare` | `wrapper(rel_atom,lower)` | `live` |
| `combine/variadic` | `join` | `splice_bare` | `wrapper(atom_list,lower)` | `live` |
| `zip/2` | `join` | `splice_bare` | `wrapper(atom_list,refuse(functor))` | `reserved` |
| `unsubscribe/1` | `time` | `refs_of_arg(1,pos,trigger)` | `wrapper(rel_atom,refuse(lifecycle))` | `reserved` |
| `complete/1` | `time` | `refs_of_arg(1,pos,trigger)` | `wrapper(rel_atom,refuse(lifecycle))` | `reserved` |
| `subscribe/1` | `time` | `refs_of_arg(1,pos,trigger)` | `wrapper(rel_atom,refuse(lifecycle))` | `reserved` |
| `error/1` | `time` | `refs_of_arg(1,pos,trigger)` | `wrapper(rel_atom,refuse(lifecycle))` | `reserved` |
| `not/1` | `sign` | `arm(neg)` | `wrapper(body_item,lower)` | `live` |
| `pre/1` | `sample` | `refs_of_arg(1,pos,sampled)` | `wrapper(rel_atom,refuse(goal))` | `refused` |
| `now/1` | `time` | `no_refs` | `wrapper(expr,refuse(goal))` | `refused` |
| `decode/2` | `guard` | `no_refs` | `wrapper(expr_pair,refuse(goal))` | `refused` |
| `json_each/2` | `guard` | `no_refs` | `wrapper(expr_pair,refuse(goal))` | `refused` |
| `true/0` | `guard` | `no_refs` | `word(lower)` | `live` |
| `:=/2` | `bind` | `no_refs` | `infix(refuse(goal))` | `refused` |
| `is/2` | `bind` | `no_refs` | `infix(refuse(goal))` | `refused` |
| `</2` | `guard` | `no_refs` | `infix(refuse(comparison))` | `refused` |
| `=</2` | `guard` | `no_refs` | `infix(refuse(comparison))` | `refused` |
| `>/2` | `guard` | `no_refs` | `infix(refuse(comparison))` | `refused` |
| `>=/2` | `guard` | `no_refs` | `infix(refuse(comparison))` | `refused` |
| `==/2` | `guard` | `no_refs` | `infix(refuse(comparison))` | `refused` |
| `\==/2` | `guard` | `no_refs` | `infix(refuse(comparison))` | `refused` |
| `count/1` | `aggregate` | `no_refs` | `head(refuse(aggregate))` | `refused` |
| `sum/1` | `aggregate` | `no_refs` | `head(refuse(aggregate))` | `refused` |
| `min/1` | `aggregate` | `no_refs` | `head(refuse(aggregate))` | `refused` |
| `max/1` | `aggregate` | `no_refs` | `head(refuse(aggregate))` | `refused` |
| `json_array/1` | `aggregate` | `no_refs` | `head(refuse(aggregate))` | `refused` |
| `json_object/2` | `aggregate` | `no_refs` | `head(refuse(aggregate))` | `refused` |
| `col_type/3` | `decl` | `no_refs` | `decl(column_type)` | `live` |
| `set/0` | `decl` | `no_refs` | `decl(refuse(removed_word))` | `refused` |
<!-- END GENERATED surface/5 TABLE -->

### Core grammar and input aliases

These rows describe syntax outside the registered body and aggregate
construct inventory.

| term-form shape | `.dl` spelling | parser treatment |
|---|---|---|
| `kind(Ref, log)` | `log` after columns | declaration modifier |
| `col_type(Ref, Column, Type)` | `Column: int` / `Column: text` | typed declaration entry; source order is preserved |
| removed `set` word | `set` after columns | `unsupported_surface(removed_word(set))` |
| `keep(Ref, all\|count(N))` | `keep(all)` / `keep(count(N))` | declaration modifier |
| `keyed(Ref, Positions)` | `key(P, P, ...)` | declaration modifier |
| `(Head <- Body)` | `Head <- Body.` | level rule |
| `(Head <+ Body)` | `Head <+ Body.` | edge rule |
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
| probe | `rel?(args)` | `unsupported_surface(probe(Name/Arity))` |
| mutation | `rel!(args)` | `unsupported_surface(mutation(Name/Arity))` |
| host declaration | `sh name(cols) = \`template\`.` | `unsupported_surface(host_decl(Name/Arity))` |
| query | `? name(args).` | `unsupported_surface(query(Name/Arity))` |
| retention marker | `rel(N) Name(...)` | `unsupported_surface(retention_marker(Ref, N))` |
| column wrapper | `Key(text)` / `Min(int)` / `Max(int)` | `unsupported_surface(column_type_wrapper(Ref, Column, Wrapper))` |
| `true` / `false` as values | unavailable | bare identifiers remain variables in argument position |
| `null` | unavailable | no term-form mapping |

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
though their rules reference many rels -- the printed `.dl` text correctly
shows zero decl lines for those, with the rule text alone still revealing
every ref's name, arity, and column names via `analyze.pl:rel_columns/5`).

## Grades (from `scripts/roundtrip.sh`, regenerate to reproduce)

- **G1**: 112 / 112 fixtures round-trip (`parse_dl(print_dl(Term)) =@= Term`
  for every `fixture/5` in `v6/prolog/conformance/fixtures/*.pl`).
- **G2**: both real files parse without error.
  - `ghcacher.dl`: Decls 16, Rules 9, 8 findings (3 host decls + 3 matching
    probes + 2 query lines -- every one a genuine term-form GAP, not a
    parser defect).
  - `conformance.dl`: Decls 29, Rules 28, 0 findings (the named/positional
    mix resolves silently, per the construct table above).
- **G3**: `v6/prolog/conformance/go.pl` unchanged, 112 pass / 0 fail.

## What `dl_view/*.dl` is

Every fixture in the 112-fixture corpus, printed as `.dl` text by this
parser's own printer, committed under `v6/prolog/compile/dl_view/`. This is
the "language you can see" deliverable: inspect any file there to read a
conformance fixture's PROGRAM (not its test scaffolding -- `Initial`,
`Schedule`, and `Expectations` are deliberately not printed, since they are
harness concepts, not part of `prog(Decls, Rules)`) as ordinary source text
instead of a Prolog term. Regenerate via `scripts/roundtrip.sh` (G1's run
writes every file as a side effect).
