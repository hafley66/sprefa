# CONSUMPTION + ARMS SHAPE-UP LAB (planner contract, user go 2026-07-28 night)

User word: "get a reasonable lab on actually getting things into proper
shape from this convo, in worktree, iterate a few times to figure some
assertions out." The convo's unshaped design residue becomes graded
assertions ready to be fixtures and rulings.

## Threads under test (all designed in-session, none fixture-graded yet)

1. LIFECYCLE ARMS with the ruled vocabulary (rulings.pl
   lifecycle_arm_vocabulary): next / finalize / unsubscribe / complete /
   subscribe / error. Per arm: does it have coherent per-row semantics in
   the delta model, what does it bind, when does it fire (Ti drain per
   the match-frontier lab), and which arms are rel-level vs row-level.
   The ERROR ARM question graded, not assumed: reconcile with the
   failure-is-a-value envelope ruling -- an error arm must not become a
   second failure channel; either it reads envelope error variants (pure
   sugar) or it is refused; find out which survives.
2. CONSUMPTION AXIS: switch vs queue (exhaust optional). switch =
   boundary read + collapse logging (ruled: transition_rule_semantics).
   queue = compiler-generated DURABLE pending rel + min-ordinal consume
   (the C7-sidestep claim: verify durability across a modeled crash).
   THE PACING SUB-CHOICE graded both ways with hand-computed tick logs:
   (a) all N firings same tick ordered by ordinal, (b) one per drain
   tick. Name what downstream rules observe differently and which
   composes with the arms model without a new construct.
3. CHANNEL = log + consumed(reader, ordinal) + watermark retention.
   Model N readers M writers; show the min-ordinal-per-reader read, the
   low-watermark prune, and PRECISELY what static keep(n) cannot express
   (the named gap: retention bound as a derived aggregate). Produce the
   smallest honest spelling proposal for rel-driven retention as a
   NAMED SLOT, not a fiat.
4. TRANSITION COLLAPSE LOGGING: the ruled trace obligation made
   concrete -- the event shape (rel, key, collapsed count), where it
   fires in the model, one graded scenario showing the log line.
5. OPTIONAL (only if rounds allow): the `<-` as signed `<+` desugar
   claim: show one level rule executed as an edge rule over signed
   deltas producing an identical tick log in the model.

## Protocol

- Lab dir v6/prolog/labs/consumption_arms/ (dies on landing). Entry
  self-loading, PASS-only stdout. Reuse the match-frontier lab's model
  interpreter as the starting point: recover with
  `git checkout 5ba7b0c5 -- v6/prolog/labs/match_frontier/` then adapt
  into your own dir (do not resurrect that lab's dir at landing).
- ITERATE, fixpoint-style (this is the explicit user ask): each round
  (a) tries to break the previous round's assertions with new
  scenarios, (b) encodes findings as checks, (c) journals the round.
  Stop at a zero-finding round. Journal section in the verdict:
  numbered assertions with the round that minted or amended each.
- Every .dl snippet carries its pure-rxjs lowering. rx/prolog/sql
  vocabulary only, no @ symbols. Descriptive prolog variables. No em
  dashes in markdown. Banned words: provenance, substrate, load-bearing,
  regime.

## Deliverables

- The lab (PASS-only) + plans/2026-07-28-consumption-arms-verdict.md:
  verdict line first, the ASSERTION SET (numbered, each with its
  validating check name -- these are the future fixtures/rulings), the
  pacing comparison logs, the error-arm resolution, the watermark slot,
  round journal.
- Prospective fixture/5 terms for: one arms fixture, one queue fixture,
  one channel fixture (in the lab, NOT added to conformance).

## Ownership fence (three other agents live)

ONLY v6/prolog/labs/consumption_arms/** + the verdict doc. Never touch
v6/prolog/compile/** (sol), v6/sprefa-store/bench/** (luna),
v6/prolog/labs/sqlite_retraction/** (sonnet), conformance/**.
Validation: lab PASS-only exit 0; conformance go.pl 110 and
compile/scripts/roundtrip.sh re-run green untouched.
