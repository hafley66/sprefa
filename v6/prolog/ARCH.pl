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
% * ONE DL SURFACE, ONE PARSER. compile/parse_dl_dcg.pl is the text door and
%   the only parser in the repo. The classic hand-threaded parse_dl.pl and the
%   langium grammar (with the whole v6/dl app that was its only consumer) are
%   deleted. The verdict on any construct is compile/out/manifest.json (bucket
%   + unsupported construct reason per fixture), never a comment header.
%
% * JSON IS TWO PLANES, ONLY ONE CLOSED. READING compiles: spread/1 array
%   explode, `$name` key holes, `**` uncapped descent, typed captures
%   ({k: V: int}), nested objects, the json column type. WRITING is partly
%   refused: json_group_array/1..2 and struct columns render, while a brace
%   literal in value or head position raises json_value_expression, and
%   json_array / json_object heads raise aggregate_head. Consequence:
%   scan-into-json has no spelling, because `:=` computes arithmetic, concat
%   and seq only.
%
% * AGGREGATES ARE DELTA-LOCAL. A head aggregate groups over ONE body atom;
%   taking the group key from a second atom raises
%   aggregate_group_not_delta_local. Materialize the join into its own rel
%   first, then aggregate over that rel.
%
% * NO STRING SPLIT. concat/1 and regexp/2 are the whole string surface; there
%   is no split or substr, so path-prefix work (ancestor directories) has no
%   in-language spelling and arrives as facts or from a host.
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
%   carries its refCount; injected and derived rows may share a head, and
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
% * NO UNSIGHTED SYNTAX (user-named 2026-07-29 evening, standing). Verbatim
%   from the fable close-out handoff block, folded here by the world-health
%   audit so it is read with the other callouts rather than at the file's end:
%
%     PROCESS FAILURE, user-named, recorded: the language grew unsighted
%     syntax (`:=`/`==` split, the `type` keyword spelling, the naked span
%     pair) through agent arcs the coordinator toured as settled. Standing
%     consequence for every future lane: NO new surface spelling lands
%     without a user decision card presented BEFORE merge.
%
%   The three named warts are all live in this file: bind_goal's status was
%   corrected to `new` in the construct table for exactly this reason, `type`
%   is construct struct_type_decl (t5), and the naked span pair is what
%   task file_span_redesign replaces.
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
algorithm(tsv2_ts_emit,    ast,         rewrite,           'compile/emit_ts.pl (backend #1 over lowered/8; the rust consumer v6/dd-runner reads the dd_plan JSON twin from compile/6_isolated_compiler_dd.pl)').
algorithm(tsv2_surface_dcg, ast,        rewrite,           'compile/parse_dl_dcg.pl + print_dl.pl (phase D, LANDED: the DCG is the ONLY parser since the classic parse_dl.pl deletion; compile_dl6/2 is the text door)').

% Added by the world-health audit 2026-07-29: five modules that carry real
% analyses and had no row, three of them born in the org refactor (rank
% numbers = plans/2026-07-29-prolog-org-review.md).
algorithm(tsv2_expansion,   ast,         rewrite,           '1_expansion.pl declared phase order enum -> decl spread -> row spread -> match, shared by oracle + compiler').
algorithm(tsv2_host_expand, ast,         rewrite,           '1_host_expand.pl: sh_decl/probe/bind_decl/query/ts_query -> demand rules + keyed response rels').
algorithm(tsv2_type_plane,  ast,         fold,              '0_type_plane.pl: struct decls, topological type order, canonicalize_world_rows/3, cycle witness').
algorithm(tsv2_program_check, ast,       fold,              '0_program_check.pl: the six cross-plane invalid-program triggers, one implementation, two doors (rank 2)').
algorithm(tsv2_body_walk,   ast,         fold,              '0_body_walk.pl walk_body/3: the one body traversal 10 sites used to hand-roll (rank 1)').
algorithm(refcount_retraction, delta_flow, monotone_fixpoint, 'emitted SQL (P3): plain refCount where the rule graph is acyclic, recursive-CTE reseed where cycles are reachable; the guard is per rule graph').

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

% RECEIPT STATE of the inventory above, audit 2026-07-29. The V6Home column is
% still the aspirational tier name; these are the receipts that now exist, so
% the row is not read as all-aspiration:
%   lsp                -- GRADED. diag-rail.dl6 declares diag_v5 in v5's 9-column
%                         shape; v5's own `dl --lsp --diag-db` reads it over real
%                         stdio (`just lsp-diags`). Line numbers were 0 until the
%                         comment lab's byte-span flattener; still unwired.
%   comment_management -- GRADED BYTE-EXACT. comment_node 745/745, arch_node 4/4
%                         vs v5 (plans/2026-07-29-comment-node-verdict.md), zero
%                         new constructs. Markdown is the one extractor hole.
%   flow_analysis      -- PARTIAL. flagship-flow.dl6 grades four queries against
%                         v5: flow_edge 2184/2184 v6 rows matched (278 v5-only),
%                         flow_reach 9112 matched, flow_param_type 0 matched
%                         (referee key gap), flow_node_type EMPTY v6-side.
%   language_parsing   -- PARTIAL. cst/type/call/df families over ts/rust/go/
%                         kotlin/prolog; html/xml/md/json/yaml/toml planned only
%                         (plans/2026-07-29-extract-doc-formats-header.md).
%   doc_management, type_measurement, cross_repo_pointing, refactoring,
%   auto_docing, codegen_typegen -- NO v6 receipt yet.

% ── tech roles: who is allowed to do what ───────────────────────────────────

tech(prolog, compiler_tier, [parse_via_ops, desugar, check, weave, emit],
     'never runs fixpoints at scale; bundled with the eventual rust binary. THE parser (user 2026-07-28: langium/0_generated in v6/dl was a stopgap superseded by the phase D DCG; user 2026-08-12: v6/dl and its langium grammar deleted outright)').
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
% instead of silently picking one.
%
% POST-BUDGET TIER (t5, added 2026-07-29 by the world-health audit). The
% T0-T4 rows above are the FROZEN LANG.md-era budget and their accounting is
% not reopened. Everything the 2026-07-28/29 rulings ADDED to the surface
% lives in the t5 block at the end of this section, so the table stops
% understating the language: compile/registry.pl carries 41 surface/5 rows
% today against this file's 29, and the difference was invisible. t5 rows
% are exactly the constructs that (a) have a live registry row and (b) were
% ruled after the budget was written. Constructs that exist only as design
% (rel spreading, priced in plans/2026-07-29-rel-spreading-verdict.md, NOT
% wired) get no row until they are wired.
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
construct(bind_goal,            t0, new).     % STATUS CORRECTED 2026-07-29 (was
%   `kept`, which claims LANG.md carried the spelling unchanged). LANG.md's
%   keyword list is enum/struct/rel/bind and names no bind-goal operator; `:=`
%   first appears at f26fc6ef (2026-07-27, the reference-interpreter commit)
%   and is a live registry row today (registry.pl:72, beside is/2). The
%   spelling is itself under an open question: SLOT-BIND-SPELLING
%   (plans/2026-07-29-simplify-wave-brief.md tail) records that `:=` is not an
%   rxjs, prolog or SQL word (the vocabulary law), that prolog's own
%   candidates are `is` and `=`, and that `Var = expr` currently dies as
%   unbound_head_var with no mention of `=` at all.
construct(coalesce,             t0, new).     % WIRED 2026-07-30 (ruling
%   null_design = get_else_use_site_never_storage). The use-site total read:
%   `coalesce(rel_atom(Bound..., Out), Default)` binds Out from the row when
%   one exists and from Default when none does, so the tuple survives instead
%   of dropping out of the join. Null never enters storage or the type system.
%   Compiler LEVEL lowering emits one LEFT JOIN + SQL COALESCE per item; EDGE
%   rewrites one rule into two ordinary clauses -- the read, and `not(...)`
%   plus a `:=` of the default -- so the emitter gained nothing. The name is
%   the SQL word, per the vocabulary law; Datomic's `get-else` is the prior
%   art (plans/2026-07-30-option-versus-null-lab.md section 5, candidate D).
construct(fn_application,       t0, kept).
construct(interpolation,        t0, kept).
construct(named_column_atoms,   t0, kept).
construct(wildcard,             t0, kept).
construct(snapshot_ask,         t0, kept).

construct(from_world_modifier,  t1, new).    % unbundled from the killed `source` keyword
construct(bind_decl,            t1, killed).  % 2026-08-21 sh_bind_surface_removed: bind rows are plain arrival rels
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

% ── t5: what the 2026-07-28/29 rulings added to the surface ──────────────────
% Each row has a live compile/registry.pl surface/5 entry AND a ruling; each
% is grounded below by a fixture FILE that exercises it (counts are grep of
% the functor in that file, taken 2026-07-29).

construct(struct_type_decl,     t5, new).   % ruling compound_storage; `type name(col: type)`.
%   The `type` KEYWORD SPELLING is one of the three warts the no-unsighted-
%   syntax callout names (93fd9ea6 files the user objection: `type` and `rel`
%   decls are indistinguishable on sight), and its first value -- the naked
%   span(start,end) pair -- is what task file_span_redesign replaces. The
%   construct is real and graded; its spelling is open.
construct(decode_join,          t5, new).   % decode/2 = dictionary join, same ruling
construct(match_block,          t5, new).   % ruling match_block_word; arms expand to rules
construct(host_decl,            t5, respecified).   % sh_decl/4 is the TERM; the `sh` keyword died 2026-08-21
construct(arrival_rel_decl,     t5, new).   % `rel n(ins) -> (outs) key(P..)`, ruling arrival_arrow_spelling; desugars to sh_decl/4 + arrival_identity/2
construct(query_form,           t5, new).   % query/1 and query/2, `? rel(args)` with an optional `order by col [asc|desc], ...` tail that lowers onto final_select alone (top-level only; RHS probe + @ salt riders REMOVED by v6.2 host-surface locks)
construct(ts_query_value,       t5, new).   % ts_query/1 compiles to exact query text
construct(latest_sample,        t5, new).   % replacement spelling for the killed only()

construct_status(kept).
construct_status(killed).
construct_status(respecified).
construct_status(new).

construct_tier(t0).
construct_tier(t1).
construct_tier(t2).
construct_tier(t3).
construct_tier(t4).
construct_tier(t5).   % post-budget, see the section header

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

% ── t5 coverage (audit 2026-07-29) ───────────────────────────────────────────
% Fixture-file endpoints, each verified by grepping the functor in that file
% on this sha; ruling endpoints name the row in conformance/rulings.pl that
% decided the spelling.
covers('4_struct_values',         struct_type_decl).   % type_decl/2 x19
covers(compound_storage,          struct_type_decl).   % ruling: struct_as_rows
covers('4_struct_values',         decode_join).        % decode/2 x7 on declared types
covers(compound_storage,          decode_join).
covers('1_match_block',           match_block).        % match/2 x3
covers(match_block_word,          match_block).
covers('2_hosts_wiring',          host_decl).          % sh_decl/4 x4
covers(host_residency,            host_decl).
covers('2_hosts_wiring',          query_form).         % query/1; arrival rels feed the schedules
covers('2_hosts_wiring',          arrival_rel_decl).   % the batching/identity/stop fixtures
covers('2_hosts_wiring',          ts_query_value).     % ts_query/1 x1
covers(merge_family,              latest_sample).      % latest/1 x18
covers(engine_core,               latest_sample).      % latest/1 x5

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
fork('2026-07-29', pre_lowering_premise, unsupported_with_measured_receipt, force_sampled_read_lowering,
     "coordinator's dispatch premise (pre = sampled read like latest) was WRONG -- agent measured pre-as-sampled projecting [1,1] where the oracle pins 2; pre is a chained mid-tick read through ordered occurrences; the honest unsupported construct + receipt beats a silently-wrong lowering, and pre_occurrence_loop is now a priced arc instead of a hidden defect").
fork('2026-07-29', tick_alignment_tier, opus_worktree_fourth_lane, sonnet_or_wait_for_user,
     "runtime tick phase order vs the oracle freeze IS the semantics of the engine; a wrong ordering grades wrong only on join-storm shapes (exactly the flagship shape), so the misfire cost dwarfs the tier cost; flagship is gated on it and the night has hours left").
fork('2026-07-29', rig_coordinate_translation, referee_translates_coordinates, change_one_engines_output,
     "v5 prints path:line:col:kind through _txt views, v6 carries byte spans; the flow rig needed one key space. Translating INSIDE the classifier (pinned-corpus newline index, calibration receipt in flagship-flow-classify.py's header) keeps BOTH engines unbent -- a v6 line/col column would have been a program change made to satisfy a referee, and a v5 span column is not ours to add. Backtrack = the translation is 40 lines in one python file").
fork('2026-07-29', sweep_gc_false, disable_gc_for_the_one_shot_batch, restructure_the_sweep_into_chunks,
     "swipl 10.0.2 aborts the compile sweep with 'system error: Mismatch in up phase' (GC compaction), deterministically at the 88th collection once the corpus reached 163 fixtures; the same goal typed at the toplevel completes. gc(false) on a one-shot process whose peak RSS is tens of MB costs nothing and keeps the sweep shape; chunking would have changed the receipt's meaning to chase someone else's bug. Receipt in sweep.sh:26. Upstream-shaped, see task swipl_gc_abort").
fork('2026-07-29', bind_spelling_filed, file_the_slot_and_keep_running, rename_mid_rig,
     "`:=` is nobody's word (vocabulary law) and terra typed `=` first, which dies as unbound_head_var; both are real. Renaming mid-flow-rig would have churned registry + parse/print + engine op + 16 fixture files inside a lane graded on row counts. Filed as SLOT-BIND-SPELLING with the `=`-must-refuse-by-name half attached; backtrack = the rename stays mechanical").
fork('2026-07-31', repo_extraction_executor, distinct_sprefa_extract_repo_executor, fall_through_to_the_generic_shell_executor,
     "the brief's second fork, resolved the same way ruling A resolved the first. A repo-scoped extraction host's inputs are three columns wide, which host_executor_contract(sprefa_extract, ...) refuses by exact positional list (measured: host_executor_mismatch). Widening that row was ruled out by the brief; the remaining two were a DISTINCT executor name with its own exact contract, or letting the declaration fall to `shell`. Falling through needs no row at \c
     all -- writing the template as '{repo}/{path}' already fails the match -- and costs the applicative fold (N named projections over one file become N subprocesses) while making a QUOTING CHARACTER decide which executor runs, which is the silence class this repo files as a defect. The distinct row is six lines of prolog and three of TS, and the fold is real value on a crawl. Backtrack = deleting the row falls straight through to shell with no other edit").
fork('2026-07-31', crawl_bench_extraction_shape, keep_one_row_per_file_as_before, capture_the_extractor_jsonl,
     "the rewritten v6 leg's first draft let repo_extract answer the extractor's whole cst/type/call/df JSONL as EDB arrivals, which is the more natural program. MEASURED: same 779-file corpus 20.26s -> 62.97s, scratch db 1.0MB -> 595MB. That is a real number about the extraction seam and it is a DIFFERENT question from the one \c
     this bench asks -- the before/after here isolates the repository loop, so the extraction leg has to stay byte-for-byte the work it was. Kept `>/dev/null && printf`, which is what forced the executor template match from ends-with to contains. Backtrack = the JSONL shape is one line of program text and its number is recorded here").
fork('2026-07-29', flagship_pick, callgraph_rail_first_then_flow_interproc, flow_interproc_directly,
     "both need the same new rig (v5 rel output captured on a pinned corpus, v6 graded against it); a callgraph rail is the smaller program so the rig gets proven on less surface, and flow-interproc then rides the SAME rig -- best of both: the pick is not either-or, it is an ordering; backtrack = the rig outlives whichever program disappoints").

% ═════════════════════════════════════════════════════════════════════════════
% BATTERY OF RECORD — measured by the world-health audit at b535ca62
% (2026-07-29 midday), each number from that agent's OWN run, not a report.
% Full assessment: plans/2026-07-29-v6-world-health.md.
%
%   conformance          163 PASS / 0 findings   (swipl conformance/go.pl -g go)
%   plunit               140 / 140
%   TEXT_DOOR            compiled=102 byte_identical=102 failures=0
%   sweep (artifacts)    163 swept: 102 compiled / 61 named unsupported constructs;
%                        of the 102: 100 IDENTICAL, 0 WRONG, 2 run_error
%                        (one is a rejection fixture, one is a real defect --
%                        see task fork_join_malformed_json)
%   emitted modules      103 in tsv2/gen_emitted, 161 .dl6 views
%
% NOT re-run here (this audit worktree has no node_modules): tsv2 / dl / store
% suites and every shell receipt. Last committed receipt for those is the
% host-seam landing (42d11f47): tsv2 69 pass / 1 skip, extraction-live HOLDS,
% leak-soak PASS.
%
% STALE ELSEWHERE, verified at this sha and NOT edited here (v6/justfile is
% another lane's file): the justfile expect comments read conformance 156,
% text-door 95/95, sweep 95/93, plunit 137/137, tsv2 65/1, dl 97/1, and the
% corpus line says "135-fixture baseline". SCOREBOARD.md totals read 155/94/
% 92/0/2. Both are one to three landings behind the numbers above.
% ═════════════════════════════════════════════════════════════════════════════

% ═════════════════════════════════════════════════════════════════════════════
% BUILD ORDER — task(Name, Status, Needs). `roadmap` topsorts it; learning
% order = the same sort, because a technique is learnable once its inputs are.
% ═════════════════════════════════════════════════════════════════════════════

task(kernel_sql_lowering, done,    []).                  % lower_sql + dl_in_prolog, measured
task(desugar_machinery,   done,    []).                  % op/3 + term_expansion (rel_island)
% duplicate clock_check row (labbed, [desugar_machinery]) DELETED 2026-07-30: the self-map rail's task_state_conflict caught two live states; the :active row below (user-ruled 2026-07-30) survives.
task(init_retention,      done,    []).                  % landed: missing_retention analyze.pl:1305, emit_ts.pl:1193-1203; 4 compiled + 3 guard fixtures (manifest 209-223); status audited 2026-08-11
task(causality_check,     labbed,  []).
task(envelope_types,      done,    []).                  % enum_match landed: 0_match_expand.pl:120 match_nonexhaustive + plunit_tests.pl:2432; enum planes compiled (manifest 2-6, 32-37); status audited 2026-08-11
task(demand_clocking,     labbed,  [kernel_sql_lowering]).
task(clock_inference,     parked,  [clock_check]).       % swap ground clocks for holes; user-parked 2026-07-27
task(surface_dcg,         done,    [desugar_machinery, tsv2_pipeline]). % LANDED 2026-07-28 (10053236): parse_dl.pl DCG + print_dl.pl + dl_view/ (109 fixtures as .dl text) + SYNTAX.md.
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
task(tsv2_unmarked_trigger, done,  [tsv2_pipeline]).       % LANDED 2026-08-08 (55d453b5): any-body-atom occurrence lowering grounded in engine.pl trigger_items/occurrence_trigger.
task(clock_bind,          done,    []).                    % LANDED 2026-07-28 (378a39cf): BindDef/BindRunner input twin of HostDef in 1_binds.ts, clock_period rows -> interval per period, bucket = floor(epoch/period), teardown rides program-swap switchMap.
task(sub_graph_disk,      unbuilt, [emit_ts_direct]).
task(count_ivm_port,      unbuilt, [kernel_sql_lowering]).
task(cost_model,          unbuilt, [pushdown_optimizer]). % perf rows feed plan choice
task(incremental_emitter, done,    [tsv2_pipeline]).       % LANDED 2026-07-28/29 (P1 delta joins default -> P2 frontier -> P3 refCount retraction w/ cycle-guard reseed): 1M competition won on DAG (429ms vs rust 443), CYC correct; naive referee mode retained
task(expression_lift,     done,    [tsv2_typed_columns]).  % LANDED 2026-07-29 (opus arc): comparisons/arith/binds/concat fused to SQL; aggregates count/sum accumulators + min/max group-scoped recompute; final-state grading leg; 3 miscompile classes caught incl cross-type join
task(hosts_wiring_p1,     done,    [tsv2_pipeline]).       % LANDED 2026-07-29 (merge 60631393): sh_decl/probe/bind_decl/query/ts_query end to end, schedule-fed; ghcacher.dl6 G2 gaps 8 -> 0; execution = named phase-2 unsupported constructs
task(edge_carry_seam,     done,    [tsv2_unmarked_trigger]). % LANDED 2026-07-29 (merge 78919aea): edge_trigger_is_derived unsupported construct removed, derived edges read P1 frontier; enum state machine compiles; door receipt byte-identical
task(match_block,         done,    [tsv2_pipeline]).       % LANDED 2026-07-29 (merge 05f8ad29): match/2 sugar via shared expand_match_program; keyed_level_head unsupported construct; keep(count) lowered as DELETE..RETURNING
task(latest_edge_sample,  done,    [edge_carry_seam]).     % LANDED 2026-07-29 (merge 066bf3c3): latest(Atom) = base-table sample in edge bodies (the N->(0|1) coercion, TICK-MODEL.md s4); backlog-replay inversion dead
task(runtime_bridge_p1,   done,    [incremental_emitter, hosts_wiring_p1]). % LANDED 2026-07-29 (merge 22607c08): PATH A wrap, v6/tsv2/serve/ 7 files; graded engine served over HTTP byte-identical; live sh + interval binds; serve-endurance 4 generations; serve-leak 20 swaps
task(tick_model,          done,    [tsv2_pipeline]).       % compile/TICK-MODEL.md: B/N/Z semirings + tick grading; 6 cross-plane unsupported constructs are its hand-proven theorems; clock_check implements it
task(clock_check,         active, [tick_model]).          % USER RULED 2026-07-30: label ring/sign/grade dependencies, then infer clocks. Phase-5 value rulings: bool is INTEGER NOT NULL CHECK(value IN (0,1)), canonical true/false boundary; float is finite SQLite REAL/binary64 with exact comparison/join and shortest round-trip output, no epsilon. Implement historical bug-class replay gate before declaring checker complete.
task(phase5_ingest_binary, done, [v6_2_ts_closeout]). % LANDED 2026-07-30 (805d077d): LANDED 2026-07-30. v6/dl ingest resolves executable DL_EXTRACT_BIN override, then in-tree release extract, then one shared cargo release build.
task(extraction_live_p2,  done, [runtime_bridge_p1]).      % LANDED 2026-07-29 (ea938d6c): watch bind on node fs.watch behind IWatchSource seam.
task(memory_soak,         done, [runtime_bridge_p1]).      % LANDED 2026-07-29 (f99ec022): GET /stats (ServeStats: IServeStats.
task(prolog_org_refactor, done, []).                       % LANDED 2026-07-29 (04daaa0a): prolog-lint gate ratcheted baseline 1 (in `just green` now, coordinator wiring).
task(org_banked_findings, done, [prolog_org_refactor]). % LANDED 2026-07-29 (01ac896e): 4 findings banked in the org journal, each PINNED BY A TEST so drift is loud: (1) trigger_items/body_atoms misclassify next/combine/comparisons/lifecycle wrappers as relation atoms.
task(watcher_buy_research, done, []).                      % LANDED 2026-07-29 (8b0b49a8): plans/2026-07-29-watcher-buy-research.md.
task(save_session_pl,     done, []).                       % LANDED 2026-07-29 (c8eca31e): chat_log/20260729.0.*.pl = module session_20260729_0 with session/2, in_flight/4, landed/2, session_task/2, awaiting_user/2, ruled_this_session/2.
task(edge_body_constructs, done, [latest_edge_sample]).    % LANDED 2026-07-29 (fce2658b): negation/comparisons/binds guard seam + now/1 emitted tick counter + edge-head column typing from feeding bodies.
task(tick_phase_alignment, done, [edge_body_constructs]).  % LANDED 2026-07-29 (55d453b5): (a) mid-tick level plane frozen where engine.pl freezes it -- recomputeLevelsBeforeEdges shares the emitter's 5 supportSql, naive recomputes before AND after edges.
task(c7_durable_carry,     unbuilt, [tick_phase_alignment]). % match-frontier C7 INHERITED by the departure frontier (measured, departureFrontier.test.ts non-vacuous probe): frontier + departure TEMP tables die with the connection, kill -9 loses staged carries in BOTH implementations incl the oracle-side Ti carry. Endurance-law violation, whole-carry-set durability = own arc
task(mutual_recursion_in_tick, done, [divergence_backstop]). % LANDED 2026-08-15 (67d70ba0): strat.pl:cyclic_head_groups/2 pairs every head on a positive INDIRECT stratum cycle with its group index (self edge still dropped, so the expand wavefront keeps direct self-recursion).
task(divergence_backstop,  done, []). % LANDED 2026-08-15 (PR #263): fixpoint_round_cap/1=1000 at lower.pl:4543 carried in the expand plan.
task(gen_staleness_gate,   done, []).                     % LANDED 2026-07-29 (c260507c): v6/tools/staleness-gate.sh in green-all -- gen half enumerates gen_emitted modules absent from manifest compiled set, regenerates via compile_dl6.sh + diffs.
task(flagship_callgraph,   done, [tick_phase_alignment, extraction_live_p2]). % LANDED 2026-07-29 (0c922d65): examples/callgraph-ast.dl BYTE-UNMODIFIED vs its v6 port over a pinned 13-file rust corpus.
task(flow_interproc_port,  done, [extract_resolve_flag]). % LANDED 2026-07-29 (d322c93f): portable callable-plane port merged d322c93f (terra) with every gap named, value-plane rewrite merged 837fe7f2, rig grading ed81cdc6.
task(extract_resolve_flag, done, [flagship_callgraph]). % LANDED 2026-07-29 (17778bbb): LANDED 2026-07-29 morning (merge 17778bbb + 0_prolog ledger refresh c26b4e0e, codex terra, worktree+branch removed).
task(extra_drain_tick,     done, [tick_phase_alignment]).   % LANDED 2026-07-29 (540e876c): recomputeLevelsAfterEdges refCount branch staged reconcile re-INSERTs into nextFrontier phase 1 (the P3 shape its own aggregate-branch comment had already called out as the unfixed asymmetry).
task(cq_bundle_lane,       done, []).              % LANDED 2026-07-29 (733d4c1b): sweep both modes 98/96/0-wrong, TEXT_DOOR 98/98/0, roundtrip, staleness gate, LSP DIAGS HOLDS w/ 0+0 workaround REMOVED): groupby literals emit.
task(crawl_bench,          done, []).                   % LANDED 2026-07-29 (a192cd35): LANDED 2026-07-29 (merge a192cd35, codex luna, review-gated.
task(flow_parity_upgrade,  done, [struct_host_output_seam]). % LANDED 2026-07-29 (837fe7f2): df hosts + arg->param positional hop + sig-owner joins + 2 fixtures (5_flow_value_plane.pl).
task(struct_dictionary_gc, unbuilt, [struct_as_rows]).     % SLOT-GC-TIMING named debt: dictionaries are MONOTONE (unobservable in tick log -- collected and uncollected print identical bytes; edge-2 receipt). Zero cost for churning programs (content-addressed reuse); real growth only for ever-new distinct values (one row ~= canonical JSON twice + flat columns). Collecting = refcount across arrivals/level heads/edge heads/frontier/retention (arrival-only count deletes rows a derived row still renders). Own arc
task(struct_host_output_seam, done, [struct_as_rows, cq_bundle_lane]).  % LANDED 2026-07-29 (265da55f): decl-B host OUTPUT columns admit declared type names via host_output_columns (WRAPPER spellings keep the column_type_wrapper unsupported construct, unknown names stop as column_type_unknown).
task(probe_output_guard,   done, []).                    % LANDED 2026-07-29 (733d4c1b): the diagnosis said "widen the bound set", the root cause was GOAL PLACEMENT in host expansion (pre-probe goals belong in the demand rule, post-probe guards after the response).
task(cli_bop,              done, []).                       % LANDED 2026-07-29 (55d453b5): registry cli_command/3 inventory -> commander bop.ts, verbs serve/run/check/load/q, run+check boot serveTsv2 IN-PROCESS (no daemon).
task(prolog_folder_flatten, done, []). % LANDED 2026-07-31 (b402fda4): luna lane, user-approved verdict-4c repair: 9 files (3_clock_check/6_profile/analyze/compile/emit_ts/lower/print_dl/strat/sweep) moved compile/ -> v6/prolog.
task(battery_load_flake,   done, []). % LANDED 2026-07-31 (b402fda4): root cause = spawn-heavy tsv2 tests (bop run boots node+swipl per test) x node --test default file concurrency = ncpu (12 here).
task(golden_flake_hunt,    done, []).                       % LANDED 2026-07-29 (4d8d8e8c): REPRODUCED 1/18 sub-runs under 3x-concurrent full-suite load -- all 11 subtests print pass yet the FILE fails with bare 'test failed', ZERO error payload (native/process-level signature, not a JS assertion).
task(reactor_buffertime_flake, done, []).                % LANDED 2026-07-31 (0c9fbe75): NEW, found by the golden RCA lane: v6/sprefa-store/js/tests/labs/reactor.test.ts "reactor A file+folder coalesce" is a MORE frequent real flake.
task(lsp_diags,            done, [extraction_live_p2, cli_bop]). % LANDED 2026-07-29 (77b5bbce): LSP DIAGS HOLDS): ZERO new LSP code -- diag-rail.dl6 declares rel diag_v5 in v5's exact 9-column shape and tsv2's bare-name tables.
task(emitter_groupby_literal, done, []).                 % LANDED 2026-07-29 (733d4c1b): REAL EMITTER DEFECT (lsp arc), FIXED IN TWO PASSES: a rule head with >=2 bare integer-literal columns reaches the GROUP BY verbatim and SQLite reads a bare integer there as a POSITIONAL column ref.
task(v5_lsp_exit_hang,     done, []).                    % LANDED 2026-08-02 (721be80a): root cause = background sender clones, not the loops.
task(pre_occurrence_loop,  done, [edge_body_constructs]). % LANDED 2026-07-30 (7f086fd3): Ordered occurrence execution snapshots each referenced rel once into __pre_<rel>, processes arrivals/departures in sequence, applies each accepted keyed write before the next occurrence, recomputes levels.
task(decode_arc,           closed, []).                    % SUPERSEDED 2026-07-29 morning by ruling compound_storage = struct_as_rows: there is no blob to decode; destructuring becomes joins. See struct_as_rows
task(struct_as_rows,       done, [tick_phase_alignment]).  % LANDED 2026-07-29 (dcfa6fcc): conformance 156/0, sweep BOTH modes 95/93/0-wrong, TEXT_DOOR 95/95/0, plunit 137/137, roundtrip ALL PASS, tsv2 65/1skip, dl 96/1skip, store 74/74, staleness gate OK.

% ── rows added by the world-health audit, 2026-07-29 midday ──────────────────
% Every row below was verified against the tree at b535ca62, not carried over
% from a report. Statuses: in_flight = a lane is running as this was written.
% The fable close-out handoff block (83538cef) is FOLDED IN HERE: where it and
% the audit filed the same task under two names, the handoff name wins
% (amplification_sensors over storage_amplification_sensors,
% doc_format_extraction over extract_doc_formats) and the two texts are merged.

task(file_span_redesign,   unbuilt, []).  % USER-DIRECTED 2026-07-29 evening, plans/2026-07-29-file-span-design.md: a span is a FILE_SPAN -- it references its parent file, carries a range, and its TEXT IS DERIVED, never stored ((digest,start,end) IS the text identity). (history: plans/2026-08-23-arch-history.md#file-span-redesign)
task(file_span_storage_lab, labbed, [file_span_redesign]). % COMPLETED 2026-07-29, plans/2026-07-29-file-span-storage-lab.md + executable/raw results in v6/sprefa-store/bench/file_span/. REAL CENSUS: release extract over 1048 tracked source files, 0 failures, 7,345,805 refs / 2,073,233 distinct spans = 3.543 refs/span; 99.0% have >=2 refs, 61.5% >=3. (history: plans/2026-08-23-arch-history.md#file-span-storage-lab)
task(rel_value_unification_lab, labbing, [struct_as_rows]). % USER-DIRECTED ACTUAL-WORLD LAB 2026-07-29, plans/2026-07-29-rel-value-unification-lab.md. `type` surface removed. (history: plans/2026-08-23-arch-history.md#rel-value-unification-lab)
task(file_span_kernel_host_boundary_lab, labbed, [file_span_storage_lab, rel_value_unification_lab]). % COMPLETED 2026-07-29, plans/2026-07-29-file-span-kernel-host-boundary-lab.md. ACTUAL COMPILER: slicing, line count, and column anchor use existing arithmetic/count/max; byte substring has no pure expression. (history: plans/2026-08-23-arch-history.md#file-span-kernel-host-boundary-lab)
task(rel_edge_clock_fixpoint, labbing, [file_span_kernel_host_boundary_lab]). % ACTIVE 2026-07-29, chat_log/20260729.4.rel-edge-clock-fixpoint.pl. ITERATION 2: make existing key(...) positions drive automatic typed relation-edge construction for single/composite keys; split keyed entity cycles from content-key cycles; add real SQLite receipts before changing syntax. (history: plans/2026-08-23-arch-history.md#rel-edge-clock-fixpoint)
task(key_edge_case_census, labbed, [rel_edge_clock_fixpoint]). % COMPLETED 2026-07-29, v6/prolog/labs/rel_value_unification/8_key_edge_case_census.pl, 12 PASS. READY: existing single/composite key positions already produce target-table UNIQUE constraints; typed parent is INTEGER endpoint. (history: plans/2026-08-23-arch-history.md#key-edge-case-census)
task(key_position_validation, done, [key_edge_case_census]). % LANDED 2026-07-29 (55d453b5): Existing key(...) now rejects zero and above-arity positions as key_position_out_of_range(Ref,Position,Arity), and repeated positions as key_position_duplicate(Ref,Position), before DDL.
task(keyed_signed_row_clock, done, [key_edge_case_census]). % LANDED 2026-07-29 (a2bbc73b): LANDED IN ACTIVE BRANCH 2026-07-29, scopes.pl stale_keyed_retraction_keeps_replacement, oracle 3 expectations PASS and emitted SQLite 3 PASS.
task(reference_construction_contexts, labbed, [rel_edge_clock_fixpoint]). % COMPLETED 2026-07-29, v6/prolog/labs/rel_value_unification/9_reference_construction_contexts.pl, 8 PASS. Existing RHS target query already joins the target table but head construction discards available identity into JSON. (history: plans/2026-08-23-arch-history.md#reference-construction-contexts)
task(existing_target_identity_prototype, labbing, [reference_construction_contexts]). % ACTIVE BRANCH 2026-07-29. When a positive body contains target(Id,Fields) and a relation-shaped head column contains the identical target(Id,Fields) term, lower now projects that already-joined public table alias's __id directly. (history: plans/2026-08-23-arch-history.md#existing-target-identity-prototype)
task(direct_trigger_identity_prototype, labbing, [existing_target_identity_prototype]). % ACTIVE BRANCH 2026-07-29. An arrival edge trigger that is a referenced public relation now samples its current target row by the trigger fields and projects joined __id. One indexed equality join; departure triggers do not join absent current membership. Construction contexts 8 PASS, plunit 147 PASS. (history: plans/2026-08-23-arch-history.md#direct-trigger-identity-prototype)
task(automatic_derived_reference_match, done, [direct_trigger_identity_prototype]). % LANDED 2026-07-29 (35ff808c): New shared expansion phase 50, after match: relation-shaped level-head values add ordinary target membership.
task(clock_checker_proof_payoff, labbed, [tick_model, clock_check]). % SOL SHAKEDOWN 2026-07-29, plans/2026-07-29-clock-checker-proof-payoff.md. EXISTING STATIC PROOFS: negative/aggregate stratification, acyclic positive ordering for the emitted SQL subset, five named cross-plane unsupported constructs, declaration/key validity. (history: plans/2026-08-23-arch-history.md#clock-checker-proof-payoff)
task(world_reference_key_resolution, done, [automatic_derived_reference_match]). % LANDED 2026-07-30 (9a245a2e): One rel/checker/table graph: nested rel-shaped ingress expands topologically into ordinary public target arrivals, then rewrites parent columns to dense integer endpoints.
task(ref_necessity_proof, done, [world_reference_key_resolution]). % LANDED 2026-07-29 (87610329): No ref surface construct selected.
task(reference_membership_boundary, done, [ref_necessity_proof]). % LANDED 2026-07-30 (9a245a2e): Nested wire values become same-tick ordinary target arrivals before parent reference rows.
task(schema_import_epic,   unbuilt, []).  % USER: TypeSpec / JSON-Schema / OpenAPI v3 (json + yaml) import as its own epic. Prior art is ours already (prior_art hafley_tsp: TypeSpec app-gen, config/env/CLI sources, @secret redaction). Build-vs-buy law applies before any bespoke line. Reads on doc_format_extraction for the yaml/json halves
task(world_health_reconcile, done, []).   % THIS AUDIT (opus worktree, 2026-07-29 midday): full ARCH reconcile + plans/2026-07-29-v6-world-health.md; owned resolving the fable handoff block into the reconciled file, which is what the rows around this one are
task(flow_parity_residue,  active, [flow_parity_upgrade]). % ADVANCED 2026-07-30 with existing facts only. CallF node rows plus TypeF owner-start joins and lambda exclusion restore flow_node_type: V6 58 rows/33 matched, previously empty. (history: plans/2026-08-23-arch-history.md#flow-parity-residue)
task(fork_join_malformed_json, done, []). % LANDED 2026-07-31 (55d453b5): RCA: the failing statement is any_failed's incremental level insert.
% duplicate comment_rail_wiring row (unbuilt) DELETED 2026-07-30 (self-map task_state_conflict catch): the wiring LANDED (merge 786b5daa, the :done row below survives). The old row's residue that stays real: block-range pairing refused unbound_head_var (misnamed-unsupported construct suspect, tracked on the done row).
task(text_expression_parity, unbuilt, [comment_rail_wiring]). % BOUNDED V5-PARITY CLEANUP, user-ruled 2026-07-29: preserve the working switch model and do not open a switch_map syntax/runtime redesign. Add text operations only when a V5-parity or golden-use-case program proves the missing operation. (history: plans/2026-08-23-arch-history.md#text-expression-parity)
task(higher_order_rel_scan, superseded, [pre_occurrence_loop, clock_check]). % RECONCILED 2026-07-30. Its ordinary-rel specialization and switch-cache evidence remains historical input, but its scan compiler gap receipt is false-positive: catch/3 succeeds with the error variable free, then the receipt unifies that variable with edge_body_needs_pre. Current ordered-pre planning/lowering succeeds. RuleRef, scan_signature, runtime relation values, and parallel signature type systems are excluded by the one-rel lock.
task(v6_completion_drive, active, [higher_order_rel_scan]). % USER GO 2026-07-29. Consultable task graph: plans/2026-07-29-v6-completion-drive.pl. Three parallel lanes: ordered_event_scan production implementation; ghcacher deterministic tick golden; extraction deterministic tick golden. Follow-ons: queryable clock signatures, checker-visible switchMap expansion, scan specialization, combined golden gate. No surface spelling in parallel lanes. Coordinator owns integration, shared generated files, commits, and push.
task(match_left_to_right_surface, done, []). % LANDED 2026-07-30 (9cadb419): Optional leading semicolon; Guard |-> Head is the existing level arm and Guard |+> Head the existing event arm.
task(scan_match_value_lab, superseded, [higher_order_rel_scan]). % RECONCILED 2026-07-30 by plans/2026-07-30-scan-match-reconciliation.pl. Its 0/1/N cases remain evidence; RuleRef, scan signatures, anonymous relation results, and surface proposals are excluded. Current scan candidates are semidet ordinary keyed edge writes: zero silent, one write, differing N keyed_conflict, equal N dedupe.
task(rel_definition_hash_lab, canonical_plan, []). % COMPLETED 2026-07-30, plans/2026-07-30-rel-definition-hash-lab.pl + v6/prolog/labs/rel_definition_hash/0_receipts.pl, 11/11 PASS. Separates semantic definition hash, storage instance identity, and rendered SQL binding; recursive SCC renames preserve content identity while schema/body changes invalidate it. No production integration or surface decision. One open canonical-label backend card remains.
task(scan_instantiation_generics_lab, closed, [higher_order_rel_scan]). % RECONCILED 2026-07-30. Receipts measured 0 generated persistent tables and 1 TEMP pre table. scan_signature and relation-role metadata remain lab-only and are excluded; named event/state/init/reducer rels remain ordinary rules.
task(select_scan_cache_lab, canonical_plan, [scan_match_value_lab]). % RECONCILED 2026-07-30. Ordinary-rel switch-cache, stale-result gate, and delayed-effect graph remain a golden algorithm plan. No runtime rel value, new syntax, or production construct selected.
task(v6_2_ts_closeout, active, [v6_completion_drive]). % USER GO 2026-07-30. Canonical live ledger: chat_log/20260730.0.v6-2-ts-closeout.pl. Remove RHS `?` host probes and `@ salt` riders; ordinary registered host rels lower through target-neutral contracts carrying typed inputs/outputs, identity projection, clock transition, and executor key. TS executes V6.2; Rust consumes the identical checked plan in V7. Extraction, ghcacher, parity, clock/type, and golden gates are exit conditions.
task(v6_2_host_contract_cleanup, done, [v6_2_ts_closeout]). % LANDED 2026-07-30 (7f086fd3): RHS host calls use ordinary relation spelling.
task(extraction_host_batching_lab, labbed, [v6_2_host_contract_cleanup]). % COMPLETED 2026-07-30, plans/2026-07-30-extraction-host-batching-lab.pl + v6/prolog/labs/extraction_host_batching/0_receipts.pl, 9/9 PASS. Current -> grouped extractor runs per N path/digest: callgraph 2N->N, diagnostics 2N->N, flow 1+7N->1+N; golden V unchanged. (history: plans/2026-08-23-arch-history.md#extraction-host-batching-lab)
task(extraction_host_batching, done, [extraction_host_batching_lab]). % LANDED 2026-07-30 (7f05d934): Compiler classifies fixed DL_EXTRACT_BIN templates as sprefa_extract.
task(rtkq_extraction_golden, done, [v6_2_host_contract_cleanup]). % LANDED 2026-07-30 (51ba4161): LANDED 2026-07-30. sprefa-extract adds typed AstPatternQuery/AstCaptureFact and CLI --ast-pattern/--ast-selector/--ast-capture.
task(v6_2_scale_gates, done, [v6_2_ts_closeout]). % LANDED 2026-07-30 (68f5e54b): WATCHER: real WatchBindRunner/LiveEngine/file-SQLite receipt. 100/1000 files, 480/4800 events, 3 ticks, 1 subscription, 1.185185 write amplification, 90/900 exact final rows, 0 wrong.
task(v6_2_http_cli_dogfood, done, [v6_2_ts_closeout]). % LANDED 2026-07-30 (7f086fd3): Canonical registry http_route/3 + cli_command/3 facts generate v6/tsv2/cli/0_inventory.ts through compile/2_emit_cli_inventory.pl.
task(norm_runtime_parity, done, [v6_2_ts_closeout]). % LANDED 2026-07-30 (9aa853bf): Existing norm/1 expression call only: registry text_scalar row, analyzer text operand rule, and emitted SQLite recursive scalar expression retain ASCII alphanumerics and lowercase letters.
task(v6_2_lab_reconciliation, active, [v6_2_ts_closeout]). % USER-RULED 2026-07-30. Every lab exits as implemented, canonical-plan, closed, or superseded; no unattached experiment directories or unindexed decisions. Reconcile scan and nested match composition against actual lowering, named intermediate rels, exact-one reducer output, init, clocks, and SQLite before any syntax ruling.
task(receipt_folding, active, [v6_2_lab_reconciliation]). % USER-RULED 2026-07-30. Every passing receipt must be folded into production unless it is blocked by an indexed user decision or named as superseded by a production mechanism. (history: plans/2026-08-23-arch-history.md#receipt-folding)
task(coordinator_pause_checkpoint, done, [receipt_folding]). % LANDED 2026-07-30 (c6e2bf7b): Every dirty tracked and untracked file was checkpointed together by explicit user instruction.
task(scan_match_reconciliation, labbed, [pre_occurrence_loop]). % COMPLETED 2026-07-30, plans/2026-07-30-scan-match-reconciliation.pl + v6/prolog/labs/scan_match_reconciliation/0_receipts.pl. 10/10 executable PASS, 8 plan records, 4 unselected cards. Nested match is two top-level matches joined by one ordinary rel: 4 persistent rel tables, 15 TEMP support tables, 3 level groups; direct nested block remains dl_parse_error(statement). (history: plans/2026-08-23-arch-history.md#scan-match-reconciliation)
task(scan_surface_composition_lab, labbed, [scan_match_reconciliation]). % COMPLETED SOL LAB 2026-07-30, plans/2026-07-30-scan-surface-composition-lab.pl + v6/prolog/labs/scan_surface_composition/0_receipts.pl, 10/10 executable + 10/10 plan PASS. Multiple demands share one reducer; state key is instance/owner/key. (history: plans/2026-08-23-arch-history.md#scan-surface-composition-lab)
task(single_rel_type_system_audit, done, [rel_value_unification_lab]). % LANDED 2026-07-30 (9a245a2e): Production reference runtime uses ordinary target arrivals followed by integer-edge parent arrivals in one tick.
task(json_interop_lab, labbed, [single_rel_type_system_audit]). % COMPLETED 2026-07-30, plans/2026-07-30-json-interop-lab.pl + v6/prolog/labs/json_interop/0_receipts.pl. Canonical plan only, zero production/syntax decisions. (history: plans/2026-08-23-arch-history.md#json-interop-lab)
task(byte_span_flattener,  closed, [file_span_redesign]).   % SUPERSEDED the evening it was filed, and the supersession is the interesting part. (history: plans/2026-08-23-arch-history.md#byte-span-flattener)
task(doc_format_extraction, unbuilt, []).  % USER DIRECTIVE 2026-07-29: raw html/xml/md/json/yaml/toml as extraction possibilities. Header plans/2026-07-29-extract-doc-formats-header.md (committed b535ca62, NOT dispatched): step 0 is the standing buy research (which grammars ship in our ast-grep registry vs need a tree-sitter dep), then a cst family for all six plus a doc family (key_path, value_text, value_kind, span) for json/yaml/toml and (element_path, attr, text, span) for html/xml. (history: plans/2026-08-23-arch-history.md#doc-format-extraction)
task(simplify_wave,        unbuilt, []).   % 19 deduped items from four read-only opus reviewers over 934dcc4d..HEAD (reuse/simplification/efficiency/altitude), brief plans/2026-07-29-simplify-wave-brief.md. P0 THREE ARE CORRECTNESS-ADJACENT, not cleanup: (1) lower.pl:1767 sniffs a `__dict_` NAME PREFIX to find dictionary relplans -- the banned magic-name pattern, and a user rel called __dict_x silently loses its delta arms; (history: plans/2026-08-23-arch-history.md#simplify-wave)
task(swipl_gc_abort,       unbuilt, []).   % UPSTREAM-SHAPED: swipl 10.0.2 aborts the compile sweep with "system error: Mismatch in up phase" under -g, deterministically at the 88th collection once the corpus reached 163 fixtures; the same goal typed at the toplevel completes. Worked around by set_prolog_flag(gc,false) for that one-shot process (sweep.sh:26, fork sweep_gc_false). Open work is not the workaround: it is (a) a minimal reproducer worth reporting upstream and (b) the knowledge that our compiler batch sits near a GC corner as the corpus grows
task(analysis_oracle_exam, unbuilt, []).   % USER-DIRECTED: a glean/joern/CodeQL-style graded analysis exam that REPLACES v5 as the standing oracle (v5 is a peer we are already beating on expressiveness and losing to on ingest, so grading against it forever caps the ceiling at v5). Research brief first, per the standing law. (history: plans/2026-08-23-arch-history.md#analysis-oracle-exam)
task(amplification_sensors, unbuilt, []).  % NO COMMITTED RECEIPT YET, and that is the point. Coordinator measurement relayed at audit time: a ~3.4MB comment-facts database roughly two-thirds duplicated join-key TEXT (163KB for 56 distinct paths in the same db). v5's storage diet fought the identical disease and won it with dense ids. (history: plans/2026-08-23-arch-history.md#amplification-sensors)
task(equals_unsupported_by_name, unbuilt, []). % SMALL, and the first thing a prolog reader trips on: `Var = expr` is unregistered, so it dies as unbound_head_var with no mention of `=` anywhere in the message (terra typed it in the flow rig; the sole reason it surfaced is that a lane was watching). Whatever SLOT-BIND-SPELLING rules, `=` must refuse BY NAME or bind. Same row carries the rename cost if the ruling moves `:=`: registry + parse/print + engine op + 16 fixture files, mechanical
task(assign_composition_lab, done, []). % LANDED 2026-07-29 (d0104974): plans/2026-07-29-assign-composition-verdict.md.
task(finish_the_job_epic,  done, []). % LANDED 2026-07-29 (21ecd6ac): plans/2026-07-29-finish-the-job-epic.md supersedes v6-alpha-golden-plan.

% ── fable coordination session 2026-07-30 (ledger chat_log/20260730.1.*.pl is the detail record) ──
task(clock_checker_full,   done, []). % LANDED 2026-07-30 (ffcddfc7): sol implemented, OPUS REVIEW GATE FALSIFIED the A6 "proven" claim by sabotage (constant observer + Grade=0 hardcode both stayed green).
task(roundtrip_two_door_fix, done, []). % LANDED 2026-07-30 (97133a85): the 18 roundtrip reds were ONE defect (parse_dl keeps both decl forms, print_dl printed both = duplicate decl line, reparse dropped the type).
task(comment_rail_wiring,  done, []). % LANDED 2026-07-30 (786b5daa): 6 of 7 verdict techniques as standing rails + parity referee, comment_node 745/745 v5-exact.
task(json_language_recovery, done, []). % LANDED 2026-07-30 (f9bf09df): plans/2026-07-30-json-query-language-recovery.md -- the v3/v4 json language is NOT lost, it is v5's src/datapath.rs brace walker (9 productions, stable since v1).
task(filespan_reconcile,   done, []). % LANDED 2026-07-30 (f94fc5c1): plans/2026-07-30-file-span-spine-reconciled.md -- 11 of 14 cards SETTLED by the v6.2 locks + landed rel-ref runtime, 3 open (rev naming, line/col residency, work-rev identity).
task(depth2_ref_fix,       done, []). % LANDED 2026-07-30 (2e2b983b): 176 PASS/10 fail red before the fix).
task(golden_flex_e2e,      done, []). % LANDED 2026-07-30 (60359862): HOLDS, conformance 186).
task(norm_oracle_emitter_divergence, unbuilt, []). % OPEN DEFECT found by golden_flex_e2e, zero conformance coverage before it: norm/1 gives oracle "norm(Hello World)" vs emitter "helloworld". The golden routes around it and step 5 of the rig is an INVERTED receipt asserting the divergence still exists, so the fixer gets a red gate telling them to promote norm into the golden. Owner unassigned
task(golden_flex_residue,  unbuilt, []). % 5 more defects indexed by golden_flex_e2e, none fixed: float/bool unspellable in sh and bind decls (decl_b_column_type/5 knows only int/text/json and silently degrades to none (history: plans/2026-08-23-arch-history.md#golden-flex-residue)
task(json_syntax_arc,      done, []). % LANDED 2026-07-30 (62f9ce84): cards only, parse_dl.pl untouched.
task(openapi_codegen_spine, done, []). % LANDED 2026-07-30 (bf71c5b6): 5 routes/5 ops/14 responses/14 schemas from prolog facts.
task(rx_oracle_harness,    done, []). % LANDED 2026-07-30 (feedbac): Same scenario written twice, leg A literal rxjs importing NOTHING from the repo, leg B bash only through bop serve + curl.
task(rel_as_value_lab,     done, []). % LANDED 2026-07-30 (ddec9718): what the user means by rels-as-values (pass a REL into an arg slot, higher order) is NOT what the relation-pattern feature does (construct + destructure ref columns, first order).
task(scan_surface_ruled,   done, []). % LANDED 2026-07-30 (de7146fe): scan gets NO NEW SURFACE.
task(json_edge_body_unblock, done, []). % LANDED 2026-08-22 (2bc49397): decode/2 in an edge body lowers when the source column is json (lower.pl check_edge_decode_sources/3, compile_edge_guards/6).
task(null_coherence_lab,   unbuilt, []). % IN FLIGHT lab (codex sol, ../sprefa-codex-null): user OVERTURNED the null ban ("we should just add json/sql nulls and get it over with in a way that is coherent"). Lab must make json null and SQL null one story, and price it honestly. (history: plans/2026-08-23-arch-history.md#null-coherence-lab)
task(prolog_graph_cleanup, done, []). % LANDED 2026-07-30 (71050af8): 0.368s real). flagship-flow.dl6 compile 257.09s -> 0.22s.
task(extract_spelunk,      unbuilt, []). % IN FLIGHT (codex luna, ../sprefa-codex-extspelunk): capability inventory of sprefa-extract itself, which nobody has done -- the parity lane inventoried v5's OPS and RELS, not the extractor. Deliverable's most useful column: of v5's 106 absent rels, which are absent because the EXTRACTOR cannot produce the facts vs because the LANGUAGE has no spelling (different owners, different fixes). (history: plans/2026-08-23-arch-history.md#extract-spelunk)
task(scip_passthrough,     done, []). % LANDED 2026-07-30 (8f6a4377): SCIP coverage 17/43 -> 43/43 serialized fields.
task(stale_labs_sweep,     done, []). % LANDED 2026-07-30 (74200375): LANDED 2026-07-30 (merge 74200375). 5 folded, 2 kept, and ghcacher_tick_golden PROMOTED after the lane refused the coordinator's framing: it was filed as debt and is a WORKING GATE nobody ran.
task(effect_chain_batch,   done, []). % LANDED 2026-07-30 (c8d25a6d): N stages = N+1 ticks, one hop per stage.
task(group_concat_silent_miscompile, done, []). % LANDED 2026-08-02 (17f64e18): OPEN DEFECT F9 from effect_chain_batch, coordinator-reproduced: group_concat(x) in a head COMPILES CLEAN at 200 and is not an aggregate.
task(host_column_shadows_runtime, done, []). % LANDED 2026-08-02 (54dc7604): OPEN DEFECT from staged_writes, coordinator-reproduced side by side: a host declaring an input or output column named ordinal or witness_digest compiles CLEAN.
task(staged_writes_lab,    done, []). % LANDED 2026-07-30 (787d994d): Marker writing already works and stops in three measured places: a host's input is a ROW while a write's payload is a RELATION with no string aggregate available.
task(ts_lowering_review,   done, []). % LANDED 2026-07-30 (be97647c): one tick fault PERMANENTLY kills the engine and the served process.
task(prolog_main_review,   done, []). % LANDED 2026-07-30 (9374bf5b): F1 the ORACLE HAS NO CLAUSE for combine/variadic or next/1, both registry LIVE, so term-form combine derives zero rows while the compiler emits a real cross join.
task(null_implementation_plan, done, []). % LANDED 2026-07-30 (660fedcb): LANDED 2026-07-30 (merge 660fedcb). 10 ordered steps, each with a fail-first receipt and a reversibility mark.
task(option_vs_null_lab,   done, []). % LANDED 2026-07-30 (0f521736): The user's three-variant read WORKS TODAY via json_type/2 with zero new semantics.
task(defect_wave_0730,     done, []). % LANDED 2026-07-30 (8d71f543): just green exit 0, sweep 133 compiled/131 identical/0 wrong, GOLDEN FLEX HOLDS, conformance 186 -> 193).
task(prolog_compile_profiling, done, []). % LANDED 2026-07-30 (4dd7e230): LANDED 2026-07-30 (merge 4dd7e230, codex sol). plan phase = 255,333ms of 255,490ms at 6.01e9 inferences.
task(teardown_flatten_lab, done, []). % LANDED 2026-07-30 (d966b407): TEARDOWN LAB HOLDS, 15).
task(rel_as_stream_lab,    done, []). % LANDED 2026-07-30 (7d040ab0): TIER-0 LIST EMPTY across 19 constructs.
task(external_oracle_scout, done, []). % LANDED 2026-07-30 (d9b808bf): external oracle scout is recorded in the roadmap.
task(v5_parity_spelunk,    done, []). % LANDED 2026-07-30 (754889b5): LANDED 2026-07-30 (merge 754889b5 -- the lane the coordinator relayed and forgot to merge, caught by the user asking.
task(flow_residue_partial, unbuilt, []). % terra STOPPED with reconstructed tables (dirty tree ../sprefa-codex-flowres): call-target partition = 110 extraction-input + 32 REAL resolver divergences (same coordinate, different callee); per-edge attribution blocked on the serve-lifecycle race golden_flex_e2e owns. match_arm_tokens syntax violation (9cadb419 |-> |+> unruled) awaits the user card
task(json_wiring,          done, [struct_as_rows]). % LANDED 2026-07-30 (c04b6cfd): green exit 0, conformance 201/0, TEXT_DOOR 141/141/0, plunit 236/236, sweep both modes 141/139/0-wrong).
task(ts_critical_fixes,    done, [ts_lowering_review]). % LANDED 2026-07-30 (bd0d567c): green exit 0, sweep 141/139/0, MEMORY SOAK HOLDS, serve-leak-soak clean).
task(bind_submit_error_arm, unbuilt, [ts_critical_fixes]). % NAMED GAP from the tsfix lane, recorded not patched blind: bind runners do not handle submit's error arm (2_binds.ts:164, :354) -- HTTP and the host runner both do, so a fault raised by a bind's own arrival still ends the app. Reachable only through an emitted-program defect on a bind-fed rel. Receipt to write: fail-first through a sabotaged emitted module
task(time_plane_lab,       done, []). % LANDED 2026-07-30 (dc9a5030): LANDED 2026-07-30 (lab commit dc9a5030, verdict plans/2026-07-30-time-plane-unification-verdict.md, lab-death executed by the retention landing lane, last copy f044ce06.
task(retention_minus,      done, [time_plane_lab]). % LANDED 2026-07-30 (d0a6bbd4): LANDED 2026-07-30 (lane commit d0a6bbd4, merged.
task(golden_gates,         done, []). % LANDED 2026-07-30 (c00043be): LANDED 2026-07-30 (lane commit c00043be, merged.
task(json_flex_lab,        done, []). % LANDED 2026-07-30 (7612fdc1): LANDED 2026-07-30 (lab commit 7612fdc1, merged.
task(dep_ver_minmax_reversed, unbuilt, []). % FOUND by the multirepo golden 2026-07-30, ungraded (v6 refuses the construct so no leg exercises it): v5 dep_ver min/max returns lo=v0.9.1 hi=v0.8.0 for github.com/pkg/errors, against the lexicographic order its own header claims, while example.com/shared is correct. Written in tsv2/goldens/multirepo_crawl/README.md. Suspect: v5-side aggregate over text versions. Verify against src/ before assuming
task(stream_cards_ruled,   done, []). % LANDED 2026-07-30 (55d453b5): seq(name) column-type sugar.
task(csp_idioms_lab,       done, []). % LANDED 2026-07-30 (bb84a26b): LANDED 2026-07-30 (lab commit bb84a26b, merged.
task(per_row_consumption,  unbuilt, [csp_idioms_lab]). % PARKED by user word 2026-07-30 ("park"). The W1/W2 root cause needs a RULED semantics: some spelling where taking a row CONSUMES it against same-tick competitors (one winner, losers see post-state). Pricing lab + fold of the csp fixture candidates + seq sugar wiring + readable-parse-errors all wake together on user word
task(oracle_body_gate,     done, []). % LANDED 2026-07-30 (fec109db): LANDED 2026-07-30 (lane commit fec109db, merged.
task(extract_t2_lab,       done, []). % LANDED 2026-07-30 (447ee181): LANDED 2026-07-30 (lab commit 447ee181, merged.
task(self_map_rail,        done, []). % LANDED 2026-07-30 (7d4cdeb5): LANDED 2026-07-30 (lane commit 7d4cdeb5, merged.
task(ordered_aggregate_lab, labbed, [json_flex_lab]). % LANDED 2026-07-30 (merge 0fbe27d0, lab-death cde5d9ec, last copy c45e3a46, verdict plans/2026-07-30-ordered-aggregate-verdict.md; first luna-mid lab per user word): (history: plans/2026-08-23-arch-history.md#ordered-aggregate-lab)
task(seq_sugar_wire, done, [point_free_lab]). % LANDED 2026-07-30 (d2dadb93): Ordinal := seq(name) via shared 0_seq_expand.pl, byte-identical to the 4-rule desugar both doors.
task(selfmap_single_file, done, [seq_sugar_wire]). % LANDED 2026-07-30 (0d194c3a): RELEASE GATE ruling release_gate_v620 SATISFIED -- ARCH-MAP.md from ONE dl6 file, python renderer deleted, run-twice identical, 4 mermaid fences.
task(devlog_rail, done, [ordered_aggregate_arc]). % LANDED 2026-07-30 (4dd6b76a): DEVLOG.md from ONE dl6 program over chat_log/*.pl session ledgers -- sh host consults each .pl and emits JSONL, decode/spread into rels, per-session sections, group_concat('\n', Ordinal) assembly.
task(golden_readiness, done, [devlog_rail]). % LANDED 2026-07-31 (d0c03cba): v6/READINESS.md = the 9 stopping-point programs graded by timed run. ghcacher-golden was RED on main.
task(float_avg_arc, done, [golden_readiness]). % LANDED 2026-07-31 (13653a77): beta gate 2 -- float REAL/binary64 exact, shortest round-trip shared oracle/TS (js_float_text), avg delta-maintained sum+count, int_out_of_range boundary unsupported construct both sides.
task(bigint_seam_normalize, unbuilt, [float_avg_arc]). % PRICED 2026-07-31: intMode bigint reverted -- emitted raw-row projections (ordered-occurrence arms, edge projections, ~6 sites per module) read driver rows RAW; a global bigint leaks into JSON.stringify (3 fixtures threw). Correct flip = normalize at the ONE SqlRunner seam (safe bigint -> number, unsafe -> named int_out_of_range with statement text) so no raw reader ever sees one; until then read-side SQL-computed overflow = driver RangeError (matrix residual). (history: plans/2026-08-23-arch-history.md#bigint-seam-normalize)
task(clock_legs, done, [clock_check]). % LANDED 2026-07-31 (55d453b5): replay gate over 5 historical bug classes.
task(bench_cli, done, []). % LANDED 2026-07-31 (62ff636b): rust-course phase 0 -- language-agnostic CLI contract, buy-verdict (hyperfine secondary, /usr/bin/time -l for RSS), 11 timed cells byte-identical, floor gate.
task(bench_reference_tier, done, [bench_cli]). % LANDED 2026-07-31 (8972dac1): ruling bench_reference EXECUTED -- identical_vs_reference verdict, tsv2(proven) referee, 3-condition promotion.
task(oracle_scale_ceiling, done, [bench_cli]). % LANDED 2026-07-31 (8972dac1): ANSWERED 2026-07-31 (merge 8972dac1): ruling bench_reference = proven_engine_reference (conformance/rulings.pl:538.
task(perl_alarm_orphan, unbuilt, []). % MEASURED 2026-07-31 (bench lane): the house perl -e alarm+exec timeout kills the wrapper, orphans the child (timed-out swipl left burning a core). Same one-liner in sprefa-store/bench/engines/tsv2_gen.sh + coordinator scratch runs. Fix = process-group kill (setsid + kill -TERM -pgid). Small lane.
task(unsupported_messages_arc, done, []). % LANDED 2026-07-31 (3ffc1074): beta gate 1 -- 107 specific unsupported construct message clauses 0 fallback, parse errors line:col furthest-failure, text door file:line at/3.
task(getting_started, done, [unsupported_messages_arc]). % LANDED 2026-07-31 (a5ad6233): beta gate 4 -- 24-block executed doc, all replay-gated (just getting-started, green-all).
task(cold_author_defects, unbuilt, [getting_started]). % FILED 2026-07-31 by the doc lane, none fixed in-lane: D1 host_input_contract keyed on hardcoded host NAMES (registry.pl -- any new host name refuses the standard (path,digest) shape); D2 template_mismatch renders without payload (which column? where?); D3 bop check bypasses throw_text_door_error -> unsupported constructs lose file:line the compile door prints (SMALL, high beta value); D4 served errors name gen_served temp copies not the user's file; (history: plans/2026-08-23-arch-history.md#cold-author-defects)
task(grader_exit_gate, done, []). % LANDED 2026-07-31 (4a6a6c1c): grader run/1 was exit-0 advisory since birth.
task(manifest_reason_diff, done, []). % LANDED 2026-07-31 (55d453b5): PROPOSED by the fork_join lane 2026-07-31: a unsupported construct-reason-level manifest diff as a standing check.
task(scan_spelling_card, labbed, []). % ANALYSIS LANDED 2026-07-31 (plans/2026-07-31-scan-spelling-card.md, opus): 242 scan( call sites / 105 of 129 v5 examples, 9 shapes -- 181 expressible today (A/B/F/G), 48 expressible-but-WRONG (glob dialect), 19 inexpressible (braces D, org fan-out I). 6 ruling cards AWAITING USER: SLOT-SCAN-NAME (note: 'scan' spent twice in-tree, rx accumulator + SQL SCAN verdict = B8 hazard), SLOT-GLOB-DIALECT (node matcher agrees with v5 globset on every measured case), SLOT-FEED-RESIDENCY, SLOT-FEED-REUSE, SLOT-ORG-FANOUT, SLOT-BRACE-ALTERNATION.
task(glob_dialect_split, done, []). % LANDED 2026-07-31 (bee38a95): LIVE DEFECT measured 2026-07-31 by the scan-card lane: bind watch's BOOT half (git ls-files pathspec, 2_binds.ts) and LIVE half (node path.matchesGlob) disagree on 170/242 corpus globs.
task(pathspec_brace_unsupported, unbuilt, []). % RESIDUAL from the glob fix 2026-07-31: a brace glob posted to a PATHSPEC-backed want(glob) row (enumerate/grep_at hosts) answers zero rows silently; crawl-bench.sh and flagship-callgraph.sh each hand-split braces independently. Fix = named unsupported construct at the demand row, not a dialect swap (pathspec is those hosts' documented contract). Small.
task(files_naming_p1, done, []). % LANDED 2026-07-31 (e3f5064f): ruling files_naming half-executed -- enumerate/enumerate_at -> files/files_at (program-text sh decls, rename not kernel).
task(repo_column_fork, done, [files_naming_p1]). % RULED + EXECUTED 2026-07-31 (ruling repo_column_spelling = distinct_name_hosts, option A; user: "no magic strings repeated all the time since we dont have defaults or nulls"). See files_repos_p2 for the landing.
task(files_repos_p2, done, [repo_column_fork]). % LANDED 2026-07-31 (e7fc24fe): repo_files/repo_files_at (git -C templates, unscoped pair byte-untouched) + repo_file/3 as its OWN rel (a path is only a key alongside its repo).
task(unprobed_host_no_rels, done, [files_repos_p2]). % LANDED 2026-07-31 (e7fc24fe): DEFECT FOUND+FIXED 2026-07-31 by the files/repos p2 lane writing the ungraded gh_repos decl: generated_host_decls/7 is only reached from expand_probe_rules/5.
task(host_arity_overload_miscompile, done, [files_repos_p2]). % LANDED 2026-07-31 (e7fc24fe): 1_host_expand.pl:no_duplicate_host_names/1 throws duplicate_host_decl(Name) at load, both doors, with the reason stated.
task(bind_repo_column_unsupported, done, [files_repos_p2]). % LANDED 2026-07-31 (55d453b5): `repo` on ANY bind decl is the named unsupported construct bind_repo_column(Name) rather than the generic bind_mismatch, with four reasons written at validate_bind_decl/3.
task(dataflow_atlas, done, []). % LANDED 2026-07-31 (55d453b5): dataflow atlas is recorded in the roadmap.
task(sqlite_reserved_rel_name, unbuilt, []). % DEFECT found by the atlas lane 2026-07-31: a rel named sqlite_* emits a reserved table name, compiles clean BOTH doors, then kills the served process at DDL time (POST /program = curl 000, no body). Two fixes owed: load-time unsupported construct on reserved table names, AND the load path answering an error instead of exiting (self-diagnosis law).
task(atlas_negation_zero_rows, unbuilt, []). % UNEXPLAINED, honestly unattributed 2026-07-31: a negation answered zero rows where the same negation over a projection rel answered correctly; the lane's first attribution (wildcard-in-negation) was measured and REFUTED by a minimal repro. Cause unidentified; repro pointer in dataflow-atlas.dl6 header. Scout before any fix.
task(clock_check_path_blowup, labbed, [clock_checker_full]). % 2026-08-21 REOPENED then PINNED OFF (rulings.pl clock_path_check_pinned_off): the offset fast path covered zero-weight cycles only, the resource bound was never written, ghcache.dl6 overflowed the stack after 20 min. The path walk is off the compile path behind prolog flag dl6_clock_path_walk; kept as the seed of the clock/det-mode calculus. % DEFECT found by the atlas-variants lane 2026-07-31: (history: plans/2026-08-23-arch-history.md#clock-check-path-blowup)
task(assign_unknown_functor_escape, unbuilt, []). % P0-CLASS HOLE found by the atlas-variants lane 2026-07-31: an unknown functor on the RIGHT of := (split(path,'/')) compiles CLEAN through the text door and stores unevaluated term text as the value -- unsupported construct-by-absence covers body GOALS, not expression functors. Silent wrong values, both doors. Fix = the expression operator inventory (0_expression walk) refuses unknown functors by name at load.
task(inferred_clock_path_residual, unbuilt, [clock_check_path_blowup]). % RESIDUAL from the clock fix 2026-07-31: inferred_clock/4 still calls the old clock_path/7 enumeration. NOT on the compile path (check_clock_program only calls clock_violation/2) so no compile cost, but anything querying clock_fact/5 on a wide-route program pays exponential. Cannot take propagation blindly: it searches the FULL graph incl productive-delayed cycles where propagation does not terminate. Own arc.
task(scip_families, done, []). % LANDED 2026-07-31 (6fc751f6): sprefa-extract --family scip = real index data (v5 scip_setup/scip_import ported forward: INDEXERS rust/ts/go, ensure_index mtime cache, prost decode, 8 scip_* records + scip_index/scip_skip data rows).
task(timeout_gun, done, []). % LANDED 2026-07-31 (b10defaa): run-capped.sh hoisted (run_capped/capped/cap_self/capped_curl, pgroup KILL exit 124), served compile door = budgeted named compile_timeout answer + server survives.
task(gen_templating_card, labbed, [scan_spelling_card]). % ANALYSIS LANDED 2026-07-31 (plans/2026-07-31-gen-templating-card.md, opus, merge 3cefd752): 109 gen( call sites / 25 files (the 60 .dl hits are symlinks into examples/), 4 shapes -- append 47 + whole-file 9 EXPRESSIBLE, block-splice 39 + named-zone 14 NOT (need coordinate writes + byte spans). 47% of sites have ZERO holes; v5 scalars and v6 concat are exact complements (0 overlap). 5 ruling cards AWAITING USER, ranked: (history: plans/2026-08-23-arch-history.md#gen-templating-card)
task(design_archaeology, labbed, []). % LANDED 2026-07-31 (codex luna, merge 65854266): plans/2026-07-31-design-archaeology.md, snapshot-cited (archives carry no git; luna stopped correctly, resumed with file-level sourcing). 11-idea ledger: extraction/reactivity/datalog-over-SQL/content-addressing/self-hosting SURVIVED, coordinates/daemon/globs/diags/codegen/tick-model TRANSFORMED, author-facing cursor-xref-capture-write syntax DIED -- the cursor's responsibilities SPLIT into data columns (spans, witnesses) + tick/host streams. (history: plans/2026-08-23-arch-history.md#design-archaeology)
task(ordered_aggregate_arc, done, [ordered_aggregate_lab]). % LANDED 2026-07-30 (a2a98d5d): json_group_array/1 value axis + /2 int ordinal, group_concat/2 + /3, both doors byte-graded through the SHARED canonical json encoder.
task(json_pattern_expand, unbuilt, [json_wiring]). % THIRD SILENCE, stop-and-reported by the potholes lane 2026-07-30: brace pattern directly in a body atom argument (event({repo: Repo, ...})) derives nothing on EITHER door with no unsupported construct -- oracle unification can never match '{}'/1 vs obj(...), compiler lowers the brace as a tagged-term VALUE (permanently-empty rule), grades identical vacuously. (history: plans/2026-08-23-arch-history.md#json-pattern-expand)
task(golden_oracle_arrival_fix, done, [json_wiring]). % LANDED 2026-07-30 (c0180b82): 0_json_arrival.pl = ONE shared arrival-mapping module, dl6_oracle.pl -83 lines onto it, golden_oracle.pl consumes the same predicate (second copy DEAD, no third born).
task(type_matrix_lab, labbed, [golden_flex_e2e]). % LANDED 2026-07-30 (merged, coordinator regraded on merged main: 422 cells = 79 identical / 156 DIVERGENT / 71 silent coercion / 116 unsupported construct; verdict plans/2026-07-30-type-matrix-verdict.md + addendum; lab dir ALIVE as fix-wave verification tool, symlinks committed). (history: plans/2026-08-23-arch-history.md#type-matrix-lab)
task(incremental_affinity_drop, done, [type_matrix_lab]). % LANDED 2026-07-30 (32916613): deltas carry the STORED value from RETURNING.
task(point_free_lab, labbed, [csp_idioms_lab]). % LANDED 2026-07-30 (merge of 89ccaccf, lab dead at 1b17a7ea, verdict + addendum): M2 seq CONFIRMED (the payoff: 4->1 per cursor, csp 94->67 from M2 alone), M3 stages CONFIRMED level-only / REFUSED in edge (loud + silent receipts), M1 scan AMENDED weak (pre_occurrence_loop already shrank folds; two-trigger fold gets LONGER; residual value = keyed-head check, log-headed fold FORKS unrefused). Rx-doc corpus 30->20 rules; half already point-free. AWAITING USER: slot_pipe_word ruling read, minted-rel visibility. Wiring unqueued until word.
task(pairwise_single_tick_wrong, done, [point_free_lab]). % LANDED 2026-07-31 (89ccaccf): NOT a one-door bug.
task(pre_registry_drift, unbuilt, [tick_phase_alignment]). % DRIFT DEFECT, coordinator-reproduced 2026-07-30: pre-in-edge-body compiles bop exit 0 while registry.pl pre/1 row says refused; SYNTAX/SCOREBOARD stale in 3 places; the sweep's 13 edge_body_needs_pre fixtures STILL refuse, so two compile paths disagree on pre legality. Scout questions: which arc opened the path, is the compiled semantics the measured-wrong sampled reading on chained occurrences, why the fixture shapes differ. Semantics-danger: may need a ruling after scout.
task(type_ruling_round, done, [type_matrix_lab]). % LANDED 2026-07-31 (55d453b5): arrival gate ALL declared types all positions.
task(compile_trace, done, []). % LANDED 2026-07-31 (dd6a4a23): luna lane, merge dd6a4a23: always-on COMPILE-TRACE stderr line (per-phase wall ms + inferences) from compile_dl6/compile_program, one shared measurement impl in compile.pl (6_profile JSONL consumes it).
task(compile_speed_regression, done, []). % LANDED 2026-07-31 (55d453b5): cause was NOT mark_furthest/1's length/2 (that is the WALL cost, 59.9% of profiler self time).
task(get_else_wiring,      done, []). % LANDED 2026-07-30 (55d453b5): LANDED 2026-07-30 (ruling null_design = get_else_use_site_never_storage.
task(aggregate_text_unsupported, done, []). % LANDED 2026-08-02 (6263a183): a numeric aggregate (sum/avg/min/max, all through compile_aggregate_number_operand) over a column whose DECLARED type is not a number = named unsupported construct aggregate_operand_not_number.
task(refcount_rename, done, []). % LANDED 2026-08-02 (4b9b0efc): refcount rename is recorded in the roadmap.
task(cap_self_pgroup_inversion, unbuilt, [timeout_gun]). % FILED 2026-08-02 (opus lane t4-ledger, merge e076a68e = failure-modes classes 39+40): cap_self re-execs into its OWN process group, so an outer run_capped kill -TERM -pgid can never reach it -- nested caps INVERT (the incident mechanism the coordinator fed the lane was backwards; opus falsified it). Rail design in the t4 REPORT.md; 7 receipt rails exposed, 2 scripts share port 17571. Class 40 (aggregate emits no row for an empty group; coalesce is the idiom) = documentation entry, no code owed.
task(bounded_log_arm_order, active, []). % FILED 2026-08-03, MEASURED then RULED the same day. Sits in the conflict plane the unbuilt rw_disjoint algorithm row names (this file, algorithm/4 block). (history: plans/2026-08-23-arch-history.md#bounded-log-arm-order)
task(watch_bind_hazards, unbuilt, []). % FILED 2026-08-02 (opus lane t3-watcher, merge 29cb3097): 2_binds.ts maxQueue overflow drops watch events SILENTLY; a watch error kills the whole served process via process.exit. The lane also landed watchRealSource.test.ts, the missing real-fs N-saves regression test. (history: plans/2026-08-23-arch-history.md#watch-bind-hazards)
task(text_door_fact_seam, done, []). % LANDED 2026-08-04 (1640b768): FILED as finding F1 by the comment-rail opus lane 2026-08-04.
task(regexp_builtin, done, []). % LANDED 2026-08-04 (7462b380): regexp/2 positive body condition, SQL vocabulary word.
task(ast_op_wire, done, []). % LANDED 2026-08-04 (d88e858f): LANDED 2026-08-04 (two parallel codex luna lanes, merges d88e858f + 598d5182, contract plans/2026-08-04-ast-op-contract.md): lane A `extract query` subcommand (src/0_query.rs, full tree-sitter queries.
task(cst_native_syntax, done, []). % LANDED 2026-08-04 (add78de9): unquoted `cst(path, digest, lang) { s-expr }` body item parsed by parse_dl.pl into the ts_query term vocabulary that already existed in conformance fixture native_ts_query_term (2_hosts_wiring.pl:200).
task(comment_prod_expression, done, []). % LANDED 2026-08-04 (f76ef6f4): v6/dl/fixtures/comment-prod.dl6 = the comment budget with string+syntax logic in-language.
task(comment_rail_prose_count, done, []). % LANDED 2026-08-04 (fd7944ca): the budget counts PROSE lines only (letter-bearing after token strip).
task(graded_file_subprocess_fold, unbuilt, []). % FILED as finding F6 by the comment-rail opus lane 2026-08-04: grading one staged file spawns 3 subprocesses (added/nodes+comment-lines/markers legs, plus raw-lines now = 4); fold when a real commit exceeds ~2s wall, measure first. Cold-start friction sibling: a fresh worktree cannot commit until sprefa-extract is built (comment-budget-rail.sh:18-22 exits 1 with the build hint) and the rail needs tsv2 node_modules; both bit lanes today (fallback env used).
task(extract_md_html_query, done, []). % LANDED 2026-08-05 (247c6561): extract query gains langs md (tree-sitter-md 0.5.3 BLOCK grammar, the exact v5 pin so block parses match the v5 oracle), md_inline (INLINE_LANGUAGE, dropped in with zero executor change).
task(catalog_g1_producer, unbuilt, []). % SCAFFOLDED (decision record rulings.pl:613 catalog_universe): catalog rows describe USER-PROGRAM rel decls, produced from the compiler's relplan/5 decl table, materialized into the COMPILED PROGRAM db through the same door __tick uses; the store spine was REJECTED because the fact plane and a compiled program are separate SQLite dbs with no ATTACH (scratchStore.ts:1-11). (history: plans/2026-08-23-arch-history.md#catalog-g1-producer)
task(catalog_g2_oracle_parity, unbuilt, [catalog_g1_producer]). % conformance/ticklog.pl needs the same seed only once a FIXTURE derives from a catalog row; a DDL-time seed emits no delta at any tick, so g1 alone cannot diverge from the oracle. The first fixture whose rule reads a catalog row emits deltas the oracle never produces.
task(rel_catalog_ts_field, done, []). % LANDED 2026-08-07 (1ca2f5ff): catalog_rows/4 lifted out of catalog_row_ddl/4 so the INSERT and the emitted constant read ONE source (lower.pl:708).
task(emit_observers_quadratic, done, [rel_catalog_ts_field]). % LANDED 2026-08-07 (ce6286bd): DEFECT FOUND+FIXED 2026-08-07 by the compile-speed ratchet reading RED on clean main HEAD.
task(files_at_receipt_false_green, done, []). % LANDED 2026-08-07 (1ca2f5ff): DEFECT FOUND+FIXED 2026-08-07: files.sh step 4 waited with `await_rows file "$before"` where before = the count step 1 had just asserted.
task(enum_nullary_variant_empty_pk, unbuilt, []). % DEFECT FOUND 2026-08-08 by probe, NOT by any fixture: a variant with zero fields emits `PRIMARY KEY ()` and the program cannot boot. Receipt: `rel maybe_text(none() ; some(value: text)).` compiles green through compile_dl6 (rc 0, "wrote"), emits `CREATE TABLE "maybe_text_none" ("id" INTEGER NOT NULL, PRIMARY KEY ()) WITHOUT ROWID`; sqlite3 3.43 rejects it with `near ")": syntax error` and the tsv2 runtime dies at ScratchStore.boot with `LibsqlError: (history: plans/2026-08-23-arch-history.md#enum-nullary-variant-empty-pk)
task(enum_column_type_erased, done, [enum_nullary_variant_empty_pk]). % LANDED 2026-08-08 (3da285b0): DEFECT FOUND 2026-08-08: an enum name cannot be used as a column type even though it monomorphizes into real tables.
task(derived_rel_as_reference_target_duplicates, unbuilt, []). % DEFECT FOUND 2026-08-08 while fixing enum_column_type_erased, and it is a SILENT WRONG ANSWER rather than a unsupported construct. Wiring a DERIVED rel as a reference target makes it an arrival target too, because a reference column's arrival normalizes into an arrival for its target (0_type_plane.pl:1-30). (history: plans/2026-08-23-arch-history.md#derived-rel-as-reference-target-duplicates)
task(module_identity_bytes, done, []). % LANDED 2026-08-10 (8016050b): THREE RIDERS.
task(ast_query_blob_door, done, []). % LANDED 2026-08-04 (99aec99e): `extract query --digest <oid>` reads the source via `git cat-file blob`.
% ── DD line (2026-08-10/11), the differential-dataflow transfer arc ──────────
% Records the eight sprefa-store commits that landed the signed-delta retraction
% line plus the dd_plan/dd-runner arc. Numbers are DAG 960k, banked in
% sprefa-store/PERF-REPORT.md at 471d0be9 (input hash ef153ee39296ef0f, 800002
% survivors) and plans/2026-08-11-dd-line-recon.md. Forks 2/3/4 of the ranked
% transfer-forks list stay unbuilt; the comment names which part exists.

task(dd_source_hunt_recon, done, []). % LANDED 2026-08-10 (bff004ab): the dd 10x over sqlite is algorithmic (retraction is one signed pass), not memory -- the DL_SQLITE_RAM_PROBE flag saves only 3-6%. Reads: plans/2026-08-10-dd-source-hunt.{RECON,CLOSEOUT}.md.
task(dd_wall_ram_ceiling, done, [dd_source_hunt_recon]). % LANDED 2026-08-10 (c16029e2): streaming generator isolates dd's true resident wall; dd aborts at 3,168,002 nodes, ~224.1 B/node.
task(dd_plan_dd_runner, done, []). % LANDED 2026-08-09 (d6a3987a): 6_isolated_compiler_dd.pl (733 lines) emits the dd_plan term + JSON twin over lowered/8 (3 goldens byte-clean).
task(retract_signed_delta, done, [dd_source_hunt_recon]). % LANDED 2026-08-10 (f5f2eaf4): fork 1 first cut -- one signed pass over a delta(round,key,diff) table plus cx_refcount/cx_delta schema; engine.rs:642 retract_signed_delta.
task(signed_delta_agreement, done, [retract_signed_delta]). % LANDED 2026-08-10 (ee99ca0d): agreement tests for signed-delta across the DAG + cyclic matrix, all correct against the oracle.
task(signed_delta_bench_row, done, [signed_delta_agreement]). % LANDED 2026-08-10 (8676a752): perf_report gains the sqlite-signed-delta bench row + PERF-REPORT appendix. The floor estimate (near 874.6 ms) did not survive cycle-correctness and is recorded honestly (1135.6 ms measured).
task(recursive_cte_probe, done, [signed_delta_agreement]). % LANDED 2026-08-11 (00ad3f68): adds retract_signed_delta_v2 + retract_delta_fold + the recursive-CTE probe (examples/recursive_probe.rs, PROBE-REPORT.md).
task(signed_delta_v2_promote, done, [recursive_cte_probe]). % LANDED 2026-08-11 (43bdbc4e): v2 promoted to three dispatches, v1's bench row retired, sqlite-signed-delta-v2 becomes the measured default. DAG 960k: dd 174.6 ms vs sqlite-signed-delta-v2 1135.6 ms / 3 statements / 6.50x.
task(perf_report_refresh, done, [signed_delta_v2_promote]). % LANDED 2026-08-11 (471d0be9): PERF-REPORT refreshed with the coordinator verify run.
task(dd_runner_tick_phases, done, [dd_plan_dd_runner]). % LANDED 2026-08-11 (b42f1d6c): dd-runner's tick loop matched ONE of the twelve tick_order phases (6_isolated_compiler_dd.pl:729-733) and.
task(emit_rust_sqlite, unbuilt, [dd_plan_dd_runner]). % DISPATCHED 2026-08-12 (user: "i want rust + sqlite emitted now", "literally copy the ts into rs with idiomatic tokio and channeling/rx semantics in rust form with streamext and signals and spawns"): emit_rust.pl, the second language emitter beside emit_ts.pl (2814 lines, emit_program/5 at :2687), plus v6/sprefa-engine-rs, the Rust port of v6/tsv2/runtime (3568 lines TS, 1439 of them 1_incremental.ts). (history: plans/2026-08-23-arch-history.md#emit-rust-sqlite)
task(dd_fork2_epoch_batches, unbuilt, [perf_report_refresh]). % NOT BUILT as a Rust kernel. SQLite-shaped partial only: cx_delta(round,key,diff) + cx_refcount appended per round, engine.rs:139-146; PROBE-REPORT.md:61 states it does no periodic GROUP BY/HAVING sweep.
task(dd_fork3_arranged_halfjoin, unbuilt, [perf_report_refresh]). % NOT BUILT, plan-term only: arr(Id,Ref,KeyColumns,ValueColumns,signed) emitted at compile/6_isolated_compiler_dd.pl:258-262; v6/dd-runner/src/kernel.rs never reads arrangements.
task(dd_fork4_signed_multiplicity_consolidation, unbuilt, [perf_report_refresh]). % NOT BUILT, no code: no signed weight exists in v6/dd-runner/src/kernel.rs (relation type is BTreeMap<String, Vec<Tuple>>, kernel.rs:15).
task(shared_frontier_lowering, done, [emit_rust_sqlite]). % LANDED 2026-08-22 (PR #386): LANDED via PR #386 (b0c319e57, supersedes #378), NOT the branch feature/shared-frontier-fable, whose six commits main already carries byte-for-byte.
task(shared_frontier_view_inflation, unbuilt, [shared_frontier_lowering]). % THE MEASURED DEFECT, not a guess: over the 202 corpus fixtures the guard admits, every non-frontier DDL statement is byte-identical between the arms (942,811 bytes both sides) and the frontier objects alone go 397,463 -> 595,934, +49.9%. (history: plans/2026-08-23-arch-history.md#shared-frontier-view-inflation)
task(shared_frontier_guard_lift, unbuilt, [shared_frontier_lowering]). % THE REACH CEILING. lower.pl shared_frontier_todo/3 stops 139 of the 341 corpus fixtures that lower, and it stops every real program: v6/dl/ghcache/ghcache.dl6 (157 rels, 220 rules) throws frontier_shared_todo(edge_rules) and carries four more families behind it -- aggregate_head-11, host-8, non_set_rel-4, tick-1. Corpus histogram: non_set_rel 80, edge_rules 72, aggregate_head 46, tick 7, recursion 7, departure 6, host 5, retention 4. (history: plans/2026-08-23-arch-history.md#shared-frontier-guard-lift)
task(shared_frontier_default_flip, unbuilt, [shared_frontier_guard_lift, shared_frontier_view_inflation]). % Plan step 6. Deliberately NOT this arc's: the default cannot move to shared while the option reaches no program a user runs, and the flip would trade a measured -26% statements per tick against a measured +14.8% DDL bytes with no program able to show both at once. Blocked on both rows above, in that order.
task(one_tick_path, done, []). % LANDED 2026-08-23 (c36e7ef9): ordered_program/1 and ordered.rs are DELETED.

task(dd_oracle_crosscheck, done, []). % BUILT 2026-08-23 (PR #438): the real differential-dataflow ecosystem runs 10 conformance programs and its per-tick delta stream is diffed against the oracle's, tick for tick, as a multiset of (row, sign). conformance/dd_panel_export.pl exports the panel; sprefa-engine-rs/tests/dd_oracle_crosscheck.rs hand-builds one circuit per program name, dev-dependency only. dbsp 0.337 lost the pick on a compile receipt: it ICEs rustc 1.97.0-nightly and adds 223 lock packages against dd's 15 (docs/failure-modes.md 90).

task(delta_arm_subset_expansion, done, []). % BUILT 2026-08-23 (PR #435): ordered plain-join delta identity, one current-state transition arm per optional item, and signed-loss insert arms for shrinking negated inputs. page_response 248,015 bytes/256 arms/64 clauses -> 7,548/7/1; callgraph tick 4 adds unused(main); grade 445/341 and ghcache tick log byte-identical. The runtime compatibility case it left open is retired: a negated input loss no longer makes a head recount-eligible (docs/failure-modes.md 89).

task(extract_rename_arc1_ts_anchor, done, []). % LANDED 2026-08-27 (PR #511): `extract rename <file>#<old> <new>` over oxc_semantic, single TS anchor file, dry run default, --commit writes. v6/sprefa-extract/src/0_rename.rs, lang/ts_rename.rs, tests/4_rename_ts.rs.
task(extract_rename_arc2_stops, done, [extract_rename_arc1_ts_anchor]). % LANDED 2026-08-27 (PR #514): named RenameStop variants, --at <byte> for a shadowed declaration, exit codes 3-6. Every stop writes nothing.
task(extract_rename_arc3_ts_crossfile, done, [extract_rename_arc2_stops]). % LANDED 2026-08-27 (PR #516): TS rename follows imports/re-exports across files; RenameStop::Dynamic became Dynamic(Vec<SymbolSeat>) (types.rs, one span under-reported).
task(extract_rename_arc4_text_refs, done, [extract_rename_arc3_ts_crossfile]). % LANDED 2026-08-27 (PR #517): --text-refs reports old-name spellings in plain text (comments, strings, md) the plan leaves behind; report only. src/2_move_text.rs.
task(extract_rename_arc5_rust_syn, done, [extract_rename_arc2_stops]). % LANDED 2026-08-27 (PR #518): Rust arm over syn, lang/rust_rename.rs, roster line in lang/mod.rs. Self-rename oracle in tests/5_rename_rust.rs is #[ignore] (25.2 s by hand, passes).
task(extract_rename_arc6_scip_verify, done, [extract_rename_arc3_ts_crossfile, extract_rename_arc5_rust_syn]). % LANDED 2026-08-27 (PR #519): --verify-scip <index> cross-checks the plan against a prebuilt SCIP index; count only, never changes plan or exit code. scip-typescript 0.4.0 writes DEFINITION roles only.
task(extract_rename_arc7_kotlin, done, [extract_rename_arc5_rust_syn]). % LANDED 2026-08-28 (PR #521): lang/kotlin_rename.rs over tree-sitter-kotlin-sg, tests/7_rename_kotlin.rs 3/3, wildcard import is a Dynamic stop.
task(extract_rename_arc8_prolog, done, [extract_rename_arc5_rust_syn]). % LANDED 2026-08-28 (PR #522): lang/prolog/_2_rename.rs, symbol = name/arity, two arities without --at is Ambiguous, =.. functor is a Dynamic stop; tests/8_rename_prolog.rs 4/4 with a swipl load oracle.
task(leaky_types_rehome_legs, done, []). % LANDED 2026-08-28 (PR #520): Rehome optional methods split into RehomeManifests/RehomeShim/RehomeTextSpellings/RehomePlanCheck on a RehomeArm roster (plans/2026-08-27-leaky-types-review.PLAN.md row 12).
task(extract_rename_root_scope_wins, done, [extract_rename_arc8_prolog]). % LANDED 2026-08-28: user decision, a root-scope binding wins without --at (TS ts_rename.rs symbol_scope_id == root; Rust rust_rename.rs Decl.block shadows its own block only). Rust oracle: cargo check on the no-dep fixture crate runs in the battery (tests/5_rename_rust.rs renamed_fixture_crate_passes_cargo_check); the 25 s self-rename stays #[ignore].
task(grade_ratchet_bisect, done, []). % LANDED 2026-08-27 (PR #513, #515): 3-day grade red 345->322 bisected to #481 adding oxc_resolver, whose default features flip serde_json/preserve_order for the unified build graph; ticklog.rs canonical JSON now sorts keys through BTreeMap. docs/failure-modes.md 95. graded.tsv regenerated at 346/449 (65eab58b2).
task(main_reconcile_codex3, done, []). % LANDED 2026-08-27 (9f61970f5, 6a1fbf0c1): local main (39 codex-3 type-system commits) merged with origin (70). plunit_tests.pl three-way merged, plane count re-measured refcount-1664. Gates on the merge: plunit 1167/0, conformance 449/0, RUST-GRADE 346/449.
task(tsi_a1_witness_envelope, done, []). % LANDED 2026-09-02 (PR #645): extract --witness envelope, protocol 1, run/fact/witness/coverage/diagnostic records, FlatFact: Deserialize; goldens byte-identical flag-off. tests/96_witness_wire.rs 8. Contract: issues/extract-semantic-fact-roundtrip/item.md ## Decisions.
task(tsi_a3_registry_ingest, done, [tsi_a1_witness_envelope]). % LANDED 2026-09-02 (PR #648, #651, #654): src/tsi/registry.rs REGISTRY as data (tsi.*, ts.*, rust.*, go.*, plus tsi.symbol/value/value_argument/scip_symbol), TsiSink, extract --ingest (decode, registry, id-closure fixpoint, coverage, renumber, sorted re-emit, idempotent). tests/97_ingest.rs 17.
task(tsi_a2_multi_witness, done, [tsi_a3_registry_ingest]). % LANDED 2026-09-02 (PR #652): --witness over --resolve, ProjectEdge.witnesses, every resolver leg a witness, run row per tier. tests/98_resolve_witness.rs 8. Repaired sprefa-engine-rs hosts.rs (red on main, CI-KNOWN-RED.md:131 stale).
task(tsi_a4_syntax_rows, done, [tsi_a3_registry_ingest]). % LANDED 2026-09-02 (PR #653): syntax-tier tsi rows for ts and rust under --witness; variance atom unspecified; alias = tsi.symbol + tsi.denotes. tests/99_syntax_tsi_rows.rs 12.
task(tsi_a7_v7_loader, done, [tsi_a3_registry_ingest]). % LANDED 2026-09-02 (PR #655): v7/src/2_comptime/0c_extract_loader.pl, accepted/1 verbatim, tsi.product -> product node, tsi.edge -> :/4, prelude 5_tsi_primitives.dl7; v7 battery 55/55.
task(tsi_a5_ts_semantic, done, [tsi_a2_multi_witness, tsi_a4_syntax_rows]). % LANDED 2026-09-02 (PR #657): tsc walk emits tsi rows under --witness, 17 relations complete over probe.ts, tsi.edge partial (lib owners are leaves), src/tsi/semantic.rs SemanticRows + emit_semantic. tests/101_ts_semantic_tsi.rs 10. Cost 0.76s -> 1.10s over 20 files with the flag.
task(tsi_a6_rust_semantic, done, [tsi_a5_ts_semantic]). % LANDED 2026-09-02 (PR #659): rust-analyzer walk emits tsi rows under --witness, 17 relations complete over rust_probe, tsi.edge/conforms/has_type partial with diagnostics; max RSS 570 MB (failure-mode 105 unregressed). tests/102_rust_semantic_tsi.rs 9.
task(tsi_a8_intersection, done, [tsi_a6_rust_semantic]). % LANDED 2026-09-02 (PR #661): tests/100_tsi_intersection.rs 5, shared tsi.* projection equal across probe.ts and rust_probe (13 rows), 10 ts asymmetries pinned. Criterion 8 of issues/extract-semantic-fact-roundtrip.
task(tsi_contract_vs_trait_shape, unbuilt, [tsi_a8_intersection]). % FORK for Chris (from A8): ts interface projects as tsi.product, rust trait as rust.trait only; rust symbols carry no tsi.origin (A6) so they project unnamed while ts symbols (A5 deviation 8) do.
task(tsi_typespec_rows, unbuilt, [tsi_a3_registry_ingest]). % HELD by user 2026-09-03 ("no tsp right now"); codex-tsi told. codex-tsi session: tsp.* REGISTRY rows plus an --ingest-accepted fixture; research plans/2026-09-02-typespec-tsi-fact-parity.md on perf/v7-cold-compile.
task(extract_emit_throughput_budget, done, []). % LANDED 2026-09-03 (PR #664): the real first bad commit was #562 (33a2643b4), a second blake3 over every file in go_bind_plan_store; the coordinator's one-shot bisect misread #562's 5.51 s as a pass and named #567. Fixed 2 hashes -> 1, go call span 735 -> 421 ms; 45_emit_throughput 5.40-5.50 s (10/11) against a 5.5 s budget whose own commit reads 4.9 s today (header band 4.03-4.15): 0.8 s of machine drift stays open, the budget is not moved. docs/failure-modes.md 107.
task(extract_trail, done, [extract_emit_throughput_budget]). % LANDED 2026-09-03 (PR #668): Phase spans (hash on content_id_of itself, parse, family, bind_plan, chain, tsi_syntax, tsi_semantic, flatten, write, resolve_leg), load average beside every timing, extract_run + extract_phase rows in ~/.agent/dl6.db under --bench or DL_TRACE_SUMMARY=1 (DL_TRAIL=0 disables), extract --trail N, --bench through tracing (eprintln 8 -> 7). tests/31_tracing.rs 9 (hash calls per file pinned go 2 / ts 2 / rust 1), tests/103_trail.rs 6. Brief:
task(extract_one_hash_per_file, done, [extract_trail]). % LANDED 2026-09-03 (PR #673): dispatch.rs keeps the blob it keyed the extract on in a thread-local tagged with the byte slice; go_parse_shared_keyed and the ts receiver store read it back; 31_tracing pins go 1 / ts 1 / rust 1.
task(extract_tier_decline_is_a_diagnostic, done, [tsi_a2_multi_witness]). % LANDED 2026-09-03 (PR #674): a requested tier that answered nothing is a diagnostic record on run 0, relation tier.tsc / tier.rust-analyzer; tests/104_tier_decline_diagnostic.rs.
task(extract_resolve_carries_syntax_tsi_rows, done, [tsi_a4_syntax_rows]). % LANDED 2026-09-03 (PR #674): --witness --resolve --family type carries the syntax tier tsi rows, ids rebased per file (wire::tsi_rows_rebased); tests/105_resolve_syntax_tsi.rs.
task(v7_loader_skips_foreign_records_by_name, done, [tsi_a7_v7_loader]). % LANDED 2026-09-03 (PR #671): foreign_record/1 names the 56 non-TSI extract records; only a record outside both lists is malformed_record; v7 battery 57/57.
task(rust_syntax_type_graph, done, [tsi_a4_syntax_rows]). % LANDED 2026-09-03 (PR #678): last-segment origin spans, tsi.called on every written Name<Args> (tuples are anonymous products), variant payload edges, method edges from the owner and the trait, const/static tsi.has_type, tsi.primitive for the 17 prelude builtins plus unit; tests/106_rust_syntax_graph.rs 7. src/trail.rs: tsi.called 0 -> 7, primitive 0 -> 5, edge 11 -> 29. v7 prelude lacks the class `unit` (tsi_primitive_class_absent(unit)); dl7 side.
task(rust_checker_all_features, labbed, [tsi_a6_rust_semantic]). % FOUND 2026-09-03 by hand probe: --rust-checker over src/trail.rs (a #[cfg(feature = "cli")] module) loads a run 1 with zero facts and no diagnostic; rust_checker_ra.rs:44 CargoConfig leaves features at Selected{[]}, so the module is absent from the crate graph and file_to_module_defs is empty. Brief plans/2026-09-03-rust-checker-all-features.BRIEF.md: CargoFeatures::All plus a tier.rust-analyzer diagnostic per supplied file owning no module.
task(rust_checker_walk_scales_with_crate, unbuilt, [rust_checker_all_features]). % FOUND 2026-09-03 by hand probe: --rust-checker over ONE ungated file (src/trace.rs, 1 file answered) walked 187s and emitted 1129 rust.impl / 1129 tsi.conforms / 983 tsi.callable / 1501 tsi.type: rust_checker_ra.rs:392 Impl::all_in_crate walks every impl of every crate a supplied file touches, so the walk is priced by the crate, never by the file. Under the 10-second law the walk is a defect (only the workspace LOAD carries the SCIP exception, project.rs CHECKER_BUDGET comment). Fix shape: impls whose self type or trait is declared in a supplied module, plus impls the supplied declarations name; everything else stays a leaf. Dispatch after rust_checker_all_features lands (same file).
task(resolve_syntax_tsi_beside_loaded_checker, unbuilt, [extract_resolve_carries_syntax_tsi_rows]). % FORK for Chris (from PR #674, lane call 1): the syntax tier tsi rows ride --resolve only for a language whose checker tier did NOT answer, because every tier numbers Arg::Id from 0 and --ingest renumber_ids would merge two types under one number. Alternative: rebase the semantic tier past the syntax ids and emit both, letting witness rows pair them (identity rule 5).
task(ts_syntax_type_graph_parity, unbuilt, [rust_syntax_type_graph]). % FOLLOW-UP: ts.rs tsi_target mints one id per written text with no tsi.called outside alias bodies, no tsi.primitive for string/number/boolean, no tsi.has_type for const; the rust lane fixes D2/D5/D6 on the rust door only.

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

:- use_module('src/grader', [run/1]).
% go rides grader run/1 so a red check FAILS the goal and `-g go -g halt`
% exits 1. The private copy of the loop here was the exit-blind class-37
% shape a second time: roadmap_is_total went red (dangling dep) and every
% `just arch` tail still said PASS/exit 0.
go :- run(check).

% ═════════════════════════════════════════════════════════════════════════════
% FABLE HANDOFF BLOCK (2026-07-29 evening, coordinator final save; sol drives
% next) -- FOLDED 2026-07-29 by world_health_reconcile, which owned the ARCH
% reconcile the block itself names. Nothing was dropped:
%   * its PROCESS FAILURE note is now a CALLOUT at the top of this file,
%     verbatim, so it is read with the other standing laws;
%   * its ten task rows live in the audit block above, deduped against the
%     rows the audit filed independently. Two names collided and the HANDOFF
%     name won both times: amplification_sensors (audit had
%     storage_amplification_sensors) and doc_format_extraction (audit had
%     extract_doc_formats). world_health_reconcile itself is `done` there.
% Session record: chat_log/20260729.2.fable-closeout-handoff-to-sol.md.
% ═════════════════════════════════════════════════════════════════════════════
