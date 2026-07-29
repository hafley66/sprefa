# CODEX BRIEF: doc truth wave (luna-class; review C3/C4/C6 + stale gates)

Make the language's documentation stop lying. Four mechanical pieces,
zero semantics changes.

1. registry.pl gains surface rows for the hosts constructs
   (sh_decl, probe, bind_decl, query, ts_query, sg_pattern) so the
   GENERATED SYNTAX.md construct table covers them (review C3: the
   generated table cannot cover what has no row). Status column
   truthful (live where wired, phase-2 refusal names where not).
   Re-run the doc emitter; committed SYNTAX.md updates.
2. SYNTAX.md hand-written half refreshed: the rows claiming probe/
   sh/query become unsupported_surface findings are FALSE since
   hosts wiring (they parse to first-class terms; top term is
   program/3); the G2 description updates (ghcacher.dl6 = zero
   findings). AMENDMENT (coordinator, after the first run correctly
   STOPPED on real-file usage): the dead spellings (retention marker
   rel(N), column wrappers Key/Min/Max) are NOT removed from
   parse_dl -- G2's real-file contract (clock-swr-demo.dl6,
   sg-rail.dl6 parse with findings) requires recognition, and
   v6/dl's langium surface still uses them (v6/dl is out of fence
   entirely, tests included). Instead: SYNTAX.md moves their rows
   into an explicit "legacy surface: parsed, then refused" section
   showing the unsupported_surface term each produces; grammar
   clauses and findings stay exactly as they are.
3. Refused-vs-live presentation (review C4): the construct table
   gains a clear split or marker so refused constructs are not
   presented as writable surface; latest's row reflects the actual
   status per bucket (live level = refused! edge = refused pending
   the concurrent lowering lane -- state it as of YOUR tree, a
   concurrent lane may land latest lowering after you; that is the
   coordinator's merge problem, not yours).
4. Stale gate docs: regenerate SCOREBOARD.md via the sweep runner
   (current corpus numbers); v6/justfile comment lines updated to
   current expected values (135 corpus, 70/67/0, plunit 70, dl 96,
   conformance 135). ALSO: keep(...) on a non-log rel currently
   accepted-and-inert (review C7) -- add the load-time refusal
   keep_on_non_log_rel in engine.pl check_program/1 AND analyze.pl
   beside the five existing cross-plane refusals (TICK-MODEL.md
   theorem style), one fail-first fixture. This is the one
   semantics-adjacent item; if any existing fixture trips it, STOP
   AND REPORT.

Grades: conformance (grows only by the new refusal fixture), sweep
both modes zero movement (except the fixture), TEXT_DOOR, roundtrip
ALL PASS over the changed grammar/doc, plunit growth, tsv2 + gate.

Laws: codex no-commit flow (git READ-ONLY). FIRST ACTION verify HEAD
= dispatch sha. Concurrent lanes own v6/tsv2/runtime+serve and the
latest()-lowering in analyze/lower/emit -- your analyze.pl touch is
ONLY the keep_on_non_log_rel refusal; do not touch latest handling
or lower.pl/emit_ts.pl at all. No em dashes; banned words
provenance, substrate, load-bearing, regime. Summary: per-piece
receipts, the removed spellings list, grades, cracks.
