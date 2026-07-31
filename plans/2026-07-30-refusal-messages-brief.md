# Refusal messages + parse positions — brief (codex luna)

Beta gate item 1 (plans/2026-07-30-v6-beta-plan.md). Language design review
B4: refusals print as raw swipl "Unknown message" terms with no file/line;
parse failures print char-code dumps. Zero prolog:message//1 clauses exist.
This lane makes every user-facing failure readable.

## Part 1: message clauses

- New module v6/prolog/0_messages.pl: prolog:message//1 clauses for every
  named refusal term the compiler/oracle throws. Inventory them first:
  grep throw sites in analyze.pl, 0_program_check.pl, compile.pl, engine.pl,
  lower.pl, parse_dl.pl; list every refusal functor in your report, and
  state which got a clause (target: all of them).
- Message text: one line, plain english, names the rel/rule/column involved.
  Example shape: `door.dl6:12: log rel 'events' cannot head a level rule
  (log_on_level_headed_rel)`. Keep the functor name in parens — receipts
  and tests grep for it.
- Wire: compile.pl toplevel catch + bop check/run error path + text door
  print the rendered message, exit codes unchanged (bop contract: 2 = named
  refusal).

## Part 2: parse positions

- parse_dl.pl: on parse failure report line:col of the furthest point
  consumed (standard DCG furthest-failure technique — track position in the
  token stream; do NOT thread positions through every grammar rule if a
  lazy furthest-mark is enough). No more char-code lists in errors.
- Refusals raised during analyze carry file + the rule/decl's line when the
  parse can supply it cheaply (token stream already line-tracks or can);
  if attaching lines to analyze refusals needs invasive threading, STOP
  that half and report — messages without line numbers still land.

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

- Touch: new 0_messages.pl, parse_dl.pl (failure reporting only),
  compile.pl/bop error paths, prolog-lint baseline if it trips.
- Do NOT touch: lower.pl, emit_ts.pl, registry.pl, engine.pl semantics,
  fixtures' expected results (message rendering must not change what is
  thrown, only how it prints). A concurrent lane owns typing/emitter files.
- No-commit flow. STOP AND REPORT on blocked commands. Report EPERM legs.
