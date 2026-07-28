% lowering.pl : Q3 RX DIRECTNESS.
%
% Standing user law: every .dl construct shown carries its intended pure-rxjs
% lowering, and a construct whose lowering cannot be written is a design
% defect. This file is that lowering, as data, for every legal Q1 cell and
% every coherent Q2 cell.
%
% Grades:
%   direct         plain operator composition; the callback bodies are pure
%                  functions over the emitted value, the same exemption a
%                  `.map` callback gets.
%   direct_vacuous the expression is plain rx AND compiles, but it does not
%                  deliver the semantics the cell claims. Counted separately
%                  because counting it as DIRECT would be a lie.
%   encoded        needs a state table, a scan, or an externally imposed
%                  order beyond vanilla operator composition.
%   impossible     the semantics exceed what rx can express at all.

:- module(mf_lowering, [ lowering/4, lowering_grade/1 ]).

lowering_grade(direct).
lowering_grade(direct_vacuous).
lowering_grade(encoded).
lowering_grade(impossible).

% lowering(Key, Expression, Grade, Note)

% ═══ Q1 legal cells ═════════════════════════════════════════════════════════

lowering(cell(bare_atom, arm_plus),
         'arrivals$.pipe(filter(row => row.rel === "cache"), map(row => head(row)))',
         direct,
         'the marked trigger is one filter over the arrival stream').
lowering(cell(next_form, arm_plus),
         'arrivals$.pipe(filter(row => row.rel === "cache"), map(row => head(row)))',
         direct,
         'byte-identical to the bare atom; next() is spelling, not semantics').
lowering(cell(bare_atom, arm_level),
         'combineLatest([cacheRows$, otherRows$]).pipe(map(([cache, other]) => join(cache, other)))',
         direct,
         'a level rule is combineLatest over its body rels; NON-recursive only').
lowering(cell(bare_atom, body_edge),
         'merge(...bodyAtoms.map(name => arrivalsOf(name))).pipe(mergeMap(hit => from(joinRestAgainstStore(hit))))',
         encoded,
         'the C2 any-body-atom model: every body atom is its own trigger and the REST of the body is re-read from the store, which vanilla rx cannot hold').
lowering(cell(bare_atom, body_level),
         'combineLatest([...bodyRels$]).pipe(map(rows => derive(rows)))',
         direct,
         'same as the arm_level cell').
lowering(cell(finalize_form, arm_plus),
         'relRows$.pipe(startWith([]), pairwise(), mergeMap(([prev, next]) => from(prev.filter(row => !next.includes(row)))))',
         direct,
         'R7 boundary diff IS pairwise plus a pure set-difference callback; no new machinery').
lowering(cell(finalize_form, body_edge),
         'relRows$.pipe(startWith([]), pairwise(), mergeMap(([prev, next]) => from(prev.filter(row => !next.includes(row)))))',
         direct,
         'identical; departed/1 and a finalize arm are the same goal').
lowering(cell(comparison_guard, any),
         'filter(row => row.size < 10)',
         direct,
         'holds in all four columns').
lowering(cell(row_pattern, any),
         'filter(row => row.phase === "fetching")',
         direct,
         'holds in all four columns').
lowering(cell(enum_destructure, any),
         'map(row => JSON.parse(row.body)), filter(value => value.tag !== undefined)',
         direct,
         'json1 in the SQL lowering, a pure map/filter pair in the rx one').
lowering(cell(negation, arm_plus),
         'withLatestFrom(liveTable$), filter(([row, live]) => !live.has(row.key)), map(([row]) => head(row))',
         encoded,
         'liveTable$ is a held state table, not an rx value; and see scenario d, this composition also silently reorders when the negated rel is edge-headed').
lowering(cell(negation, body_edge),
         'withLatestFrom(liveTable$), filter(([row, live]) => !live.has(row.key)), map(([row]) => head(row))',
         encoded,
         'same').
lowering(cell(negation, arm_level),
         'concat(...strata.map(stratum => defer(() => of(recompute(stratum))))).pipe(last())',
         encoded,
         'stratification is an ORDER imposed from outside rx; concat expresses the order but nothing in rx computes the strata').
lowering(cell(negation, body_level),
         'concat(...strata.map(stratum => defer(() => of(recompute(stratum))))).pipe(last())',
         encoded,
         'same').

% ═══ Q2 coherent cells ══════════════════════════════════════════════════════

lowering(frontier(value, tn, edge_set),
         'from(occurrences).pipe(concatMap(occurrence => applyWritesToStore(occurrence)))',
         encoded,
         'the sequential per-occurrence store read is a mutation, not an rx value; concatMap only supplies the ordering').
lowering(frontier(value, tn, level),
         'map(base => levelClosure(base))',
         direct,
         'one pure function per tick; but see the frozen-MidLevel asymmetry in frontier.pl').
lowering(frontier(value, ti, level),
         'map(base => levelClosure(base))',
         direct,
         'the post-write recompute is the same pure function called a second time').
lowering(frontier(value, tn, edge_log),
         'concatMap(row => runner.run(insertWithStamp(row, tickNumber, seq++)))',
         direct,
         'the stamp is a pure function of (tick, seq); the write is the one SqlRunner seam').
lowering(frontier(value, tn, effect_demand),
         'concatMap(row => runner.run(insertDemand(row)))',
         direct,
         'a demand row is an ordinary edge write; only the FILL is asynchronous, and that is the occurrence cell below').
lowering(frontier(value, ti, edge_set),
         'concatMap(row => runner.run(upsertKeyed(row)))',
         direct,
         'identical to the Tn cell; Ti adds nothing to VALUE visibility for an edge-set head').
lowering(frontier(value, ti, edge_log),
         'concatMap(row => runner.run(insertWithStamp(row, tickNumber, seq++)))',
         direct,
         'identical to the Tn cell').
lowering(frontier(value, ti, effect_demand),
         'demandRows$.pipe(mergeMap(row => host.run(row)), map(result => fillRow(result)))',
         direct,
         'the host is an Observable; mergeMap is the whole story. Which TICK the fill lands on is the schedule s business, not the expression s.').
lowering(frontier(occurrence, ti, edge_set),
         'of(boot).pipe(expand(step => step.carryPending ? program.tick(seam, []) : EMPTY))',
         direct,
         'this is literally tsv2 tickLoop.ts:47-64, already shipped').
lowering(frontier(occurrence, ti, edge_log),
         'of(boot).pipe(expand(step => step.carryPending ? program.tick(seam, []) : EMPTY))',
         direct,
         'same operator, multiset delta inside the tick').
lowering(frontier(occurrence, ti, level),
         'of(boot).pipe(expand(step => step.carryPending ? program.tick(seam, []) : EMPTY))',
         direct,
         'same').
lowering(frontier(occurrence, ti, effect_demand),
         'demandRows$.pipe(groupBy(row => row.salt), mergeMap(group => group.pipe(exhaustMap(row => host.run(row)))))',
         direct,
         'content-addressed salt = the groupBy key, so two scopes demanding one identity share one in-flight run, which is the salt ruling exactly').
lowering(frontier(occurrence, tn, any),
         'NONE',
         impossible,
         're-entrant synchronous emission into the subscriber that is mid-next(); with queueScheduler it becomes an unbounded trampoline with no cap. Confirms the design sketch s own refusal.').

% ═══ the Ta cells, both readings ════════════════════════════════════════════

lowering(ta_as_scheduler,
         'edgeWrites$.pipe(observeOn(asyncScheduler))',
         direct_vacuous,
         'THE FINDING: rx schedulers change WHEN, never WHAT. The emitted sequence and therefore the tick log are unchanged, so this expression compiles and delivers none of the Ta semantics.').
lowering(ta_as_primitive_queue,
         'const queue$ = new Subject(); merge(arrivals$, queue$).pipe(...); // rule writes push into queue$',
         encoded,
         'the only rx shape that re-enters a source is a Subject bridge, which the standing no-Subject-bridge corollary bans outright. So primitive Ta cannot be lowered without breaking a standing law.').
lowering(ta_as_pending_rel,
         'pending: arrivals$.pipe(filter(isSrc), map(toPending));  consume: merge(pendingArrivals$, clockArrivals$).pipe(mergeMap(hit => from(joinRestAgainstStore(hit))))',
         direct,
         'DISSOLUTION: two ordinary rules, both already-lowered shapes above. The queue is a durable rel, so the endurance law covers it for free, and the rows are matchable with ordinary arms.').

% ═══ the arm words that have no kernel form yet ═════════════════════════════

lowering(complete_arm,
         'rows$.pipe(groupBy(row => row.scope, { duration: group => scopeGone$.pipe(filter(scope => scope === group.key)) }))',
         direct,
         'rx groupBy s duration selector completes the inner group exactly when the scope row departs. This is the receipt that `complete` == `finalize(scope_row)` and needs no construct.').
lowering(update_arm,
         'relRows$.pipe(pairwise(), mergeMap(([prev, next]) => from(pairUpByKey(prev, next).filter(([o, n]) => o && n && o !== n))))',
         direct,
         'the SQL-trigger reading (AFTER UPDATE, OLD/NEW in one body). Lowers cleanly at the BOUNDARY; a per-occurrence update arm would need the intermediate fold states R2 hides, which is SLOT-UPDATE-ARM.').
lowering(aggregate_min_over_retractable_set,
         'rows$.pipe(map(rows => Math.min(...rows)))',
         direct,
         'full recompute only. The incremental version is impossible: scan/reduce are monotone and a retraction can lower a min with no way to undo the fold.').
lowering(aggregate_min_incremental,
         'NONE',
         impossible,
         'scan cannot un-fold; a retracted contribution requires the whole bag, which rx does not retain.').
