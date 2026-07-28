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

The 109-fixture term-form corpus needs the opposite in places: a bareword
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

| term-form construct | `.dl` spelling | dl.langium rule (or GAP/EXT) | notes |
|---|---|---|---|
| `kind(Ref, log\|set)` | `log` / `set` word after the decl's columns | GAP(EXT) -- no rule; `RelDecl` (`dl.langium:28-30`) ends right after `)` `.` | see decl-line row below |
| `keep(Ref, all\|count(N))` | `keep(all)` / `keep(count(N))` | GAP(EXT) -- no rule | omitted from the printed line only when the ref has literally no `keep/2` entry -- never inferred from `decl_keep/3`'s own `all` fallback, see the round-trip note below |
| `keyed(Ref, Positions)` | `key(P, P, ...)` | GAP(EXT) -- no rule; closest analog is `WrapperType`'s `Key(text)` (`dl.langium:38,49-50`), a single-column marker with no position-list shape at all | |
| decl line as a whole | `rel Name(col, ...) [log\|set] [keep(...)] [key(...)]` | EXT widening of `RelDecl` (`dl.langium:28-30`) | modifiers are optional and accepted in ANY order (parsed as a loop, not a fixed kind-then-keep-then-keyed sequence) -- the corpus declares any subset of {kind, keep, keyed} for one ref, including `keyed` alone with no `kind/2` at all, and `kind(Ref,log)` alone with NO `keep/2` (`engine_core.pl:log_without_retention_rejected`, deliberately testing the missing-retention throw) |
| `(Head <- Body)` level rule | `Head <- Body.` | `DlRule` (`dl.langium:66-67`) -- DIRECT MATCH | |
| `(Head <+ Body)` edge rule | `Head <+ Body.` | GAP(EXT) -- `DlRule` has only one arrow (`<-`); no edge/level distinction exists in the grammar at all | |
| bare fact (no body) | `Head.` | `DlRule`'s optional body (`dl.langium:66-67`) -- DIRECT MATCH | `Body` becomes `true` |
| bare positive atom / `latest(Atom)` | bare positive atom / `latest(Atom)` | GAP(EXT) -- no trigger/sample distinction exists in the grammar | bare atoms are edge triggers; `latest(Atom)` is a sampled positive read and is never a trigger |
| `pre(Atom)` | `pre(Atom)` | GAP(EXT) | |
| `finalize(Atom)` (standalone) | `finalize(Atom)` | GAP(EXT) | departure trigger; the ARCH construct name remains `departure_form` |
| `now(Var)` | `now(Var)` | GAP(EXT) | |
| `decode(Expr, Pattern)` | `decode(Expr, Pattern)` | GAP(EXT) | `Pattern` reuses the general term grammar (vars, wildcards, nested compounds, braces) |
| `json_each(Expr, Elem)` | `json_each(Expr, Elem)` | GAP(EXT) | |
| `not(Goal)` | `not(Goal)` (canonical print); `!rel(args)` accepted on **input** as a legacy alias | `NegItem` (`dl.langium:112-113`) is the grammar's own negation, but it only wraps a bare relation atom -- it cannot spell `not(pre(X))` or `not(finalize(X))`, both real corpus shapes, so it is SUPERSEDED here rather than extended | printer never emits `!`; parser accepts both spellings |
| `Var := Expr` | `Var := Expr` | GAP(EXT) -- no assignment operator anywhere in the grammar | |
| `Var is Expr` | `Var is Expr` | GAP(EXT) | present in the reference evaluator's vocabulary (`expressions.pl` header comment) but zero corpus occurrences; parsed defensively, never exercised by a fixture |
| comparisons `< =< > >= \==` | same operator text, printed from the term's own functor | GAP(EXT) widening of `CompareItem` (`dl.langium:119-121`), which only allows `Var op Literal` | this grammar allows an arbitrary arithmetic expression on EITHER side (`Shared * 100 / Union >= 40`); alias table below |
| comparison alias `<=` | accepted on input, never printed | maps to `=<` | `dl.langium:120` spells "less-or-equal" as `<=`; the term-form corpus spells it `=<` (Prolog's own reader); parser accepts both, printer always emits `=<` |
| comparison alias `!=` | accepted on input, never printed | maps to `\==` | `dl.langium:120` spells "not-equal" `!=`; corpus spells `\==` |
| comparison alias bare `=` | accepted on input, never printed | maps to `==` | `dl.langium:120`'s only equality spelling; the term form has no bare `=`/2 anywhere in the 109-fixture corpus, so `==` (structural/value equality, already in the reference evaluator's own vocabulary per `analyze.pl:comparison_goal/1`) is the closest existing slot -- a real interpretation call, not a mechanical fact, flagged here rather than asserted quietly |
| arithmetic `+ - * / mod` | infix, precedence-safe (parens added only where flattening would change meaning) | GAP(EXT) -- `ArgTerm` has zero expression grammar (`dl.langium:150-151`) | |
| `concat([e1, e2, ...])` | same | GAP(EXT) -- no function-call-over-a-list-literal shape anywhere; list literals themselves are GAP(EXT) too | |
| `count(X)` / `sum(X)` / `min(X)` / `max(X)` in HEAD position | `count(X)` / `sum(X)` / `min(X)` / `max(X)` | `AggCall` (`dl.langium:83-84`) -- DIRECT MATCH | |
| `json_array(X)` / `json_object(K, V)` in HEAD position | same call shape | GAP(EXT) widening of `AggCall`'s validated fn set, which the grammar's own comment (`dl.langium:80-82`) limits to exactly `count`/`sum`/`min`/`max` | same production, wider name set |
| `'{}'(Pairs)` braces literal | `{key: value, ...}` | GAP(EXT) -- no bare `{...}` value production exists anywhere; `Member` (`key: value`, `dl.langium:139-140`) is reused CONCEPTUALLY (same `ident : value` shape) but never as a freestanding grouped value | key is a LABEL (bare, never quoted, never a variable -- the same lexical role as a relation functor name) |
| list `[e1, e2, ...]` | same | GAP(EXT) -- no list-literal production anywhere | only ever used as `concat/1`'s argument in this corpus |
| wildcard `_` | `_` | `Wildcard` (`dl.langium:162-163`) -- DIRECT MATCH | fresh anonymous variable per occurrence |
| named variable | bare identifier | `Var` (`dl.langium:153-154`) -- DIRECT MATCH | one Vars accumulator threads the WHOLE FILE (matches `read_term`'s one-clause scope for a fixture; same name anywhere in the file is the same variable object) |
| atom-literal constant | `'text'` (always single-quoted) | GAP -- grammar has no unquoted-OR-quoted atom-literal production at all | see the superseding-decision section above |
| string literal | `"text"` | `StrLit` (`dl.langium:168-169`) -- DIRECT MATCH | SWI string type, not atom |
| integer literal | `123` / `-123` | `IntLit` (`dl.langium:171-172`) -- DIRECT MATCH | |
| named args `col: val` | `col: val` at any call site | `Member` (`dl.langium:139-140`) -- DIRECT MATCH | resolved to positional order via the rel's declared column order, silently -- surface sugar, not a term-form gap; handles a genuine MIX of named and positional args in one call (real case: `conformance.dl`'s `proves_group_count(source, fanout: count(target))`) |
| probe `rel?(args)` | parses; args become an ordinary positional atom, marked as a finding | `ProbeItem` (`dl.langium:102-104`) | `unsupported_surface(probe(Name/Arity))` -- the term form has no wrapper for "this atom is an async host demand" at all |
| mutation `rel!(args)` | parses; same treatment | `MutationItem` (`dl.langium:108-109`) | `unsupported_surface(mutation(Name/Arity))` |
| `sh name(cols) = \`template\`.` host decl | parses; produces NO Decls entry | `ShDecl` (`dl.langium:56-58`) | `unsupported_surface(host_decl(Name/Arity))` -- no term-form shape holds a host declaration at all |
| `? name(args).` query line | parses; produces nothing | `QueryStmt` (`dl.langium:125-126`) | `unsupported_surface(query(Name/Arity))` -- REPL-only concept, not part of `prog(Decls, Rules)` |
| `rel(N) Name(...)` retention marker | parses; `kind(Ref, set)` is still emitted, retention itself dropped | `RelDecl`'s `'(' retention=INT ')'` (`dl.langium:19-21, 29`) | `unsupported_surface(retention_marker(Ref, N))` -- the grammar's own comment already calls this "semantics land later"; this finding just makes the same gap visible on the tsv2 side |
| `Key(text)` / `Min(int)` / `Max(int)` column-type wrappers | parses; wrapper name discarded | `WrapperType` (`dl.langium:38-39, 49-50`) | `unsupported_surface(column_type_wrapper(Ref, Column, Wrapper))` -- kind/keyed/keep carry no per-column type info in this term form. **UNTESTED by G2**: grepped both real files, neither uses any of the three; exercised only by a hand-written synthetic check during this build |
| `true`/`false` BoolLit as a value | not implemented as a value literal | `BoolLit` (`dl.langium:174-175`) | GAP, honestly unimplemented -- a bare `true`/`false` in an ARG position parses as a VARIABLE under this grammar's "bare id = variable" rule instead of the literal. Neither the 109-fixture corpus nor either real file ever uses `true`/`false` as a value (only `true` as a whole rule BODY, a different term-form concept -- see next row), so this was never exercised and is flagged rather than silently claimed as covered |
| bare `true` as a whole rule body | `true` | no grammar equivalent (a rule always needs a real body per `DlRule`) | matches `analyze.pl:body_ref_uses(true, [])`'s own vocabulary slot; zero corpus occurrences, kept defensively |
| `null` NullLit | not implemented | `NullLit` (`dl.langium:177-178`) | GAP, honestly unimplemented -- same reasoning as BoolLit. The term-form corpus's "absent value" sentinel is the ordinary atom `none` (single-quoted under this grammar's rule), which is a DIFFERENT vocabulary item than the grammar's `null` keyword; the two were never reconciled because neither real file uses `null` |

## Round-trip design note (why decl lines are exact, not fallback-merged)

G1 is a `=@=` variant check over `prog(Decls, Rules)`, and `=@=` over a LIST
is position- and content-exact: `[a, b] =@= [b, a]` is false, and
`[kind(r,log), keep(r,all)] =@= [kind(r,log)]` is false even though
`decl_keep/3`'s own fallback makes them mean the same thing at analysis
time. `print_dl.pl` therefore reproduces the LITERAL `kind`/`keep`/`keyed`
entries a ref has in the original `Decls` list, in their original relative
order -- never `rel_kind/3`/`decl_keep/3`/`decl_key/3`'s fallback-merged
view, and never a synthesized decl line for a ref that has zero entries
(the extreme case: `expressions.pl`'s fixtures all have `Decls = []` even
though their rules reference many rels -- the printed `.dl` text correctly
shows zero decl lines for those, with the rule text alone still revealing
every ref's name, arity, and column names via `analyze.pl:rel_columns/4`).

## Grades (from `scripts/roundtrip.sh`, regenerate to reproduce)

- **G1**: 109 / 109 fixtures round-trip (`parse_dl(print_dl(Term)) =@= Term`
  for every `fixture/5` in `v6/prolog/conformance/fixtures/*.pl`).
- **G2**: both real files parse without error.
  - `ghcacher.dl`: Decls 7, Rules 9, 8 findings (3 host decls + 3 matching
    probes + 2 query lines -- every one a genuine term-form GAP, not a
    parser defect).
  - `conformance.dl`: Decls 23, Rules 28, 0 findings (the named/positional
    mix resolves silently, per the construct table above).
- **G3**: `v6/prolog/conformance/go.pl` unchanged, 109 pass / 0 fail (this
  parser edits nothing under `compile.pl`/`analyze.pl`/`strat.pl`/
  `lower.pl`/`emit_ts.pl`, so this is a no-regression sanity check, not a
  claim of new coverage).

## What `dl_view/*.dl` is

Every fixture in the 109-fixture corpus, printed as `.dl` text by this
parser's own printer, committed under `v6/prolog/compile/dl_view/`. This is
the "language you can see" deliverable: inspect any file there to read a
conformance fixture's PROGRAM (not its test scaffolding -- `Initial`,
`Schedule`, and `Expectations` are deliberately not printed, since they are
harness concepts, not part of `prog(Decls, Rules)`) as ordinary source text
instead of a Prolog term. Regenerate via `scripts/roundtrip.sh` (G1's run
writes every file as a side effect).
