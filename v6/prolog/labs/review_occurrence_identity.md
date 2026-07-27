# Review: occurrence_identity (post-lab, informs ruling R1)

Reviewed: `v6/prolog/labs/occurrence_identity.pl` + `.md` against `LANG.md`,
`AUDIT.md`, `plans/2026-07-27-lab-consolidation.md`,
`plans/2026-07-27-tier-topology.md`, `merge_family.md`.

Verified by running: `swipl -q -l v6/prolog/labs/occurrence_identity.pl -g go -g halt`
printed 20 PASS lines, exit 0. Matches the lab's claim.

## 1. Construct cardinality

| construct | verdict | audit reconciliation |
|---|---|---|
| occurrence-rel vs set-rel declaration (`event_rel`) | **instance, collapse into AUDIT finding 5's Set/Log rel kind** for derived rels; for bind-filled rels it is already carried by shell_stream's `Stream(Item, End)` / `Tail(Item)` result wrappers (shell_stream.md:22-33). New only if shipped as a separate keyword, which it should not be. | AUDIT.md:229-231 (finding 5 resolution 2: "type the rel: `hit` becomes `Set` or `Log`, never both") proposes this exact declaration for a different reason. AUDIT.md finding 10 (retention declaration, keep/kill row "retention declaration: add") targets the same rel subset. The keep/kill row "`<+` edge rule: kill as specified, respecify" is where the respecification lands. |
| `@ count` fold binding (the count-aware body form, lab ambiguity 7) | **collapse into the aggregate surface** if A or hybrid is ruled; genuinely new grammar only if pure B wins. Under A/hybrid every occurrence carries count 1 (`bind_count`, occurrence_identity.pl:184) and folds run per occurrence, so the binding form is dead surface. Multiplicity access is an aggregate-read question (lab ambiguity 10), and aggregates are already an "add" in the audit (76 of 173 files). | AUDIT.md keep/kill: "aggregation: absent: add". No new row needed. |
| stamps-on-event-streams marker (if hybrid adopted) | **collapse into row 1.** Under the hybrid, stamps go exactly on the rels the occurrence declaration names (`carries_stamps`, occurrence_identity.pl:67-72). Zero standalone constructs. | Same rows as above. |

Net new surface constructs if A or hybrid is ruled: **at most one**, and that one
already has two other audit findings asking for it.

Fourth-axis check, per mandate. The design's existing axes on a rel: level/edge
headedness (per rule, arrow), keyed/unkeyed (`Key`), world/derived (bind, `->`),
plus the shell lab's result-type wrappers. Occurrence-vs-set is not reducible to
any single one of them:

- Not edge-vs-level: `fetch_demand` and `change_log` are both edge-headed;
  one must be a set (dedup, pl:437-446) and one an occurrence rel (md:274).
- Not Key-vs-unkeyed: `fetch_demand` is unkeyed and a set rel. Keyed rels can
  never be occurrence rels (replace discipline collapses stamps), so the axis
  only ranges over unkeyed edge/bind-fed rels.
- Partially the Stream/Tail axis: a rel filled through `Stream`/`Tail` is an
  occurrence rel by construction, so the bind-filled half is inferable. Only
  derived unkeyed edge-headed rels need the declaration.

So: it IS a fourth axis if added as `event_rel` alongside a separate Set/Log
kind and a separate retention declaration. The loud flag is exactly that
failure mode: three declarations for one distinction. One rel-kind declaration
should carry all three jobs (stamp scope, finding-5 mixed-head safety,
finding-10 retention target). The lab's ambiguity 2 lists inference candidates;
inference by propagation from stamped drivers is refuted by the lab's own
dedup receipt (an edge rule over an occurrence rel must be able to head a set
rel), so full inference is off the table and one declaration is the floor.

## 2. Tier and ruling impact

**register_lowering.** Position in the topology (T4-adjacent, consolidation
line 88) does not move. Three amendments:

- The no-driver-loop claim survives for the cataloged fold shapes only
  (accumulate / lww / concat, md:382-386). "The checker owes nothing extra"
  under A (md:387-388) is an overclaim: the compiler must recognize which of
  the three lowerings applies, and a fold step outside the catalog (arbitrary
  `Next is f(Total, X)`) lowers to none of them; it needs a recursive CTE or
  rejection. Shape recognition is checker work under both mechanisms.
- New stated dependency, correctly identified at md:393-394: the tick's
  arrivals must be addressable as a table. That is an arrival-staging
  requirement on the T4 tick transaction that no doc currently owns. Record it.
- The sqlite 3.44 caveat is discharged, measured: the js store's
  better-sqlite3 13.0.1 bundles SQLite 3.53.3 (queried via
  `sqlite_version()`), the rust store links libsqlite3-sys 0.37.0 which
  bundles 3.51.3 (`sqlite3.h` in the vendored crate). Both clear 3.44
  (`group_concat(... ORDER BY ...)`) and 3.25 (window functions).
- Residual blocker: consolidation line 88 blocks register_lowering on "R1 +
  disjointness proof". This lab lifts only the R1 half; the pairwise-body
  disjointness obligation (merge_family tier note 1) stands. The lab's section
  5 does not claim otherwise but does not mention it; a reader could take
  "unblocks" as total.

**count_ivm_port / R7.** Ambiguity 1 does change the contract as written.
Consolidation line 89 says "contract = R7 boundary diffing"; that is now
incomplete in two ways:

1. R7 needs the one-line restatement the lab proposes: the delta set is a
   delta multiset on occurrence rels (md:210-211). Graded by
   `delta_shape_measured_per_mechanism`. Cheap, fold into R7's text.
2. The support-count/occurrence-multiplicity split is not derivable from R7
   and needs its own ruling: support count decrements on retraction,
   occurrence multiplicity must not, they coincide only on derived set rels
   (md:243-249). This gates count_ivm_port directly (the port cannot reuse the
   store's count column as occurrence count without deciding what retraction
   does to history, lab ambiguity 11 is the same question from the rule side).
   Record it as a new ruling, not a rider on R7.

**Does R1 split in two, as the lab claims (md:145-149)?** Yes, and the receipt
is genuine: the scoping decision is needed under both mechanisms
(`occurrence_firing_breaks_demand_dedup` grades a_naive AND b_naive at 2
fires; `membership_firing_keeps_demand_dedup` grades a, b, hybrid at 1). The
second ruling (which rels carry occurrence identity) is mechanism-independent
and should be ruled jointly with AUDIT finding 5's rel-kind question per
section 1 above, not as a fresh construct.

**retention_bound.** The lab's move (requirement -> gate on R1, md:477-478) is
supported by the retention table (md:259-267): direct under A, schema addition
under B. That is a dependency-edge change inside T4, not a tier move.

## 3. Lab-specific scrutiny

**(a) The order-collision receipt is genuinely graded.**
`b_state_collides_on_distinct_arrival_orders` (pl:534-541) extracts the
`append_line/2` store from the concat_ab run and unifies the concat_ba run's
store against the SAME variable (`Shared` at pl:537-538), so byte-identity is
enforced by unification, then requires the two A results to differ
(`AbResult \== BaResult`, pl:541). `b_concat_has_two_admissible_answers`
(pl:529-531) separately grades both admissible B answers. Verified in the
observed 20 PASS. Two narrated sharpenings sit one check short of graded:

- "concat_ba silently returns ab" (md:30, md:111-113). The default B run's
  term-order fallback follows from `msort` in `counted_occurrences`
  (pl:127), but no check grades `run_scenario(b, concat_ba, ...)` yielding
  `log(main, ab)`. One-line check would close it.
- "Any B implementation has a fallback order and it is never the arrival
  order" (md:113) is definitionally true (a B that kept arrival order would
  be carrying a stamp, i.e. A) but is stated as an implementation fact rather
  than argued.

Neither weakens the anti-B verdict; the collision check alone carries it.

**(b) The hybrid is A wearing B's storage, and should be ruled as such.**
Stamps subsume counts on event rels: the lab's own store keeps both and the
md concedes the count is derivable (md:223, "tick, seq; count is derivable";
`hybrid_settles_undercount_order_and_dedup` reads multiplicity 2 alongside
`st(1,1)`/`st(1,2)`, pl:626-631). On every semantic check where hybrid
appears, its result equals A's (mechanism table pl:57-61 gives hybrid the
same scope and firing as `a`; the delta-shape check grades a-3 and hybrid-3
identically, pl:637). So there is no third semantics. The honest statement of
the recommendation is: **A's semantics, plus the store keeps its existing IVM
support count as engine bookkeeping.** That framing is simpler than "counts
everywhere + stamps on event rels" and it is also safer: the lab's own
ambiguity 1 says support count and occurrence multiplicity are different
integers, and the hybrid as framed reuses one column for both on event rels,
which is the conflation ambiguity 1 warns against. "Stamps on event rels,
counts derived, support count stays internal" resolves ambiguity 1 on event
rels by construction (multiplicity = stamp count, never decremented; support
count remains the IVM-private integer). Recommend the ruling adopt that
phrasing; the mechanism-switch code needs no change, only the md's section 3.

**(c) The flip condition, priced.** "If the surface bans order-dependent
folds, B wins on simplicity" (md:327-331). What the ban costs, with receipts:

- Against the v5 corpus: nothing. v5 has no within-tick fold construct at
  all. Its carries are `@next`, one value per tick (6 files, tier doc census);
  its accumulations are set aggregates (`count`/`sum`/`collect`, 76 files,
  order-unspecified). The order-dependent shapes that do exist in the corpus
  are not arrival-order folds: `gen(:append, ...)` file writes are program
  order (30 files, AUDIT finding 15); gh-cache's latest-wins rides an
  explicit `max(bucket)` column (gh-cache.dl:99-102 via AUDIT finding 12);
  v5's `change_log` SSE tail order is already undefined (md:281-282). No real
  v5 program needs concat or last-write-in-arrival-order within a tick.
- Against the mission: the streaming binds pay. "This line came after that
  line" moves into every producer as a hand-emitted seq field, unaudited
  (md:161-165), and `Stream(Item, End)`'s terminal loses positional meaning
  relative to the last item (md:166-168). Cross-tick lww survives regardless
  as `Key` semantics (LANG.md:20-21); within-tick lww on one stream becomes
  inexpressible.
- The counter-receipt the md understates: even under the ban, B's simplicity
  win is partial. The ban must be ENFORCED, which is the same unscoped
  commutativity/count-scalability checker the md itself names as pure B's
  precondition (md:321-325); retention still drags a tick column into B
  (md:230-234); and ambiguity 1's two-integer split still applies. A needs
  none of those. So the flip removes B's silent-wrong-answer case and leaves
  three standing costs.

The user's ruling receipt in one line: banning order-dependent folds costs
zero against the existing 173-file corpus and costs producer-side seq fields
plus weakened stream terminals against the streaming mission; even then B
carries the checker, the retention column, and the two-count split.

## 4. Wrong or overclaimed

| where | issue |
|---|---|
| occurrence_identity.md:387-388 | "The checker owes nothing extra" under A. Fold-shape recognition (which of the three window lowerings applies) and rejection of out-of-catalog steps are checker work. The no-driver-loop claim holds for the catalog, not for arbitrary fold bodies. |
| occurrence_identity.md:274 | "stars going 42, 43, 42 records three occurrences" under A: no scenario in the .pl has a value returning to a previous value; this row of the v5 table is narrated, not graded. |
| occurrence_identity.md:30, 111-113 | The default-run "silently returns ab" claim is derivable from pl:127 but ungraded (see 3a). |
| occurrence_identity.md:203-204 | The delta-shape table's two-tick column lists all five mechanisms; the check (pl:641-645) grades only `a` and `b` for the two-tick shape. a_naive/b_naive/hybrid rows in that column are unverified. |
| occurrence_identity.md:364 | "The consolidation doc has register_lowering blocked on R1" omits the doc's second blocker (disjointness proof, consolidation line 88). Not contradicted, but incomplete. |

Nothing found that is wrong in the sense of a check asserting something other
than what the md reports. All 20 check names cited in the md exist in the .pl
and passed in this review's run.

## 5. Disposition

Accept with notes. The grading is honest (20/20 re-verified, the strongest
anti-B receipt is enforced by unification rather than narration, and the
failure modes are measured under naive settings instead of asserted), the
sqlite caveat the lab flagged is now discharged by measurement on both
stores, and the construct bill is smaller than the lab's own framing
suggests: at most one new declaration, which should be unified with AUDIT
finding 5's Set/Log rel kind and finding 10's retention target rather than
minted as `event_rel`, and zero new fold syntax unless pure B is ruled. Before
the ruling is taken: restate the hybrid as "A plus retained support count"
(section 3b), add the one-line check for the concat_ba default run, record
the arrival-staging-table requirement against register_lowering, and open the
support-count/occurrence-multiplicity split as its own ruling gating
count_ivm_port.
