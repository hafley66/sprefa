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
algorithm(tsv2_ts_emit,    ast,         rewrite,           'compile/emit_ts.pl (backend #1 over lowered/8; emit_rust.pl plugs the same plan via compile_fixture/4)').
algorithm(tsv2_surface_dcg, ast,        rewrite,           'compile/parse_dl.pl + print_dl.pl (phase D, LANDED: DCG is the CANONICAL parser; langium was stopgap; compile_dl6/2 is the text door)').

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
%   Tier 0 and NOT a new lowering: 0_coalesce_expand.pl (expansion phase 45)
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
construct(host_decl,            t5, new).   % sh_decl/4, EXPLICIT input->output split
construct(query_form,           t5, new).   % query/1, `? rel(args)` (top-level only; RHS probe + @ salt riders REMOVED by v6.2 host-surface locks)
construct(ts_query_value,       t5, new).   % ts_query/1 compiles to exact query text
construct(latest_sample,        t5, new).   % replacement spelling for the killed only()

construct_status(kept).
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
covers('2_hosts_wiring',          query_form).         % query/1 + bind_decl/2 x7
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
fork('2026-07-29', pre_lowering_premise, refusal_with_measured_receipt, force_sampled_read_lowering,
     "coordinator's dispatch premise (pre = sampled read like latest) was WRONG -- agent measured pre-as-sampled projecting [1,1] where the oracle pins 2; pre is a chained mid-tick read through ordered occurrences; the honest refusal + receipt beats a silently-wrong lowering, and pre_occurrence_loop is now a priced arc instead of a hidden defect").
fork('2026-07-29', tick_alignment_tier, opus_worktree_fourth_lane, sonnet_or_wait_for_user,
     "runtime tick phase order vs the oracle freeze IS the semantics of the engine; a wrong ordering grades wrong only on join-storm shapes (exactly the flagship shape), so the misfire cost dwarfs the tier cost; flagship is gated on it and the night has hours left").
fork('2026-07-29', rig_coordinate_translation, referee_translates_coordinates, change_one_engines_output,
     "v5 prints path:line:col:kind through _txt views, v6 carries byte spans; the flow rig needed one key space. Translating INSIDE the classifier (pinned-corpus newline index, calibration receipt in flagship-flow-classify.py's header) keeps BOTH engines unbent -- a v6 line/col column would have been a program change made to satisfy a referee, and a v5 span column is not ours to add. Backtrack = the translation is 40 lines in one python file").
fork('2026-07-29', sweep_gc_false, disable_gc_for_the_one_shot_batch, restructure_the_sweep_into_chunks,
     "swipl 10.0.2 aborts the compile sweep with 'system error: Mismatch in up phase' (GC compaction), deterministically at the 88th collection once the corpus reached 163 fixtures; the same goal typed at the toplevel completes. gc(false) on a one-shot process whose peak RSS is tens of MB costs nothing and keeps the sweep shape; chunking would have changed the receipt's meaning to chase someone else's bug. Receipt in sweep.sh:26. Upstream-shaped, see task swipl_gc_abort").
fork('2026-07-29', bind_spelling_filed, file_the_slot_and_keep_running, rename_mid_rig,
     "`:=` is nobody's word (vocabulary law) and terra typed `=` first, which dies as unbound_head_var; both are real. Renaming mid-flow-rig would have churned registry + parse/print + engine op + 16 fixture files inside a lane graded on row counts. Filed as SLOT-BIND-SPELLING with the `=`-must-refuse-by-name half attached; backtrack = the rename stays mechanical").
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
%   sweep (artifacts)    163 swept: 102 compiled / 61 named refusals;
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
task(clock_check,         active, [tick_model]).          % USER RULED 2026-07-30: label ring/sign/grade dependencies, then infer clocks. Phase-5 value rulings: bool is INTEGER NOT NULL CHECK(value IN (0,1)), canonical true/false boundary; float is finite SQLite REAL/binary64 with exact comparison/join and shortest round-trip output, no epsilon. Implement historical bug-class replay gate before declaring checker complete.
task(phase5_ingest_binary, done, [v6_2_ts_closeout]). % LANDED 2026-07-30. v6/dl ingest resolves executable DL_EXTRACT_BIN override, then in-tree release extract, then one shared cargo release build; removes absolute debug-worktree default. Typecheck PASS, focused ingest 10/10, full DL 96 PASS/1 explicit skip. Release bad.ts census updated from stale 79 to 82 records. No language syntax/type/host change.
task(extraction_live_p2,  done, [runtime_bridge_p1]).      % LANDED 2026-07-29 night (opus worktree, 6 commits, coordinator re-ran everything in worktree AND merged main): watch bind on node fs.watch behind IWatchSource seam (zero deps; events collapse to watch(glob,path,digest) rows w/ arrival SIGN -- rename = -old/+new same batch, atomic save = digest change, identical bytes = zero delta; bufferTime(100ms) never debounce); enumerate/enumerate_at = git ls-files pathspec (tracked-only, node_modules never walked; ls-tree takes NO glob, oids via rev-parse); extraction host = generic sh host, demand (path,digest) content-addressed, in-tree RELEASE extract bin (cargo --features cli REQUIRED). LIVE DEFECT FIXED: __host_response_* keyed on witness digest alone lost all but last row of multi-row answers -- ordinal:int column, key (witness,ordinal), fail-first receipt, oracle+emitter same-arc. EXIT RECEIPT extraction-live.sh 8 phases HOLDS incl kill -9 exactly-once. STANDING: sg_pattern refusal untouched; queryPlans emitted-not-executed; watcher restart+delete gap CLOSED 2026-07-29 morning (sol lane, merge ea938d6c: boot reconcile = engine rows vs git ls-files at subscribe, one boot batch, lastDigest seeded; A12 one-shot crossing sanctioned + recorded in the 2_binds.ts header); one-host-decl = one record shape (two shapes of same file spawn twice; fix named = (path,digest,family) content cache)
task(memory_soak,         done, [runtime_bridge_p1]).      % LANDED 2026-07-29 night (sonnet worktree, coordinator re-verified everything incl sabotage red): GET /stats (ServeStats: IServeStats; PRAGMA page_count/page_size/freelist_count + ONE grouped dbstat statement via json_each bind, forkJoin on the existing seam; dbstat PROVEN available on @libsql 0.17.4) + memory-soak.{sh,ts} (keyed-replace + log-keep(count) + derived edge churn, 2500 ticks; rss/heap/page-count/stmts-per-tick flat asserted on quarter means, sabotage keep_all goes RED exit 1). STEP-0 FINDING: rust has NO sqlite3_status wrappers -- v5's whole surface is db.rs rel_stats dbstat sums + health.rs PRAGMAs; tsv2 mirrors exactly that. justfile memory-soak recipe wired by coordinator, added to green-all. Banked finding: tests/serveHelpers startServed retains all events by design (fixture replay), a false-positive growth source at soak scale -- soak uses a private non-retaining subscribe (run-fixture precedent, outside the one-subscribe scan)
task(prolog_org_refactor, done, []).                       % LANDED 2026-07-29 night (opus worktree, 12 commits, ALL 10 review ranks, coordinator re-ran everything in worktree AND merged main): prolog-lint gate ratcheted baseline 1 (in `just green` now, coordinator wiring); emit_ts collision renamed; 0_body_walk.pl walk_body/3 (10 sites); 0_program_check.pl (6 mirrored checks one impl + BOTH engine-only holes closed compiler-side w/ fail-first receipts); 1_expansion.pl declared phase order + enum context (analyzer double-expansion gone, spread phases are placeholder rows); expression operator inventory; R7 14 dead exports removed of 44 classified; R8 private sites 10 -> 1; R4 oracle aggregate classification on registry axis (oracle stays wider). Battery: conformance 137/0, plunit 124/124, TEXT_DOOR 72/72/0, sweep 72/70/0-wrong both modes, green exit 0. DELIBERATE moves: compiled+TEXT_DOOR 73 -> 72 (log_without_retention emitted a module the oracle rejects -- checked-in gen module DELETED), conformance +aggregate_in_edge_head_rejected. Journal: plans/2026-07-29-prolog-org-refactor-journal.md
task(org_banked_findings, done, [prolog_org_refactor]). % 4 findings banked in the org journal, each PINNED BY A TEST so drift is loud: (1) trigger_items/body_atoms misclassify next/combine/comparisons/lifecycle wrappers as relation atoms; (2) goal_rel_refs reports next/1+combine/2 as positive refs; (3) finalize_in_level_rule diagnostic drift + both doors accept not(finalize(...)); (4) 3 private cross-module calls in sprefa-store/bench/v1-scale-gen.pl outside the lint gate's load set. Fix wave = one small lane, unowned
task(watcher_buy_research, done, []).                      % LANDED 2026-07-29 night (merge 8b0b49a8): plans/2026-07-29-watcher-buy-research.md. VERDICT @parcel/watcher first (native batch callback matches engine.submit(IArrivalBatch) one-tick commits, ignore-filter below JS, prebuilds all platforms, MIT, 31M weekly dl); node fs.watch = zero-dep fallback (IS chokidar v4/v5's mac/win backend); watchman = optional backend upgrade later, not the buy. Open residuals in doc: node ignore walk-vs-receive time on linux, watchman atomic-save, parcel symlink default. Pick gets its fork/5 row at phase-2 dispatch
task(save_session_pl,     done, []).                       % EXECUTED at the 20260729.0 save: chat_log/20260729.0.*.pl = module session_20260729_0 with session/2, in_flight/4, landed/2, session_task/2, awaiting_user/2, ruled_this_session/2; consult-verified. Convention: every save now emits the .pl sibling; load-session can consult it
task(edge_body_constructs, done, [latest_edge_sample]).    % LANDED 2026-07-29 night (opus worktree, 4 commits, coordinator re-verified; sweep 72/70 -> 82/80 identical, 0 wrong, TEXT_DOOR 82/82/0, plunit 134/134): negation/comparisons/binds guard seam + now/1 emitted tick counter + edge-head column typing from feeding bodies. Refusals removed: edge_body_needs_{negation,bind,comparison,now} + edge_head_column_type_mismatch; added: edge_body_with_negation (not/1 beyond one plain atom), edge_body_with_now, now_in_level_rule (compiler-only, oracle solves it), edge_body_joins_arrival_fed_level (SEE tick_phase_alignment). Fallout fixes: analyze seeded_refs Initial-only refs silently dropped final-state rows; print_dl type synthesis keyed off missing col_type (48 dl_view regen). Receipts in SCOREBOARD.md
task(tick_phase_alignment, done, [edge_body_constructs]).  % LANDED 2026-07-29 night (opus worktree, 2 commits, coordinator re-verified worktree AND merged main; sweep 82/80 -> 85/83/0-wrong both modes, TEXT_DOOR 85/85/0, plunit 137/137): (a) mid-tick level plane frozen where engine.pl freezes it -- recomputeLevelsBeforeEdges shares the emitter's 5 supportSql, naive recomputes before AND after edges; edge_body_joins_arrival_fed_level REMOVED, clock_rel_join_storms byte-identical (was 3-vs-1). THE FLAGSHIP SHAPE COMPILES. (b) separate __departure_frontier_<rel> TEMP table per listened rel (sign-column alternative rejected: touches every rel's DDL; receipt = all 83 prior modules byte-identical); departures in carryPending; finalize-in-edge flipped. HOLE FORCED SHUT en route: flipping the finalize registry row deleted the generic refused-goal catch, compiler ACCEPTED finalize-in-level -- finalize_in_level_rule restored in analyze shared_refusal order, drift became an agreement test
task(c7_durable_carry,     unbuilt, [tick_phase_alignment]). % match-frontier C7 INHERITED by the departure frontier (measured, departureFrontier.test.ts non-vacuous probe): frontier + departure TEMP tables die with the connection, kill -9 loses staged carries in BOTH implementations incl the oracle-side Ti carry. Endurance-law violation, whole-carry-set durability = own arc
task(gen_staleness_gate,   done, []).                       % LANDED 2026-07-29 morning (sonnet worktree lane, coordinator review-merged): v6/tools/staleness-gate.sh in green-all -- gen half enumerates gen_emitted modules absent from manifest compiled set, regenerates via compile_dl6.sh + diffs (door-handwritten covered, unknown provenance = named FAIL); binary half fails when src .rs/Cargo.toml newer than existing target/release/{dl,extract} (missing binary = pass, receipt scripts own building). Sabotage receipts in script header. FIRST MAIN-TREE RUN CAUGHT A REAL ONE: extract binary predated terra's --resolve types.rs merge, gate red -> rebuild -> green. Agent also found+fixed a pipefail SIGPIPE race in its own draft (grep -q closing early)
task(flagship_callgraph,   done, [tick_phase_alignment, extraction_live_p2]). % LANDED 2026-07-29 night (opus worktree, 2 commits, coordinator re-ran the rig + full battery): examples/callgraph-ast.dl BYTE-UNMODIFIED vs its v6 port over a pinned 13-file rust corpus; flagship-callgraph.sh + flagship-classify.py; every diff row bucketed, 0 expression gaps, 0 defects (def +10 = function_signature_item, call +181 = method/path/struct-literal sites v5's bare-identifier ast query cannot match; calls/unused proven by RULE FIDELITY -- v5's rule bodies run against each engine's own inputs); unused inverts as the anti-monotone rel must. 2 fixtures promoted (conformance 139/0, sweep 87/85/0, TEXT_DOOR 87/87/0). In green-all as `just flagship`
task(flow_interproc_port,  done, [extract_resolve_flag]). % UNBLOCKED then PORTED 2026-07-29: portable callable-plane port merged d322c93f (terra) with every gap named, value-plane rewrite merged 837fe7f2, rig grading ed81cdc6. The program is v6/dl/fixtures/flagship-flow.dl6 and it runs against v5's own std/flow.dl output on the pinned corpus (`bash v6/tsv2/scripts/flagship-flow.sh`). PORTED IS NOT PARITY -- see flow_parity_residue for the three open columns. closure/reaches over the extraction feed is graded here as flow_reach (9112 matched); the general closure() spelling still rides the graph-algo queue item
task(extract_resolve_flag, done, [flagship_callgraph]). % LANDED 2026-07-29 morning (merge 17778bbb + 0_prolog ledger refresh c26b4e0e, codex terra, worktree+branch removed); the staleness gate then caught the stale extract binary twice on the same day, which is how the merge was proven live. Original brief text follows. USER-WAIVED extractor touch, smallest correct: wire the EXISTING library-tested Resolve pass (tests/0_prolog.rs:70-95 is the exact recipe -- def index over all files, ProjectCx, per-file resolve) to a project-mode CLI entry emitting FLAT resolved-edge JSONL + a CLI-level golden test pinning the phase-2 contract that never existed (bin was phase-1 BY DESIGN, extract.rs:176; lib tests covered resolve; nothing asserted bin-vs-lib capability parity -- that asymmetry is the lesson). Brief plans/2026-07-29-extract-resolve-flag-brief.md. DISPATCHED codex terra, no-commit flow
task(extra_drain_tick,     done, [tick_phase_alignment]).   % FIXED 2026-07-29 morning (coordinator, main tree): recomputeLevelsAfterEdges refCount branch staged reconcile re-INSERTs into nextFrontier phase 1 (the P3 shape its own aggregate-branch comment had already called out as the unfixed asymmetry) -> promoteFrontiers read them as carry -> one empty drain. Fix = frontier copies [] on the reconcile call, matching the aggregate branch's afterEdges=false rationale (reconcile = same-closure correction, never post-write growth). Fail-first receipt in tests/extraDrainTick.test.ts: truncated 4-tick callgraph_unused schedule -> 5 lines w/ {"tick":5,"deltas":{}} red, 4 green; full-schedule oracle byte-identity guarded in the same file. Sweep both modes 87/85/0-wrong zero movement, tsv2 58/0/1skip
task(cq_bundle_lane,       done, []).              % LANDED (merge 733d4c1b, coordinator re-ran conformance 158/plunit 138 in-worktree AND full battery on merged main: sweep both modes 98/96/0-wrong, TEXT_DOOR 98/98/0, roundtrip, staleness gate, LSP DIAGS HOLDS w/ 0+0 workaround REMOVED): groupby literals emit (N+0) (8 diag modules moved as predicted); probe guards fixed by GOAL PLACEMENT in host expansion (pre-probe goals -> demand rule, post-probe guards after response -- root cause, not a bound-set widen); 4 drift pins flipped; 0_refusal_messages.pl ONE umbrella prolog:message//1 over 77 dynamically inventoried refusal signatures + coverage test. Residue: locations say rule-index unavailable (parse_dl keeps no source positions). Was: codex sol, base 67719352, ../sprefa-codex-cqbundle: carries emitter_groupby_literal + probe_output_guard + org_banked_findings + b4_refusal_messages (prolog:message//1 umbrella w/ coverage test) IN ORDER; brief plans/2026-07-29-compiler-quality-bundle-brief.md; phase 5 + struct_host_output_seam both QUEUE BEHIND this lane (shared compile files)
task(crawl_bench,          done, []).                   % LANDED 2026-07-29 (merge a192cd35, codex luna, review-gated; coordinator re-ran the bench itself and matched luna's stmts/tick 54.03 exactly). v6/tsv2/CRAWL-BENCH.md + scripts/crawl-bench.sh, hermetic scratch, ~/orgs/grafana read-only, nice -19 (the managed host REFUSED the niceness and said so), NOT in green-all. THE NUMBER: v5 org-fan 42,739 files / 389 repos / 12.07s = 3,540.9 files/s; v6 served extraction 779 files / 8 repos / 19.15s = 40.7 files/s. That is ~87x on the same machine, and the v5 memory-doc 7,244 files/s (5.9s) is a SECOND yardstick this run did not reproduce -- the honest v5 number on this host today is 3,541. Stated gaps in the doc: v5 reads a git tree at HEAD, v6 hashes the working tree; v6 has NO org fan-out spelling at all (the shell loop supplies it, one served process per repo); v6 runs cst+type+call+df families where v5 does a scan fact. 250 of 389 repos are usable for v6 (139 have no go/ts/tsx); the default --max-repos 8 exists because the linear projection of the full corpus is 1,050s
task(flow_parity_upgrade,  done, [struct_host_output_seam]). % MERGED 2026-07-29 (837fe7f2, terra's ccfe53ec) once the seam opened, then GRADED the same morning (ed81cdc6): df hosts + arg->param positional hop + sig-owner joins + 2 fixtures (5_flow_value_plane.pl). FOUR-QUERY TABLE, first real-seam contact: flow_edge v5 2462 / v6 2184 / matched 2184 (v6 is a strict subset, 278 v5-only); flow_reach 9112 matched + 177 v6-only reflexive rows; flow_param_type 0 matched; flow_node_type EMPTY v6-side. Three fixes the contact forced: `= concat` -> `:=` at 7 sites (`=` refuses under the WRONG name, filed as SLOT-BIND-SPELLING), smaller_type_owner was missing its df_value join (the compiler refusal was CORRECT), and the referee learned to translate coordinates (fork rig_coordinate_translation). Residue = task flow_parity_residue
task(struct_dictionary_gc, unbuilt, [struct_as_rows]).     % SLOT-GC-TIMING named debt: dictionaries are MONOTONE (unobservable in tick log -- collected and uncollected print identical bytes; edge-2 receipt). Zero cost for churning programs (content-addressed reuse); real growth only for ever-new distinct values (one row ~= canonical JSON twice + flat columns). Collecting = refcount across arrivals/level heads/edge heads/frontier/retention (arrival-only count deletes rows a derived row still renders). Own arc
task(struct_host_output_seam, done, [struct_as_rows, cq_bundle_lane]).  % LANDED 2026-07-29 (merge 265da55f, codex sol, review-gated; in-worktree battery conformance 160/0, plunit 140/140, sweep both modes 99/97/0-wrong, TEXT_DOOR 99/99/0, tsv2 69/1skip; merged-tree sweep 100/98/0): decl-B host OUTPUT columns admit declared type names via host_output_columns (WRAPPER spellings keep the column_type_wrapper refusal, unknown names stop as column_type_unknown); serve carries host stdout across the arrival seam as JSON text and StructPlane decodes text for ref columns before shape-check + intern; IHostColumnPlan.type widens to string and is refused pre-emission by the shared type-plane check. Fail-first receipts both directions in 4_struct_values.pl. UNBLOCKED TWO ARCS: flow_parity_upgrade merged immediately after, and the comment lab's technique-1 payload destructure (which hit the same wall independently)
task(probe_output_guard,   done, []).                    % FIXED by the cq bundle (merge 733d4c1b) and the fix was NOT the one the row predicted: the diagnosis said "widen the bound set", the root cause was GOAL PLACEMENT in host expansion (pre-probe goals belong in the demand rule, post-probe guards after the response), so the widen would have papered it. Fixture probe_output_comparison_guard (5_compiler_quality.pl). Original symptom: a comparison guard over a probe OUTPUT var in a level rule refused as unsupported_construct(unbound_head_var(_G)) -- wrong name, no location, review-B4 in the wild. RESIDUE: locations still print "rule-index unavailable" because parse_dl keeps no source positions
task(cli_bop,              done, []).                       % LANDED 2026-07-29 night (sonnet worktree; agent left tree UNCOMMITTED-but-clean, coordinator reviewed file-by-file, ran every receipt itself, committed on the branch -- process deviation logged): registry cli_command/3 inventory -> commander bop.ts, verbs serve/run/check/load/q, run+check boot serveTsv2 IN-PROCESS (no daemon), exit contract 0 clean/2 named-refusal/1 broken verified by coordinator runs (ghcacher = 2 w/ recursive_stratum receipt). 12 tests + inventory-parity (swipl vs commander lines); one dep commander (user-required); one-subscribe 1/1 both apps (cli/ = run-fixture exemption). halt-inside-catch swipl trap documented in bop_check.pl. THE 6.2.0 TAG GATE IS SATISFIED; push = user. LSP milestone still open on task 11
task(golden_flake_hunt,    done, []).                       % RCA LANE RAN 2026-07-29 morning (sonnet worktree, diagnostics merged): REPRODUCED 1/18 sub-runs under 3x-concurrent full-suite load -- all 11 subtests print pass yet the FILE fails with bare 'test failed', ZERO error payload (native/process-level signature, not a JS assertion). Ruled out: tmp paths/ports (all :memory:, zero fs), memcap singleton (process isolation confirmed), RSS budget (peak 186MB vs 512MB, rss subtest passed in the failing run). Leading candidate unproven: ~40 unclosed :memory: @libsql clients per run. Landed: additive exit-listener diagnostics (pid/exitCode/rss/storesOpened printed on nonzero exit only) so the next occurrence carries context. Rate too low (5.5%) to validate any fix inside a lane
task(reactor_buffertime_flake, unbuilt, []).                % NEW, found by the golden RCA lane: v6/sprefa-store/js/tests/labs/reactor.test.ts "reactor A file+folder coalesce" is a MORE frequent real flake (6/18 under 3x load; AssertionError actual [1] vs expected [1,2,3]) -- wall-clock bufferTime coalescing assertion, the same class F3 killed in v6/dl with TestScheduler/virtual time. Fix shape known; small lane, unowned
task(lsp_diags,            done, [extraction_live_p2, cli_bop]). % LANDED 2026-07-29 night (sonnet worktree after 2 coordinator continue-nudges, committed 77b5bbce, coordinator re-ran receipt: LSP DIAGS HOLDS): ZERO new LSP code -- diag-rail.dl6 declares rel diag_v5 in v5's exact 9-column shape and tsv2's bare-name tables (lower.pl table_name) make it THE table src/lsp.rs:545 selects; bridge fully in-language. Real v5 dl --lsp --diag-db over real stdio JSON-RPC: publishDiagnostics appear + retract for no-eval + unused-def rails over the live watcher/extraction feed. Line numbers HONESTLY 0 until decode_arc lands spans. Sabotage: column rename passed engine-side (positional curl) and went red at the real LSP client. In green-all as `just lsp-diags`
task(emitter_groupby_literal, done, []).                 % REAL EMITTER DEFECT (lsp arc), FIXED IN TWO PASSES: a rule head with >=2 bare integer-literal columns reaches the GROUP BY verbatim and SQLite reads a bare integer there as a POSITIONAL column ref -> SQLITE_ERROR "Nth GROUP BY term out of range". Pass 1 (cq bundle, merge 733d4c1b) wrapped literals as (N+0) on support_group_exprs and REMOVED the 0+0 workaround from diag-rail.dl6; 8 diag modules moved as predicted. Pass 2 (coordinator, 6522f848, from the altitude review's finding 1) found aggregate_group_exprs -- the scoped-delta insert + recompute path -- still emitting bare integers, and gave both sites ONE shared group_expr/3. Fail-first fixtures: groupby_two_bare_integer_literals and groupby_aggregate_two_bare_integer_literals (5_compiler_quality.pl), each RED with the SQLITE_ERROR and GREEN oracle-identical both modes. THE LESSON WORTH KEEPING: the first fix was verified by a fixture that only exercised one of the two call sites
task(v5_lsp_exit_hang,     unbuilt, []).                    % v5 DEFECT disclosed by the lsp receipt, reproduced standalone: dl --lsp --diag-db answers shutdown correctly then hangs after exit + stdin EOF; receipt SIGKILLs after both directions proven. Owner src/lsp.rs, v5 side. ESCALATED 2026-07-29 morning: the receipt's SIGKILL does NOT reach every path -- coordinator found 3 hung dl --lsp processes ~4h old (one per lsp-diags run) and killed them; lsp-diags now rides green-all so EVERY battery run can leak one. Until the v5 fix, lsp-diags.sh owes a belt-and-suspenders pkill of its own spawned pid on exit. MECHANISM CONFIRMED by the audit 2026-07-29: the script's stop_all trap kills DRIVER_PID, but DRIVER_PID is the PYTHON driver (lsp-diags.sh:239, scripts/lsp_diag_driver.py) and `dl --lsp` is its CHILD -- kill -9 on the parent orphans the binary that is already refusing to exit. Still unfixed at b535ca62 (zero pkill in the script), and lsp-diags is in green-all, so every full battery run can still leave one
task(pre_occurrence_loop,  done, [edge_body_constructs]). % LANDED 2026-07-30. Ordered occurrence execution snapshots each referenced rel once into __pre_<rel>, processes arrivals/departures in sequence, applies each accepted keyed write before the next occurrence, recomputes levels, and stages carry through existing frontiers. No surface or type construct. 13 former edge_body_needs_pre fixtures compile; focused runtime 2/2 PASS; PLUnit pins one snapshot plus mirrored writes. The earlier higher_order_scan gap receipt was false-positive because catch/3 left its error variable unbound on success.
task(decode_arc,           closed, []).                    % SUPERSEDED 2026-07-29 morning by ruling compound_storage = struct_as_rows: there is no blob to decode; destructuring becomes joins. See struct_as_rows
task(struct_as_rows,       done, [tick_phase_alignment]).  % LANDED 2026-07-29 morning (opus worktree, 6 commits, merge dcfa6fcc; coordinator resolved manifest/run-results by regen, re-ran EVERYTHING on merged main: conformance 156/0, sweep BOTH modes 95/93/0-wrong, TEXT_DOOR 95/95/0, plunit 137/137, roundtrip ALL PASS, tsv2 65/1skip, dl 96/1skip, store 74/74, staleness gate OK, 0_prolog ledger 54->56): type name(col: type) decl (SQL word, NOT rel -- edge 2 forced it: a rel-spelled struct makes its dictionary nameable), intern-at-arrival dictionaries w/ __semantic full canonical text (no UDF hash exists) + __rendered memoized JSON, boundary render join (EXPLAIN: rowid SEARCH), decode/2 = sugar lowering to dictionary join via rule rewrite across all 7 level families, spans = declared struct (host columns accept type names). BOTH EDGES GRADED: values-never-ids (intern_order_a/b different dense ids, identical bytes both orders) + dictionary boundary-invisible (structural: dict relplans reach body compiler only). Migration receipt STRONGER than header: inline twin emitted log NEVER matched oracle (obj-term text vs canonical JSON) -- ref side byte-identical, migration is the fix. +16 fixtures (7 identical/9 named refusals); 2 json fixtures -> sharper decode_source_not_struct; 9 temporal_pipe compound-destructure stay (SLOT-TERM-STRUCT: prolog compound is NOT a struct spelling). Defect fixed en route: sweep.ts final-state encoder had drifted from ticklog canonicalization -- one TickLogEmitter.valueText now. Slots for user: SLOT-ARRIVAL-CANONICAL-ORDER (oracle refuses non-sorted keys; lifting = absorb_arrivals canonicalization ruling), SLOT-GC-TIMING debt + host-output seam = rows below

% ── rows added by the world-health audit, 2026-07-29 midday ──────────────────
% Every row below was verified against the tree at b535ca62, not carried over
% from a report. Statuses: in_flight = a lane is running as this was written.
% The fable close-out handoff block (83538cef) is FOLDED IN HERE: where it and
% the audit filed the same task under two names, the handoff name wins
% (amplification_sensors over storage_amplification_sensors,
% doc_format_extraction over extract_doc_formats) and the two texts are merged.

task(file_span_redesign,   unbuilt, []).  % USER-DIRECTED 2026-07-29 evening, plans/2026-07-29-file-span-design.md: a span is a FILE_SPAN -- it references its parent file, carries a range, and its TEXT IS DERIVED, never stored ((digest,start,end) IS the text identity). The struct arc's free-floating `type span(start: int, end: int)` is the single hole behind four shipped warts: path riding as a sibling column beside every span; flagship-flow.dl6 hand-building identity with concat([path,':',start,':',end]) (that concat IS the missing file reference); comment rails shelling out to grep because text lives nowhere; line/col living in a python referee translator while LSP line numbers ship as 0. line(span)/col(span) derive through a per-digest newline index IN-LANGUAGE; slice(span, from, to) is sub-range projection (NOT destructuring -- the running assign lab was briefed on the wrong reading). Extractor untouched: the wire keeps bare byte ranges and the HOST BOUNDARY pairs each record span with the demand's file. 3 user decision cards open in the doc
task(file_span_storage_lab, labbed, [file_span_redesign]). % COMPLETED 2026-07-29, plans/2026-07-29-file-span-storage-lab.md + executable/raw results in v6/sprefa-store/bench/file_span/. REAL CENSUS: release extract over 1048 tracked source files, 0 failures, 7,345,805 refs / 2,073,233 distinct spans = 3.543 refs/span; 99.0% have >=2 refs, 61.5% >=3. SELECTED physical row: file_span(file_span_id,rev_file_id,start,end), facts carry one dense id; no blob_span table. Real-distribution replay, 405,696 facts, two fresh-process runs: selected 32.24 bytes/fact and 517.6/530.5ms ingest; two-level located/content span 38.30; content-span ref 43.89; embedded coordinates 51.95; repeated TEXT 315.31. Selected filter + reverse placement paths are covering SEARCH only. PATH VERDICT over 2996 real paths x20 refs: whole-path dictionary 954,368B / 0.0385-0.0387ms prefix search; segment+junction 1,146,880B / 0.0573-0.0588ms; repeated TEXT 3,448,832B / 1.022-1.023ms. CONTENT VERDICT over 300 Git blobs/8.50MB x3 reads: persistent git cat-file --batch 58.25-58.88ms; optional SQLite stored_blob 12.85-12.88ms and 8.67MB; 1MiB LRU held at 1,048,564B, all newline indexes 212,892B. NULL-FREE shape: committed_rev and work_rev are total variant rels with ordinary union rules; git_blob and stored_blob are ADDITIVE capability rels, so a BlobId may have either or both. HOST verdict: reuse sh_decl/probe's one demand-response plan with a registered typed non-shell executor; bind_decl remains continuous discovery; no file-specific call syntax. TWO user cards remain before implementation: generic relation-reference column type (physical dense int, static FileRef/BlobRef/FileSpanRef) and typed-host authoring spelling. Existing enum tag projection stores TEXT and must use declaration-order ordinal when materialized or be omitted when match reads variant rels directly
task(rel_value_unification_lab, labbing, [struct_as_rows]). % USER-DIRECTED ACTUAL-WORLD LAB 2026-07-29, plans/2026-07-29-rel-value-unification-lab.md. `type` surface removed. CORRECTED MODEL: referenced rel remains an ordinary public/queryable rel; parent typed column stores an edge endpoint to the target row; one target table with hidden physical __id and content/key uniqueness; temporary __ref_<name> join view only; zero __dict_* tables and zero stored __semantic/__rendered JSON. 9-check actual compiler hole lab passes: public target, typed edge, one table, direct RHS query, indexed dereference, no FK/cascade, and confirms two holes. HOLES: existing key(...) is not yet the reference identity; old content-DAG check still rejects keyed entity cycles. Both use existing semantics and justify no new construct. Sweep: 102 compiled, 91 unchanged identical, 9 expected old nested-JSON oracle disagreements, 2 pre-existing run errors. NEXT: key-driven identity, keyed-cycle acceptance, oracle relation-row migration, delete StructPlane/dictionary naming
task(file_span_kernel_host_boundary_lab, labbed, [file_span_storage_lab, rel_value_unification_lab]). % COMPLETED 2026-07-29, plans/2026-07-29-file-span-kernel-host-boundary-lab.md. ACTUAL COMPILER: slicing, line count, and column anchor use existing arithmetic/count/max; byte substring has no pure expression. REL EDGE: typed relation constructor currently emits a JSON compound into an INTEGER endpoint; opaque row identity capture is unavailable; ref/1 is unregistered and emits another JSON compound. Therefore automatic key-driven edge construction is the next implementation, while ref remains unselected pending an opaque-identity receipt. STRINGS: 3019 paths x20, dedicated whole path 962560B/15.9417B per ref/0.0374-0.0381ms; universal strings+path 1036288B/17.1628B per ref/0.1472-0.1476ms. Across 200 extracted files, 39642 name occurrences and 6269 names had zero path/name text overlap; universal and separate dictionaries both used 1728512B. HOST: existing shell executor is one spawn per witness; span text must use registered batched execution behind existing demand/response rels, grouped by blob and repo, with no new DL6 spelling. SELF-HOST: relation joins, slice arithmetic, line/column aggregates, variants, and capability selection. EXTERNAL: Git/filesystem observation, byte acquisition, optional persistence, newline scan
task(rel_edge_clock_fixpoint, labbing, [file_span_kernel_host_boundary_lab]). % ACTIVE 2026-07-29, chat_log/20260729.4.rel-edge-clock-fixpoint.pl. ITERATION 2: make existing key(...) positions drive automatic typed relation-edge construction for single/composite keys; split keyed entity cycles from content-key cycles; add real SQLite receipts before changing syntax. ITERATION 3: pin target arrival, missing target, keyed replacement, retraction, dangling-edge antijoin, and exact oracle/emitter ticks. ITERATION 4: attempt opaque identity capture/transport through current variables and modes; ref remains unregistered unless a required actual-world case survives. ITERATION 5: extractor -> rev_file -> file_span -> batched span_text/newline provider vertical slice. LOCKED: rel-only declarations, public queryable target rel, ordinary target table with hidden dense id, integer parent endpoint, key defines semantic entity identity, no dictionary/stored JSON/NULL payload/cascade, demand-response provider grouped by repo/blob
task(key_edge_case_census, labbed, [rel_edge_clock_fixpoint]). % COMPLETED 2026-07-29, v6/prolog/labs/rel_value_unification/8_key_edge_case_census.pl, 12 PASS. READY: existing single/composite key positions already produce target-table UNIQUE constraints; typed parent is INTEGER endpoint. HOLES: key(0), key(arity+1), and duplicate positions survive planning; keyed self/mutual entity cycles inherit the content-DAG refusal; construction still consumes full target arity and emits JSON. CLOCK EDGE: positive keyed arrival replaces by key while negative arrival deletes the exact full row, so stale retraction does not remove the replacement. GATES before reference implementation: validate key positions, pin replacement/retraction ticks, and pin conflicting non-key fields for one key
task(key_position_validation, done, [key_edge_case_census]). % LANDED IN ACTIVE BRANCH 2026-07-29. Existing key(...) now rejects zero and above-arity positions as key_position_out_of_range(Ref,Position,Arity), and repeated positions as key_position_duplicate(Ref,Position), before DDL. Shared 0_program_check invariant, identical oracle/compiler refusal terms, 5 focused plunit receipts. Battery: plunit 147 PASS, conformance 163 PASS, key census 12 PASS. COST: no parser/SQL/runtime/surface change
task(keyed_signed_row_clock, done, [key_edge_case_census]). % LANDED IN ACTIVE BRANCH 2026-07-29, scopes.pl stale_keyed_retraction_keeps_replacement, oracle 3 expectations PASS and emitted SQLite 3 PASS. LOCKED CLOCK: +row may replace the current row sharing its key and reports -old/+new in that tick; -row retracts the exact row named, so delayed -old after +new is silent and cannot delete the replacement; -current removes it. This follows the existing signed-full-row event shape and adds no key-delete operation
task(reference_construction_contexts, labbed, [rel_edge_clock_fixpoint]). % COMPLETED 2026-07-29, v6/prolog/labs/rel_value_unification/9_reference_construction_contexts.pl, 8 PASS. Existing RHS target query already joins the target table but head construction discards available identity into JSON. Missing target constructor performs no existence join. Runtime INSERT OR IGNORE is key-constrained but lookup uses the full row, so same-key/non-key conflict finds no id. Key-only arity is currently just a compound term. Boot path queries removed __semantic columns absent from DDL. LEADING MINIMAL LOWERING: in derived rules, relation-shaped value is an indexed match against an existing public target row and projects its __id; missing target means no parent derivation; creating the target is an ordinary target-headed rule. World arrivals remain the boundary case: atomically resolve/assert target before parent and refuse same-key conflicts by name
task(existing_target_identity_prototype, labbing, [reference_construction_contexts]). % ACTIVE BRANCH 2026-07-29. When a positive body contains target(Id,Fields) and a relation-shaped head column contains the identical target(Id,Fields) term, lower now projects that already-joined public table alias's __id directly. COST: one compile-time term binding, zero JSON/subquery/extra SQL/hidden write/surface syntax. Receipts: construction contexts 8 PASS, plunit 147 PASS, sweep 164 total/103 compiled/61 unsupported/0 crash, runtime 92 identical/9 expected old relation-value oracle disagreements/2 recorded run errors. OPEN: direct edge trigger still lacks __id binding; constructor without explicit target atom still emits JSON; boot/world arrival still uses obsolete full-row interner and __semantic path
task(direct_trigger_identity_prototype, labbing, [existing_target_identity_prototype]). % ACTIVE BRANCH 2026-07-29. An arrival edge trigger that is a referenced public relation now samples its current target row by the trigger fields and projects joined __id. One indexed equality join; departure triggers do not join absent current membership. Construction contexts 8 PASS, plunit 147 PASS. NEXT separate hypothesis: automatically inject a target membership match when a relation-shaped head value lacks an explicit target atom; must grade recursive level fixpoint timing before landing. World/boot arrivals remain separate
task(automatic_derived_reference_match, done, [direct_trigger_identity_prototype]). % LANDED ACTIVE BRANCH 2026-07-29. New shared expansion phase 50, after match: relation-shaped level-head values add ordinary target membership; edge-head values add latest(target) to sample without another trigger. Dependency is visible to oracle, checker, stratifier, SQL planner, and emitter. Missing target yields no parent row. Clock receipt: keyed target edge creation and parent level membership settle same tick; target replacement emits -old-parent/+new-parent same tick. COST: no surface/runtime primitive. Battery: expansion plunit 150 PASS, conformance 164 PASS, fixpoint clock 6 PASS, construction contexts 8 PASS. NEXT world/boot key-driven batched resolution
task(clock_checker_proof_payoff, labbed, [tick_model, clock_check]). % SOL SHAKEDOWN 2026-07-29, plans/2026-07-29-clock-checker-proof-payoff.md. EXISTING STATIC PROOFS: negative/aggregate stratification, acyclic positive ordering for the emitted SQL subset, five named cross-plane refusals, declaration/key validity. EXECUTION-ONLY: general tick placement, glitch behavior, keyed batch order, host response timing, oracle/emitter equality. MISSING: registry ring/grade metadata, inferred clock expressions, labelled-SCC causality/productivity, external provider liveness theorem, boundary referential integrity. RANKED ZERO-SURFACE ITERATION: (1) project rule dependencies labelled ring/sign/grade from current AST and registry; (2) infer path offsets and reject unequal clocks; (3) accept only monotone-B zero-grade SCCs and positive-delay recurrence; (4) compile live-parent/missing-target as boundary antijoin; (5) expose proof facts for fixture comparison. Rust borrow checking supplies only the boundary-lifetime analogy because relation IDs are durable graph edges, not memory borrows. Lustre supplies clock compatibility/delay/initialization comparisons. Esterel supplies constructive same-instant SCC comparison. Four decision cards, each <=5 options; no new syntax selected
task(world_reference_key_resolution, done, [automatic_derived_reference_match]). % LANDED 2026-07-30. One rel/checker/table graph: nested rel-shaped ingress expands topologically into ordinary public target arrivals, then rewrites parent columns to dense integer endpoints. Target rows, parents, and target-dependent level rows publish inside one external tick through the mode's ordinary arrival path. StructPlane.intern returns IArrivalBatch; no special result type, ref construct, dictionary, stored JSON, or new syntax. Key conflict and stable-id rules remain. Gates: PLUnit 167/167, conformance 165/165, typecheck PASS, struct runtime 11/11 incremental + 11/11 naive, both sweeps 115 total/113 identical/0 wrong/2 known run errors.
task(ref_necessity_proof, done, [world_reference_key_resolution]). % LANDED ACTIVE BRANCH 2026-07-29, v6/prolog/labs/rel_value_unification/11_ref_necessity.pl, 7 PASS. No ref surface construct selected. Existing mechanics cover the cases: target scan binds dense __id; typed variable forwards it with no target rejoin; decode joins __ref_<target> when fields are needed; arbitrary graph cycles are rows in a separate edge rel with two entity endpoints. DEFAULT-PATH DEFECT FIXED: full recompute projected __id while incremental frontier emitted JSON; frontier now rejoins the current target row and projects __id. INLINE recursive entity columns remain type_cycle: relaxing them produced recursive render views and no finite full-value boundary representation. KEYED TARGET ID FIXED: arrivals use ON CONFLICT DO UPDATE instead of INSERT OR REPLACE, preserving __id and every stored parent endpoint across non-key replacement. Runtime receipt 11 PASS, plunit 150 PASS, sweep 101 identical/0 wrong/2 recorded errors
task(reference_membership_boundary, done, [ref_necessity_proof]). % IMPLEMENTED 2026-07-30 using the already locked one-rel normalization model. Nested wire values become same-tick ordinary target arrivals before parent reference rows. New relation_reference_target_and_parent_share_tick fixture proves target, parent, and target-dependent level addition at tick 1. The former delta-silent membership exception and identity-catalog split are closed.
task(schema_import_epic,   unbuilt, []).  % USER: TypeSpec / JSON-Schema / OpenAPI v3 (json + yaml) import as its own epic. Prior art is ours already (prior_art hafley_tsp: TypeSpec app-gen, config/env/CLI sources, @secret redaction). Build-vs-buy law applies before any bespoke line. Reads on doc_format_extraction for the yaml/json halves
task(world_health_reconcile, done, []).   % THIS AUDIT (opus worktree, 2026-07-29 midday): full ARCH reconcile + plans/2026-07-29-v6-world-health.md; owned resolving the fable handoff block into the reconciled file, which is what the rows around this one are
task(flow_parity_residue,  active, [flow_parity_upgrade]). % ADVANCED 2026-07-30 with existing facts only. CallF node rows plus TypeF owner-start joins and lambda exclusion restore flow_node_type: V6 58 rows/33 matched, previously empty. Referee normalizes root:: symbol keys, method owner prefixes, and root-qualified types; flow_param_type 35/40 matched. Direct flow remains 2457/2457 exact. Total flow_edge: V5 2772, V6 3114, matched 2654, V5-only 118 (down from current baseline 315). BLOCKER is Rust call-target resolution evidence: targets V5 200/V6 168, matched 113, V5-only 87, V6-only 55. Current resolver picks first same-file/unique-blob definition while V5 retains qualified method targets and omits some contexts. Close with improved typed Rust resolution facts or a pinned SCIP index, not another DL rule. Classifier now fails on direct drift or empty/nonmatching node types.
task(fork_join_malformed_json, unbuilt, []). % REAL EMITTER DEFECT hiding in the sweep's "2 pre-existing run_error" bucket. fork_join_error_arm_is_a_value (operators.pl:48) has a FULL oracle tick log (out/fork_join_error_arm_is_a_value.oracle.jsonl, 2 ticks) and the emitted module dies with SQLITE_ERROR: malformed JSON. The other run_error, log_retraction_rejected, is a genuine rejection-path fixture where the oracle throws too -- these two are NOT the same class and the scoreboard's one-line bucket hides that. Shape: compound arrivals ok(body_one)/error(502) matched in a level body, i.e. the SLOT-TERM-STRUCT family. Owner unassigned
task(comment_rail_wiring,  unbuilt, []).   % the comment lab landed GRADED (745/745 vs v5) but promoted nothing: 4 fixture/5 candidates + 2 receipt programs live only in commit 9b5ba958. Wiring = promote the four fixtures (comment_witness_gates_a_scanner_hit, disable_next_line_shifts_the_effect_by_one, unused_suppression_antijoins_the_finding, arch_hierarchy_from_decomposed_marker_rows), port techniques 3/4/6, and write the block-range pairing rule (std/suppress.dl:135-149's nearest-enable argmax is ordinary datalog over rows the host already emits -- WORK, not a gap). The lab's byte-span flattener half is SUPERSEDED by file_span_redesign; its grep-host half demotes to an optimization once slice(span,..) exists. Verdict: plans/2026-07-29-comment-node-verdict.md
task(text_expression_parity, unbuilt, [comment_rail_wiring]). % BOUNDED V5-PARITY CLEANUP, user-ruled 2026-07-29: preserve the working switch model and do not open a switch_map syntax/runtime redesign. Add text operations only when a V5-parity or golden-use-case program proves the missing operation. Census already measured 57 V5 text-operation sites across =~, replace_re, trim, split, json, match_line, and match_ast; V6 registry currently has zero writable text operations. Each admitted operation lowers to SQLite when the loaded SQLite capability has an exact semantics receipt; otherwise it uses the existing typed host demand/response boundary over batched deltas. File byte slicing stays with file_span_redesign's blob/newline provider, not SQLite character substr. Exit receipts: motivating program moves from named refusal to oracle-identical, SQL path has generated-SQL/EXPLAIN evidence, host path has batching and clock receipts, zero speculative string-method surface.
task(higher_order_rel_scan, superseded, [pre_occurrence_loop, clock_check]). % RECONCILED 2026-07-30. Its ordinary-rel specialization and switch-cache evidence remains historical input, but its scan compiler gap receipt is false-positive: catch/3 succeeds with the error variable free, then the receipt unifies that variable with edge_body_needs_pre. Current ordered-pre planning/lowering succeeds. RuleRef, scan_signature, runtime relation values, and parallel signature type systems are excluded by the one-rel lock.
task(v6_completion_drive, active, [higher_order_rel_scan]). % USER GO 2026-07-29. Consultable task graph: plans/2026-07-29-v6-completion-drive.pl. Three parallel lanes: ordered_event_scan production implementation; ghcacher deterministic tick golden; extraction deterministic tick golden. Follow-ons: queryable clock signatures, checker-visible switchMap expansion, scan specialization, combined golden gate. No surface spelling in parallel lanes. Coordinator owns integration, shared generated files, commits, and push.
task(match_left_to_right_surface, done, []). % LANDED 2026-07-30 from USER RULING 2026-07-29. Optional leading semicolon; Guard |-> Head is the existing level arm and Guard |+> Head the existing event arm. Parser/printer migration only; 0_match_expand erases both into ordinary Head <- Source,Guard or Head <+ Source,Guard rules. 167/167 PLUnit PASS. Roundtrip G2 real files parse with zero findings; G1 remains the known 147/164 with 17 removed-type not_variant fixtures.
task(scan_match_value_lab, superseded, [higher_order_rel_scan]). % RECONCILED 2026-07-30 by plans/2026-07-30-scan-match-reconciliation.pl. Its 0/1/N cases remain evidence; RuleRef, scan signatures, anonymous relation results, and surface proposals are excluded. Current scan candidates are semidet ordinary keyed edge writes: zero silent, one write, differing N keyed_conflict, equal N dedupe.
task(rel_definition_hash_lab, canonical_plan, []). % COMPLETED 2026-07-30, plans/2026-07-30-rel-definition-hash-lab.pl + v6/prolog/labs/rel_definition_hash/0_receipts.pl, 11/11 PASS. Separates semantic definition hash, storage instance identity, and rendered SQL binding; recursive SCC renames preserve content identity while schema/body changes invalidate it. No production integration or surface decision. One open canonical-label backend card remains.
task(scan_instantiation_generics_lab, closed, [higher_order_rel_scan]). % RECONCILED 2026-07-30. Receipts measured 0 generated persistent tables and 1 TEMP pre table. scan_signature and relation-role metadata remain lab-only and are excluded; named event/state/init/reducer rels remain ordinary rules.
task(select_scan_cache_lab, canonical_plan, [scan_match_value_lab]). % RECONCILED 2026-07-30. Ordinary-rel switch-cache, stale-result gate, and delayed-effect graph remain a golden algorithm plan. No runtime rel value, new syntax, or production construct selected.
task(v6_2_ts_closeout, active, [v6_completion_drive]). % USER GO 2026-07-30. Canonical live ledger: chat_log/20260730.0.v6-2-ts-closeout.pl. Remove RHS `?` host probes and `@ salt` riders; ordinary registered host rels lower through target-neutral contracts carrying typed inputs/outputs, identity projection, clock transition, and executor key. TS executes V6.2; Rust consumes the identical checked plan in V7. Extraction, ghcacher, parity, clock/type, and golden gates are exit conditions.
task(v6_2_host_contract_cleanup, done, [v6_2_ts_closeout]). % LANDED 2026-07-30. RHS host calls use ordinary relation spelling; RHS prefix/postfix question-mark and at-sign salt riders refuse. Registered exact positional input_roles preserve identity versus freshness inside existing probe IR. Checked host_plan/7 carries executor key; TS HostRunner dispatches shell or sprefa_extract. PLUnit 167/167 and conformance PASS; extraction clock 3 ticks PASS; ghcacher clock 5 ticks/final PASS; served-host focused 2/2. No new host syntax. Shell overuse remains measured in the scale-gate row.
task(extraction_host_batching_lab, labbed, [v6_2_host_contract_cleanup]). % COMPLETED 2026-07-30, plans/2026-07-30-extraction-host-batching-lab.pl + v6/prolog/labs/extraction_host_batching/0_receipts.pl, 9/9 PASS. Current -> grouped extractor runs per N path/digest: callgraph 2N->N, diagnostics 2N->N, flow 1+7N->1+N; golden V unchanged. Release sprefa-extract proves --family df,call,type equals three separate runs. Smallest internal route groups same-tick fixed extractor plans by executor/template/path/digest/ordered inputs, runs once, projects heterogeneous stdout through existing typed output schemas, and settles existing witnesses separately. Ordinary response rel rows only; no syntax, JSON storage, or second type model.
task(extraction_host_batching, done, [extraction_host_batching_lab]). % LANDED 2026-07-30. Compiler classifies fixed DL_EXTRACT_BIN templates as sprefa_extract; callgraph/diagnostics/flow projections share multi-family commands. HostRunner groups same-frontier extractor demands by executor, template, and ordered typed inputs; one stdout projects into ordinary response rels and witnesses settle independently. Shell remains singleton; cached sibling projections omit independently. Gates: PLUnit 168/168, batching lab 9/9, runtime 4/4, typecheck, extraction clock 3 ticks/final empty/59 response statements, flagship callgraph 13 files/0 unclassified. Served 7/8 with the remaining old fixture still spelling removed type and RHS question-mark syntax.
task(rtkq_extraction_golden, done, [v6_2_host_contract_cleanup]). % LANDED 2026-07-30. sprefa-extract adds typed AstPatternQuery/AstCaptureFact and CLI --ast-pattern/--ast-selector/--ast-capture; one parse batches four literal/contextual ast-grep patterns and emits flat query/capture/text/capture-span/match-span rows. DL6 unions create/inject and generic/plain rules, joins by whole-match span, and contains endpoints in API scopes. Golden: 5 ticks, demand +2/-2, 2 extractor processes, response rows 9 then 6, exact endpoint rows 3 then 2, final four rels empty after delete. Focused Rust 2/2, DL compile, and served golden PASS. No DL syntax/type/storage/compiler/runtime addition. Full extractor 16/18: stale shared corpus expected-count and sandbox-blocked scip-go proxy are named external/pre-existing failures.
task(v6_2_scale_gates, done, [v6_2_ts_closeout]). % COMPLETED 2026-07-30. WATCHER: real WatchBindRunner/LiveEngine/file-SQLite receipt. 100/1000 files, 480/4800 events, 3 ticks, 1 subscription, 1.185185 write amplification, 90/900 exact final rows, 0 wrong; wall 39.198/265.241292ms, RSS 153845760/189202432B, SQLite 40960/253952B. GRAFANA: gate exit 0 over 389 discovered repos/250 usable. V5 full 42739 files, 389 repos, 13.00s, 3287.62 files/s, 356794368B RSS, 52387840B DB. V6 pinned first 8 usable, 779 files, 19.35s, 40.26 files/s, 176553984B RSS, 1069056B DB, 54.03 statements/tick. HOST OVERUSE: green V6 throughput exposes the per-witness extraction subprocess boundary; batching or V7 direct Rust link remains required.
task(v6_2_http_cli_dogfood, done, [v6_2_ts_closeout]). % LANDED 2026-07-30. Canonical registry http_route/3 + cli_command/3 facts generate v6/tsv2/cli/0_inventory.ts through compile/2_emit_cli_inventory.pl; explicit handlers remain hand-written. bop load/run/q already use HTTP; stats now GETs /stats and ticks streams /ticks SSE. Focused HTTP dogfood 8/8 PASS. No DL syntax.
task(norm_runtime_parity, done, [v6_2_ts_closeout]). % LANDED 2026-07-30. Existing norm/1 expression call only: registry text_scalar row, analyzer text operand rule, and emitted SQLite recursive scalar expression retain ASCII alphanumerics and lowercase letters. No parser, host, shell, or surface change. Receipts: full PLUnit 167/167; emitted runtime covers 'Route /V2: Café_42' -> routev2caf42, '---' -> empty, and 'AZ-09_é' -> az09; integer operand refuses by name.
task(v6_2_lab_reconciliation, active, [v6_2_ts_closeout]). % USER-RULED 2026-07-30. Every lab exits as implemented, canonical-plan, closed, or superseded; no unattached experiment directories or unindexed decisions. Reconcile scan and nested match composition against actual lowering, named intermediate rels, exact-one reducer output, init, clocks, and SQLite before any syntax ruling.
task(receipt_folding, active, [v6_2_lab_reconciliation]). % USER-RULED 2026-07-30. Every passing receipt must be folded into production unless it is blocked by an indexed user decision or named as superseded by a production mechanism. A canonical-plan or labbed status is temporary: preserve the receipt as regression evidence, attach its production consumer or decision card in ARCH, and remove duplicate lab-only semantics. Current fold order: phase5_bool_float, clock_checker_proof_payoff, rel_definition_hash_lab, file_span_redesign, comment_rail_wiring, mode/scope proof views, flow_parity_residue. Scan surface and optional JSON policy stop only at their indexed decision cards.
task(coordinator_pause_checkpoint, done, [receipt_folding]). % PAUSED 2026-07-30 for coordination from another session. Every dirty tracked and untracked file was checkpointed together by explicit user instruction. Completed bool/float production work and unfinished clock-checker work coexist in the checkpoint. Resume from chat_log/20260730.0.v6-2-ts-closeout.pl. Clock boundary: historical A2 is not statically provable under current either-source trigger semantics; A6 needs inferred-offset versus runtime-tick comparison.
task(scan_match_reconciliation, labbed, [pre_occurrence_loop]). % COMPLETED 2026-07-30, plans/2026-07-30-scan-match-reconciliation.pl + v6/prolog/labs/scan_match_reconciliation/0_receipts.pl. 10/10 executable PASS, 8 plan records, 4 unselected cards. Nested match is two top-level matches joined by one ordinary rel: 4 persistent rel tables, 15 TEMP support tables, 3 level groups; direct nested block remains dl_parse_error(statement). Scan+match expands to ordinary guarded ordered edge rules: 2 persistent rel tables, 7 TEMP support tables including __pre_machine, 2 arms. Init order and T boundary/T+1 listener clock pinned. No exact-one construct exists; current 0/1/differing-N/equal-N behaviors are silent/write/keyed_conflict/dedupe.
task(scan_surface_composition_lab, labbed, [scan_match_reconciliation]). % COMPLETED SOL LAB 2026-07-30, plans/2026-07-30-scan-surface-composition-lab.pl + v6/prolog/labs/scan_surface_composition/0_receipts.pl, 10/10 executable + 10/10 plan PASS. Multiple demands share one reducer; state key is instance/owner/key. Subscriber identity stays outside reducer key/body, so two subscribers create two views and exactly one reduction. Demand removal retracts views at T, finalize cancellation arrives T+1, later events gate; inactive cell remains because edge heads cannot delete, and re-demand resets it. Nested match reads named scan_view. Exact lowering: 10 rules, 7 edge statements, 3 level groups, 9 persistent tables, 32 TEMP, one keyed __pre_scan_cell; rule count fixed for 1 or 1000 demands. Three unselected surface cards remain.
task(single_rel_type_system_audit, done, [rel_value_unification_lab]). % COMPLETED 2026-07-30. Production reference runtime uses ordinary target arrivals followed by integer-edge parent arrivals in one tick. The temporary IReferenceResolution wrapper was removed; StructPlane.intern returns ordinary IArrivalBatch. Scan/match and JSON labs are reconciled under the same model. Zero relation-like intermediate types, stored nested JSON/dictionaries, ref constructs, parallel checkers, or new syntax.
task(json_interop_lab, labbed, [single_rel_type_system_audit]). % COMPLETED 2026-07-30, plans/2026-07-30-json-interop-lab.pl + v6/prolog/labs/json_interop/0_receipts.pl. Canonical plan only, zero production/syntax decisions. Locked boundary: JSON/json1 is transient wire adaptation; queryable objects and arrays normalize to ordinary target rel rows plus integer reference edges and generated joins. No JSON dictionary, nested-blob storage, relation-like intermediate type, or second checker. 12/12 receipts PASS. SQLite storage receipt: 10,000 repeated objects consume 921,600B inline JSON versus 98,304B relational references, 9.38x. Five bounded user cards remain for module residency, array relation shape, null mapping, schema contribution, and recursive entity identity.
task(byte_span_flattener,  closed, [file_span_redesign]).   % SUPERSEDED the evening it was filed, and the supersession is the interesting part. The comment lab's flattener (host template lifts "span":{start,end} to flat line/col, since decodeObjectItems projects TOP-LEVEL declared columns only) was the cheapest fix for diag-rail.dl6's whole-file zeros and flagship-callgraph.dl6's dropped line column. file_span_redesign does the same job STRUCTURALLY -- line/col derive in-language from a per-digest newline index -- so the template hack would be work aimed at a wart that is being removed. Recoverable if the redesign stalls: 9b5ba958:v6/prolog/labs/comment_node/cn.py
task(doc_format_extraction, unbuilt, []).  % USER DIRECTIVE 2026-07-29: raw html/xml/md/json/yaml/toml as extraction possibilities. Header plans/2026-07-29-extract-doc-formats-header.md (committed b535ca62, NOT dispatched): step 0 is the standing buy research (which grammars ship in our ast-grep registry vs need a tree-sitter dep), then a cst family for all six plus a doc family (key_path, value_text, value_kind, span) for json/yaml/toml and (element_path, attr, text, span) for html/xml. MARKDOWN is the one item with an existing receipt behind it: the comment lab named it the single extractor hole in comment parity (v5 has walk_md_comments, the cst family has no md grammar) and scoped SLOT-EXTRACTOR-WAIVER to exactly it. Four named slots incl SLOT-KEYPATH-SPELLING. Feeds schema_import_epic's json/yaml halves
task(simplify_wave,        unbuilt, []).   % 19 deduped items from four read-only opus reviewers over 934dcc4d..HEAD (reuse/simplification/efficiency/altitude), brief plans/2026-07-29-simplify-wave-brief.md. P0 THREE ARE CORRECTNESS-ADJACENT, not cleanup: (1) lower.pl:1767 sniffs a `__dict_` NAME PREFIX to find dictionary relplans -- the banned magic-name pattern, and a user rel called __dict_x silently loses its delta arms; (2) 0_refusal_messages.pl's umbrella renders unsupported_construct/1 only, so 1_host_expand.pl's 15 bare throws still print "Unknown message" (the B4 complaint, half-closed); (3) watch and enumerate write two DIFFERENT hash functions into one column named `digest` (2_binds.ts sha256 vs git hash-object) and nothing asserts they agree. Item 1 of the altitude set already landed at 6522f848
task(swipl_gc_abort,       unbuilt, []).   % UPSTREAM-SHAPED: swipl 10.0.2 aborts the compile sweep with "system error: Mismatch in up phase" under -g, deterministically at the 88th collection once the corpus reached 163 fixtures; the same goal typed at the toplevel completes. Worked around by set_prolog_flag(gc,false) for that one-shot process (sweep.sh:26, fork sweep_gc_false). Open work is not the workaround: it is (a) a minimal reproducer worth reporting upstream and (b) the knowledge that our compiler batch sits near a GC corner as the corpus grows
task(analysis_oracle_exam, unbuilt, []).   % USER-DIRECTED: a glean/joern/CodeQL-style graded analysis exam that REPLACES v5 as the standing oracle (v5 is a peer we are already beating on expressiveness and losing to on ingest, so grading against it forever caps the ceiling at v5). Research brief first, per the standing law. Prior art already written: plans/2026-07-25-analysis-engine-bakeoff-labs.md holds the constraints (parse-only, no builds -- which disqualifies CodeQL's compiled-language extractors by rule; native-speed lens; a declared RAM budget per tier; same corpus, same question battery, answers diffed against an oracle) and a fixed Q battery. What has changed since that doc: we now HAVE three graded v5-vs-v6 rigs (callgraph, flow, comment) whose shape a third-engine leg can reuse instead of inventing
task(amplification_sensors, unbuilt, []).  % NO COMMITTED RECEIPT YET, and that is the point. Coordinator measurement relayed at audit time: a ~3.4MB comment-facts database roughly two-thirds duplicated join-key TEXT (163KB for 56 distinct paths in the same db). v5's storage diet fought the identical disease and won it with dense ids. The gap is that NO bench in this repo SENSES amplification: `just memory-soak` asserts page-count flatness under churn and GET /stats reports page_count/freelist/dbstat sums, but nothing reports db-bytes/corpus-bytes or boundary-rows/input-row, so a 3x storage regression lands green. Sensor columns belong in the SHARED bench CSV. Diet arc only if the sensor says so -- and file_span_redesign removes the biggest class (raw path text per row) structurally, so build the sensor first or the redesign will be credited with a number nobody measured
task(equals_refusal_by_name, unbuilt, []). % SMALL, and the first thing a prolog reader trips on: `Var = expr` is unregistered, so it dies as unbound_head_var with no mention of `=` anywhere in the message (terra typed it in the flow rig; the sole reason it surfaced is that a lane was watching). Whatever SLOT-BIND-SPELLING rules, `=` must refuse BY NAME or bind. Same row carries the rename cost if the ruling moves `:=`: registry + parse/print + engine op + 16 fixture files, mechanical
task(assign_composition_lab, done, []). % LANDED 2026-07-29 (opus lane, merge d0104974): plans/2026-07-29-assign-composition-verdict.md. Census 30 real := sites: 14 map, 15 scan/pre folds, 1 naming-for-reuse, zero other. Whole-corpus desugar prototype graded 19/19 identical; paired compiler modules byte-identical. Seven concat-coordinate sites dissolve under file_span. Verdict: := is already sugar over argument-position expressions; a rel head write is next, := is a local name only. Three user cards remain: keep status quo vs shared expansion vs remove; expression evaluation in constructor/edge-head positions; fate of = and zero-use is/2. Two defects found: constructor sub-arguments diverge oracle vs emitter, and edge-head arithmetic refusal is stale
task(finish_the_job_epic,  done, []). % LANDED 2026-07-29 (opus lane, merge 21ecd6ac): plans/2026-07-29-finish-the-job-epic.md supersedes v6-alpha-golden-plan; 12 epics, 12 user cards, codex-driveable owner map. Measured tail: 61 unsupported = 41 intentional named refusals + 26 construct debts (pre 13, JSON destructure 9, aggregate heads 4). Critical path E1 simplify -> E2 phase 5 bool/float/checker/ingest -> E7 schema import; E3 span/comment -> E4 flow residue -> E8 analysis exam runs alongside. Carries bool/precision rulings and the decl-legibility cluster; implementation remains represented by the individual ARCH task rows

% ── fable coordination session 2026-07-30 (ledger chat_log/20260730.1.*.pl is the detail record) ──
task(clock_checker_full,   done, []). % LANDED 2026-07-30 (merge ffcddfc7): sol implemented, OPUS REVIEW GATE FALSIFIED the A6 "proven" claim by sabotage (constant observer + Grade=0 hardcode both stayed green), sol applied the 3-item fix list (nonzero-offset pipe crosscheck pinning a 4-tick delta sequence, clock_boundary negative, ClockCatch pinning restored, A6 renamed runtime_clock_crosscheck). Focused 24/24, plunit 197/197 coordinator-verified. The review-gate pipeline is the reusable lesson: the one affirmative claim was the one wrong claim
task(roundtrip_two_door_fix, done, []). % LANDED 2026-07-30: the 18 roundtrip reds were ONE defect (parse_dl keeps both decl forms, print_dl printed both = duplicate decl line, reparse dropped the type). Fixed as the measured coupled pair: print_dl shadowed_by_type_decl/2 guard + explicit col_type rows in 4_struct_values.pl. roundtrip ALL GRADES, plunit 197/197, text door 123/123/0, conformance 175. Branch gate green again
task(comment_rail_wiring,  done, []). % LANDED 2026-07-30 (merge 786b5daa, luna no-commit flow, coordinator re-ran gate): 6 of 7 verdict techniques as standing rails + parity referee, comment_node 745/745 v5-exact. Named skips: markdown grammar (doc_format_extraction owns), block-pairing refused unbound_head_var (MISNAMED-refusal suspect, equals_refusal_by_name class)
task(json_language_recovery, done, []). % LANDED 2026-07-30 (f9bf09df): plans/2026-07-30-json-query-language-recovery.md -- the v3/v4 json language is NOT lost, it is v5's src/datapath.rs brace walker (9 productions, stable since v1); v6 dropped it. 22 constructs: 8 express today / 4 ugly / 6 storage-carded / 5 new-surface ALL on the key axis. 9 user cards, top leverage CARD-KEY-CAPTURE
task(filespan_reconcile,   done, []). % LANDED 2026-07-30 (f94fc5c1): plans/2026-07-30-file-span-spine-reconciled.md -- 11 of 14 cards SETTLED by the v6.2 locks + landed rel-ref runtime, 3 open (rev naming, line/col residency, work-rev identity). Found the depth2 ref pair of bugs en route (depth2_ref_fix owns). Staged order: depth defect gates everything
task(depth2_ref_fix,       done, []). % LANDED 2026-07-30 (merge 2e2b983b, opus lane; coordinator re-ran EVERY leg incl the fail-first at the fixtures-only commit: 176 PASS/10 fail red before the fix). Relation-shaped term over a ref column = SUGAR for the canonical obj(SortedPairs) a world arrival already produces, rewritten once (0_relation_pattern.pl) before anything stores or unifies, so rule-built and world-arrived values are the same term at EVERY depth; depth-N needs no depth-N machinery. Emitter lowers to one __ref_<T> dictionary atom per level, memoized by term identity, ordered before expand_decode_rules so both dereference spellings compose. Shared refusal relation_pattern_not_a_relation_value replaces an ACCIDENTAL catch (json_extract of an INTEGER endpoint only caught when the phantom TEXT expression happened to land on an int column); 2 compiler-only refusals name where the rewrite does not enter (under not/1, edge statements). Conformance 175->186, plunit 200, text door 131/131/0, sweep 131/129/0-wrong both modes, tsv2 92. SABOTAGE WORTH KEEPING: disabling the memoization leaves every fixture and the whole sweep GREEN while the depth-2 span insert grows 3->5 joins/arm; only the EXPLAIN hop-count test sees it. Old row: IN FLIGHT (opus, lane/depth2-ref-fix): relation patterns at depth>=2 silently empty in emitter (json_extract on INTEGER __id), chained decode depth-2 dead in oracle. Fail-first at depth 2+3, cardinality 0/1/many, EXPLAIN SEARCH receipts, shared named refusal for out-of-scope shapes
task(golden_flex_e2e,      done, []). % LANDED 2026-07-30 (merge on main, opus lane; coordinator re-ran green + golden-flex + sweep + its OWN coverage sabotage, and re-ran golden-flex on the MERGED tree beside depth2: HOLDS, conformance 186). One .dl6 exercising 36 of 48 registry constructs + 12 named absences, each absence needing its reason in the golden header AND agreement with the registry status. Graded at 0/1/100/perturbed: oracle == incremental == naive on BOTH tick log and final state (6/45/1791/1802 final row groups). Served e2e leg: program POSTed as text, arrivals POSTed, rows read from /idb, real printf subprocess host, served tick log byte-identical to the oracle. golden-flex is wired into `green` itself, not merely green-all. COVERAGE SABOTAGE (coordinator's own): renaming latest( in the golden exits 1 naming latest/1; restore goes green. Also fixed en route: serve_lifecycle_idb_read_race (close the http server BEFORE disposing the program; the receipt holds an SSE client open and advances the interval bind on a virtual scheduler so a tick must touch sqlite after close began) and hostdecode_hardcoded_port_collision (17 hardcoded ports gone, port 0 + read it back). Old row: IN FLIGHT
task(norm_oracle_emitter_divergence, unbuilt, []). % OPEN DEFECT found by golden_flex_e2e, zero conformance coverage before it: norm/1 gives oracle "norm(Hello World)" vs emitter "helloworld". The golden routes around it and step 5 of the rig is an INVERTED receipt asserting the divergence still exists, so the fixer gets a red gate telling them to promote norm into the golden. Owner unassigned
task(golden_flex_residue,  unbuilt, []). % 5 more defects indexed by golden_flex_e2e, none fixed: float/bool unspellable in sh and bind decls (decl_b_column_type/5 knows only int/text/json and silently degrades to none, while struct type names DO work there); dl6_oracle schedule_value/2 term_to_atoms a dict so struct-typed arrival columns are ungradeable through the text door; .dl6 text has no final-state oracle leg (print_ticklog/3 discards FinalAll); one ITickOutcome renders a struct column two ways (outcome.line canonicalizes per the ruling, outcome.deltas and GET /idb hand back raw stored text in arrival key order); IRowValue cannot type a struct value the engine both accepts and prints. Also banked: 14 fixtures are strict subsets of the golden = retirement candidates, and one unexplained `swipl exit null` run recorded in the rig header, not reproduced in 5 later runs (opus, lane/golden-e2e): USER DIRECTIVE -- one golden-flex.dl6 exercising EVERY live registry construct, registry-driven coverage gate (new construct fails golden until exercised = the language-kept-in-check mechanism), 0/1/many + perturbed cardinality grading, served-HTTP e2e leg; owns the serve-exit /idb race + hardcoded-port collision fixes. Motivated by the unit-maxing diagnosis: every bug this session was a composition bug under all-green single-construct suites
task(json_syntax_arc,      done, []). % LANDED 2026-07-30 (merge 62f9ce84, opus lane, coordinator re-ran JSON_SYNTAX_LAB 25 PASS): cards only, parse_dl.pl untouched. Both constructs the recovery doc graded needs-new-surface have EXACT json1 lowerings (key capture = json_each(key,value) zero new SQL; ** = json_tree, fullkey = v4's dropped path bind free). Literal grammar IS the pattern grammar minus holes; key axis pattern-only forever. Lists: json carrier wins 5/5, list(T) = typed view, checker delta 4 clauses. RULED same day: json_key_hole_marker = dollar. 12 cards still open. Old row text: IN FLIGHT lab (opus, lane/json-syntax-lab): USER DIRECTIVES -- json rel type lowering to sqlite json1, native json5-ish {} literals with v3/v4-style holes ({ means json for now), list types + generics folded in. Lab returns exact spelling CARDS; nothing lands without user sign-off (no-unsighted-syntax law)
task(openapi_codegen_spine, done, []). % LANDED 2026-07-30 (merge bf71c5b6, opus lane, coordinator re-ran receipts.sh exit 0): 5 routes/5 ops/14 responses/14 schemas from prolog facts; four-source parity gate (spec vs ROUTE_LIST vs parsed dispatch branches vs a LIVE server's own 404 list). KEY RECEIPT: drop one route fact and the spec STILL passes Redocly validation while the gate goes 0/3 -- a validator cannot catch a lie by omission. Buy: openapi-typescript dev dep + Redocly via dlx + progenitor post-flip; direct axum emission over utoipa. 11 cards open. Old row text: IN FLIGHT lab (opus, lane/openapi-codegen): USER DIRECTIVE -- one openapi spec generated from prolog facts drives ts server now + rust axum post-flip + CLI; generalizes the cli_command/3 inventory-parity mechanism to HTTP routes; buy research first; emit_openapi.pl prototype + parity gate red/green receipt
task(rx_oracle_harness,    done, []). % LANDED 2026-07-30 (merge, opus lane; coordinator ran run.sh itself: RXORACLE HOLDS 8/8). Same scenario written twice, leg A literal rxjs importing NOTHING from the repo, leg B bash only through bop serve + curl. Line format <step> <name> <sign> <payload>; the clock is the STEP not the tick, batches land at step midpoints, an event within guardMs of a boundary fails LOUDLY as a straddle (that guard caught the agent's own driver drifting 40ms/step). 4 declared normalizations, and N3 (drop sprefa's del lines) is opt-in because OPTING IN IS THE FINDING: rxjs has no retraction channel. Verdicts: mergemap EXACT, latest/keyed MODULO N3, same_tick_collapse + scan_state_feedback + host_concurrency + switchmap DIVERGE, unsubscribe INEXPRESSIBLE. THE CANCEL ANSWER, measured: sprefa does not cancel, it MEMOIZES -- liveDemand$ reads delta.add and nothing reads delta.del, so no cancel site exists; the loser runs to completion, its answer lands durably, and re-demand is a same-tick cache hit where rxjs re-subscribes and re-runs. Discriminating outside observation = TIME the re-demand
task(rel_as_value_lab,     done, []). % LANDED 2026-07-30 (merge, opus lane): what the user means by rels-as-values (pass a REL into an arg slot, higher order) is NOT what the relation-pattern feature does (construct + destructure ref columns, first order). Coordinator had mislabeled it. Findings: reading is sugar, WRITING is not (the head term is the only constructor for a ref column, __id has no surface spelling); the ask is ALREADY RULED by locked(higher_order_runtime_boundary) + locked(higher_order_lowering) = Souffle components + compile-time monomorphization, and ALREADY PROTOTYPED in labs/generic_scan_instantiation (8/9). 6 candidate spellings: 2 expressible, 1 ugly-expressible, 3 INEXPRESSIBLE AND SILENT (call/N has no registry row so edb_definition claims it as a phantom empty rel -- now defect D4). Coordinate crux: a content hash CANNOT name a rel (names members, changes every tick, a schema is not data); rel_definition_hash proves 3 axes (shape hash, semantic hash, Name/Arity as storage identity) and the users content-hash-plus-instance-hash IS the specialization cache key. Generics: nothing is parametric, that stands, rule-level generics are a separate axis
task(scan_surface_ruled,   done, []). % RULED 2026-07-30, no build needed: scan gets NO NEW SURFACE. The rel_as_stream lab returned an EMPTY tier-0 list across 19 constructs, so nothing about scan justifies syntax. Canonical spelling = keyed state rel (accumulator) + log rel (sequence) + a match block whose arms are ordinary |+> edge rules; graded byte-identical on both doors, and the match form already reads left to right per the ratified |-> |+> pair. UNBLOCKS FOUR LABS that were waiting on this one decision: scan_surface_composition, scan_match_reconciliation, select_scan_cache fold now; generic_scan_instantiation is held ONLY for the rule-generics card. Sugar comes later from repetition evidence, not from a guess. Still open beneath it: ordinal spelling (four rules / seq() sugar / engine-minted, the last carrying the two-doors ordering crack) and loop/select mechanics, which the user wants nearby-and-intuitive for lowering
task(json_edge_body_unblock, unbuilt, []). % REASON WENT STALE, unowned. lower.pl:931 keeps edge_body_needs_json_destructure because a compound arriving into an untyped column is stored as canonical term text and the encoding question was SLOT-TERM-STRUCT's. That slot WAS RULED 2026-07-29 (compound_storage = struct_as_rows). The guard seam for edge bodies already landed for negation/comparisons/binds (edge_body_constructs). User's argument, and it holds: json is not a time thing, so edge bodies should accept decode. Small, and it matters because edge rules are the state-machine idiom
task(null_coherence_lab,   unbuilt, []). % IN FLIGHT lab (codex sol, ../sprefa-codex-null): user OVERTURNED the null ban ("we should just add json/sql nulls and get it over with in a way that is coherent"). Lab must make json null and SQL null one story, and price it honestly. First receipt decides much of it: json_extract returns SQL NULL for BOTH an absent key and a json null, which is the silent-wrong-answer class this repo keeps filing. Also: SQLite allows MULTIPLE NULLs in a UNIQUE index against our PK-over-key-columns keyed replace; DISTINCT treats NULLs equal while = does not, which lands in delta computation; prolog has no null so the oracle needs a representation that survives the sign decomposition. Prior art is unanimously against (Souffle none, classical datalog absence, Datomic absence, Flix Option), so the useful output is the SMALLEST coherent null. Tier-0 verdict required; if null is tier 0 it is the first this project accepts
task(prolog_graph_cleanup, done, []). % LANDED 2026-07-30 (merge 71050af8, opus lane; coordinator verified the headline on the MERGED tree: 0.368s real). flagship-flow.dl6 compile 257.09s -> 0.22s; plan phase 256,950ms -> 62ms; plan inferences 6,011,087,004 -> 1,305,819; growth rules^4.864 -> rules^1.265; causal_dependency/4 39,046,545 calls -> 33,865. TWO causes, not one: SCC by all-pairs simple-path enumeration, AND recurrence_free_clock calling clock_scc/3 INSIDE ITS OWN NEGATION so a full component search ran per simple path. BUY VERDICT measured not read: library(ugraphs) wins for representation/transpose/neighbours/top_sort/cycles -- its transitive_closure/2 is EXACTLY the strict positive-length reachability the checker meant while reachable/3 is REFLEXIVE and would have been a silent wrong swap, and top_sort/2 fails on cycles and self-loops so it is the cycle detector too. ugraphs ships NO SCC, nor does any SWI library or pack (pack_list('scc') empty); composing SCC from transitive_closure is Warshall-cubic and tracks VERTEX COUNT not sparsity (27,082ms on a 1000-node CHAIN), so SCC alone is hand Kosaraju ~40 lines with the Warshall composition KEPT as the differential oracle; all four candidates agreed at 9 shapes. Tabling priced and REJECTED on measurement (5ms vs 1ms). HASH IDENTICAL: flagship sha256 28faeb19 both sides, corpus-wide 135 shared emitted modules 0 differ. Count rails pinned on exponent and ceiling, both sabotages red (un-hoisted 1.89, simple-path restored 3.83). LEFT UNSWEPT 11 sites with reasons; the toposort cluster (0_type_plane.pl, strat.pl, ARCH.pl, four hand-rolled Kahn variants) each fixes an EMISSION ORDER that byte-identity grading depends on, so top_sort/2 would change generated bytes for no measured gain, and none appears in the post-fix profile % IN FLIGHT (opus, lane/prolog-graph-cleanup): TWO COPIES of the same naive all-pairs-mutual-reachability SCC exist (compile/3_clock_check.pl:224-234 = the 255s hot spot, and labs/rel_definition_hash:296-315), library(ugraphs) ships with SWI and has ZERO uses in the repo, and :- table appears in exactly two files and in none of the type/clock/inference walkers. Brief order: buy-before-build analysis FIRST per the standing law (ugraphs vs SWI tabling vs hand Tarjan), then ONE memoized graph module both call sites move onto, then the wider sweep, then before/after numbers off the profiling bench plus a COUNT assertion so it cannot silently regress. User intent verbatim: types and clocks and inference unification memoized and close to each other
task(extract_spelunk,      unbuilt, []). % IN FLIGHT (codex luna, ../sprefa-codex-extspelunk): capability inventory of sprefa-extract itself, which nobody has done -- the parity lane inventoried v5's OPS and RELS, not the extractor. Deliverable's most useful column: of v5's 106 absent rels, which are absent because the EXTRACTOR cannot produce the facts vs because the LANGUAGE has no spelling (different owners, different fixes). Also: bin-vs-lib capability parity (nothing asserts it today), whether the missing TS module resolver is really the madge blocker given scip-typescript is already invoked by the ratchets, and the document-sources gap (no md/html/xml/toml/yaml Source, BlobSource unimplemented)
task(scip_passthrough,     done, []). % LANDED 2026-07-30 (merge 8f6a4377). SCIP coverage 17/43 -> 43/43 serialized fields; the whole Metadata/ToolInfo block was LOADED AND NEVER EMITTED. Cost priced rather than hidden: 204 docs went 150,640 rows/32.1MB -> 177,967/59.4MB and ~17MB of the 21.5MB growth is CONSTANT columns (syntax_kind is 0 in 123,655 of 123,655 rows). VERDICT: the engine cannot hold 100pct as a standing feed at 291k rows per 1,000 files against 74.2 files/s ingest with commit_ms dominant, so --scip-record KINDS filters AT PRODUCTION and a narrowed stream also skips reading the corpus. DIET MODULE RESOLUTION landed over the oxc specifiers ts.rs:883 already captured (one step, not zero): scip 764 edges/755 agree/recall .992/prec .988 vs diet 761/761/1.000/1.000, and the lane stated the honest reading itself -- diet's 1.000 is agreement between TWO SYNTACTIC SCANNERS, not correctness; the 9 scip-only edges reach a declaration through an INFERRED TYPE with no import statement, which no syntactic resolver closes without a type checker
task(stale_labs_sweep,     done, []). % LANDED 2026-07-30 (merge 74200375). 5 folded, 2 kept, and ghcacher_tick_golden PROMOTED after the lane refused the coordinator's framing: it was filed as debt and is a WORKING GATE nobody ran, now in v6/tsv2/goldens/ and wired into green-all with a sabotage receipt. CLAIMS NOW KNOWN WRONG: the bootstrap lab did NOT self-host while its plan doc still reads like a live 7-phase arc; interning is a 2.44x win ONLY when names repeat and a 1.2% LOSS when unique, which applies to a SHIPPED dep (sprefa-store uses lasso) and was written down nowhere; rel_value_unification's two red checks were correct behaviour with only the ENCODING moved; bidirectional pattern_value/3 is a toy-scale claim declined at real scale. HANDED FORWARD: swipl packaging unanswered (otool shows @rpath/libswipl.10.dylib, a distributable needs a static SWI build, and this is the ONLY measurement of the bundled-with-rust tier)
task(effect_chain_batch,   done, []). % LANDED 2026-07-30. N stages = N+1 ticks, one hop per stage. Same-tick chaining forbidden TWICE (compiler refuses two host atoms per body; runtime re-enters only via engine.submit) and the general reason is that it would require suspending the fixpoint on I/O. Cost is PER ITEM: ticks = seeds*stages + 1. Batching 7-to-1 verified (700 demands -> 100 spawns) and it collapses ticks 700 -> 100 too; compatibility = same tick frontier AND sprefa_extract AND byte-identical template AND identical input values; a plain sh host CANNOT BATCH AT ALL. V5 COLLECT IS FAN-IN OVER DISTINCT VALUES, today's grouping is FAN-OUT DEDUPE OVER IDENTICAL VALUES, opposite requirements. Planning is already fully comptime and is NOT the bottleneck; every gap is a missing FOLD, and one list-valued aggregate head makes collect expressible with zero new effect machinery
task(group_concat_silent_miscompile, unbuilt, []). % OPEN DEFECT F9 from effect_chain_batch, coordinator-reproduced: group_concat(x) in a head COMPILES CLEAN at 200 and is not an aggregate. It stores the literal text group_concat(1), one row per input, silently. That is the exact spelling a cold author reaches for when they want v5's collect
task(host_column_shadows_runtime, unbuilt, []). % OPEN DEFECT from staged_writes, coordinator-reproduced side by side: a host declaring an input or output column named ordinal or witness_digest compiles CLEAN, the compiler renames ITS OWN runtime columns to col1/col2 to dodge the duplicate while serve/1_hosts.ts project() fills BY LITERAL NAME. Empty witness, PK collapsed to ("",""), multi-row answers cut to one, dead demand-to-response join, program derives nothing, no refusal and no trace. Fix is a load-time refusal, not a rename. Bit its own lab twice in one sitting
task(staged_writes_lab,    done, []). % LANDED 2026-07-30 (merge 787d994d), the tool's NAMESAKE arc graded. Marker writing already works and stops in three measured places: a host's input is a ROW while a write's payload is a RELATION with no string aggregate available, so 3 staged lines = 3 spawns = 3 whole-file rewrites ordered by DELTA ORDER (v5 folded rows in the engine, one write per file per tick); the engine is BLIND to the disk it wrote because effect identity is content-addressed over the DEMAND only; writes are at-least-once. THE do-not-advance-until-answered RULE IS ONE concatMap AND IT BUYS ORDERING, NOT DURABILITY -- kill -9 between the disk write and the answer replayed the write on restart. Its real costs: the tick log stops being a pure function of (program, schedule) and becomes a function of effect WALL TIME, ending byte grading for any program that writes; one slow write freezes the whole engine; a host that curls the engine becomes a hard undetected deadlock. Dry-run-as-rows works today and beats --fix because armed(zone) is an ordinary row
task(ts_lowering_review,   done, []). % LANDED 2026-07-30 (merge be97647c). 2 CRITICAL: one tick fault PERMANENTLY kills the engine and the served process (3_engine.ts:176 re-throws into the shared concatMap behind ticks$, and 4_http.ts:315 states the opposite law in its own comment; no gate catches it), and the HTTP arrival boundary checks row LENGTH never row SHAPE (row:"ab" kills the server; row:[{a:1},[2,3]] returns 200 and STORES [null,"[2,3]"] into TEXT NOT NULL where it persists). 3 HIGH: unescaped shell injection in the {col} template splice with spawn(shell:true) and the probe CREATED A FILE ON DISK; host subprocesses run at concurrency 1.0, the structural cause of 40.68 files/s vs v5's 3,540.93 same-run; the ordered/pre family is constant+2n statements per tick with no count assertion. 4 of 8 standing TS laws violated. pnpm typecheck is RED with 4 errors and NO gate runs the compiler
task(prolog_main_review,   done, []). % LANDED 2026-07-30 (merge 9374bf5b). 15 proved / 6 suspected. F1 the ORACLE HAS NO CLAUSE for combine/variadic or next/1, both registry LIVE, so term-form combine derives zero rows while the compiler emits a real cross join; hidden because parse splices at parse time, print_dl ERASES the spelling, and no term-corpus fixture uses either, while the oracle's own stratifier DOES handle it. F2 zip/subscribe/complete/unsubscribe/error silently derive nothing on the oracle and are refused by name on the compiler. F3 a level-head expression contradicting its declared column type is checked by NOBODY (bool and float get a CHECK in that same predicate; int, text and ref get none). F4 today's D3 fix fires only when BOTH columns are declared. F5 CORRECTS THE PROFILING HEADLINE: dominant_phase emit and rules^0.964 came from a ONE-ATOM SYNTHETIC at 0.5ms resolution; all four real programs are still PLAN-dominant and the same generator at n=200 gives rules^1.84, with the hot spot at analyze.pl:310 re-walking every rule body once per (ref,column), 16,315 walk_node calls for a 181-line file
task(null_implementation_plan, done, []). % LANDED 2026-07-30 (merge 660fedcb). 10 ordered steps, each with a fail-first receipt and a reversibility mark; 18 emitted SQL sites change with line numbers, plus 10 MEASURED NO-OPS not to touch. Oracle door = null_value(sql), a reserved ground compound, because engine.pl decides row identity with memberchk/==/sort/ord_subtract/set_diff_delta, all TOTAL on a ground term, so THE ENGINE NEEDS NO CHANGE AT ALL; an atom is unusable since none has 84 corpus uses as text and body.pl:147 reads it as JSON absence; bool_lit(true) is the precedent; a T? column is a disjoint sum on the VALUE axis so TICK-MODEL's B/N/Z are untouched. FOUR CORRECTIONS to the null lab, incl BARE null CURRENTLY PARSES AS A FRESH VARIABLE so repo_latest(Repo,null) silently means any value, and present(X) is not caught by refusal-by-absence so a typo becomes a silent empty EDB. Step 6 is the estimate risk: narrowing would be the FIRST REBINDING OPERATION in this compiler
task(option_vs_null_lab,   done, []). % LANDED 2026-07-30 (merge 0f521736), deliberately NOT closed, 9 cards none recommended. The user's three-variant read WORKS TODAY via json_type/2 with zero new semantics, and the negative control is the finding: the SHIPPED json_extract IS NOT NULL predicate reports 5 present where json_type reports 6, so ONE DOCUMENT IS SILENTLY LOST TODAY. An Option has FOUR observable states; the fourth is row absence and no variant expresses it. THE EXPLOSION WORRY INVERTS: at 100k/10pct absent, Option is 1.57x bytes while the NULLABLE COLUMN ON A DERIVED REL is 2.14x and the LARGEST, forced off WITHOUT ROWID onto rowid+UNIQUE. THE WALL: an Option from a DERIVED rule does not compile, since 0_enum_expand.pl attaches keyed(...) to every variant rel so the outer-join shape hits keyed_level_head; class (b), highest-value card. SOUFFLE: no general null, nil only in record-typed columns, no json type, shape declared up front so a maybe-missing key CANNOT ARISE; issue #2315 leaves negative ADT branch matching unshipped. THREE DISAGREEMENTS with the null lab in writing, including its own refuted hypothesis: IS NOT DISTINCT FROM does NOT lose the index, identical SEARCH plans on both builds
task(defect_wave_0730,     done, []). % LANDED 2026-07-30 (merge 8d71f543, opus lane; coordinator re-ran on the branch: just green exit 0, sweep 133 compiled/131 identical/0 wrong, GOLDEN FLEX HOLDS, conformance 186 -> 193). All 8 fail-first. D1 parseWhitespace mangled any 2-row host answer on the SHIPPED enumerate_at path, red receipt straight off the trace: decoded rows per demand [0,1,1,3] where it must be [0,1,2,3] (right at 1 and 3, wrong at 2). D2 backslash deleted twice; rule now stated (\n \t \r escape, \\ is one backslash, own quote is itself, every other \X is two chars) and it exposed a THIRD conflation -- three emit_ts joins built multi-statement SQL from the two-char sequence backslash-n and leaned on the template literal to convert it, so escaping correctly turned 11 fixtures into SQLITE_ERROR unrecognized token; they join with a real newline now. D3 a variable carrying text into a ref column reached INSERT OR IGNORE; fixed as a LOAD-TIME check because the contradiction is fully decidable from prog/2. D4 silent call/N, and its negative leg matters: `call` is a legal relation name and the flagship declares call/2, so the trigger is an UNDECLARED UNHEADED call/N (the first cut broke the flagship). D5 depth-1 join regression, and the RECEIPT STANDARD WAS RAISED as instructed -- relationDepth.test.ts now asserts every row source BY NAME in FROM order per delta arm, not just access method, with a sabotage receipt in the header. D6 refuse on BOTH doors rather than lower, with a real reason: a relation value under not/1 needs its dictionary joins scoped INSIDE the NOT EXISTS (hoisting inverts the answer) and an edge rule has no dictionary-join seam at all; deleting the shared refusal is how the capability lands later. D7 norm/1, where the INVERTED golden step fired by design and the agent did all three things its message demanded. D8 float and bool now spell in sh and bind decls; premise correction, struct type names did not work there either (only host OUTPUTS resolved them), and a second half remains one layer down where the witness-digest concat refuses them, which is an emitter agreement decision not a widening. BANKED, unfixed: zip/2 is the same class as D4 (compiler refuses by name, oracle silently derives nothing, reserved-word gate on one door only) % IN FLIGHT (opus, lane/defect-wave): 8 defects, fail-first each. D1 parseWhitespace mangles any 2-row host answer on the SHIPPED enumerate_at path; D2 backslash silently deleted in .dl6 string constants twice (parser + emitted template literal); D3 a VARIABLE bypasses relation_argument_violation and writes text into an INTEGER PK column; D4 silent call/N; D5 depth-1 join regression from 472320f4 (self-join through __ref_<T>, 2 rel_value_unification checks already red); D6 sol burrs incl the two refusals that are unfinished work the oracle executes fine; D7 norm/1 oracle-vs-emitter divergence (fixing it turns golden-flex step 5 red BY DESIGN, an inverted receipt); D8 float/bool unspellable in sh and bind decls
task(prolog_compile_profiling, done, []). % LANDED 2026-07-30 (merge 4dd7e230, codex sol). plan phase = 255,333ms of 255,490ms at 6.01e9 inferences; hot predicate clock_check:graph_reachable/4 under clock_scc/3; growth exponent 4.899. Root cause read and confirmed by the coordinator: SCC by ALL-PAIRS MUTUAL REACHABILITY with simple-path enumeration and no memo. A 27-rule real program took 255s vs 3.6s for a 30-rule chain, so graph SHAPE dominates size. statistics/2 in setup_call_cleanup/3 + library(prolog_profile); trace interception rejected at a measured 5.3x; off path proven free by sha256 equality. JSONL under DL_PERF_LOG matching the runtime shape so numbers survive the rust flip. Old row: % IN FLIGHT (codex sol, ../sprefa-codex-plprof): compile cost is steeply superlinear, 7.9s at 60 statements vs >232s at 117, cause unknown and misdiagnosed as a hang twice. Must use SWI's OWN tooling (statistics/2, library(prof), setup_call_cleanup boundaries) and SAY WHY each was chosen or rejected; output is JSONL matching the existing DL_PERF_LOG shape so the numbers survive the rust flip; shell bench reproduces the N-vs-time curve from outside the process; deliverable names the dominant phase and its growth shape
task(teardown_flatten_lab, done, []). % LANDED 2026-07-30 (merge d966b407, opus lane; coordinator ran receipts: TEARDOWN LAB HOLDS, 15). Door 1 was ALREADY RULED and unimplemented since 2026-07-27: effect_abort's own text says demand-row deletion IS the abort signal and names the unbuilt site. 1_hosts.ts:175 already returns () => child.kill() and it has never been called. Four flatteners graded on one /ticks stream, one operator changed. merge and concat are the SAME PROGRAM (which inners die vs how many run are different axes); switch needs NO key because del carries the decision, so teardown-on-del is MORE GENERAL than switchMap; NO ordinal needed. Teardown = 4 sites in serve/1_hosts.ts. Old row: % IN FLIGHT lab (opus, lane/teardown-flatten): grade all FOUR flatteners (concat/merge/switch/exhaust) as ordinary rules, on the thesis that identity and flattening are two axes we mashed into one. Is teardown one site or many; does door 1 obey the effect_abort ruling or need it amended; is concatMap serialization the bigger cost than the missing cancel; where does the ordinal live
task(rel_as_stream_lab,    done, []). % LANDED 2026-07-30 (merge 7d040ab0, opus lane; coordinator ran receipts 10 PASS 0 FAIL). TIER-0 LIST EMPTY across 19 constructs. A stream is a VIEW OF A TABLE PLUS A NAMING CONVENTION; the single-rel lock is not fought because log_on_level_headed_rel already forces a reader's projection to be a table. rel-as-stream = log rel + ordinal from a keyed cursor read through pre/1, byte-identical both doors. Backpressure needs ZERO constructs (derived watermark + visible dropped rel). keep(count(0)) = deliver-and-forget, so the rx Subject family is a two-word decl matrix. zip = equijoin on ordinal, bufferCount = integer division. CRACK: the two doors already carry different internal orders (engine.pl:357 global counter vs lower.pl:2275 per-table rowid), ungraded because unobservable; surfacing an ordinal picks one and the pick becomes a contract. Old row: % IN FLIGHT lab (opus, lane/rel-as-stream): can a rel BE a stream with existing mechanics (log + ordinal), is a stream a different THING or a VIEW of a table, and the TIER-0 TEST on every wanted construct (sugar / new lowering / genuinely new semantic). Must describe a lowering target several runtimes could hit, not "whatever rxjs does" -- the user's warning is that hosts already got welded to concatMap
task(external_oracle_scout, done, []). % LANDED 2026-07-30 (merge d9b808bf, codex luna analysis-only): answers 'does sprefa-extract go against a madge oracle like v5'. v5 YES (oracle_madge.rs grades 8 rels from examples/madge.dl against madge json/circular/leaves/summary/depends/warning) but ALL 14 v5 oracle test fns are #[ignore]d = 0 run by default. v6 engine legs have NO external oracle; v6 EXTRACTOR does (golden_parity.rs runs scip-typescript/scip-go/rust-analyzer as ordinary non-ignored ratchets). Real gap = no v6 resolved TypeScript dep(src,dst) rel to diff (no ts module resolver in the extractor, types.rs:587-603); the diff harness is the easy half
task(v5_parity_spelunk,    done, []). % LANDED 2026-07-30 (merge 754889b5 -- the lane the coordinator relayed and forgot to merge, caught by the user asking; finding filed). v5 is SELF-DESCRIBING (op_catalog/fn_catalog/rel_catalog are builtin rels) so the inventory comes from ASKING v5 through an sh host, line-regex leg graded against that. 28 ops / 16 fns / 112 rels; v6 covers 20, partial 4, absent 132, ZERO of sixteen scalar functions. scan() appears in 105/129 examples and 29/33 rails and is the shell-glob spelling. Plus the git-fact diags rail class (enumerate vs enumerate_at antijoin = new-file diags)
task(flow_residue_partial, unbuilt, []). % terra STOPPED with reconstructed tables (dirty tree ../sprefa-codex-flowres): call-target partition = 110 extraction-input + 32 REAL resolver divergences (same coordinate, different callee); per-edge attribution blocked on the serve-lifecycle race golden_flex_e2e owns. match_arm_tokens syntax violation (9cadb419 |-> |+> unruled) awaits the user card
task(json_wiring,          done, [struct_as_rows]). % LANDED 2026-07-30 (merge before c04b6cfd, opus lane; coordinator re-ran on merged main: green exit 0, conformance 201/0, TEXT_DOOR 141/141/0, plunit 236/236, sweep both modes 141/139/0-wrong). Brace spelling settled BY MEASUREMENT: _{} and Tag{} are SWI dict syntax the term door would never see as {}/1, so bare {...} wins and both dict forms RESERVED (tagged_brace_reserved). json became its OWN STORAGE KIND (was a text alias, which erased the dispatch bit). Exact keys 0 joins / spread+key-capture 1 json_each / ** 1 json_tree; delta arm reads json_each off the frontier, zero delta-side scans; guards FIRST in WHERE (SQL states no AND order). 20 of 27 archive examples verbatim through the production text door incl gh-cache both spellings. KEPT REFUSED with measurements: json agg heads (json_group_object row order vs sorted keys; no ORDER BY slot in the flat aggregate SELECT) and edge-body json (blocker located: analyze.pl:edge_goal_refusal/4 receives only Prog, no relplans, cannot tell json from struct)
task(ts_critical_fixes,    done, [ts_lowering_review]). % LANDED 2026-07-30 (merge bd0d567c, opus lane; coordinator re-ran on merged main: green exit 0, sweep 141/139/0, MEMORY SOAK HOLDS, serve-leak-soak clean). F1 was WORSE than reviewed: the tick fault closed the LISTENING SOCKET through runProgram's merge (ECONNREFUSED). F2's first draft was too strict, golden-flex caught it (struct values arrive as whole JSON objects; ref columns accept any JSON value, only absence refused); validation lives at the hand-written boundary, the only trust boundary and the only holder of relColumns/relColumnTypes for a named 400. F3 all 5 injection payloads executed pre-fix. F5 pinned not fixed: ordered/pre 13+2n vs incremental flat 31, plus the zero-arrival full-table SCAN measured; fix owned by pre_occurrence_loop. typecheck WIRED INTO GREEN (npm test strips types; the package sat 4 errors red with no gate noticing)
task(bind_submit_error_arm, unbuilt, [ts_critical_fixes]). % NAMED GAP from the tsfix lane, recorded not patched blind: bind runners do not handle submit's error arm (2_binds.ts:164, :354) -- HTTP and the host runner both do, so a fault raised by a bind's own arrival still ends the app. Reachable only through an emitted-program defect on a bind-fed rel. Receipt to write: fail-first through a sabotaged emitted module
task(time_plane_lab,       done, []). % LANDED 2026-07-30 (lab commit dc9a5030, verdict plans/2026-07-30-time-plane-unification-verdict.md, lab-death executed by the retention landing lane, last copy f044ce06; coordinator re-ran receipts.sh 7 PASS 0 FAIL). H1 (log = rel + time column as sugar) REFUTED ON MECHANISM: a set rel absorbs an identical second arrival as a no-op BEFORE any rule runs, so log's job is the ENGINE minting row distinctness and no expansion written in rules can mint it. Blast radius 6 die / 6 move / ~105 survive / 2 new; seq-as-visible-column = 80-fixture regrade + reorders every tick. H2 (created/updated_at_tick) already expressible in two edge rules (now/1 + pre/1 + not/1); the naive one-rule spelling is silently updated_at wearing created_at's name; sugar if ever = opt-in per rel (automatic costs 7.5 bytes/row). H3 history: shadow log +15.2% vs rel-as-log +72.4%. Slots: seq_scope = per-rel (the ORACLE is the side that must change; counter = next_seq/3 engine.pl:379-381 + Seq0 thread :291-313); updated_at = keyed rels only
task(retention_minus,      done, [time_plane_lab]). % LANDED 2026-07-30 (lane commit d0a6bbd4, merged; coordinator re-ran conformance 214/0 + TEXT_DOOR 150/150/0 on branch, green exit 0 + sweep BOTH modes 150/148/0-wrong + compile-speed OK on merged main). Retention prune = VISIBLE minus delta on both doors; finalize-over-log FIRES (fixture supersedes SLOT-LOG-FINALIZE-REFUSAL + stream card 4a + consumption-arms 17). THE LAB UNDERCOUNTED: THREE suppression sites not two -- emit_ts diff_local_line/3 emitted literal del:[] for kept rels, the NAIVE REFEREE's copy, invisible to a single-emitter-mode lab sweep (method lesson banked). Exactly ONE artifact line regraded corpus-wide. +5 fixtures incl the log/set occurrence-identity pair; retention_count_prunes_oldest gained its missing deltas/2 leg. TICK-MODEL 5.1 splits R7: occurrence in N (no minus, ever) vs storage row in Z (keep alone)
task(golden_gates,         done, []). % LANDED 2026-07-30 (lane commit c00043be, merged; coordinator re-ran all four gates in-worktree + green and compile-speed on merged main). green-all grew compile-speed (INFERENCE-COUNT ratchet, 3 runs byte-identical at 2,120,677 while wall wobbled 7->11ms; sabotage scan = +899.8% named FAIL where wall moved 7->12ms), scale-floor (statement-count SET across cell sizes 10x apart; single-size assertion cannot tell fixed overhead from a per-row loop), multirepo-golden (program 5's v6 leg: v5 version-skew.dl UNMODIFIED vs v6 port + a third opinion reading go.mod bytes, 4/4 rels byte-identical 0 unclassified; named gap dep_ver min/max over text refused), rtkq-golden (EXISTED COMPLETE since 51ba4161, never wired into any battery; sabotage flips on byte spans). Findings: v5 dep_ver min/max REVERSED on pkg/errors (README, ungraded); rtkq runner under tsv2/labs/ = protocol debt; gen/scale_generated.ts stale (gen_staleness_gate class)
task(json_flex_lab,        done, []). % LANDED 2026-07-30 (lab commit 7612fdc1, merged; lab-death done, last copy 6dde7f9a; verdict plans/2026-07-30-json-flex-verdict.md; coordinator re-ran conformance 221/0 + TEXT_DOOR on branch, sweep BOTH modes + green on merged main = 226 fixtures, 162/160/0-wrong). Semantics HELD under every value kind; FOUR invisible defects under them fixed fail-first: oracle tick log was not JSON (~4| hex-escape column stop, THREE encoder copies), text-column json object -> SQL NULL, first-char structure sniffing (json now its own IRowColumnType end to end), json_canon binding a free variable to {}. ref columns needed the same encoder arm (memoized __rendered NOT key-sorted -- refutes struct-as-rows canonical-at-intern-time, card C6). JSONTestSuite y 95/95 / n 187/188 / i 31-4, zero crashes, 127 round-trips idempotent. OPEN CARDS (11, measured): Q2 null-collapse SHARED by both doors (unilateral fix creates divergence); oracle json null = atom none makes text "none" unreachable oracle-side; dup keys throw-vs-last-wins; 1e999 -> null; wide-int RangeError cliff at 2^53-1; nesting cap 1000/1001. Compile-speed gate made its FIRST CATCH on this merge (+11.7%/+14.2% emit inferences, accepted via --write-baseline as the correctness cost)
task(dep_ver_minmax_reversed, unbuilt, []). % FOUND by the multirepo golden 2026-07-30, ungraded (v6 refuses the construct so no leg exercises it): v5 dep_ver min/max returns lo=v0.9.1 hi=v0.8.0 for github.com/pkg/errors, against the lexicographic order its own header claims, while example.com/shared is correct. Written in tsv2/goldens/multirepo_crawl/README.md. Suspect: v5-side aggregate over text versions. Verify against src/ before assuming
task(get_else_wiring,      done, []). % LANDED 2026-07-30 (ruling null_design = get_else_use_site_never_storage; candidate B, T? nullable columns, stayed DEAD and plans/2026-07-30-null-implementation-plan.md stayed superseded). SURFACE WORD IS `coalesce`, not `get_else`: the vocabulary law admits only rxjs/prolog/SQL words and COALESCE is SQL's name for exactly this. THE LOWERING PLAN THE ROW CARRIED WAS NOT NEEDED -- no LEFT JOIN family was written and the emitter gained nothing. 0_coalesce_expand.pl (expansion phase 45, between match and relation_edge, the ONE module both doors consult) rewrites one rule into two ordinary clauses of the same head: the read, and `not(...)` plus a `:=` of the default. Multiple head clauses, stratified negation and `:=` binds were all already shipped, so the construct inherits the incremental delta path, the negation path's retraction flip and the naive referee. N coalesce goals fan out to 2^N clauses. The EDGE arm is latest(...), not the bare atom, since a bare atom in an edge body is a trigger (same split, same reason, as 0_relation_edge_expand.pl). Seven refusals, all thrown by the shared expander so the doors cannot disagree: coalesce_no_output, coalesce_multiple_outputs, coalesce_output_not_column, coalesce_default_not_literal, coalesce_source_not_rel_atom, coalesce_not_top_level, coalesce_in_head -- the last two are what keep a coalesce from ever reaching analyze.pl, where the live row's refs_of_arg role would read the source as an ordinary join and drop the default in SILENCE. Receipts: conformance 201 -> 209/0 (incl a retraction-flip fixture where the defaulted row RETURNS after the source row leaves, and a derived-source fixture), sweep BOTH modes 141/139/0 -> 145/143/0 with zero movement in any prior bucket, TEXT_DOOR 141 -> 145/145/0, roundtrip G1 209/209, plunit 236 -> 239, GOLDEN FLEX HOLDS with coalesce exercised non-vacuously at all four cardinalities, `just green` exit 0. COUNT/PLAN receipt v6/tsv2/tests/coalesceCounts.test.ts: 33 statements/tick flat at 5/100/1000 source rows, default arm plans SEARCH n0 USING PRIMARY KEY off __frontier_repo, three sabotage receipts in the header. some/none stays the existing enum machinery and stacks with coalesce; generic Option<T> still rides monomorphization

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
