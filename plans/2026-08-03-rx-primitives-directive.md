# Directive (user, 2026-08-03): merge, match, scan are primary surface

Verbatim: "i want merge to be representable kinda like match sugar, i
want a way to say 'any of these' vs 'one of these'" and "merge match
and scan are very very important primitives".

Today's spellings and the gap:
- merge ("any of these") EXISTS as N separate edge arms on one head;
  the gap is a grouping sugar. Sketch:
      gate_fire(Repo, B) <+ any {
          latest(armed(Repo)), interval(1, B);
          pre_commit(Repo, B);
      }.
  Desugars to today's N arms exactly. rx: merge(armA$, armB$).
- "one of these" = per-tick exclusive choice, arm order = priority.
  Sketch: same block with `one { ... }`; desugars to arms with
  cascading not() guards (arm k guarded on arms 1..k-1 producing
  nothing this tick). rx: merge of guarded streams; the honest rx name
  is a priority merge, not race. Interacts with the
  multi_trigger_batch_invariance label; a duel/lab must price it.
- scan = fold over pulses (rx scan). Surface today is a
  self-referential level rule; wants a named form. NOTE rulings.pl:
  the word "gen" is banned for the codegen-sink construct "alongside
  scan" -- check the stream_cards for the existing scan card before
  naming anything.

Status: directive recorded, undesigned. Next step = a design lane with
the match-block desugar discipline (term_expansion, one construct per
card, registry.pl row, golden-flex coverage) and rx lowerings for every
form. Blocked on nothing.
