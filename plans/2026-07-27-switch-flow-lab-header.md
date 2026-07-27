# LAB HEADER: switch_flow — can switchMap itself flow, and what is complete?

Planner-seeded contract per the lab protocol (CLAUDE.md 2026-07-27). The lab runs
in a worktree, implements `v6/prolog/labs/switch_flow.pl` + `.md` there, and dies
on landing; this header is the only main-tree artifact until distillation.

## Base you build on (read first, in this order)

1. plans/2026-07-27-sub-forest.md — the subscription forest: sub/sub_path/demand
   rels, switch_scope decl, scope_done, teardown = prefix range-DELETE, tick
   alphabet subscribe/unsubscribe/complete/fill. The mechanism reference
   implementation is recoverable: `git show 2fff3f61:v6/prolog/labs/sub_lifetimes.pl`.
2. plans/2026-07-27-mode-lattice.md — lifetime = free distributive lattice over
   end-signals; scope_min = OR, join_max = AND. Recover the operators:
   `git show 2fff3f61:v6/prolog/labs/mode_lab.pl`.
3. v6/prolog/conformance/rulings.pl — every user ruling; none may be re-litigated.
4. v6/prolog/conformance/fixtures/state_machine.pl and operators.pl — envelope
   arms, error-arm-is-a-value, repeat as self-carry, forkJoin as conjunctive body.
5. v6/prolog/conformance/engine.pl (+ body.pl, level_eval.pl) — the reference
   semantics your per-row traces must agree with.

## The five questions this lab must answer (each with graded checks)

Q1 **Can the switch itself flow?** `switch_scope(Pattern, ParentScope, Target)`:
   what may Pattern be? A keyed rel's key columns (switch-on-key), an envelope
   arm (switch-on-state), an arbitrary body condition? Grade at least: switch
   keyed by a register value, switch keyed by an enum arm, and whether a
   row-driven switch (the pattern itself read from a row) is expressible or
   needs a construct. The dream being tested: the switch is DATA, not syntax.

Q2 **What is complete?** For a rel-backed stream: who derives `scope_done` — a
   terminal enum arm (Stream(Item, End)'s End), a conjunctive body's last input
   (forkJoin completion), an explicit rule head? How does completion PROPAGATE:
   show that completion formulas compose by join_max across rule bodies and
   scope_min across nesting, i.e. the mode lattice IS the completion calculus at
   runtime, not just at check time. Grade a two-stage pipeline where the
   downstream view completes exactly when the lattice formula says it does.

Q3 **The full rx contract as tick items.** next/error/complete on the value
   side; subscribe/unsubscribe/finalize on the lifetime side. Error and complete
   are BOTH values here (error-arm ruling); grade their asymmetry at teardown:
   an errored scope vs a completed scope vs a torn-down scope — do downstream
   departure rules distinguish the three, and should they?

Q4 **Flattening strategies as one parameter.** switchMap replaces the old scope;
   exhaustMap ignores new rows while a scope lives; concatMap queues scopes in
   arrival order; mergeMap runs them in parallel. Can ONE decl carry the policy
   (e.g. switch_scope(Pattern, Parent, Target, Policy)) with the other three
   graded as policy values, or do queue semantics (concat's buffer) demand kernel
   state the forest does not have? If concat needs a queue rel, say exactly what
   its rows and teardown are. This is the domain-expansion question.

Q5 **Switch × state machine.** A scope keyed by a state register: entering a new
   state completes the old state's scope (takeUntil semantics as keyed replace).
   Grade the interaction with same-tick error-then-fresh chains from
   state_machine.pl (does a state flap net to zero scope churn or two teardowns?).

## Hard constraints

- ZERO new constructs beyond the sub-forest's switch_scope + scope_done unless a
  graded check PROVES the gap; every proposed addition names its budget cost
  (extraction lab discipline: the answer "it is a library + laws" beats "it is
  syntax").
- Every claim is a runnable check: `swipl -q -l v6/prolog/labs/switch_flow.pl -g go -g halt`
  exits 0, only PASS lines. Hand-rolled PASS harness consistent with prior labs.
- NEVER build trigger lists with findall (copies templates, severs shared
  variables — documented engine defect); use maplist.
- Style: descriptive variable names, no single letters; banned words: provenance,
  substrate, load-bearing, regime; no em dashes in prose.
- Ambiguities: numbered section in the .md, each stating the options and which
  one the lab graded; flag any that need a USER ruling and say what they block.
- Deliverable ends with an ENGINE-ABSORPTION DELTA: what changes vs the existing
  7-item sub-forest list (additions, modifications, nothing-new is a fine answer).

## File ownership

You own v6/prolog/labs/switch_flow.pl and switch_flow.md in YOUR WORKTREE only.
Do not touch conformance/, plans/, ARCH.pl, or any other file. The coordinator
distills and deletes on landing.
