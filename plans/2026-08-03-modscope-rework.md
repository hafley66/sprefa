# REWORK: PLAN.md + PLAN.visual.human.unga.md (audit 2026-08-03)

The plan was finished before CONTRACT-ADDENDUM.md existed. Read the
addendum (note ruling 3 carries a correction) and apply exactly these
changes IN PLACE to both docs. Do not restructure anything else; the
audit held 27 of 30 receipts and sections 1, 2, 5 are untouched.

1. ADD a section: typed clock-world external event sources (addendum
   ruling 2). Worked example: a git pre-commit hook entering as a typed
   EDB row. Design the decl surface consistent with your scope/demand
   story; carry its pure-rxjs lowering. Defer deep checker/engine work
   to the impact lane by name (/Users/chrishafley/projects/sprefa-impact-lazy).
2. Section 3.2 (~PLAN.md:284-287) asserts "ordinary rxjs cold
   semantics" as settled. Refcount/teardown is an OPEN fork (corrected
   addendum ruling 3). Rewrite as a presented fork with prices:
   cold-per-subscriber vs connect-once, including what the last
   unsubscribe does. Same for the :260 "unsubscribing is the scope
   exit" sentence. Recommend, do not decide.
3. Section 4 (~:319-324) flatly decides "persisted rows, then live
   tail, nothing else" and "NOT reconstructed". Hedge it down to the
   register F6 (~:438-443) already uses; F6 stays the single place the
   fork is presented.
4. Step 4 (~:373-381): name the impact lane and stop prescribing
   pruning mechanism/sites; keep the phasing ladder itself. Fix the
   wrong label: host-demand generation lives in 1_host_expand.pl, not
   analyze.pl:124-175 (that range is event_use/atom_ref_args/guard_goal/
   bind_goal/tick_goal).
5. Small: (a) M5 mangled names carry the digest suffix
   (a__b__c__<digest>), ~:146-147 dropped it; (b) stance 5: state that
   relative path spellings (.., self) are refused in v1, ~:127-131;
   (c) mark the M4 "never clocked but DDL exists" reading (~:268-270)
   as this plan's own reconciliation of M4 against F1, a fork if
   contested, not the ruling's text.
6. Mirror the hedges into the unga doc's "choices I did not make" list,
   plain words, zero citations, ascii only.

Style laws: banned words provenance/substrate/load-bearing/regime; rxjs/
prolog/SQL vocabulary; every dl snippet carries its rx lowering;
descriptive dl variable names. Edit in place; write nothing outside
this worktree; no commits.
