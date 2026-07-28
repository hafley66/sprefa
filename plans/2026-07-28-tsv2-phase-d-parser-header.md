# TSV2 PHASE D PARSER HEADER (planner contract, user go 2026-07-28 AM)

User word: "can we finally get the parsing in place bc i cannot ensure the
language has correct semantics when its hosted without its own syntax."
Phase D from the tsv2 arc header starts NOW (parser half only; hosts stay
queued). Goal: the compiler front accepts .dl TEXT, and every fixture is
readable AS .dl text, so the surface language is inspectable and gradeable
instead of living only as prolog terms.

## Deliverables (all new files; ZERO edits to compile/analyze/strat/lower/
## emit_ts — the C2 agent owns those; coordinator wires the entry point at merge)

1. `v6/prolog/compile/parse_dl.pl` — DCG parser: .dl text in, the EXACT
   fixture term form out (`prog(Decls, Rules)` with the same op spellings the
   fixtures use: `<-`, `<+`, `:=`, `only/1`, `not/1`, `kind/2`, `keyed/2`,
   `keep/2`). Plain SWI-Prolog DCG over codes, no new deps, no consult of
   generated text.
2. `v6/prolog/compile/print_dl.pl` — printer: term form in, canonical .dl
   text out. Printer output MUST be re-parseable by parse_dl.pl.
3. `v6/prolog/compile/dl_view/<fixture>.dl` — every conformance fixture
   printed as .dl text, committed (this is the "language you can see"
   deliverable; regenerable, checked in like gen/).
4. `v6/prolog/compile/scripts/roundtrip.sh` — the grade runner (below).
5. A `SYNTAX.md` mapping table: term-form construct -> .dl text spelling ->
   dl.langium rule name (or GAP). Every GAP is a named finding with evidence,
   never a silent invention.

## Syntax authority

`v6/dl/grammar/dl.langium` (190 lines) is the reference surface. Where the
fixture term form has constructs the grammar can spell, the printer uses the
grammar's spelling (e.g. negation, decls, rule arrow forms as the grammar
writes them — read the grammar and `v6/dl/fixtures/ghcacher.dl` +
`v6/dl/fixtures/conformance.dl` as live examples). Where the term form has a
construct the grammar CANNOT spell (candidates: `only/1`, `<+` edge marking,
`keep/2` retention, `@next` carry), the printer picks a spelling, marks it
`% EXT` in SYNTAX.md, and files it as a language finding — that list is
exactly the phase D grammar-gap report the user needs. Do not silently
diverge from the grammar where it already has a spelling.

## Grades (mechanical, all must pass)

- G1 ROUND-TRIP, the binding grade: for ALL fixtures in
  conformance/fixtures/*.pl, `parse_dl(print_dl(Term))` =@= Term (variant
  check, variable names preserved via read_term-style variable_names so
  column mining still works downstream — stage 2 mines names from variable
  identity; a parser that loses names breaks rel_columns/4. Parse must
  return variable_names bindings the same way compile.pl:read_fixture_term/4
  does).
- G2 REAL-FILE parse: `v6/dl/fixtures/ghcacher.dl` and
  `v6/dl/fixtures/conformance.dl` parse without error into term form;
  constructs the term form cannot hold become named findings
  (`unsupported_surface(NamedThing)`), never dropped rows. Report the
  Decls/Rules counts for each.
- G3 the existing suites stay green untouched: conformance 109, tsv2 6/6,
  sweep scoreboard unchanged (parser must not perturb compile.pl et al —
  it doesn't edit them, so this is a no-regression sanity run).

## Explicitly out of scope

- Wiring parse_dl into compile_fixture (coordinator does it at merge, after
  C2 lands, to avoid file collision).
- Hosts/effects execution (phase D second half).
- Any grammar change in v6/dl (extraction-lab discipline: gaps are findings).

## Protocol

Sonnet, worktree isolation, disjoint file ownership as listed. Every green
state commits. Style laws apply: descriptive variable names, no banned words,
no em dashes in prose output.
