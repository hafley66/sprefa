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
algorithm(ts_emit,         ast,         rewrite,           'src/emit_ts.pl (engine-v1 seam experiment; superseded by the tsv2 rows below)').

% tsv2 (2026-07-27 reorientation): prolog owns the WHOLE compiler front;
% stages documented with worked example in compile/PIPELINE.md.
algorithm(tsv2_analyze,    ast,         fold,              'compile/analyze.pl (column mining from variable identity, supported-subset gate)').
algorithm(tsv2_strata,     strata,      monotone_fixpoint, 'compile/strat.pl (mirrors engine relax_strata; Kahn once-per-tick order)').
algorithm(tsv2_lower,      ast,         rewrite,           'compile/lower.pl (lowered/8 target-neutral plan: SQL text + structure, zero TS idiom)').
algorithm(tsv2_ts_emit,    ast,         rewrite,           'compile/emit_ts.pl (backend #1 over lowered/8; emit_rust.pl plugs the same plan via compile_fixture/4)').
algorithm(tsv2_surface_dcg, ast,        rewrite,           'compile/parse_dl.pl + print_dl.pl (phase D, in flight: DCG is the CANONICAL parser; langium was stopgap)').

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
     'never runs fixpoints at scale; bundled with the eventual rust binary. CANONICAL parser (user 2026-07-28: langium/0_generated in v6/dl was a stopgap; the phase D DCG supersedes it, dl.langium stays a spelling reference only)').
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
% CONSTRUCT-COVERAGE — the language construct budget and its receipts. Answers
% the user's own question (2026-07-27, "where is the automated map of language
% features and what they cover"): arch_map.pl's atlas covered ARCHITECTURE
% only; this section adds the LANGUAGE dimension. construct/3 is the budget;
% covers/2 grounds each construct in a conformance fixture file or a user
% ruling that actually exercises or decides it, the same way `algorithm`
% grounds in a Home file. `go` checks that every status is closed and every
% covers/2 endpoint resolves to a declared construct plus a real fixture file
% (on disk under conformance/fixtures/) or a real ruling id (in rulings.pl).
%
% COUNT RECONCILIATION (full line-by-line accounting in
% plans/2026-07-27-construct-coverage.md): plans/2026-07-27-aggregate-
% analysis.md:120-130 totals 30 T0-T4 grammar constructs. rulings.pl:100-104
% then cuts `|>` and `quote(...)` (-2) and rulings.pl:84-89 adopts
% `departed/1` as a new T4 construct (+1) — rulings.pl's own line 102 says so:
% "30 - 2 cuts + 1 departure form = 29". plans/2026-07-27-extraction-
% spellings.md:25 and :444-445, filed LATER the same day (commit timestamps:
% aggregate-analysis 15:38, rulings.pl 16:06, extraction-spellings 18:07),
% states the budget stayed at "28" and does not carry the departure addition
% forward. This section counts 29, the fuller accounting that includes every
% ruled addition; the gap against a naive read of "28" is exactly that one
% uncarried digit in extraction-spellings.md, and it is cited here both ways
% instead of silently picking one. Tiers T5 and above (effect arrow, envelope
% enums, streaming wrappers, tail asks) are NOT part of this budget — the
% aggregate doc's own count table (1d) stops at T4, and this section follows
% it exactly.
%
% construct(Name, Tier, Status). Tier in {t0,t1,t2,t3,t4}, matching the
% aggregate doc's tier map. Status is a CLOSED 3-value set (coarser than the
% aggregate doc's own inconsistent per-row prose -- "(add)", "(add to
% Surface)", "NEW", "RESPECIFIED" -- because the coarse split is what a
% mechanical check can actually verify without parsing prose):
%   kept        - name and semantics carried from LANG.md (or an earlier
%                 ruled lab) unchanged.
%   respecified - the NAME survives but this arc changed its semantics
%                 materially (`<+` under R2 is the only one).
%   new         - no prior name or spelling; either invented this arc or a
%                 replacement spelling for a keyword LANG.md itself killed.
% ═════════════════════════════════════════════════════════════════════════════

:- use_module('conformance/rulings.pl').

construct(enum_decl,            t0, kept).
construct(struct_decl,          t0, kept).
construct(rel_decl,             t0, kept).
construct(key_type,             t0, kept).
construct(option_type,          t0, kept).
construct(level_rule,           t0, kept).
construct(fact,                 t0, kept).
construct(negation,             t0, kept).
construct(aggregate_head_forms, t0, kept).
construct(comparison_ops,       t0, kept).
construct(arithmetic_ops,       t0, kept).
construct(bind_goal,            t0, kept).
construct(fn_application,       t0, kept).
construct(interpolation,        t0, kept).
construct(named_column_atoms,   t0, kept).
construct(wildcard,             t0, kept).
construct(snapshot_ask,         t0, kept).

construct(from_world_modifier,  t1, new).    % unbundled from the killed `source` keyword
construct(bind_decl,            t1, kept).
construct(quoted_region,        t1, kept).
construct(grammar_import,       t1, kept).

construct(graph_operator_position, t2, kept).

construct(edge_rule,            t4, respecified).  % R2: arrow=trigger, rel-kind=storage
construct(rel_kind_decl,        t4, new).          % the 1b convergence construct
construct(trigger_marker,       t4, new).          % Q6; replaces the killed delta()
construct(now_read,             t4, kept).
construct(pre_read,             t4, kept).
construct(retention_clause,     t4, kept).
construct(departure_form,       t4, new).          % R4, ruled 2026-07-27 PM

construct_status(kept).
construct_status(respecified).
construct_status(new).

construct_tier(t0).
construct_tier(t1).
construct_tier(t2).
construct_tier(t3).
construct_tier(t4).

% ── covers(FixtureFileOrRuling, Construct) ───────────────────────────────────
% FixtureFileOrRuling is a fixture basename (matches fixture_file/1 in
% tools/arch_map.pl, one of v6/prolog/conformance/fixtures/*.pl) or a ruling
% id (matches ruling/4 in conformance/rulings.pl). One atom can be both --
% `json_arm` names a fixture file AND a ruling; a covers/2 fact naming it is
% grounded either way. Every edge below is cited line-by-line in
% plans/2026-07-27-construct-coverage.md against a fixture header comment, a
% FIXTURES.md rule, or a ruling comment; no edge is asserted from naming
% alone, and constructs with NO covers/2 fact are the uncovered list (also
% itemized in that file, each with the citation for WHY it is uncovered).

covers(check_eventing,       rel_kind_decl).
covers(check_eventing,       key_type).
covers(check_eventing,       retention_clause).
covers(check_eventing,       edge_rule).
covers(check_eventing,       negation).
covers(check_eventing,       now_read).
covers(check_eventing,       aggregate_head_forms).
covers(check_eventing,       trigger_marker).

covers(engine_core,          rel_kind_decl).
covers(engine_core,          retention_clause).
covers(engine_core,          key_type).
covers(engine_core,          edge_rule).
covers(engine_core,          now_read).
covers(engine_core,          aggregate_head_forms).
covers(engine_core,          trigger_marker).
covers(engine_core,          negation).

covers(expressions,          bind_goal).
covers(expressions,          comparison_ops).
covers(expressions,          arithmetic_ops).
covers(expressions,          fn_application).
covers(expressions,          interpolation).

covers(json_arm,             struct_decl).
covers(json_arm,             aggregate_head_forms).
covers(json_arm,             bind_goal).

covers(merge_family,         rel_kind_decl).
covers(merge_family,         key_type).
covers(merge_family,         retention_clause).
covers(merge_family,         edge_rule).
covers(merge_family,         pre_read).
covers(merge_family,         bind_goal).

covers(occurrence_identity,  rel_kind_decl).
covers(occurrence_identity,  key_type).
covers(occurrence_identity,  retention_clause).
covers(occurrence_identity,  edge_rule).
covers(occurrence_identity,  pre_read).
covers(occurrence_identity,  trigger_marker).
covers(occurrence_identity,  bind_goal).

covers(operators,            rel_kind_decl).
covers(operators,            retention_clause).
covers(operators,            trigger_marker).
covers(operators,            comparison_ops).
covers(operators,            arithmetic_ops).
covers(operators,            fact).
covers(operators,            bind_goal).

covers(shell_stream,         rel_kind_decl).
covers(shell_stream,         retention_clause).
covers(shell_stream,         key_type).
covers(shell_stream,         edge_rule).
covers(shell_stream,         trigger_marker).

covers(spine_semantics,      rel_kind_decl).
covers(spine_semantics,      key_type).
covers(spine_semantics,      retention_clause).
covers(spine_semantics,      edge_rule).
covers(spine_semantics,      now_read).
covers(spine_semantics,      trigger_marker).
covers(spine_semantics,      fact).

covers(state_machine,        rel_kind_decl).
covers(state_machine,        key_type).
covers(state_machine,        retention_clause).
covers(state_machine,        edge_rule).
covers(state_machine,        pre_read).
covers(state_machine,        trigger_marker).
covers(state_machine,        bind_goal).
covers(state_machine,        enum_decl).
covers(state_machine,        struct_decl).

covers(scopes,               key_type).           % keyed(open_scope/2, [1]), scopes.pl:34
covers(scopes,               rel_kind_decl).      % kind(route_change/2, log), scopes.pl:32
covers(scopes,               retention_clause).   % keep(route_change/2, all), scopes.pl:32
covers(scopes,               edge_rule).          % <+ scopes.pl:35
covers(scopes,               level_rule).         % <- scopes.pl:36
covers(scopes,               trigger_marker).     % bare route_change(...), scopes.pl:35
covers(scopes,               pre_read).           % pre(queue_next(...)), scopes.pl:141
covers(scopes,               negation).           % not(closed(...)), scopes.pl:66
covers(scopes,               aggregate_head_forms). % queue_head(_, min(Ordinal)), scopes.pl:146
covers(scopes,               bind_goal).          % Next := SoFar + 1, scopes.pl:141
covers(scopes,               arithmetic_ops).     % SoFar + 1, scopes.pl:141
covers(scopes,               fact).               % non-empty InitialRows, scopes.pl:38
covers(scopes,               departure_form).     % finalize(live_tab(...)), scopes.pl:148
                                                  % ^ first FIXTURE coverage of departure_form
                                                  %   (was ruling-only; uncovered count 10 -> 9)

covers(temporal_pipe,        edge_rule).
covers(temporal_pipe,        trigger_marker).
covers(temporal_pipe,        retention_clause).
covers(temporal_pipe,        comparison_ops).

covers(timeless_rail,        level_rule).
covers(timeless_rail,        negation).
covers(timeless_rail,        aggregate_head_forms).
covers(timeless_rail,        option_type).
covers(timeless_rail,        comparison_ops).

covers(q2_scoping,                rel_kind_decl).
covers(q3_rel_kind_shape,         rel_kind_decl).
covers(q4_edge_propagation,       edge_rule).
covers(q6_trigger_marker,         trigger_marker).
covers(q7_aggregate_multiplicity, aggregate_head_forms).
covers(q8_key_vs_arrow,           key_type).
covers(q9_aggregate_heads,        aggregate_head_forms).
covers(q10_retention,             retention_clause).
covers(r7_boundary_diff,          rel_kind_decl).
covers(r_equal_row_write,         key_type).
covers(r1_rider_pre_chains,       pre_read).
covers(r4_departure,              departure_form).
covers(r6_pre_visibility,         pre_read).
covers(s2_file_rels,              key_type).
covers(s3_dirtiness,              level_rule).

% Resolve fixtures relative to THIS file, not the process cwd: `just arch`
% runs swipl from v6/ and the old repo-root-relative path made
% covers_endpoints_ground fail there while passing from the repo root
% (bit the coordinator and a worktree agent the same night, 2026-07-29).
:- dynamic arch_dir/1.
:- prolog_load_context(directory, Dir), asserta(arch_dir(Dir)).

covers_endpoint_exists(Subject) :- ruling(Subject, _, _, _), !.
covers_endpoint_exists(Subject) :-
    arch_dir(Dir),
    format(atom(Path), "~w/conformance/fixtures/~w.pl", [Dir, Subject]),
    exists_file(Path), !.

% ═════════════════════════════════════════════════════════════════════════════
% COORDINATOR FORKS — fork(Date, Name, Chosen, Alternative, Why).
% User directive 2026-07-29 night: every option the coordinator auto-takes
% lands here with the fork and the reasoning; prefer best-of-both-worlds or
% the easiest-backtrack path. Alternative names the road not taken.
% ═════════════════════════════════════════════════════════════════════════════

fork('2026-07-29', org_arc_agent_shape, one_opus_sequenced_worktree, parallel_rank_lanes,
     "ranks build on each other (lint gate protects later ranks) and all live in v6/prolog; one worktree kills cross-lane conflicts; backtrack = per-rank commits revert independently").
fork('2026-07-29', soak_justfile_wiring, coordinator_wires_at_merge, soak_agent_edits_justfile,
     "org agent owns the v6/justfile prolog-lint recipe; disjoint file ownership law; recipe text rides the soak report so wiring is one paste").
fork('2026-07-29', overnight_agent_tier, claude_sonnet_agent_tool, codex_luna_shell,
     "same capability tier; Agent-tool completions re-invoke the coordinator so the overnight pipeline advances unattended, codex shells need manual polling; sol/terra reserved for hard trade-off arcs per user word").
fork('2026-07-29', phase2_sequencing, soak_lands_before_hosts_p2_dispatch, parallel_dispatch,
     "soak and hosts phase 2 both touch v6/tsv2 runtime; serializing keeps the luna/sonnet-work verification honest; cost = a few hours latency, backtrack free").
fork('2026-07-29', watcher_first_impl, node_fs_watch_behind_bind_seam, parcel_watcher_dep_now,
     "research verdict ranks @parcel/watcher first, but new deps get user approval (pino precedent) and the user is asleep; fs.watch is the verdict's own zero-dep fallback AND chokidar's mac/win backend; the bind seam makes the parcel upgrade a one-adapter swap on user word -- best of both worlds").
fork('2026-07-29', phase2_agent_tier, opus_worktree, sonnet_with_detailed_header,
     "hosts phase 2 = live execution semantics + endurance + watcher, the same trade-off class as the phase-1 bridge which ran on opus; sparing-opus budget spent here deliberately").
fork('2026-07-29', phase3_split, edge_body_arc_first_flagship_pick_deferred, dispatch_flagship_now,
     "the edge-body construct buckets (pre 12, negation 6, now 5, finalize 2) gate what the flagship can express; picking flow-interproc vs a callgraph rail before those land risks picking blind; deferring the pick costs nothing and the golden plan already orders it this way").
fork('2026-07-29', phase3_agent_tier, opus_worktree, sonnet_lane,
     "edge-body lowering is the exact terrain of every silent-wrong cross-plane defect this project has logged; TICK-MODEL semantics + fail-first fixtures need mid-task judgment; third opus lane accepted knowingly against the sparingly word, reasoning recorded").
fork('2026-07-29', pre_lowering_premise, refusal_with_measured_receipt, force_sampled_read_lowering,
     "coordinator's dispatch premise (pre = sampled read like latest) was WRONG -- agent measured pre-as-sampled projecting [1,1] where the oracle pins 2; pre is a chained mid-tick read through ordered occurrences; the honest refusal + receipt beats a silently-wrong lowering, and pre_occurrence_loop is now a priced arc instead of a hidden defect").
fork('2026-07-29', tick_alignment_tier, opus_worktree_fourth_lane, sonnet_or_wait_for_user,
     "runtime tick phase order vs the oracle freeze IS the semantics of the engine; a wrong ordering grades wrong only on join-storm shapes (exactly the flagship shape), so the misfire cost dwarfs the tier cost; flagship is gated on it and the night has hours left").
fork('2026-07-29', flagship_pick, callgraph_rail_first_then_flow_interproc, flow_interproc_directly,
     "both need the same new rig (v5 rel output captured on a pinned corpus, v6 graded against it); a callgraph rail is the smaller program so the rig gets proven on less surface, and flow-interproc then rides the SAME rig -- best of both: the pick is not either-or, it is an ordering; backtrack = the rig outlives whichever program disappoints").

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
task(surface_dcg,         done,    [desugar_machinery, tsv2_pipeline]). % LANDED 2026-07-28 (merge 10053236): parse_dl.pl DCG + print_dl.pl + dl_view/ (109 fixtures as .dl text) + SYNTAX.md; round-trip 109/109, ghcacher.dl/conformance.dl parse with named gaps only. DCG is the CANONICAL parser (langium demoted). NOT yet wired into compile_fixture (term form still the compiler entry; wiring queued behind the latest/combine/zip spelling words). Hosts half of phase D still unbuilt.
task(mode_lab,            labbed,  []).                  % plans/2026-07-27-mode-lattice.md: lifetime = free distributive lattice over end-signals; scope_min=OR join_max=AND; 80/80 at 2fff3f61
task(sub_forest,          closed,  []).                  % RULED minimal kernel (rulings.pl subscription_kernel): zero stored rels; forest superseded by coverage check + ghost view. History: plans/2026-07-27-{sub-forest,switch-flow,redteam-minimal-kernel}.md
task(scope_cover_check,   unbuilt, [mode_lab]).          % obligation 1 of subscription_kernel ruling: scope-key column-flow check (mode-lattice machinery); refuses zombie-scope rules (redteam A2b)
task(ghost_forest_view,   unbuilt, []).                  % obligation 2: scope tree as derived diagnostic view, never stored
task(effect_abort,        unbuilt, []).                  % rulings.pl effect_abort: AbortSignal through HostDef.run + cancel map + pending-row delete; best-effort, warn-painted (js runtime arc)
task(register_lowering,   unbuilt, [kernel_sql_lowering]). % UPDATE..CASE per register + hist/retention
task(ts_grammar_import,   labbed,  []).                  % node-types.json -> con/enum facts; typed CST matching (astgrep lab, lab-consolidation PROVEN 5; quoted-DSL pipeline end to end)
task(purity_split,        unbuilt, [desugar_machinery]). % pure-body test per segment
task(island_partition,    unbuilt, [purity_split, clock_check]).
task(rw_sets,             unbuilt, [purity_split]).
task(pushdown_optimizer,  unbuilt, [island_partition, rw_sets]).
task(thread_schedule,     unbuilt, [rw_sets]).
task(emit_ts_direct,      done,    [kernel_sql_lowering]). % src/emit_ts.pl -> ast.ts helpers, swi-emit bench row (experiment; superseded by tsv2_pipeline)
task(tsv2_pipeline,       done,    [emit_ts_direct, desugar_machinery]). % compile/{compile,analyze,strat,lower,emit_ts}.pl: phases A+B reconciled 3/3 byte-identical to the oracle on the A runtime; stages doc = compile/PIPELINE.md
task(js_conformance_leg,  done,    [tsv2_pipeline]).       % LANDED as the phase C sweep: 109 fixtures graded by tick-log byte diff vs ticklog.pl oracle; compile/SCOREBOARD.md 9 identical / 8 wrong / 92 unsupported at sweep time; the marble-oracle idea made real
task(tsv2_typed_columns,  done,    [tsv2_pipeline]).       % LANDED 2026-07-28 (merge c32dba53): per-column int/text inference from literal witnesses, relplan/5; the 5 typing WRONGs flipped, wrong-diff bucket now 0
task(tsv2_unmarked_trigger, done,  [tsv2_pipeline]).       % LANDED same merge: any-body-atom occurrence lowering grounded in engine.pl trigger_items/occurrence_trigger; scoreboard 27 identical / 79 unsupported; three named refusals remain (edge_trigger_is_derived needs a tickLoop carry seam, edge_head_column_type_mismatch, edge_head_conflict_risk)
task(clock_bind,          done,    []).                    % LANDED 2026-07-28 (merge 378a39cf): BindDef/BindRunner input twin of HostDef in 1_binds.ts, clock_period rows -> interval per period, bucket = floor(epoch/period), teardown rides program-swap switchMap; dl 90/90, endurance PASS. Known limits in 1_binds.ts header: config read once at subscribe (mid-run clock_period row needs reload); no input-side dedupe cache (wall-clock cadence has no witness -- real asymmetry vs effect_cache)
task(sub_graph_disk,      unbuilt, [emit_ts_direct]).
task(count_ivm_port,      unbuilt, [kernel_sql_lowering]).
task(cost_model,          unbuilt, [pushdown_optimizer]). % perf rows feed plan choice
task(incremental_emitter, done,    [tsv2_pipeline]).       % LANDED 2026-07-28/29 (P1 delta joins default -> P2 frontier -> P3 refCount retraction w/ cycle-guard reseed): 1M competition won on DAG (429ms vs rust 443), CYC correct; naive referee mode retained
task(expression_lift,     done,    [tsv2_typed_columns]).  % LANDED 2026-07-29 (opus arc): comparisons/arith/binds/concat fused to SQL; aggregates count/sum accumulators + min/max group-scoped recompute; final-state grading leg; 3 miscompile classes caught incl cross-type join
task(hosts_wiring_p1,     done,    [tsv2_pipeline]).       % LANDED 2026-07-29 (merge 60631393): sh_decl/probe/bind_decl/query/ts_query end to end, schedule-fed; ghcacher.dl6 G2 gaps 8 -> 0; execution = named phase-2 refusals
task(edge_carry_seam,     done,    [tsv2_unmarked_trigger]). % LANDED 2026-07-29 (merge 78919aea): edge_trigger_is_derived refusal removed, derived edges read P1 frontier; enum state machine compiles; door receipt byte-identical
task(match_block,         done,    [tsv2_pipeline]).       % LANDED 2026-07-29 (merge 05f8ad29): match/2 sugar via shared expand_match_program; keyed_level_head refusal; keep(count) lowered as DELETE..RETURNING
task(latest_edge_sample,  done,    [edge_carry_seam]).     % LANDED 2026-07-29 (merge 066bf3c3): latest(Atom) = base-table sample in edge bodies (the N->(0|1) coercion, TICK-MODEL.md s4); backlog-replay inversion dead
task(runtime_bridge_p1,   done,    [incremental_emitter, hosts_wiring_p1]). % LANDED 2026-07-29 (merge 22607c08): PATH A wrap, v6/tsv2/serve/ 7 files; graded engine served over HTTP byte-identical; live sh + interval binds; serve-endurance 4 generations; serve-leak 20 swaps
task(tick_model,          done,    [tsv2_pipeline]).       % compile/TICK-MODEL.md: B/N/Z semirings + tick grading; 6 cross-plane refusals are its hand-proven theorems; clock_check implements it
task(clock_check,         unbuilt, [tick_model]).          % phase-5 checker: registry ring/grade columns, N-B junction coercion refusals, derivable tick-offset tables
task(extraction_live_p2,  done, [runtime_bridge_p1]).      % LANDED 2026-07-29 night (opus worktree, 6 commits, coordinator re-ran everything in worktree AND merged main): watch bind on node fs.watch behind IWatchSource seam (zero deps; events collapse to watch(glob,path,digest) rows w/ arrival SIGN -- rename = -old/+new same batch, atomic save = digest change, identical bytes = zero delta; bufferTime(100ms) never debounce); enumerate/enumerate_at = git ls-files pathspec (tracked-only, node_modules never walked; ls-tree takes NO glob, oids via rev-parse); extraction host = generic sh host, demand (path,digest) content-addressed, in-tree RELEASE extract bin (cargo --features cli REQUIRED). LIVE DEFECT FIXED: __host_response_* keyed on witness digest alone lost all but last row of multi-row answers -- ordinal:int column, key (witness,ordinal), fail-first receipt, oracle+emitter same-arc. EXIT RECEIPT extraction-live.sh 8 phases HOLDS incl kill -9 exactly-once. STANDING: sg_pattern refusal untouched; queryPlans emitted-not-executed; watcher cannot retract paths it never emitted (restart+delete gap, fix = enumerate boot set into lastDigest, crosses A12 push/demand line); one-host-decl = one record shape (two shapes of same file spawn twice; fix named = (path,digest,family) content cache)
task(memory_soak,         done, [runtime_bridge_p1]).      % LANDED 2026-07-29 night (sonnet worktree, coordinator re-verified everything incl sabotage red): GET /stats (ServeStats: IServeStats; PRAGMA page_count/page_size/freelist_count + ONE grouped dbstat statement via json_each bind, forkJoin on the existing seam; dbstat PROVEN available on @libsql 0.17.4) + memory-soak.{sh,ts} (keyed-replace + log-keep(count) + derived edge churn, 2500 ticks; rss/heap/page-count/stmts-per-tick flat asserted on quarter means, sabotage keep_all goes RED exit 1). STEP-0 FINDING: rust has NO sqlite3_status wrappers -- v5's whole surface is db.rs rel_stats dbstat sums + health.rs PRAGMAs; tsv2 mirrors exactly that. justfile memory-soak recipe wired by coordinator, added to green-all. Banked finding: tests/serveHelpers startServed retains all events by design (fixture replay), a false-positive growth source at soak scale -- soak uses a private non-retaining subscribe (run-fixture precedent, outside the one-subscribe scan)
task(prolog_org_refactor, done, []).                       % LANDED 2026-07-29 night (opus worktree, 12 commits, ALL 10 review ranks, coordinator re-ran everything in worktree AND merged main): prolog-lint gate ratcheted baseline 1 (in `just green` now, coordinator wiring); emit_ts collision renamed; 0_body_walk.pl walk_body/3 (10 sites); 0_program_check.pl (6 mirrored checks one impl + BOTH engine-only holes closed compiler-side w/ fail-first receipts); 1_expansion.pl declared phase order + enum context (analyzer double-expansion gone, spread phases are placeholder rows); expression operator inventory; R7 14 dead exports removed of 44 classified; R8 private sites 10 -> 1; R4 oracle aggregate classification on registry axis (oracle stays wider). Battery: conformance 137/0, plunit 124/124, TEXT_DOOR 72/72/0, sweep 72/70/0-wrong both modes, green exit 0. DELIBERATE moves: compiled+TEXT_DOOR 73 -> 72 (log_without_retention emitted a module the oracle rejects -- checked-in gen module DELETED), conformance +aggregate_in_edge_head_rejected. Journal: plans/2026-07-29-prolog-org-refactor-journal.md
task(org_banked_findings, unbuilt, [prolog_org_refactor]). % 4 findings banked in the org journal, each PINNED BY A TEST so drift is loud: (1) trigger_items/body_atoms misclassify next/combine/comparisons/lifecycle wrappers as relation atoms; (2) goal_rel_refs reports next/1+combine/2 as positive refs; (3) finalize_in_level_rule diagnostic drift + both doors accept not(finalize(...)); (4) 3 private cross-module calls in sprefa-store/bench/v1-scale-gen.pl outside the lint gate's load set. Fix wave = one small lane, unowned
task(watcher_buy_research, done, []).                      % LANDED 2026-07-29 night (merge 8b0b49a8): plans/2026-07-29-watcher-buy-research.md. VERDICT @parcel/watcher first (native batch callback matches engine.submit(IArrivalBatch) one-tick commits, ignore-filter below JS, prebuilds all platforms, MIT, 31M weekly dl); node fs.watch = zero-dep fallback (IS chokidar v4/v5's mac/win backend); watchman = optional backend upgrade later, not the buy. Open residuals in doc: node ignore walk-vs-receive time on linux, watchman atomic-save, parcel symlink default. Pick gets its fork/5 row at phase-2 dispatch
task(save_session_pl,     unbuilt, []).                    % user idea 2026-07-29 night: session saves as consultable .pl facts alongside chat_log md so a new session can consult/1 prior state; spell at next save
task(edge_body_constructs, done, [latest_edge_sample]).    % LANDED 2026-07-29 night (opus worktree, 4 commits, coordinator re-verified; sweep 72/70 -> 82/80 identical, 0 wrong, TEXT_DOOR 82/82/0, plunit 134/134): negation/comparisons/binds guard seam + now/1 emitted tick counter + edge-head column typing from feeding bodies. Refusals removed: edge_body_needs_{negation,bind,comparison,now} + edge_head_column_type_mismatch; added: edge_body_with_negation (not/1 beyond one plain atom), edge_body_with_now, now_in_level_rule (compiler-only, oracle solves it), edge_body_joins_arrival_fed_level (SEE tick_phase_alignment). Fallout fixes: analyze seeded_refs Initial-only refs silently dropped final-state rows; print_dl type synthesis keyed off missing col_type (48 dl_view regen). Receipts in SCOREBOARD.md
task(tick_phase_alignment, done, [edge_body_constructs]).  % LANDED 2026-07-29 night (opus worktree, 2 commits, coordinator re-verified worktree AND merged main; sweep 82/80 -> 85/83/0-wrong both modes, TEXT_DOOR 85/85/0, plunit 137/137): (a) mid-tick level plane frozen where engine.pl freezes it -- recomputeLevelsBeforeEdges shares the emitter's 5 supportSql, naive recomputes before AND after edges; edge_body_joins_arrival_fed_level REMOVED, clock_rel_join_storms byte-identical (was 3-vs-1). THE FLAGSHIP SHAPE COMPILES. (b) separate __departure_frontier_<rel> TEMP table per listened rel (sign-column alternative rejected: touches every rel's DDL; receipt = all 83 prior modules byte-identical); departures in carryPending; finalize-in-edge flipped. HOLE FORCED SHUT en route: flipping the finalize registry row deleted the generic refused-goal catch, compiler ACCEPTED finalize-in-level -- finalize_in_level_rule restored in analyze shared_refusal order, drift became an agreement test
task(c7_durable_carry,     unbuilt, [tick_phase_alignment]). % match-frontier C7 INHERITED by the departure frontier (measured, departureFrontier.test.ts non-vacuous probe): frontier + departure TEMP tables die with the connection, kill -9 loses staged carries in BOTH implementations incl the oracle-side Ti carry. Endurance-law violation, whole-carry-set durability = own arc
task(gen_staleness_gate,   unbuilt, []).                    % checked-in gen modules can silently predate their emitter (door-handwritten went stale TWICE: latest-in-level era + periods->literals rename; coordinator regen'd both times). Gate = green re-emits every checked-in non-fixture gen module and diffs, or door-handwritten becomes a fixture. SAME CLASS, binary flavor (lsp arc close-out): receipt scripts build target/release/{dl,extract} only if MISSING, so a stale existing binary silently runs -- main's Jul-20 dl broke the lsp receipt until coordinator rebuilt; gate should also compare binary mtime vs source
task(flagship_callgraph,   done, [tick_phase_alignment, extraction_live_p2]). % LANDED 2026-07-29 night (opus worktree, 2 commits, coordinator re-ran the rig + full battery): examples/callgraph-ast.dl BYTE-UNMODIFIED vs its v6 port over a pinned 13-file rust corpus; flagship-callgraph.sh + flagship-classify.py; every diff row bucketed, 0 expression gaps, 0 defects (def +10 = function_signature_item, call +181 = method/path/struct-literal sites v5's bare-identifier ast query cannot match; calls/unused proven by RULE FIDELITY -- v5's rule bodies run against each engine's own inputs); unused inverts as the anti-monotone rel must. 2 fixtures promoted (conformance 139/0, sweep 87/85/0, TEXT_DOOR 87/87/0). In green-all as `just flagship`
task(flow_interproc_port,  blocked, [flagship_callgraph]).  % BLOCKED ON EXTRACTOR SCOPE, not on the engine: rides df_node/call_edge/type_sig = SCIP-RESOLVED builtins; sprefa-extract's CLI is phase-1 only (Resolve<CallF> never runs on that path) so no resolved edges exist to feed the port; ALSO needs closure spelling (graph-algo queue). Extractor is user-FIXED -- unblocking = user call on exposing the resolve pass, or the rail regrades on ast-level edges
task(extra_drain_tick,     unbuilt, [tick_phase_alignment]). % UNOWNED DEFECT found by the flagship fixture: schedule ending on a retraction tick that RE-ASSERTS a level row mints one empty {"deltas":{}} tick the oracle lacks (reseed re-INSERT stages a next-frontier row -> carryPending -> one drain). Needs non-monotone level rule to observe. Repro = truncate callgraph_unused fixture schedule to 4 ticks. Owner = 1_incremental.ts
task(probe_output_guard,   unbuilt, []).                    % flagship gap 1: comparison guard over a probe OUTPUT var in a level rule refused as unsupported_construct(unbound_head_var(_G)) -- wrong name, no location (review-B4 in the wild; lower.pl:383 compile_guard_goals bound-set lacks response atom output columns). Shipped workaround = route through own rel. Fix = widen bound set + name the real reason + location
task(cli_bop,              done, []).                       % LANDED 2026-07-29 night (sonnet worktree; agent left tree UNCOMMITTED-but-clean, coordinator reviewed file-by-file, ran every receipt itself, committed on the branch -- process deviation logged): registry cli_command/3 inventory -> commander bop.ts, verbs serve/run/check/load/q, run+check boot serveTsv2 IN-PROCESS (no daemon), exit contract 0 clean/2 named-refusal/1 broken verified by coordinator runs (ghcacher = 2 w/ recursive_stratum receipt). 12 tests + inventory-parity (swipl vs commander lines); one dep commander (user-required); one-subscribe 1/1 both apps (cli/ = run-fixture exemption). halt-inside-catch swipl trap documented in bop_check.pl. THE 6.2.0 TAG GATE IS SATISFIED; push = user. LSP milestone still open on task 11
task(golden_flake_hunt,    unbuilt, []).                    % store tests/engine/golden.test.ts failed 2x tonight under just-green parallel load, 74/74 every isolated rerun; first filed on the one-subscribe arc. Deserves a real RCA lane (timing? port? tmp collision?) instead of rerun-and-shrug
task(lsp_diags,            done, [extraction_live_p2, cli_bop]). % LANDED 2026-07-29 night (sonnet worktree after 2 coordinator continue-nudges, committed 77b5bbce, coordinator re-ran receipt: LSP DIAGS HOLDS): ZERO new LSP code -- diag-rail.dl6 declares rel diag_v5 in v5's exact 9-column shape and tsv2's bare-name tables (lower.pl table_name) make it THE table src/lsp.rs:545 selects; bridge fully in-language. Real v5 dl --lsp --diag-db over real stdio JSON-RPC: publishDiagnostics appear + retract for no-eval + unused-def rails over the live watcher/extraction feed. Line numbers HONESTLY 0 until decode_arc lands spans. Sabotage: column rename passed engine-side (positional curl) and went red at the real LSP client. In green-all as `just lsp-diags`
task(emitter_groupby_literal, unbuilt, []).                 % REAL EMITTER DEFECT (lsp arc): a rule head with >=2 bare integer-literal columns reaches the support-count GROUP BY verbatim and SQLite reads a bare integer there as a POSITIONAL column ref -> SQLITE_ERROR 2nd GROUP BY term out of range. In-language workaround 0+0 in diag-rail.dl6. Fix = emitter wraps literal head columns (or excludes constants from GROUP BY). Owner emit_ts.pl, fail-first fixture wanted
task(v5_lsp_exit_hang,     unbuilt, []).                    % v5 DEFECT disclosed by the lsp receipt, reproduced standalone: dl --lsp --diag-db answers shutdown correctly then hangs after exit + stdin EOF; receipt SIGKILLs after both directions proven. Owner src/lsp.rs, v5 side
task(pre_occurrence_loop,  unbuilt, [edge_body_constructs]). % pre-in-edge (13 fixtures) needs an ORDERED OCCURRENCE LOOP with writes applied between occurrences (engine.pl process_occurrences chaining; cross-arm in arrival order, so no per-arm CTE reaches it) -- a new execution shape in the emitted runtime, NOT a sampled read (dispatch premise measured wrong: pre-as-sampled projects [1,1] where oracle pins 2, receipt in SCOREBOARD.md). Own arc, unowned
task(decode_arc,           unbuilt, []).                   % compound-arrival-vs-json1 encoding split blocks json destructure in edge (9) AND level (6) bodies: arrived compounds stored as canonical term text while compiled destructure reads the json1 tagged form. The encoding DECISION comes first (cross-target log contract implications), then decode/2 lowering. Coordinator/user-level call, then dispatchable

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
check(construct_status_closed, ( forall(construct(_, _, Status),
                                        construct_status(Status)) )).
check(construct_tier_known,    ( forall(construct(_, Tier, _),
                                        construct_tier(Tier)) )).
check(covers_endpoints_ground, ( forall(covers(Subject, Name),
                                        ( construct(Name, _, _),
                                          covers_endpoint_exists(Subject) )) )).

reaches_ast(ast).
reaches_ast(G) :- refines(G, Parent), reaches_ast(Parent).

go :- forall(check(N, G),
             ( catch(G, E, (print_message(error, E), fail))
             -> format("PASS  ~w~n", [N]) ; format("fail  ~w~n", [N]) )).
