% ARCH.pl : the architecture, as a prolog database that checks itself.
%
% Run:     swipl -q -l v6/prolog/ARCH.pl -g go -g halt        (verify claims)
%          swipl -q -l v6/prolog/ARCH.pl -g roadmap -g halt   (print build order)
%
% This file IS the design doc. Facts describe the graphs, algorithms, tech
% roles, and the syntax kernel; `go` machine-checks the two structural claims
% a prose doc can only assert:
%   1. every surface feature desugars, transitively, into the 5-element kernel
%      (no non-compositional core features can sneak in past this check), and
%   2. the build-order graph is acyclic and total (the roadmap is real).
%
% ═════════════════════════════════════════════════════════════════════════════
% CALLOUTS (the things to keep distinguished, hard-won this arc)
% ═════════════════════════════════════════════════════════════════════════════
%
% * ONE GRAPH, FIVE BINDING TIMES. ast -> stream_ref -> static_subs ->
%   runtime_subs -> delta_flow are refinements of each other, not five designs.
%   Compile owns the first three; sqlite owns the last two, on disk.
%
% * FOUR SOLVER SPECIES, TOTAL. unification (types, clocks), monotone fixpoint
%   (reachability, strata, IVM), fold (init, retention), rewrite (desugar,
%   pushdown). Every analysis in this file is one of the four; a fifth species
%   appearing in a design is a smell.
%
% * EVENTS ARE BASE, FRAMES ARE A TYPE. Subscription-relative time is
%   primitive (subscribe / next / complete). "Absent" is not a constructor:
%   absent(s,I) <- tick(I), not fired(s,I) — a negation against a REFERENCE
%   CLOCK, so it only exists for streams typed @on(R). Frames are entered by
%   aligning to a clock and rewarded with sampling, negation, coherent joins.
%
% * THE ISLAND'S CLOCK IS PROGRAM DATA. per-input, every(1s), on-demand
%   (magic rows), after-delay (due rows), @on(AnyRel): one mechanism. Time
%   policy, laziness, and delay are all "which stream is your clock?".
%
% * ONLY DELTAS CROSS THE COASTLINE. The sqlite/rx boundary and the
%   disk/memory boundary coincide. A stage that would drag a table into JS
%   heap is illegal, not slow. Registers are delta-fed; history is bounded by
%   retention depth and lives in sqlite.
%
% * CONTENTION IS DEGENERATE. No arbitrary mutation + tick-commit writes means
%   the conflict graph is static R/W-set disjointness, computed at compile
%   time. Disjoint components are the thread units. No runtime lock manager.
%
% * PROLOG NEVER RUNS FIXPOINTS AT SCALE. Measured: swipl tabling 1156ms vs
%   sqlite-count 50ms vs dd 26ms at 160k nodes. Prolog checks and weaves;
%   sqlite executes; rx hosts the residue that is genuinely temporal.
%
% * THE BABEL PRECEDENT. regenerator desugared yield/await into a state
%   machine whose control position is a VARIABLE (a register) — sugar first,
%   as the reference semantics; V8 later reabsorbed it natively in C++ for
%   speed. Same pipeline here: prolog desugar is the reference semantics,
%   rust/sqlite native lowering is the optimization, and the two must agree.
%
% * LIFETIME IS DOMINATED. A stream's mode is (cardinality, lifetime):
%   cardinality = prolog's det/semidet/multi; lifetime = finite | until(S) |
%   never, and lifetime(inner) = min(own binding, enclosing scope). switch_map
%   is a scope constructor: every(300s) alone is `never`, under switch_map it
%   is until(outer_next). Unsubscribe = range-DELETE of the scope's demand
%   rows, so dominance is row deletion, not a callback.
%
% * THE REGISTER ROW IS pre. Reading the row before the tick's fold IS the
%   previous value; the batched UPSERT at tick commit is both the update and
%   the downstream delta (-old/+new). pre depth > 1 = LAG reads on a hist
%   table pruned at the retention bound. Crash = roll back to last tick's row.
%
% * MIXED HEADS ARE SOUND (supersedes v5's one-rel-one-rule-kind law, which
%   guarded rebuild_derived's DELETE-all recompute). Under count-IVM a row
%   carries its support; injected and derived rows may share a head, and
%   retraction subtracts per-origin. EDB/IDB is per-ROW origin, not a rel
%   classification.
%
% * PROTOCOLS BIND AT LINK TIME. Program text declares boundary SIGNATURES
%   (external name/arity/envelope); binding(Name, Protocol) facts attach
%   shell/every/sse per deployment. First-order linking, no higher-order
%   terms; every external needs exactly one binding or the link fails. SSE
%   is not a program construct at all: a tail ask held open per connection
%   by a consumer.
%
% * LOWERING ORDER IS RXJS FIRST (user-set 2026-07-27 PM). v6 lowers onto
%   the js engine + rxjs residue and must MATCH rxjs-isms before any rust
%   port: Observables only at real async boundaries, one terminal subscribe,
%   completion as the complete notification, marble-testable behavior. The
%   conformance corpus's per-tick delta lists ARE marble diagrams (tick =
%   frame), so the js leg reuses the same fixtures as its oracle. The rust
%   port comes AFTER and must agree via that shared corpus — which is
%   json-rx's own mechanism (marble fixtures as the cross-target agreement
%   record), so the portable core that emerges from the two-target agreement
%   IS the json-rx kernel, realized on iteration three.
%
% * NO @ SYMBOL. Time is a FIELD. A clock is a tick-valued column plus a
%   join; "clocked on R" = "this struct carries tick_of(R)". Record typing
%   tracks which rels carry which tick fields, so the temporal checker is the
%   ordinary struct checker and clock_on is sugar, not kernel.
%
% * IMPURE LOOPS ARE PRODUCTIVE, NOT TERMINATING. Two recursion budgets:
%   in-tick recursion must terminate (datalog's guarantee); recursion THROUGH
%   a yield point (fetch page -> more? -> fetch next) may be infinite but is
%   productive — each turn does IO / advances a clock. Loop state = register,
%   iteration clock = response arrival, termination = stop carrying (dedalus
%   non-persistence). The gh-cache pr_number -> change_log carry chain is the
%   working precedent already in the v5 engine.
%
% ═════════════════════════════════════════════════════════════════════════════

:- use_module(library(lists)).

% ── the graphs: one structure, five binding times ───────────────────────────
% graph(Name, Shape, Rows, BindingTime)

graph(ast,          dag,      rule_terms,        compile).
graph(stream_ref,   dag,      pipe_stages,       compile).      % post-partition
graph(static_subs,  dag,      demand_edges,      compile).
graph(runtime_subs, forest,   sub_path_rows,     runtime_disk). % reconciler keys
graph(node_share,   dag,      node_hash_rows,    runtime_disk). % call variants
graph(delta_flow,   payloads, delta_rows,        runtime_tick).
graph(strata,       dag,      scc_condensation,  compile).
graph(conflict,     sets,     rw_set_pairs,      compile).      % degenerate, static
graph(clock_lattice, lattice, on_chains,         compile).

refines(stream_ref, ast).
refines(static_subs, stream_ref).
refines(runtime_subs, static_subs).
refines(node_share, runtime_subs).
refines(delta_flow, runtime_subs).
refines(strata, ast).
refines(conflict, stream_ref).
refines(clock_lattice, ast).

% ── the algorithms and their solver species ─────────────────────────────────
% algorithm(Name, RunsOn, Species, Home)   Home = shelf file | tier | unbuilt

species(unification).
species(monotone_fixpoint).
species(fold).
species(rewrite).

algorithm(hm_types,        ast,         unification,       'books/v6/algos/unify_hm.pl').
algorithm(clock_inference, ast,         unification,       'books/v6/algos/clock_calculus.pl').
algorithm(init_analysis,   ast,         fold,              'books/v6/algos/initialization.pl').
algorithm(retention_bound, ast,         fold,              'books/v6/algos/retention.pl').
algorithm(causality,       ast,         monotone_fixpoint, 'books/v6/algos/causality.pl').
algorithm(exhaustiveness,  ast,         fold,              'books/v6/enum_match.pl').
algorithm(stratification,  strata,      monotone_fixpoint, 'js: lower/rulegraph.ts').
algorithm(desugaring,      ast,         rewrite,           'books/v6/rel_island.pl (term_expansion)').
algorithm(island_split,    stream_ref,  fold,              unbuilt).
algorithm(pushdown,        stream_ref,  rewrite,           unbuilt).
algorithm(rw_disjoint,     conflict,    fold,              unbuilt).
algorithm(magic_demand,    static_subs, rewrite,           'books/v6/algos/magic_sets.pl').
algorithm(seminaive_eval,  delta_flow,  monotone_fixpoint, 'sqlite via js lowerSql / rust store').
algorithm(count_ivm,       delta_flow,  monotone_fixpoint, 'rust store (beat DRed 4-5x)').
algorithm(mode_analysis,   static_subs, monotone_fixpoint, 'plans/2026-07-27-mode-lattice.md (labbed; engine home unbuilt)').
% ^ was fold: the mode lattice lab found the graph cyclic (poll -> fetch ->
%   cache -> cache_tag -> poll), so the fold iterates to a least fixpoint.
algorithm(sql_emit,        ast,         rewrite,           'books/v6/algos/lower_sql.pl').
algorithm(ts_emit,         ast,         rewrite,           'src/emit_ts.pl (literal TS onto the engine-v1 seam)').

% ── prior art: the user's own tools and what each feeds v6 ──────────────────
% prior_art(Name, Path, Feeds, Gift). The anti-forgetting ledger (user
% 2026-07-27: "i forgot i made atlas, i make too many things"): every tool
% already built, where it lives, and which v6 piece inherits from it.

prior_art(sprefa_v5,     '~/projects/sprefa',                      extraction_binds,
          'solved machinery: scan/git/ast/watchers; enters v6 as binds, never rebuilt').
prior_art(og_coordinate, '~/projects/sprefa-archive-20260428',     file_span_types,
          'byte-span refs table + repo/tag auto-checkout = the File/FileSpan/discovery loop').
prior_art(json_rx,       '~/projects/hafley-rxjs/packages/json-rx', conformance_corpus,
          'marble fixtures as cross-target agreement; per-tick deltas ARE marbles').
prior_art(hafley_tsp,    '~/projects/hafley-tsp',                  bind_vocabulary,
          'TypeSpec app-gen; config/env/CLI sources + @secret redaction for binds').
prior_art(atlas_anim,    '~/projects/anim',                        arch_map,
          'the vis ARSENAL, not just atlas: cone-focus graph atlas, Frames animated explainers, CodeSpotlight, CssGraph, deck, datalog-to-atlas + from-git converters, shoot-* screenshotters; arch_map emits into it').
prior_art(dl_flow_panel, 'editors/vscode-dl',                      circuit_view,
          '_node/_edge layer discovery; the schema-convention precedent for program maps').

% ── the everytool inventory (user vision 2026-07-27) ────────────────────────
% capability(Name, V5Receipt, V6Home). Every capability is a query or an
% emitter over the ONE fact spine; none is a subsystem. That is the whole
% bet: parsing/doc/comment/type/flow/refactor/lsp/codegen ride the same
% rels, so each lands as a program.

capability(language_parsing,   'ast/sg/comment/json ops, tree-sitter grammars', 't1_quoted_regions_grammar_import').
capability(doc_management,     'examples/gen-doc-index.dl, doc_ref rel',        't3_library').
capability(comment_management, 'comment op, @eprintln-ok waivers',              'comment_span_region_extraction').
capability(type_measurement,   'type_entity rels, measures views',              't0_structs_plus_queries').
capability(cross_repo_pointing,'pin-skew, flow-services, openapi-lsp',          'xref_rel_plus_rev_demand').
capability(flow_analysis,      'std/flow.dl, flow_edge closure, taint',         't2_graph_operators').
capability(refactoring,        'v5 --move, auto-refactor brace arcs',           'far_write_effects').
capability(auto_docing,        'gen rules writing doc indexes',                 't5_write_effects_audit15').
capability(lsp,                'dl-lsp, --diag-db, diag_v5 loop',               't3_diag_plus_t6_tail_asks').
capability(codegen_typegen,    'hafley-tsp + json-rx lineage',                  'far_emitters_over_facts').

% ── tech roles: who is allowed to do what ───────────────────────────────────

tech(prolog, compiler_tier, [parse_via_ops, desugar, check, weave, emit],
     'never runs fixpoints at scale; bundled with the eventual rust binary').
tech(sqlite, fact_tier, [facts, fixpoints, registers, history, sub_graph, pending_delays],
     'everything diskable; the only tier allowed to hold a full relation').
tech(rxjs, temporal_tier, [yield_points, alignment_ops, spawn, one_subscribe],
     'the residue that is genuinely temporal; delta-fed only').
tech(typescript, host, [drivers, generated_target, udf_registration],
     'generated code is literal readable TS, never AST manipulation').
tech(rust, future_bundle, [extraction, daemon, udfs, store],
     'extraction is solved here; prolog ships inside it as the compiler').

% ── technique per subproblem ────────────────────────────────────────────────

technique(laziness,      demand_rows_as_clock,        'magic set = subscriber table').
technique(throttle,      island_clock_annotation,     '@async clock(N,_) exists already').
technique(delay,         due_row_plus_clock_join,     'job queue run_at; survives crashes').
technique(teardown,      path_prefix_delete,          'range DELETE on sub paths').
technique(sharing,       node_hash_consing,           'call variants; two paths, one node').
technique(memory_bound,  retention_depth,             'max pre depth = ticks kept').
technique(parallelism,   rw_disjoint_components,      'static; no lock manager').
technique(absence,       negation_vs_reference_clock, 'needs @on(R); rejected on @own').
technique(glitch_safety, clock_unification_on_joins,  'combineLatest on foreign clocks flagged').
technique(recovery,      state_at_tick_is_a_row,      'replay = reread tables').
technique(state_update,  upsert_at_tick_commit,       'register row = current state; UPDATE..CASE emitted per register, UDF escape hatch').
technique(prev_value,    row_read_before_fold,        'pre = the row pre-write; depth>1 = hist LAG, retention-pruned').
technique(ask_modes,     snapshot_vs_subscribe,       'read-1 = SELECT, always finite; tail = mode-typed (card, lifetime)').
technique(protocols,     bind_facts_at_link,          'signatures in program, binding(Name, Protocol) per deployment; test = canned rows').

% ═════════════════════════════════════════════════════════════════════════════
% THE SYNTAX KERNEL — the answer to "stop adding non-compositional features".
% FOUR primitives. EVERY surface feature must desugar into them via
% term_expansion (the machinery rel_island.pl proved). A feature that cannot
% state its sugar/2 fact does not get to exist; `go` enforces it.
% Symmetric struct/tuple discipline: a rel is a set of structs, a struct is a
% row, terms nest in columns — one value world, so matching gives branching.
%
% The facts live in src/kernel.pl (shared with every example checker);
% highlights: kernel = {ground_terms, rule, register, external_rel};
% clock_on is SUGAR (a tick field + a join, no @ syntax); impure_loop is the
% regenerator move (control position = register, iterate on the response
% clock, halt = stop carrying).
% ═════════════════════════════════════════════════════════════════════════════

:- use_module('src/kernel.pl').

% ═════════════════════════════════════════════════════════════════════════════
% BUILD ORDER — task(Name, Status, Needs). `roadmap` topsorts it; learning
% order = the same sort, because a technique is learnable once its inputs are.
% ═════════════════════════════════════════════════════════════════════════════

task(kernel_sql_lowering, done,    []).                  % lower_sql + dl_in_prolog, measured
task(desugar_machinery,   done,    []).                  % op/3 + term_expansion (rel_island)
task(clock_check,         labbed,  [desugar_machinery]).
task(init_retention,      labbed,  []).
task(causality_check,     labbed,  []).
task(envelope_types,      labbed,  []).                  % enum_match
task(demand_clocking,     labbed,  [kernel_sql_lowering]).
task(clock_inference,     parked,  [clock_check]).       % swap ground clocks for holes; user-parked 2026-07-27
task(surface_dcg,         unbuilt, [desugar_machinery]). % rust-ish grammar -> kernel facts (sample approved)
task(mode_lab,            labbed,  []).                  % plans/2026-07-27-mode-lattice.md: lifetime = free distributive lattice over end-signals; scope_min=OR join_max=AND; 80/80 at 2fff3f61
task(sub_forest,          labbed,  []).                  % plans/2026-07-27-sub-forest.md: demand rows + sub forest + prefix-DELETE teardown; 7-item engine-absorption list; 41/41 at 2fff3f61
task(register_lowering,   unbuilt, [kernel_sql_lowering]). % UPDATE..CASE per register + hist/retention
task(ts_grammar_import,   labbed,  []).                  % node-types.json -> con/enum facts; typed CST matching (astgrep lab, lab-consolidation PROVEN 5; quoted-DSL pipeline end to end)
task(purity_split,        unbuilt, [desugar_machinery]). % pure-body test per segment
task(island_partition,    unbuilt, [purity_split, clock_check]).
task(rw_sets,             unbuilt, [purity_split]).
task(pushdown_optimizer,  unbuilt, [island_partition, rw_sets]).
task(thread_schedule,     unbuilt, [rw_sets]).
task(emit_ts_direct,      done,    [kernel_sql_lowering]). % src/emit_ts.pl -> ast.ts helpers, swi-emit bench row
task(js_conformance_leg,  unbuilt, [emit_ts_direct]).      % run the conformance fixture corpus against the js engine + rxjs residue; per-tick deltas as the marble oracle; RXJS FIRST law
task(sub_graph_disk,      unbuilt, [emit_ts_direct]).
task(count_ivm_port,      unbuilt, [kernel_sql_lowering]).
task(cost_model,          unbuilt, [pushdown_optimizer]). % perf rows feed plan choice

roadmap :-
    findall(Name-Needs, task(Name, _, Needs), Pairs),
    topsort(Pairs, [], Order),
    forall(member(Name, Order),
           ( task(Name, Status, _),
             format("~w~t~28|~w~n", [Name, Status]) )).

topsort([], _, []).
topsort(Pending, Done, [Name | Order]) :-
    select(Name-Needs, Pending, Rest),
    forall(member(N, Needs), memberchk(N, Done)), !,
    topsort(Rest, [Name | Done], Order).

% ═════════════════════════════════════════════════════════════════════════════
% the self-checks
% ═════════════════════════════════════════════════════════════════════════════

check(sugar_grounds_out, ( forall(sugar(Feature, _), grounds(Feature)) )).
check(species_are_four,  ( forall(algorithm(_, _, Sp, _), species(Sp)) )).
check(graphs_refine_ast, ( forall((graph(G, _, _, _), G \== ast),
                                  reaches_ast(G)) )).
check(roadmap_is_total,  ( findall(N-Ns, task(N, _, Ns), Ps),
                           topsort(Ps, [], O),
                           length(Ps, L), length(O, L) )).

reaches_ast(ast).
reaches_ast(G) :- refines(G, Parent), reaches_ast(Parent).

go :- forall(check(N, G),
             ( catch(G, E, (print_message(error, E), fail))
             -> format("PASS  ~w~n", [N]) ; format("fail  ~w~n", [N]) )).
