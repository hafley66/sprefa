# Refusal messages + parse positions — brief (codex luna)

Beta gate item 1 (plans/2026-07-30-v6-beta-plan.md). Language design review
B4. PARTIAL WORK EXISTS: v6/prolog/0_refusal_messages.pl already renders
`unsupported_construct/1` as one line, has a derived refusal inventory, and
an `at(File, Line, Reason)` arm waiting for positions the parser does not
yet retain (its own header says "rule-index granularity only"). This lane
finishes the job: positions become real, text becomes plain english, parse
errors stop dumping char codes.

## Part 1: message quality

- Extend 0_refusal_messages.pl (do NOT start a new module). Current text is
  semi-raw (`unsupported_construct(~q); reason=...`). Target shape:
  per-functor plain-english clauses naming the rel/rule/column, functor kept
  in parens. Use the module's own refusal_inventory/1 to enumerate; the
  generic clause stays as fallback. Report: how many functors got a
  specific clause vs fallback.
- Message text: one line, plain english, names the rel/rule/column involved.
  Example shape: `door.dl6:12: log rel 'events' cannot head a level rule
  (log_on_level_headed_rel)`. Keep the functor name in parens — receipts
  and tests grep for it.
- Wire: compile.pl toplevel catch + bop check/run error path + text door
  print the rendered message, exit codes unchanged (bop contract: 2 = named
  refusal).

## Part 2: parse positions

- v6/prolog/compile/parse_dl.pl: on parse failure report line:col of the
  furthest point consumed (standard DCG furthest-failure technique; do NOT
  thread positions through every grammar rule if a lazy furthest-mark is
  enough). No more char-code lists in errors.
- Feed the existing at(File, Line, Reason) arm: text-door compiles wrap
  refusals with the owning decl/rule's line when the tokenizer can supply
  it cheaply (per-clause start line is enough — statement granularity, not
  token granularity). If that needs invasive threading, STOP that half and
  report — plain-english messages without line numbers still land.

## Receipts required

- Refusal-message battery: a script/plunit that compiles every refusal
  fixture in v6/prolog/conformance/fixtures and asserts the rendered
  message contains file, functor name, and no raw term dump. New fixtures
  not required; drive off the existing refusal set.
- bop check on a broken .dl6: exit 2, one readable line.
- Parse error on garbage input: line:col shown, receipt in report.
- Full battery: conformance (count unchanged), sweep both modes, TEXT_DOOR,
  roundtrip, plunit, bop tests. Staleness gate.

## Fences

- Touch: 0_refusal_messages.pl, compile/parse_dl.pl (failure reporting +
  clause start lines only), compile/compile.pl + bop error paths,
  prolog-lint baseline if it trips.
- Do NOT touch: lower.pl, emit_ts.pl, registry.pl, engine.pl semantics,
  fixtures' expected results (message rendering must not change what is
  thrown, only how it prints). A concurrent lane owns typing/emitter files.
- No-commit flow. STOP AND REPORT on blocked commands. Report EPERM legs.
