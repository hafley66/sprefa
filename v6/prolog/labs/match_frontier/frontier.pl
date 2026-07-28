% frontier.pl : Q2 FRONTIER MATRIX.
%
% {value visibility, occurrence visibility} x {Tn, Ti, Ta} x head kind
% {level, edge_set, edge_log, effect_demand} = 24 cells, each with a
% termination consequence and what the tick log shows.
%
% Every claim about current behavior cites engine.pl. The reader should note
% one asymmetry the design sketch did not state and this matrix makes
% explicit (verdict-doc ambiguity 3):
%
%   an EDGE WRITE's value is Tn-visible to later occurrences in the same
%   tick (apply_edge_writes/6 :236-254 mutates Store, and
%   process_occurrences/7 :210-212 recomputes Visible from the store at the
%   top of EVERY occurrence),
%   but a LEVEL row derived from that write is NOT: MidLevel is computed once
%   at :286 and passed FROZEN through the whole occurrence loop (:291), with
%   the post-write recompute only at :295.
%
% So "Tn" is two different frontiers depending on head kind, and nothing in
% the sketch says so.

:- module(mf_frontier, [ frontier/6, frontier_axis/1, frontier_time/1, head_kind/1 ]).

frontier_axis(value).
frontier_axis(occurrence).

frontier_time(tn).
frontier_time(ti).
frontier_time(ta).

head_kind(level).
head_kind(edge_set).
head_kind(edge_log).
head_kind(effect_demand).

% frontier(Axis, Time, HeadKind, Verdict, Termination, TickLogShows)
%   Verdict     = coherent(Note) | incoherent(Why) | refused(Why)
%   Termination = bounded | drain_capped | unbounded | stranded

% ═══ VALUE VISIBILITY ═══════════════════════════════════════════════════════

frontier(value, tn, level,
         coherent('only for level rows derivable from ARRIVALS; MidLevel is frozen at engine.pl:291, so level rows downstream of a same-tick edge write are Ti, not Tn'),
         bounded,
         'the +delta lands on tick T; a reader at T sees the arrival-derived half only').
frontier(value, tn, edge_set,
         coherent('apply_edge_writes :236-254 mutates the store in place; Visible is rebuilt per occurrence at :210-212, so occurrence k+1 reads occurrence k s write'),
         bounded,
         'one net +delta per row at T; intermediate fold states are invisible (R2 rider, :299-301)').
frontier(value, tn, edge_log,
         coherent('same store path plus a stamp st(Tick,Seq) at :241-242'),
         bounded,
         'one +delta PER NEW STAMP (r7, :328), so a duplicate row shows twice').
frontier(value, tn, effect_demand,
         coherent('the DEMAND row itself is an ordinary edge write and is Tn-visible; the FILL never is, no host resolves inside a tick'),
         bounded,
         'the demand row +delta at T; nothing of the result').

frontier(value, ti, level,
         coherent('PostWriteLevelRows at :296 are the level rows that only became true after edge writes; they are readable from T+1'),
         drain_capped,
         'the level +delta lands at T; a rule reading it fires at T+1').
frontier(value, ti, edge_set,
         coherent('a Tn value is trivially also a Ti value; this cell is the default reading of an edge write'),
         drain_capped,
         'unchanged from the Tn cell').
frontier(value, ti, edge_log,
         coherent('same'),
         drain_capped,
         'unchanged from the Tn cell').
frontier(value, ti, effect_demand,
         coherent('the earliest a fill COULD land, and only for a synchronous host; real transports land at an unpredictable later tick, which is a rel question not a frontier question'),
         drain_capped,
         'demand +delta at T, fill +delta at whatever tick the host answered').

% ── the Ta value row: THE CRACK ─────────────────────────────────────────────
% rx schedulers are semantically transparent: observeOn(asyncScheduler)
% changes WHEN a value is emitted in wall-clock terms and never WHICH values
% are emitted or in what order relative to the same stream. So there is no
% such thing as "this value becomes visible at the next EDB tick" delivered
% by a scheduler. The only thing that can hold a value across a tick boundary
% is a ROW IN A REL. Every Ta value cell is therefore incoherent as stated,
% and the coherent replacement is the pending-rel encoding (scenario f).
frontier(value, ta, level,
         incoherent('a level rule is a maintained view; deferring its VALUE means the view is wrong for a tick, which contradicts what a level rule is'),
         bounded,
         'nothing distinguishable from Ti in the log').
frontier(value, ta, edge_set,
         incoherent('no rx scheduler gates visibility; only a rel can hold a value across a tick boundary'),
         bounded,
         'identical to Ti except for the tick INDEX, which is engine-chosen and therefore ungradeable').
frontier(value, ta, edge_log,
         incoherent('same, and worse: the stamp would have to be minted at queue time or delivery time, and nothing says which'),
         bounded,
         'stamp order becomes engine-chosen').
frontier(value, ta, effect_demand,
         incoherent('this is the cell Ta was invented for, and it is exactly the one a pending rel already covers: the demand row IS the queue'),
         bounded,
         'the pending rel encoding shows the queue in the log; primitive Ta shows nothing').

% ═══ OCCURRENCE VISIBILITY ══════════════════════════════════════════════════

% Tn occurrences: refused by the design, and unimplementable in the oracle.
% The occurrence list is fixed at :290 BEFORE process_occurrences/7 runs;
% feeding writes back into that same list is an in-transaction loop with no
% cap anywhere (the drain cap is a TICK counter, engine.pl:79/:376).
frontier(occurrence, tn, level,
         refused('would require re-running level_closure inside the occurrence loop, unbounded'),
         unbounded,
         'nothing; the run does not terminate').
frontier(occurrence, tn, edge_set,
         refused('in-transaction loop; breaks one-body-one-time-cut, no cap exists at occurrence granularity'),
         unbounded,
         'nothing; the run does not terminate').
frontier(occurrence, tn, edge_log,
         refused('same, and every lap mints a new stamp so the log grows without bound too'),
         unbounded,
         'nothing; the run does not terminate').
frontier(occurrence, tn, effect_demand,
         refused('same; additionally a host cannot answer inside the tick that demanded it'),
         unbounded,
         'nothing; the run does not terminate').

frontier(occurrence, ti, level,
         coherent('PostWriteLevelRows :296 join CarryOut :302 and fire as occurrences at T+1'),
         drain_capped,
         'delta at T, firing at T+1; a chain of N stages costs N ticks (engine_core.pl edge_chain_hops_tick_per_stage)').
frontier(occurrence, ti, edge_set,
         coherent('ArrivalCarry :302-304 keeps only rows that are boundary +deltas; -deltas of LISTENED rels become dep(Row) departure occurrences :307-311'),
         drain_capped,
         'the collapse is visible here: a row written and overwritten inside one tick is never an occurrence, so scenario a loses N-1 of N transitions').
frontier(occurrence, ti, edge_log,
         coherent('one occurrence per new stamp'),
         drain_capped,
         'duplicates fire twice, matching the multiset delta').
frontier(occurrence, ti, effect_demand,
         coherent('the demand row s own T+1 occurrence is what starts the host run'),
         drain_capped,
         'demand +delta at T, host starts on the T+1 occurrence').

% ── the Ta occurrence row: nondeterministic, therefore ungradeable ──────────
frontier(occurrence, ta, level,
         refused('a level rel has no occurrence of its own to defer; only its delta does'),
         bounded,
         'nothing').
frontier(occurrence, ta, edge_set,
         incoherent('coherent ONLY under the primitive-queue reading, and then the delivery tick is engine-chosen, so the tick log stops being a function of program+schedule; scenario f proves ta_after(1) and ta_after(2) differ'),
         stranded,
         'the queue itself is invisible; a stranded queue is silent data loss (model residue, SLOT-SPILL)').
frontier(occurrence, ta, edge_log,
         incoherent('same, plus stamp order becomes engine-chosen'),
         stranded,
         'same').
frontier(occurrence, ta, effect_demand,
         incoherent('same; and this is precisely the case the pending rel covers with zero constructs and full visibility'),
         stranded,
         'same').
