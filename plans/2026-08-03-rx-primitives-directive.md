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

## Refinement (user, same night): "its basically a tick aligned debounce"
- Same-tick multi-arm today: keyed head folds, bounded log REFUSES
  (retention_head_conflict_risk, ruled this same night), plain log
  keeps both. "one of these" is the missing third behavior: arbitrate.
- The guard-by-sampling spelling is a trap: latest/1 samples the
  pre-tick set; "did arm A fire THIS tick" is a delta read at offset 0,
  and negating it across rules is same-tick negation (checker refuses,
  correctly). Therefore `one { }` must be ONE construct evaluated per
  tick like match arms are one construct per value: arms in priority
  order, first with rows wins. No cross-rule negation exists in the
  lowering.
  rx: tickBatch$.pipe(map(b => firstNonEmpty([armA(b), armB(b)])))
- Family name: tick-aligned pulse operators, all match-block shaped:
  any (merge) / one (per-tick priority pick, rx auditTime-with-priority)
  / debounce (fire when a tick passes quiet) / scan (fold).

## Refinement 2 (user, same night): typed merge, policy as parameter
Verbatim: "allow any semantics so choose 1 or allow same tick etc...
the correct technique is typed merge so we can tell what edge made it
in a scan... it sounds like im fighting natural prolog itself".
Resolution: not fighting prolog; fighting IMPLICIT merge. The arm tag
is a plain enum column (the enum tag-view machinery already exists and
is plunit-green). Spec sketch:
    enum gate_source = pre_commit | timer.
    gate_fire(Source: gate_source, Repo, B) <+ any {
        pre_commit: pre_commit(Repo, B);
        timer:      latest(armed(Repo)), interval(1, B);
    }.
- desugar: N ordinary arms, each writing its variant into column 1.
- policy keyword picks arbitration over the SAME body:
  any = all arms land (today's semantics, grouped+tagged);
  one = per-tick priority pick, surviving tag = arbitration receipt.
- rx: merge(a$.pipe(map(tag pre_commit)), b$.pipe(map(tag timer)));
  scan folds with match over the tag (exhaustiveness checker-enforced
  because the tag is an enum, never a bare atom).
