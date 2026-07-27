# AGGREGATE: v6 language arc reconciliation (2026-07-27)

Inputs: LANG.md (spec snapshot), AUDIT.md (18 findings), plans/2026-07-27-{surface-boil,
mode-dominance,tier-topology,lab-consolidation}.md, wave-1 labs (shell_stream, merge_family,
check_eventing, astgrep_patterns), wave-2 labs with reviews (timeless_rail, occurrence_identity,
temporal_pipe, expressions, diag_emit). All lab graders re-verified green by their reviewers
(18+20+25+22+14 wave-2 PASS; 20+12+17+36 wave-1 PASS). This file is the decision surface; the
per-lab receipts stay in the lab files.

---

## 1. FINAL CONSTRUCT INVENTORY

Deflations applied: timeless_rail's 12 inventions -> 8 grammar constructs plus a reading, an
idiom, a sentence, and a collapsed sugar (review_timeless_rail.md:30-34); occurrence_identity's
3 constructs -> 1 declaration, unified with AUDIT finding 5 and finding 10
(review_occurrence_identity.md:12-19); temporal_pipe's 4 rows -> sugar + instance + one shared
construct (trigger_marker) + one shared construct (rel-kind) (review_temporal_pipe.md:8-13).

### 1a. Keep, by tier

| construct | final verdict | tier | receipts |
|---|---|---|---|
| `enum` decl | keep | T0 | LANG.md:15; AUDIT.md:724; diag_emit review (silent severity coercion at src/lsp.rs:585-593 is the enum's argument, review_diag_emit.md:27) |
| `struct` decl | keep, specify (rel-row identity, one worked example) | T0 | AUDIT 18a (AUDIT.md:681-687); expressions review needs it for typed patch structs (review_expressions.md:100-104) |
| `rel` decl, required column types | keep | T0 | LANG.md:17; AUDIT.md:726-727; every lab |
| `Key(Type[, N])` | keep; ONE declaration, tier-indexed semantics: static FD at T0, replace (-old/+new) at T4, demand identity at T5 | T0 decl, T4/T5 semantics | timeless_rail I7 + review_timeless_rail.md:73-81; merge_family verdict 4; AUDIT.md:729-730 |
| `Option(T)` column type | keep; replaces rail I6 column defaults; default-value syntax is separable sugar | T0 | review_timeless_rail.md:18 (`col: Int = none` is ill-typed; explicit `none` suffices, timeless_rail.pl:147-150) |
| `<-` level rule | keep | T0 | 55 of 55 diag files level-only (AUDIT.md:269); diag_emit's hand-rolled DELETE+INSERT is its strongest receipt (review_diag_emit.md:20, 108-109) |
| facts (bodiless clauses) | keep | T0 | LANG.md:39; the ratchet and routing tables (timeless_rail.md:67-79) |
| `!atom` negation | keep (add; absent from LANG.md) | T0 | rail I3; AUDIT.md:754 (86-112 files depending on census) |
| reserved aggregate head forms `count/sum/min/max` | keep (add); head-column position only, excluded from stdlib and expression grammar; bag-vs-set pending R8 | T0 | rail I4; expressions review 4c ruling (review_expressions.md:142-151); the count-in-head divergence (review_expressions.md:63) |
| comparison goals `< <= > >= == !=` | keep (add) | T0 | expressions.md:48,53-55; rail I2; 166/173 files (AUDIT.md:16) |
| arithmetic exprs `+ - * / %`, Int-only, truncating | keep (add); `+` never concatenates | T0 | expressions.md:172-177; corpus-safe at 9 rewrite sites (review_expressions.md:39-51) |
| `:=` bind goal | keep (new operator) | T0 | expressions.md:74-88; resolves rail A2; v5 `=` classifies mechanically over 50 files (review_expressions.md:23-35) |
| pure function application (stdlib, 12 names after `apply` cut) | keep | T0 | expressions.md:256-282; `apply` + `it` cut, `digest` typed-and-stubbed pending 18a (review_expressions.md:96-111) |
| `quote(...)` + evaluation-default rule | keep the RULE (evaluate by default, quote stores); the construct is a cut candidate, see 1d | T0 | expressions.md:117-152; both error polarities graded; review_expressions.md:16 |
| `${name}` interpolation, name-only holes, desugar to concat, Int auto-converts, Term/enum/struct rejected | keep (add) | T0 | expressions.md:189-199; rail I9; holes stay name-only forever, `:=` is the computed-hole answer (review_expressions.md:62) |
| named-column atoms | keep; ONE spelling, TWO positions, rule stated: head = construction (omission is an error under Option), body = pattern (omission is wildcard); each head value is an `expr` | T0 | rail I5 + A3; review_timeless_rail.md:17, 36-39; grammar merge with expressions (review_expressions.md:64) |
| `_` wildcard | keep (add); binds nothing; in an expression it is an error | T0 | rail I11; expressions ambiguity 8 (expressions.md:357-361) |
| `?` snapshot ask + `--check` exit 2 on rows | keep (add to Surface) | T0 | rail I10; AUDIT 18c; mode-dominance.md:63-69 already types it |
| surface recursion | keep (permit); retarget checks.pl `no_self_union` at pre/carry twins, not plain recursion | T0 | AUDIT finding 2 (44 corpus files rejected today); timeless_rail.md:407-409 |
| multi-rule heads (rel union) | keep as a spec SENTENCE, not a construct | T0 | rail I12 deflated (review_timeless_rail.md:24) |
| unit/singleton rel | keep as an IDIOM (whole-program negation anchor), not a construct | T0 | rail I8 deflated (review_timeless_rail.md:20); v5 `true()` |
| `from world` rel modifier | keep; the killed `source` keyword unbundled into modifier position | T1 | rail I1 + review_timeless_rail.md:13; AUDIT finding 17 resolution 3 |
| `bind` decl (link-time protocols) | keep; obligation family grows: finiteness (Stream), per-emit batching, atomic single-txn commit (writer-side R7) | T1 | LANG.md:40-42; shell_stream sect 2; review_diag_emit.md:26, 75-77 |
| quoted DSL region `{|lang|| ... |}` | keep; compile-time parse+check; explicit closing delimiter, never brace-balanced | T1 | astgrep_patterns.md:24-70; surface_dcg owes the raw-text token NOW (lab-consolidation.md:90-91) |
| `match(cst, Pattern)` body relation | keep; relation over an ALREADY-CHECKED pattern value, not the quoting construct | T1 | astgrep_patterns.md:62-64 |
| grammar import (link-time, node-types.json -> facts) | keep; decide whether `bind` generalizes to link-time imports or a second link-time form arrives | T1 | astgrep_patterns.md sect 2, ambiguity 4; ts_grammar_import promoted to labbed |
| extraction ops (scan/regex/comment/ast/sg/json bodies) | still MISSING candidate syntax; the largest open gap | T1 | AUDIT finding 17 (blocker, 139/173 files); rail's 6 still-missing rows (timeless_rail.md:229-230); diag_emit confirms with force (review_diag_emit.md:30) |
| regex + path literals | still missing | T1 | timeless_rail.md:226-227 |
| graph operators (closure/scc) as body operator position | keep shape; surface unlabbed | T2 | tier-topology.md:44-48; AUDIT finding 16 (29 files) |
| diag product + gates + ratchets | library + CLI convention, ZERO syntax | T3 | timeless_rail claim (b) + review_timeless_rail.md:56-71; diag_emit (diag_v5 view is the whole contract) |
| `<+` edge rule | keep, RESPECIFIED per R2: the arrow owns the trigger (arrival), the head rel's kind/key owns the storage effect; "consequences never retract" is the Log-rel case only | T4 | AUDIT finding 4; merge ambiguity 6; lab-consolidation R2 |
| rel-kind declaration (Set vs Log) | keep, NEW; the one convergence construct, full job list in 1b | T4 | four-plus demanders, see 1b |
| trigger_marker | keep as THE R5 construct, stated once; spelling decided by ruling Q6; `delta()` (ghcacher.pl:79-80), eventing's per-atom need, and pipe's `only/1` are all this one thing | T4 | AUDIT finding 6; check_eventing finding 1; temporal_pipe.md:418-437; review_temporal_pipe.md:12 |
| `now()` phantom tick read | keep, KERNEL (not desugarable into a clock-rel join) | T4 | R3; check_eventing measured 5-vs-13 row storm (check_eventing.md:129-137) |
| `pre(atom)` | keep, define visibility (R6) and within-tick chaining (R1 rider) | T4 | AUDIT finding 7; merge ambiguity 5; occurrence ambiguity 3 |
| Key runtime semantics (replace, equal-row write = no-op; SWR served by a written_at column) | keep | T4 | merge ambiguity 1 with recommended split (merge_family.md:155-166) |
| retention clause on Log rels (`keep 30d` / `keep 100_000` shape TBD) | keep (add, REQUIRED); ranges over edge/Log-headed rels only, one-pass fold | T4 | AUDIT finding 10 (blocker); lab-consolidation:92-93; occurrence retention table (occurrence_identity.md:257-267) |
| occurrence identity (engine stamps `(tick, seq)` on event rels) | mechanism per R1 ruling; at most ONE new declaration and it is the rel-kind row above; `@ count` fold binding DEAD unless pure B ruled | T4 engine | occurrence_identity + review (hybrid = "A's semantics plus retained IVM support count", review_occurrence_identity.md:117-135) |
| clock bucket rel (quantized wall clock, third time coordinate) | keep as a PATTERN (a rel with Key on the bucket), plus the two-salt law: clock bucket = time recurrence, input digest = change recurrence; arrival-tick salt REJECTED | T4 | AUDIT finding 9 (720-vs-12 calls/hour); lab-consolidation PROVEN 4; shell_stream sect 4 |
| `\|>` temporal pipe | conditional adopt AS SUGAR, four conditions (see tier map T4); no kernel entry either way | T4 | temporal_pipe conditional yes + review_temporal_pipe.md:182-190 |
| `->` effect signature arrow | keep pending the Key ruling Q8 (labs split three ways) | T5 | AUDIT finding 3 (merge); merge_family verdict 4 (Key wins); astgrep ambiguity 8 (genuinely different) |
| envelope enums, failure-is-a-value | keep | T5 | LANG.md:25-27; AUDIT.md:732 |
| `Stream(Item, End)` / `Tail(Item)` result wrappers | keep; split item/terminal enums; mode is a function of the result type; new mode cell (multi, finite) | T5 | shell_stream sect 1-2; `->*` arrow and two-linked-rels rejected there |
| content-addressed demand dedup + salt columns | keep, with the two-salt law and the "what is the content" sentence split (det law vs cardinality mode) | T5 | shell_stream sect 4, ambiguities 1-4 |
| two-channel bind grammar (stdout_line + exit) | keep (needed; single-channel bind grammar cannot express a streaming shell) | T5 | shell_stream ambiguity 8 |
| write effects + apply gate + dry-run | still missing; AUDIT finding 15 unresolved (gen 30 files, checkout) | T5 | AUDIT finding 15 (blocker) |
| tail asks, (cardinality, lifetime) modes, dominance | keep; mode table amended per shell_stream and eventing | T6 | mode-dominance.md; check_eventing ask table; AUDIT finding 13 lattice fixes owed |

### 1b. The rel-kind declaration, once, with its full job list

Demanded independently by four-plus sources; ONE declaration in the `rel` decl carries all jobs.
Three separate declarations for this distinction is the named failure mode
(review_occurrence_identity.md:35-42).

| job | demander |
|---|---|
| Set-vs-Log storage kind (append vs membership; makes `<+` into a Set a type error) | AUDIT finding 5 resolution 2 (AUDIT.md:229-231); mixed-head stale-row hazard |
| retention target (only Log rels need a prune policy) | AUDIT finding 10; check_eventing.md:165-169 (the split falls exactly on the level/edge line) |
| event-ness (which rels carry occurrence stamps; the R1b scoping) | occurrence_identity sect (c): the scoping is needed under BOTH mechanisms; full inference refuted by the dedup receipt |
| boundary-check input (pipe cut law needs a LOCAL storage kind, not a whole-program arrow scan) | temporal_pipe.md:252-258 ambiguity 7; review_temporal_pipe.md:13 |
| R2's declaration site (the head rel's kind, not the arrow, owns retraction) | lab-consolidation R2; merge ambiguity 6 |
| keys-on-edge-rels contradiction resolves here (a keyed rel is a Set by construction; Log rels cannot be keyed) | check_eventing ambiguity 5 |

Bind-filled rels get the kind inferred from the result wrapper (`Stream`/`Tail` implies event);
only derived unkeyed edge-headed rels need the explicit word (review_occurrence_identity.md:29-33).

### 1c. Killed / merged / deflated

| construct | fate | receipts |
|---|---|---|
| `source`, `fact`, `rule`, `external`, `register` keywords | dead (LANG.md's own kill); `source` returns as the `from world` modifier | LANG.md:15; review_timeless_rail.md:13 |
| `delta()` wrapper | dead as spelled; resurrected as trigger_marker (R5) | AUDIT finding 6; lab-consolidation R5 |
| any-atom edge firing as the unqualified default | kill | AUDIT finding 6, 11 (violates the coastline law); pipe measured the backlog replay |
| one-time-cut body as stated | respecify (R6/R7 territory) | AUDIT finding 7; occurrence (d): the tick is one transaction, arrivals inside it are ordered |
| arrival-tick salt for demand rows | REJECTED (reintroduces 720-vs-12) | lab-consolidation PROVEN 4; AUDIT finding 9 |
| `->*` streaming arrow | rejected for `Stream(Item, End)` | shell_stream sect 1 |
| flat envelope (terminal ctors in the item enum) | rejected for split enums | shell_stream (`flat_envelope_costs_dead_arms`) |
| two linked rels per streaming effect | rejected | shell_stream sect 1 |
| `scan` term_expansion cluster sugar | built, graded identical, rejected | merge_family syntax experiment |
| `in` listexpr fan-out | kill for now | AUDIT.md:740; 18d |
| `=` mode-polymorphic bind/compare | killed; split into `:=` / `==` | expressions.md:74-88 |
| `let` keyword | rejected (fifth keyword) | expressions.md:87-88 |
| `eval(...)` marker | rejected (taxes 166/173 files) | expressions.md:111-114 |
| type-directed elaboration | rejected (locality argument; the circularity claim softened per review) | expressions.md:99-109; review_expressions.md:115-129 |
| `+` on Str | killed; interpolation covers all 9 corpus sites | review_expressions.md:39-51 |
| `apply` stdlib fn + `it` reserved name | cut (statically unsound, zero corpus); typed patch structs are the sound route | review_expressions.md:96-108 |
| scalar `min`/`max` | deferred; if ever, distinct names (`least`/`greatest`) | review_expressions.md:142-151 |
| column defaults `= none` | merged into Option typing; sugar optional | review_timeless_rail.md:18 |
| `event_rel` as its own keyword | merged into the rel-kind declaration | review_occurrence_identity.md:12-19 |
| `@ count` fold binding | dead unless pure B ruled | review_occurrence_identity.md:15 |
| `Display` "class" | restated as a closed whitelist {Int, Str}, not user-extensible | review_expressions.md:131-140 |
| jointly-semidet-per-key as stated | respecified as pairwise body disjointness over rules heading each keyed rel | AUDIT finding 12; merge tier note 1 |
| `checks.pl` `no_self_union` | retarget (rejects transitive closure today) | AUDIT finding 2 |
| `sugar_grounds_out` voluntary registration | replaced by census check (see tier map) | AUDIT finding 1; lab-consolidation registry-drift note |

### 1d. The count

Grammar-level constructs, T0-T4, keep column of 1a:

| tier | count | items |
|---|---|---|
| T0 | 18 | enum, struct, rel decl, Key, Option, `<-`, fact, `!`, aggregate heads, comparisons, arithmetic, `:=`, fn application, quote, interpolation, named-column atoms, `_`, `?` |
| T1 | 4 | from world, bind, quoted region, grammar import (extraction ops will add more when labbed) |
| T2 | 1 | graph operator position |
| T3 | 0 | |
| T4 | 7 | `<+`, rel-kind decl, trigger_marker, now(), pre, retention clause, `\|>` |
| total | 30 | |

30 exceeds the roughly-two-dozen budget. Cut next, in order:

1. `|>`: pure sugar, zero corpus chains exist (review_temporal_pipe.md:69-70), adds a glyph the
   lexer must own; defer until a real chain earns it. -1.
2. `quote(...)`: zero corpus occurrences and its one worked receipt (optimistic update) died with
   the `apply` cut (review_expressions.md:104-108). Keep the evaluation-default RULE as a spec
   sentence; reinstate the construct when a term column earns a corpus receipt. -1.
3. trigger_marker if R5 rules positionally: the marker stops being a construct
   (temporal_pipe.md:430-437). -1 contingent.
4. grammar import if `bind` generalizes to link-time imports (astgrep ambiguity 4): the keyword
   count holds at four. -1 contingent.

Floor after all four: 26. The remaining mass is T0, and every T0 row above carries a
corpus-percentage receipt; there is no cheaper T0 to cut without re-opening AUDIT finding 16's
11-NO score.

---

## 2. SPELLING RECONCILIATIONS

Implementation table for surface_dcg. Winner's spelling is normative.

| idea | spelling A (loser unless marked) | spelling B (winner unless marked) | winner | receipt |
|---|---|---|---|---|
| count in head | expressions head grammar parses `count(line_no)` as unknown fn | rail `eprintln_count(path, count(line_no))` | rail spelling; `count/sum/min/max` reserved head forms, carved out of stdlib lookup | review_expressions.md:63, 142-151 |
| binding | v5 `s = strip_prefix(f, p)` (mode-polymorphic) | `s := strip_prefix(f, p)` | `:=`; `==`/`!=` for comparison | expressions.md:74-88; mechanical port, review_expressions.md:23-35 |
| comparison op set | rail bare `>= <=` untyped subset | expressions full set, Int-both-sides typing | expressions (superset, byte-compatible) | review_expressions.md:58 |
| interpolation semantics | rail "render-after-join phase" (untyped) | desugar to `concat`, Display whitelist, Int auto-convert | expressions semantics at rail's sites; holes name-only forever, computed holes via `:=` | review_expressions.md:62 |
| named-column heads x expressions | rail named columns, values untyped | expressions positional heads, values typed | MERGE: named-column head where each value is an `expr` | review_expressions.md:64 |
| omitted column meaning | rail A3: head omission = declared default, body omission = wildcard | Option + explicit values | head omission = ERROR (Option columns take explicit `none`), body omission = wildcard | review_timeless_rail.md:36-39 |
| key marker | `->`-as-FD (audit merge proposal) | `Key(Type[, N])` in column type position | Key for state rels (merge receipts); `->` retained for world-filled rels (astgrep: compile-time-decidable fns want no arrow); final split = ruling Q8 | merge verdict 4; AUDIT finding 3; astgrep ambiguity 8 |
| streaming marker | `->*` arrow | `-> Stream(Item, End)` / `-> Tail(Item)` | wrapper | shell_stream sect 1 |
| envelope shape | flat enum with Line/Done/Err | split `ExtractEvent` + `ExtractEnd` | split | shell_stream sect 1 |
| demand re-fire salt | arrival tick T | clock bucket (polls) or input digest (extraction) | two salts, per recurrence kind | lab-consolidation PROVEN 4; AUDIT finding 9 |
| trigger marker spelling | `delta(atom)` (ghcacher) vs `only(atom)` (pipe lab) vs `new atom` / `+atom` (AUDIT options) vs first-atom positional | ONE form chosen by ruling Q6 | ruling; reviewer leans explicit marker | review_temporal_pipe.md:15-23 |
| fold count binding | `count_of(Atom, Count)` (lab) vs `atom @ count` (candidate) | neither | dead unless pure B ruled | review_occurrence_identity.md:15 |
| event multiplicity | merge lab's explicit id column `increment(Name, EventId)` | engine stamps, no id column | stamps (either R1 mechanism removes the column) | occurrence_identity (a) |
| quoted DSL | `sg{ ... }` brace-balanced; `cst ~ sg{...}` runtime op | `{\|sg\|\| ... \|}` explicit delimiter, compile-time | quoted region; `~` demoted to `match(cst, Pattern)` over a checked value | astgrep sect 1 |
| world-fed rel | `source` keyword | `rel foo(...) from world;` | modifier | rail I1 |
| severity | v5 open strings | `enum Severity` | enum | timeless_rail; review_diag_emit.md:27 |
| string concat | `"a" + b` | `"${a}${b}"` | interpolation; 9 sites, 4 files rewrite | review_expressions.md:39-51 |
| modulo | lab `mod` | surface `%` | `%` | review_expressions.md:174-176 |
| not-equals | lab `\=` | surface `!=` | `!=` | expressions deviations |
| ask | absent from LANG Surface | `? rel(cols);` snapshot; same text as tail under a CLI verb | `?` added to Surface; mode from verb, not spelling | rail I10; check_eventing ask table |
| pipe glyph | `\|>` (unlexable in prolog labs) | `~>` stand-in | `\|>` in the shipped surface, DCG-owned; labs use `~>` | temporal_pipe sect 1a |

No spelling both wave-2 labs actually wrote contradicts the other except the aggregate-head case;
the rest are one lab supplying typing the other assumed (review_expressions.md:68-71).

---

## 3. THE RULING QUEUE

Ordered by what blocks what. Q7, Q8, Q9 are mutually independent and independent of Q1-Q6; they
can be taken in any order or first. Q10 depends on Q3 and Q1. Q5 rides Q4.

**Q1. R1: within-tick occurrence identity.** A tick is a set; two same-tick increments fold to +1
silently (merge `scan_undercounts_batched_events`). Mechanisms: A = engine-stamped `(tick, seq)`
on event rels; B = Z-set counts everywhere; hybrid = counts + stamps.
Options: (a) A: order and multiplicity survive; folds lower to window functions, one statement,
covers non-commuting folds; sqlite floor discharged on both stores (3.53.3 js / 3.51.3 rust,
review_occurrence_identity.md:58-62). (b) B: reuses the store's count-IVM column; order-dependent
folds get a SILENT fallback order (`concat_ba` returns `ab`, graded by unification collision);
needs a commutativity checker, a retention tick column anyway, and the two-integer split.
(c) hybrid, REFRAMED per review: "A's semantics plus the store retains its IVM support count as
engine bookkeeping"; there is no third semantics (review_occurrence_identity.md:117-135).
Advisory: hybrid-as-reframed. Priced flip condition: if the surface bans order-dependent folds,
B wins on simplicity; the ban costs ZERO against the 173-file corpus (no v5 within-tick fold
exists) and costs producer-side seq fields plus weakened Stream terminals against the streaming
mission; even flipped, B still carries the commutativity checker, the retention tick column, and
the support-vs-occurrence split (review_occurrence_identity.md:137-166).
Unblocks: register_lowering (into window-function lowering under A; still gated on the
disjointness proof, review_occurrence_identity.md:63-67), retention_bound, R8-if-Z-set.

**Q2. R1b: which rels carry occurrence identity (the scoping/dedup decision).**
Mechanism-independent: naive per-occurrence firing double-fires effects under BOTH A and B
(graded, occurrence (c)); membership firing keeps dedup under both. So a rel is either an
occurrence rel (every arrival is a new thing) or a set rel (identical content is the same thing),
under any Q1 answer. Full inference is refuted (an edge rule over an occurrence rel must be able
to head a set rel); bind-filled rels infer from Stream/Tail.
Options: explicit word on the rel decl (floor) vs partial inference plus explicit override.
Advisory: one explicit declaration, which IS Q3. Unblocks: Q3, edge-rule shipping.

**Q3. Rel-kind declaration shape (Set vs Log).** The 1b convergence construct: one word on the
`rel` declaration carrying storage kind, retention target, event-ness, boundary-check input, R2's
site, and the keyed-Log exclusion. Options: (a) keyword on the decl (`rel change_log(...) log;`
or a kind position); (b) infer from heading arrows plus explicit override (makes the pipe's
boundary check non-local, temporal_pipe ambiguity 7). Advisory: explicit, one word, all six jobs
(review_occurrence_identity.md:35-42; review_temporal_pipe.md:13). Unblocks: pipe condition 2,
retention surface (Q10), R2 restatement in LANG.md, mixed-head typing.

**Q4. R9: edge-writes-as-arrivals.** merge's interpreter never lets an edge-written row trigger a
downstream edge rule (stall, or worse, nondeterministic delivery when an unrelated atom arrives
later, review_temporal_pipe.md:95-99); eventing's settle loop cascades within the tick; the pipe
carries writes to the next tick. Three interpreters, three tick semantics.
Options: (a) same-tick cascade to fixpoint: `|>` becomes free (deleting its law's justification),
within-tick ordering questions return, self-referential edge rules make the tick unbounded;
(b) next-tick propagation: rows written by edge rules at T are arrivals for T+1; each hop lands in
its own delta set (the self-diagnosis law likes this); R7 holds unqualified; costs N-1 drain
ticks. Advisory: (b), next-tick propagation (review_temporal_pipe.md:120-124).
Unblocks: every hand-written edge-feeds-edge program, pipe latency semantics, Q5.

**Q5. Drain-scheduler ownership.** Under Q4(b) the ENGINE must self-schedule drain ticks (empty
outside-arrival set) while the carry set is nonempty, or chains freeze when outside arrivals
stop; the pipe lab smuggled this in via hand-fed empty ticks (temporal_pipe.pl:485-486). Carry is
reconstructable from the delta trail restricted to Log rels, so it survives a crash with no new
persistent state. Options: engine-owned (advisory; belongs in R9's text) vs runtime-host-owned.
Blocks nothing else; blocked by Q4.

**Q6. R5: the trigger_marker form.** Killing `delta()` removed the only per-rule control over
backlog replay (AUDIT finding 6); three labs circled the same missing thing.
Options: (a) explicit per-atom marker: costs a surface form, buys an error instead of a silent
change; the pipe GENERATES it at every boundary so authors rarely type it; (b) first-body-atom
positional: zero constructs, pipe-generated rules conform for free, but comma order becomes
semantic and a reorder is a silent behavior change (the dot-space failure shape); (c) per-rel
opt-out annotation: coarser, cannot express per-consumer catch-up (AUDIT option 3).
Advisory: marker, stated once, pipe as its main generator (review_temporal_pipe.md:15-23).
Unblocks: edge-rule grammar freeze, pipe registration (its sugar/2 fact is unwritable until
ruled, temporal_pipe.md:418-424). Note: the pipe resolves R5 at boundaries only; the first stage
of every chain keeps the any-atom trigger (temporal_pipe ambiguity 9), so this ruling is needed
with or without the pipe.

**Q7. R8: aggregate multiplicity, bag vs set.** `count(x)` in a head: bag-of-derivations
(v5-SQL-compatible; two hits on one line count 2) vs set-of-projected-values (the rail
interpreter's `sort` behavior; DISTINCT is the only mode). Both rails are insensitive on real
data, which is exactly how it ships wrong invisibly (timeless_rail A1). Timeless: reachable with
no tick anywhere, so it is NOT a corollary of Q1 unless Q1 resolves via Z-sets
(review_timeless_rail.md:83-104). Advisory: none taken; v5-compatibility favors bag; the lab
interpreter implements set. Reference interpreter, lowerSql, and emit_ts must implement the same
answer; the rail grader gains a two-hits-one-line world row as the fail-pre-fix test.
Independent; blocks emit_ts/lowerSql aggregate work.

**Q8. Key: one declaration, tier-indexed semantics; and Key vs `->`.** Settle in one ruling:
(i) Key(T) reads as static FD at T0 (rail receipts: `fd_violations` is the only consumer, the
evaluator is key-blind), replace-maintaining-the-same-FD at T4, demand identity at T5; one
declaration, no per-tier syntax (review_timeless_rail.md:73-81). (ii) The Key/`->` split, labs
three ways: merge lab says Key wins on receipts (all keyed behavior off column positions); AUDIT
says merge them (`->` IS Key on demand columns for det effects, finding 3); astgrep says
genuinely different (pattern types parameterized by link-time import do not fit an FD reading;
compile-time-decidable fns want no arrow). Options: (a) `->` as pure sugar over Key + adornment,
streaming distinguished by envelope type; (b) both, with the law stated: `->` = FD left-to-right
AND the demand split, Key = undirected uniqueness, det effects are where they coincide; (c) kill
Key on effect rels, keep it on state rels. Advisory: none unified; present-both was the
consolidation's instruction (lab-consolidation.md:74-80). Independent; blocks the effect
declaration grammar freeze.

**Q9. Aggregates spelling: head-only reservation.** `count/sum/min/max` are reserved
head-position aggregate forms, excluded from the stdlib namespace and the expression grammar; no
arity- or type-directed dispatch; a future scalar pair enters under distinct names. One ruling
closes both the count-in-head divergence and the scalar-min/max collision
(review_expressions.md:142-151). Advisory: adopt as stated. Independent; blocks the surface_dcg
head grammar.

**Q10. Retention bounds surface.** The mission says tight RAM; the spec has one unbounded
construct class (Log rels) and no bound syntax; the only declared bound (`pre` depth) measures a
different table (AUDIT finding 10). Options: (a) per-rel `keep <duration|count>` clause in the
declaration, checker refuses an unbounded Log rel nobody windows; (b) mandatory bucket Key
(collapses within-bucket duplicates, changes semantics); (c) Log rels non-resident by law
(forbids the SSE backlog join outright). Advisory: (a); it only ever ranges over Log rels
(one-pass fold, check_eventing tier note), and under Q1=A it is a tick-prefix DELETE; under
Q1=B it drags a tick column into B anyway (occurrence sect 2). Depends on Q3 (the Log kind) and
Q1 (the prune key).

**Residual queue** (settled-direction or smaller, needing text not debate):
R2 restatement in LANG.md (direction settled: arrow = trigger, rel kind = storage; add the
occurrence rider: intermediate per-occurrence states of a keyed edge head are not observable at
the tick boundary, occurrence sect 5). R3 banking (`now()` into kernel facts before surface_dcg
freezes the body grammar). R4 departure triggers (open: a departure form in edge bodies, or state
the hole; decide before the arrow set freezes, check_eventing tier note). R6 pre visibility
tie-break (AUDIT finding 7 options; plus R1's rider: does `pre` chain across occurrences within a
tick, occurrence ambiguity 3). R7 restatement (one line: the delta set is a delta MULTISET on
occurrence rels). NEW: support-count vs occurrence-multiplicity split, its own ruling gating
count_ivm_port (they coincide only on derived Set rels; retraction decrements one and must never
decrement the other, review_occurrence_identity.md:69-82). A6: `diag` as ordinary rel vs
engine-known sink (gates the T3 rename, review_timeless_rail.md:66-68). Equal-row keyed write =
no-op, `written_at` serves SWR (merge ambiguity 1). Census denominator: pick 163 or 173 once
(review_expressions.md:180-182).

---

## 4. TIER MAP REWRITE

Replacement text for the "Tiers" section of plans/2026-07-27-tier-topology.md, amended by
timeless_rail, expressions, occurrence_identity, temporal_pipe, diag_emit, shell_stream, and the
reviews. Orthogonality claims 1-5 and the corpus table stand unchanged.

```
## Tiers (each orthogonal; arrows = depends-on)

T0 RELATIONAL CORE (timeless)
  enum/struct + typed rel cols + Option(T) + Key(Type) as static FD; level
  rules `<-`; stratified negation; reserved aggregate head forms
  (count/sum/min/max, bag-vs-set per R8); facts; snapshot asks `?` with
  --check exit 2; comparison + arithmetic (Int-only, truncating); `:=`
  bindings; string interpolation (name-only holes, desugar to concat,
  Display = closed {Int, Str}); named-column atoms (head = construction,
  omission error; body = pattern, omission wildcard; head values are exprs);
  `_` wildcard; pure stdlib (12 names); quote/eval-default rule; surface
  recursion (no_self_union retargeted); multi-rule heads (spec sentence);
  unit-rel idiom.
  Lowering EXISTS for the pre-amendment core: js engine v1 lowerSql + emit_ts.
  Checkers: HM/enum typing, exhaustiveness, stratification, Key-as-FD,
  aggregate group arity, range restriction via := / atom-arg rule,
  head-expression ban inside recursive SCCs. THE STRATIFIER SEES EXPRESSIONS:
  they are part of T0's checker input and cannot be bolted onto a frozen
  checker later.
  <- nothing

T1 EXTRACTION + BIND (world-fed corpus)
  `from world` rel modifier (the unbundled source keyword; canned rows and a
  bind are program-text-identical, orthogonality claim 2 MEASURED);
  bind mechanism, whose obligation family is: finiteness discharge for
  Stream-typed rels, per-emit batching, atomic single-transaction commit
  (writer-side R7); quoted DSL regions `{|lang|| ... |}` (compile-time
  parse + check; raw-text token owed to surface_dcg NOW); match(cst, pattern)
  over checked pattern values; grammar-import (node-types.json -> con facts;
  labbed; needs the target parser, not just the schema, for bare-token
  kinding). Extraction op bodies (scan/regex/comment/ast/sg/json) still owe
  surface syntax: the single largest open gap (AUDIT finding 17).
  Checkers: quoted-DSL parse/check as compile errors, pattern-vs-grammar,
  two-lowering refusal channel (a backend that cannot express a construct
  refuses, never approximates).
  <- T0

T2 GRAPH OPERATORS (on-disk algorithms)
  closure/scc (node2vec later) as operators over edge rels; recursion is
  already T0. RAM law: operators stream from sqlite, never resident.
  <- T0

T3 DIAGNOSTICS LIBRARY + CLI (milestone, not a syntax tier)
  ZERO new syntax, proven twice (timeless_rail: gate/severity/stage/exit are
  eight lines of level rules over fact tables; diag_emit: the diag_v5 view +
  one shell bind close the editor loop with no rust or extension change).
  Deliverables: std/diag library (diag decl, severity_rank, gate_threshold,
  gate_exit/check_exit rules), CLI verbs (--check exit 2 on rows, stage
  gates, LSP span rendering), the diag_v5 view contract + sqlite bind.
  Retraction reaches the editor via the reader's absence diff; no T4
  dependency even for clearing squiggles. Gate: rule A6 (ordinary rel vs
  engine sink) before rewriting this row.
  <- T0, T1 (hard: the 54-file diag corpus is extraction-fed)

T4 EDGE TIME (first temporal tier)
  `<+` edge rules (respecified: arrow = trigger, rel kind = storage);
  rel-kind declaration Set|Log (one word, six jobs: storage kind, retention
  target, event-ness, boundary-check input, R2 site, keyed-Log exclusion);
  trigger_marker (the R5 construct, one spelling); now() (kernel);
  pre (visibility per R6, within-tick chaining per R1); Key runtime
  semantics (replace -old/+new; equal-row write = no-op); occurrence
  identity per R1 (engine stamps on event rels); clock-bucket rel pattern +
  the two-salt law; retention clause on Log rels (REQUIRED, not an
  optimization); tick transaction with R7 boundary diffing (delta MULTISET
  on occurrence rels); R9 edge-write propagation + the drain scheduler;
  count-IVM port (contract: R7 + the support-count/occurrence-multiplicity
  split ruled).
  `|>` temporal pipe lives HERE AS SUGAR, adopted only under four
  conditions: (1) R5 ruled first, marker preferred, pipe generates it;
  (2) the rel-kind declaration landed (keeps the boundary check local);
  (3) R9 ruled next-tick with the drain scheduler named; (4) reserved
  namespace for generated intermediates. Pipes with edge/key cuts ship at
  T4; yield cuts additionally need the T5 effect-signature DECLARATION in
  scope, not T5 runtime.
  Checkers: pairwise body disjointness over the rules heading each keyed rel
  (REPLACES "jointly semidet per key per tick", which is neither decidable
  as quantified nor applicable to the one-rule-many-rows case), causality,
  retention presence on Log rels, fold-shape recognition (accumulate/lww/
  concat catalog; out-of-catalog steps rejected), `<+`-into-Set type error.
  Regression contract: the timeless_rail check set byte-for-byte at any
  single repo state (orthogonality claim 1, mechanically checkable).
  <- T0 (engine: count-IVM port + arrival staging table)

T5 EFFECTS
  adorned world rels (signature arrow, pending Q8), envelope enums, demand
  rows + content addressing + the two salts, shell bind with two-channel
  grammar (stdout_line + exit), STREAMING PRE-REGISTERED: Stream(Item, End)
  / Tail(Item) result wrappers land BEFORE register_lowering (they ground in
  {ground_terms, rule, external_rel} with no register dependency,
  shell_stream tier note); write effects + apply gate + dry-run (AUDIT
  finding 15, still open); checkout-style demand sinks.
  Checkers: LINK-TIME LIFETIME OBLIGATION: a bind must discharge its rel's
  finiteness claim (tail -f into a Stream-typed rel is a link error); bind
  obligation discharge for batching + atomicity; streaming retention gate.
  <- T1 (bind), T4 (edges/ticks)

T6 ASKS + MODES
  tail asks; (cardinality, lifetime) mode analysis with the new
  (multi, finite) cell; dominance/scopes (switch_map); the five ask rows
  from check_eventing (hook write, hook snapshot, LSP tail under document
  scope, commit gate, dashboard tail-with-warning). mode_lab scope:
  result-type modes, the lifetime lattice fixes (AUDIT finding 13: two
  operators, scope_min and join_max, stated; mode analysis declared a
  post-link pass), static-vs-runtime lifetime distinguished.
  <- T4, T5

T7 LAZINESS + SUB GRAPH        (unchanged)
  <- T5, T6

T8 STORAGE LOWERING            (unchanged, user-parked)
  <- T0

T9 OPTIMIZER                   (unchanged)
  <- most of the above

## Compiler self-check (cross-tier)
  The census check REPLACES voluntary registration: surface_dcg is the
  source of surface construct names; `go` fails on any parsed construct
  with no grounds chain (inverting AUDIT finding 1's quantifier). kernel.pl
  drops the dead surface_form rows (source/fact/external/register);
  checks.pl retargets no_self_union and fixes covers_enum arity matching.
  surface_dcg additionally owes: the raw-text region token (astgrep), the
  five unlexable constructs (|>, !rel, x.field, Entry {..}, match {..}),
  lexer-owned `.` with whitespace never meaning-changing, and the
  adversarial law: no single-character perturbation of a legal program may
  yield a different legal program silently (review_temporal_pipe.md:142-147).

## Shortest paths (amended)
  v6 replacing a v5 lint rail: T0(amended) + T1's one job (turn `from world`
  into bind) + T3 library. The timeless_rail program text does not move.
  Running v6 ghcacher: + T4 edges/keys/rel-kind + T5 shell effect.
```

---

## 5. CONFORMANCE PLAN

The islands-to-continent step: nine lab interpreters described overlapping fragments; one shared
fixture corpus plus one reference interpreter proves they described one language.

### 5a. Graded traces to promote into the shared fixture corpus

| lab | checks / traces | why promoted |
|---|---|---|
| merge_family | merge two-source trace; mergeByKey replace trace (-old/+new); equal-row silent tick; counter fold 4-tick trace; seed/transition disjointness; both conflict rejections; `scan_undercounts_batched_events` | the undercount is R1's fail-pre-fix fixture; the conflict rejections pin the disjointness law |
| check_eventing | the 7-tick diag scenario (T1-T7 delta lists verbatim); `clock_rel_join_storms` (5-vs-13); hook window join T6-T7 | the LSP loop end to end; the now() kernel receipt; R7's flicker fixture (`wholesale_replace_no_flicker`) |
| shell_stream | `identical_demand_dedups`, `new_salt_refires_fresh_stream`, `terminal_is_terminal`, `live_nonzero_exit_keeps_rows` + shell_stream_fixture.jsonl | streaming envelope + dedup + salt law, with two real process spawns |
| astgrep_patterns | grammar import over node_types_fixture.json; parse/check refusals with named reasons; both lowerings' exact strings; the three `blocked(...)` refusals; derived-pattern codemod round trip | the two-path agreement obligation made concrete; refusal channel fixtures |
| timeless_rail | all 18 checks, exact row sets (waiver range join, over-baseline diag, gate split, new-file diags, unwrap aggregate, tighten/fix scenarios) PLUS the two-hits-one-line row to add (R8's fail-pre-fix) | explicitly framed as the T4 port's regression fixture (timeless_rail.md:292-293); orthogonality claim 1's mechanical check |
| occurrence_identity | the mechanism table (a/b/hybrid per case); delta-shape table; storage-cost table; `b_state_collides_on_distinct_arrival_orders`; PLUS the missing concat_ba default-run check to add | R1's decision receipts; the collision is enforced by unification, not narration |
| temporal_pipe | `chain_desugars_to_three_rules`; `desugared_trace_equals_hand_written`; `trigger_marker_is_what_stops_backlog_replay` (marked vs unmarked); `pipe_stage_costs_one_tick`; `cut_law_depends_on_declarations` | pipe-vs-hand trace equality is the sugar-conformance instrument; the backlog pair is R5's fixture |
| expressions | both collision polarities with exact error terms; graph-measure transcription (jaccard rows); the range join; arch-conformance `:=` rule; `interpolation_desugars_to_concat`; `head_expression_in_recursive_rule_rejected` | T0 expression semantics + the recursion ban |
| diag_emit | `one_sqlite_process_per_emit`; `apostrophe_round_trip`; the live out/diag.sqlite row set (8 rows incl. the LANG.md self-flags) | the emission seam; batching law graded by counter |

Marble-fixture precedent: this corpus is the cross-target agreement mechanism the user's json-rx
project already proved out (surface-boil.md INHERIT item 2).

### 5b. The ONE reference interpreter: required feature list

Union of what the lab interpreters implemented, minus rejected semantics:

- T0: level fixpoint with stratified negation; aggregates per the R8 ruling; expressions
  evaluated per binding row after joins, goals left to right (atom binds, `:=` computes,
  comparison filters); named-column positionalization in declaration order; Option columns;
  Key-as-FD check over the fact set; the recursive-head-expression ban via the stratum SCC.
- Tick: arrivals as an ordered list with duplicates (the input the R1 mechanisms disagree about;
  cannot be pre-normalized); boundary diffing per R7 with the multiset restatement; keyed replace
  emitting -old/+new, equal-row write a no-op; edge firing per the R5 ruling; edge-write
  propagation per the R9 ruling with engine-owned drain ticks; occurrence identity per the R1
  ruling; `now()` read-only and never arrival-eligible; `pre` = T-1 with chaining per R1's rider.
- Effects: demand rows, content-addressed dedup scoped to Set rels, clock-bucket and input-digest
  salts, Stream/Tail envelopes with terminal-is-terminal, two-channel bind fills.
- Patterns: grammar-fact import, pattern DCG, check-annotate returning `ok/bad`, reference
  unification matcher (native query emission stays a second backend with the refusal channel; it
  is not part of the reference interpreter).
- Checks: pairwise body disjointness for keyed heads; fold-shape catalog recognition;
  `<+`-into-Set rejection; retention presence on Log rels; link-time lifetime obligation.

Excluded (rejected semantics): unqualified any-atom edge firing; arrival-tick demand salt;
count-blind Z-set folds (unless B is ruled); the scan cluster sugar; `apply`/`it`; str+str `+`;
type-directed elaboration; mode-polymorphic `=`; the flat streaming envelope; per-occurrence
demand firing (the a_naive/b_naive settings exist only as measured failure modes).

### 5c. The cross-interpreter agreement check

Mechanism: every 5a fixture becomes (program terms, world/arrival schedule, expected trace or
row set) in one shared format; the reference interpreter runs all of them; each lab's recorded
expectations are the oracle. A green run over the full corpus is the proof that the nine sketches
described one language. It cannot go green until the rulings land, because the lab interpreters
contradict each other in at least these places, which the fixture run would surface on day one:

| contradiction | interpreters | resolved by |
|---|---|---|
| arrival-set computation: edge-written rows are never arrivals (merge_family.pl:175-179) vs carried to next tick (temporal_pipe.pl:347/:356) vs cascaded within the tick's settle loop (check_eventing settle step) | merge vs pipe vs eventing | Q4 (R9) |
| COUNT DISTINCT vs bag: rail sorts projected pairs (timeless_rail.pl:290-291) vs v5 SQL counting join rows | rail vs the v5 lowering both must match | Q7 (R8) |
| drain scheduling: pipe hand-feeds empty ticks (temporal_pipe.pl:485-486); no other interpreter or doc owns drains | pipe vs everyone | Q5 |
| `pre` within a tick: chains across occurrences (occurrence lab, what makes the fold correct) vs every arm reads the same T-1 value (merge ambiguity 5, what causes the undercount) | occurrence vs merge | Q1 rider on R6 |
| same-tick visibility of edge writes to level rules: eventing's hook REQUIRES it (else `turn_diag` lags a tick); merge's two-phase order gives it only after the second closure; pipe's next-tick carry delays it | eventing vs merge vs pipe | Q4 (R9) |
| event multiplicity carrier: explicit id column (merge) vs engine stamps with no column (occurrence) | merge vs occurrence | Q1/Q2 |
| interpolation semantics: render-after-join (rail) vs desugar-to-concat before typing (expressions) | rail vs expressions | settled: expressions wins (sect 2); rail fixture re-graded under concat desugar |
| keyed equal-row write: no-op (merge's choice) vs -x/+x (the SWR reading) | merge vs LANG.md:62 | residual ruling (written_at column recommendation) |

Once the rulings land, the disagreeing fixtures are re-graded under the ruled semantics and kept;
the pre-ruling expectations move into the fixtures' comments as the rejected reading, so the
corpus also documents what was decided against and why.
