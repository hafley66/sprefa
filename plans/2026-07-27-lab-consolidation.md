# Lab-wave consolidation, 2026-07-27

Five concurrent opus labs over the candidate surface (v6/prolog/labs/LANG.md):
shell_stream (20 PASS), merge_family (12 PASS), check_eventing (17 PASS),
astgrep_patterns (36 PASS), AUDIT.md (18 findings, 9 blockers). All graders
re-run and verified green by the coordinator. 85 graded checks total.

## PROVEN (keep; receipts in the lab files)

1. Level/edge carries the whole LSP loop: diagnostics are level views that
   retract on fix, history is edge and survives, ratchets tighten by key
   replace, the agent-hook window is a snapshot join at (multi, finite).
2. merge and mergeByKey lower to plain rules + keys, no sugar warranted
   (the scan term_expansion sugar was built, graded identical, and rejected).
3. Streaming effects: result-position type wrappers `-> Stream(Item, End)` /
   `-> Tail(Item)`; mode is a function of the result type; grounds in
   {ground_terms, rule, external_rel} with NO register dependency. New mode
   cell (multi, finite). Link-time lifetime obligation: a bind must discharge
   its rel's finiteness claim (tail -f into Stream = link error).
4. Two salts, two recurrence kinds: clock bucket = time recurrence (polls),
   input digest = change recurrence (extraction); digest salt makes dedup and
   re-extract-on-change one rule. The boil-pot arrival-tick salt proposal is
   WRONG (audit: reintroduces gh-cache.dl's 720-vs-12 calls/hour failure).
5. Quoted-DSL pipeline works end to end: node-types.json -> grammar facts ->
   pattern DCG -> grammar-driven check -> two lowerings. SWI quasiquotation
   tested and preferred (compile-time phase: pattern checks are compile
   errors); `~` demoted to a relation over an already-checked pattern value.
6. Debounce needs no operator: a keyed due-row replace IS the debounce.

## PROVEN BROKEN (the labs' contradictions, need rulings)

R1 WITHIN-TICK OCCURRENCE IDENTITY (merge scan_undercounts_batched_events +
   shell #7 + audit top-5): a tick is a set; scans/folds need ordered
   multiplicity. Two same-tick increments fold to +1 not +2, silently.
   Ruling needed: occurrence identity mechanism (seq column stamped by the
   engine vs Z-set multiplicities). BLOCKS: register_lowering, mergeByKeyScan.
R2 `<+` INTO KEYED REL (audit #4 + merge #6 + eventing): "append, never
   retract" vs key replace emitting -old. Resolution direction: the arrow
   owns the TRIGGER (arrival), the rel's key owns the STORAGE effect
   (replace); restate LANG so retraction-by-key is not "the arrow retracting".
R3 now() IS KERNEL (eventing, measured 5-vs-13 row storm): a phantom
   tick read cannot desugar into a clock-rel join; enters the kernel before
   any body grammar freezes.
R4 NO EDGE ON DEPARTURE (eventing): `<+` fires on arrivals only;
   diag_closed_at / time-to-fix telemetry inexpressible. Ruling: a departure
   trigger form, or accept the hole.
R5 delta() DIED TOO EARLY (audit #5 + eventing #9): killing it removed the
   only per-rule control over backlog replay (any-atom edge trigger). Ruling:
   per-atom trigger marking in edge bodies (which atom is the clock).
R6 pre(x) + same-tick read of keyed x underdefined (audit #6): needs the
   read-before-write tie-break stated (the register-row-is-pre rule, said
   for registers, never restated for keyed rels).
R7 TICK-BOUNDARY DIFFING is the count_ivm_port CONTRACT (eventing #1):
   one tick, one delta set, diffed at the boundary; otherwise every
   keystroke flickers -x/+x and mints bogus history.

## THE AUDIT'S BIGGER VERDICT: the missing 90%

Expressiveness vs 13 sampled v5 programs: 11 NO, 2 with-sugar, 0 yes.
The candidate surface has NO syntax for: arithmetic/comparison (166/173
files), `?` queries (130), extraction (139: scan + match/ast/sg/json),
aggregates (76), string interpolation (69), diag product (55), gen/write
effects (30, incl. apply gate + dry-run + idempotent-write-vs-dedup),
graph builtin operators (29), retention bounds, the cadence bucket's home.
The week's design built the temporal 10% first. The tier doc's order stands:
T0/T1 surface (timeless fragment) is the next lab wave, not more time.

Also: registry drift is real. sugar_grounds_out only audits features that
registered; kernel.pl still declares surface_form for three dead keywords;
checks.pl no_self_union outlaws transitive closure (44 corpus files).
The self-checking-design discipline needs a "surface census" check that
enumerates constructs from the spec, not from voluntary registration.

## Key(Type) vs `->`: labs SPLIT (user decision)

- merge lab: Key wins on receipts (all keyed behavior off column positions).
- audit: merge them (-> IS Key on demand columns for det effects).
- astgrep lab: genuinely different (pattern types parameterized by link-time
  grammar import do not fit an FD reading).
Present both files' arguments; do not resolve by fiat.

## Tier/task updates earned

- ts_grammar_import: unbuilt -> labbed (astgrep lab is the lab).
- mode_lab scope grows: result-type modes, pairwise arm disjointness
  (not per-rule semidet), link-time lifetime obligation, the five ask rows
  from eventing.
- register_lowering: blocked on R1 + disjointness proof.
- count_ivm_port: contract = R7 boundary diffing.
- surface_dcg: owes a raw-text region token NOW (retrofitting a raw-text
  mode into a finished lexer is the expensive version).
- retention_bound: promoted to requirement (streaming makes one demand row
  produce unbounded rows); only ranges over edge-headed rels (one-pass fold).
- NEW candidate first milestone: timeless-fragment surface (audit top-5:
  extraction, <+/keyed restatement, within-tick visibility, retention,
  cadence home) -> a v5 lint rail running under v6 semantics, zero temporal
  tier involved.
