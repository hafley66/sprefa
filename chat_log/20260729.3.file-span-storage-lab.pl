% Consultable live session ledger.
% Load:
%   swipl -q -l chat_log/20260729.3.file-span-storage-lab.pl
% Example queries:
%   goal(Name, State, Contract).
%   user_constraint(Name, Value).
%   next_step(Order, State, Action).
%   touched(Path, Reason).
%
% This file is updated during the session so a context compaction can recover
% the task without reconstructing it from prose.

:- module(session_20260729_3, []).

:- discontiguous touched/2.
:- discontiguous observed/2.
:- discontiguous decision_pending/2.
:- discontiguous next_fixpoint/2.
:- discontiguous directive/2.
:- discontiguous verification/2.
:- discontiguous leading_hypothesis/3.

session(id, '20260729.3').
session(topic, file_span_storage_lab).
session(date, '2026-07-29').
session(main_state, 'local commits from prior Claude lanes merged; push remains user-owned').
session(concurrent_work, 'none owned here; prior assign, finish-the-job, and world-health lanes have landed').
session(progress_checkpoint, 'file-span storage lab complete; relation/value fixpoint iterations 0 through 3 pass 58 checks; adversarial composition is next').

% goal(Name, State, Contract).
goal(file_span_storage_lab, in_progress,
     'measure and select a compact relational representation for repositories, revisions, logical files, immutable content, and spans').
goal(host_relation_fit, in_progress,
     'determine whether typed file/content operations fit the uniform host-plan and bind-plan paths or require a blessed built-in model').
goal(null_free_sum_storage, in_progress,
     'represent committed/work revisions and git/stored content sources as total variant relations with ordinary union rules and no nullable payload columns').
goal(span_text_binding, in_progress,
     'price text, line, column, and slice over a content span using git batch reads, optional stored bytes, and bounded caches').

% user_constraint(Name, Value).
user_constraint(no_null_payloads, true).
user_constraint(paths_stored_once, true).
user_constraint(open_names_interned, true).
user_constraint(closed_enums_numeric, true).
user_constraint(no_literal_string_enums_in_fact_rows, true).
user_constraint(no_canonical_json_file_span_dictionary, true).
user_constraint(repo_rev_file_queryable, true).
user_constraint(spans_over_files_queryable, true).
user_constraint(git_content_may_be_re_read, true).
user_constraint(git_content_may_also_be_stored, true).
user_constraint(database_growth_measured, true).
user_constraint(resident_memory_bounded, true).
user_constraint(no_new_surface_spelling_without_user_card, true).
user_constraint(relational_union_preferred,
                'enum variants lower to total variant rels; match lowers to ordinary rules').

% proposed_semantic_identity(Name, Key).
proposed_semantic_identity(repo, repo_id).
proposed_semantic_identity(path, normalized_path).
proposed_semantic_identity(file, 'repo_id + path_id').
proposed_semantic_identity(blob, digest).
proposed_semantic_identity(rev_file, 'rev_id + file_id -> blob_id').
proposed_semantic_identity(blob_span, 'blob_id + start + end').
proposed_semantic_identity(file_span, 'rev_file_id + blob_span_id').

% storage_cell(Name, Shape).
storage_cell(interned_blob_span,
             'blob_span(blob_span_id,blob_id,start,end); fact points at blob_span_id').
storage_cell(embedded_blob_coordinate,
             'fact carries blob_id,start,end directly').
storage_cell(materialized_file_span,
             'file_span(file_span_id,rev_file_id,blob_span_id); fact points at file_span_id').
storage_cell(repeated_text_baseline,
             'fact repeats path,digest,enum spellings,start,end; measurement baseline only').

% sum_variant(Family, Variant, TotalColumns).
sum_variant(revision, committed, 'committed_rev(rev_id,repo_id,git_oid)').
sum_variant(revision, work, 'work_rev(rev_id,repo_id,root_id,base_rev_id)').
sum_variant(blob_source, git, 'git_blob(blob_id,repo_id,git_oid)').
sum_variant(blob_source, stored, 'stored_blob(blob_id,bytes)').
sum_variant(blob_source, observed, 'observed_blob(blob_id,source_id)').

% measurement(Name).
measurement(dbstat_table_and_index_bytes).
measurement(db_bytes_per_corpus_byte).
measurement(db_bytes_per_fact).
measurement(ingest_wall_ms).
measurement(peak_rss_bytes).
measurement(sql_statement_count).
measurement(query_plan_search_vs_scan).
measurement(distinct_span_reference_ratio).
measurement(repo_rev_path_filter_ms).
measurement(reverse_blob_placement_ms).
measurement(text_slice_ms).
measurement(line_column_ms).
measurement(git_cat_file_batch_ms).
measurement(sqlite_stored_blob_ms).
measurement(bounded_cache_bytes).

% observed(Name, Evidence).
observed(enum_lowering,
         'v6/prolog/0_enum_expand.pl expands variants to ordinary rels and tag rules').
observed(match_lowering,
         'v6/prolog/0_match_expand.pl expands each arm to an ordinary rule').
observed(current_enum_tag_storage,
         '0_enum_expand.pl currently declares enum tag as text; lab requires numeric closed ordinals for physical fact rows').
observed(host_plan,
         'sh_decl/probe lowers to demand and response rels through 1_host_expand.pl; served runtime currently names live_sh execution').
observed(bind_plan,
         'bind_decl lowers a registered world source subscription to EDB arrivals').
observed(current_struct_storage,
         'declared structs intern canonical semantic and rendered JSON rows; unsuitable as the physical file_span representation').
observed(assign_lab,
         'plans/2026-07-29-assign-composition-verdict.md measured := as redundant sugar; seven coordinate-concat sites dissolve under file_span').
observed(real_span_census,
         '1048 tracked source files, 7345805 span references, 2073233 distinct spans, mean 3.543 references per span; 2053038 spans have >=2 refs and 1275701 have >=3').
observed(schema_fixed_three_refs,
         '345600 facts: direct file_span 34.99 bytes/fact, located_ref 42.10, span_ref 45.19, embedded 52.94, repeated text 329.30; two repeats byte-identical in size').
observed(schema_one_ref,
         '115200 facts: embedded 69.19 bytes/fact, span_ref 70.76, direct file_span 75.34, located_ref 96.68').
observed(schema_two_refs,
         '230400 facts: direct file_span 44.80 bytes/fact, span_ref 51.00, located_ref 55.47, embedded 56.44').
observed(schema_real_census_distribution,
         '405696 facts replaying capped real multiplicities: located_ref 38.30 bytes/fact, span_ref 43.89, embedded 51.95, repeated text 315.31; both repeats same sizes').
observed(schema_direct_file_span,
         'file_span(file_span_id,rev_file_id,start,end) removes blob_span and wins at 32.24 bytes/fact; 517.6/530.5ms ingest; 77.9/78.5MB peak RSS; filter and reverse paths are covering SEARCH only').
observed(schema_real_census_ingest,
         'located_ref 585.2/593.9ms, span_ref 588.8/588.7ms, embedded 599.8/610.9ms, repeated text 1957.3/2006.1ms').
observed(schema_real_census_rss,
         'located_ref 70.5/70.3MB, span_ref 72.6/72.6MB, embedded 85.8/86.3MB, repeated text 98.9/100.1MB').
observed(content_git_batch,
         '300 tracked blobs, 8,498,015 source bytes, three read rounds: persistent git cat-file --batch 58.25/58.88ms').
observed(content_sqlite_store,
         'same 300 blobs and three read rounds: SQLite stored_blob 12.85/12.88ms; database 8,667,136 bytes; rowid SEARCH').
observed(content_bounded_cache,
         '1MiB limit peaked at 1,048,564 bytes and retained 153 of 300 blobs; newline indexes used 212,892 bytes').
observed(path_cells_first_run,
         '2996 real tracked paths, 20 references/path: whole-path dictionary 954,368 bytes; segment dictionary+junction 1,146,880; repeated path text 3,448,832. Prefix query rerun pending after switching LIKE to indexed GLOB').
observed(path_cells_final,
         'whole path prefix GLOB 0.0385/0.0387ms with covering range SEARCH; segments 0.0573/0.0588ms with two SEARCHes; repeated path 1.022/1.023ms').
observed(enum_union_caveat,
         'enum variants already lower to total variant rels and match to ordinary rules; generated enum tag relation currently stores text and must use an ordinal or be omitted from physical rows').
observed(blob_source_cardinality,
         'git_blob and stored_blob are additive capability rels, not exclusive variants; the same BlobId may inhabit both without NULL or identity rewrite').
observed(host_uniformity,
         'sh_decl/probe has one demand-response rel lowering and IHostPlan data shape, but its live executor is shell-specific; bind_decl is the continuous source path. File content reads fit a typed non-shell executor on the host-plan data path without new function syntax').

% inherited_landing(Name, MergeSha, Evidence).
inherited_landing(finish_the_job_epic, '21ecd6ac',
                  '12 epics and 12 user cards; 61 unsupported split into 41 intentional refusals and 26 construct debts; critical path simplify -> phase5 -> schema import').
inherited_landing(world_health_reconcile, '3cb67af3',
                  'ARCH and plans/2026-07-29-v6-world-health.md reconciled; exposed malformed-json wrong answer, 278 unexplained flow edges, empty flow_node_type, construct-ledger drift, lsp child leak, and missing amplification sensors').
inherited_landing(assign_composition_lab, 'd0104974',
                  '30-site census; 19/19 desugar equivalence; := provides local naming only; seven coordinate concat sites dissolve under file_span; three user cards and two compiler defects remain').
inherited_landing(file_span_handoff, '83538cef',
                  'plans/2026-07-29-file-span-design.md plus standing no-unsighted-surface-spelling rule').
inherited_landing(session_handoff, '29977137',
                  'chat_log/20260729.2.fable-closeout-handoff-to-sol.md records battery and reading order').

% touched(Path, Reason).
touched('plans/2026-07-29-file-identity-span-spine.md',
        'initial semantic identity, lifetime, storage, wire, and verification design').
touched('PLANS.md',
        'generated index entries for the initial design decision cards').
touched('v6/prolog/ARCH.pl',
        'in-flight file_span_storage_lab task with intent, cells, metrics, and exit conditions; stale assign and finish lane statuses reconciled after their merges').
touched('chat_log/20260729.3.file-span-storage-lab.pl',
        'this resumable live session ledger').
touched('v6/sprefa-store/bench/file_span/0_bench.py',
        'fresh-process null-free schema benchmark with fixed and real-census multiplicity cells').
touched('v6/sprefa-store/bench/file_span/1_content.py',
        'persistent git batch versus SQLite stored-content and bounded cache measurement').
touched('v6/sprefa-store/bench/file_span/2_census.py',
        'real sprefa-extract span-reference multiplicity census').
touched('v6/sprefa-store/bench/file_span/3_paths.py',
        'whole-path dictionary versus segment normalization and repeated path text').
touched('v6/sprefa-store/bench/file_span/results.json',
        'two schema repeats at three references per located span').
touched('v6/sprefa-store/bench/file_span/results-f1.json',
        'two schema repeats at one reference per located span').
touched('v6/sprefa-store/bench/file_span/results-f2.json',
        'two schema repeats at two references per located span').
touched('v6/sprefa-store/bench/file_span/results-census.json',
        'two schema repeats using real extractor multiplicity distribution capped at 32').
touched('v6/sprefa-store/bench/file_span/census-results.json',
        'real extractor census over 1048 source files').
touched('v6/sprefa-store/bench/file_span/content-results-1.json',
        'content source repeat 1').
touched('v6/sprefa-store/bench/file_span/content-results-2.json',
        'content source repeat 2').
touched('v6/sprefa-store/bench/file_span/path-results-1.json',
        'path representation repeat 1 with indexed GLOB prefix query').
touched('v6/sprefa-store/bench/file_span/path-results-2.json',
        'path representation repeat 2 with indexed GLOB prefix query').
touched('plans/2026-07-29-file-span-storage-lab.md',
        'plain-language result, complete measurements, selected schema, host/kernel boundary, two user cards').

% next_step(Order, State, Action).
next_step(1, done,
          'write deterministic executable schema benchmark under v6/sprefa-store/bench').
next_step(2, done,
          'run every schema cell twice at fixed scale and preserve raw results').
next_step(3, done,
          'benchmark persistent git cat-file batch against optional SQLite-stored bytes and bounded newline cache').
next_step(4, done,
          'rerun whole-path/segments/repeated path cells after indexed GLOB correction').
next_step(5, done,
          'write plans/2026-07-29-file-span-storage-lab.md with measurements and verdict').
next_step(6, done,
          'update ARCH task status and this ledger with the selected representation').
next_step(7, done,
          'run ARCH check, plans index check, and git diff check').

% decision_pending(Name, Options).
decision_pending(relation_reference_spelling,
                 'generic ref(Relation) semantics selected; surface spelling requires user review').
decision_pending(typed_host_declaration_spelling,
                 'typed executor on the existing host plan selected; authoring spelling requires user review').

% leading_verdict(Name, Value, Basis).
leading_verdict(span_storage, direct_materialized_file_span,
                'file_span(file_span_id,rev_file_id,start,end): real extractor multiplicity replay 32.24 bytes/fact, 15.8% below two-level located ref, 26.5% below span_ref, 37.9% below embedded').
leading_verdict(blob_content, additive_source_relations,
                'git is queryable later; stored bytes are faster and optional; one BlobId may have both git_blob and stored_blob rows').
leading_verdict(revision_sum, per_variant_relations,
                'committed_rev and work_rev carry total columns; common membership is UNION ALL; match reads variants directly').
leading_verdict(path_storage, whole_path_dictionary,
                'first path run: 954,368 bytes vs 1,146,880 segmented and 3,448,832 repeated; corrected prefix timing pending').
leading_verdict(host_boundary, typed_host_plan_executor,
                'reuse demand/response rel lowering and EDB arrivals; register a non-shell executor for blob_span text/position; no file-specific surface syntax').

% Follow-on relation/value unification, after the span storage result exposed
% the collision between current struct ref(Type) and relation identities.
touched('plans/2026-07-29-rel-value-unification-lab.md',
        'removes the type declaration category and specifies rel values, RHS membership, nested matching, storage, and the reference-lab boundary').
touched('v6/prolog/labs/rel_value_unification/0_rel_value_unification.pl',
        '14-check executable semantics model for rel values without ref spelling').

observed(previous_types_as_rels_verdict,
         'plans/2026-07-28-types-as-rels-verdict.md selected one rel construct with value and entity policy bundles; the later struct arc changed only the declaration spelling').
observed(type_word_origin,
         '0_type_plane.pl says type was chosen so emitted-only dictionary rows could not become program-nameable boundary rows').
discarded_evidence(rel_value_semantic_models,
                   'four standalone Prolog model files were removed after user clarified that labs must exercise the existing parser/compiler/runtime world').

leading_hypothesis(declaration_unification, rel_only,
                   'type may be removed; a bare relation name in column type position can denote the relation row domain while preserving current struct storage invariants').
leading_hypothesis(rhs_rel_use, contextual,
                   'top-level rel atom is membership; nested rel constructor in a relation-domain column can dereference and match; plain variable carries opaque identity').

decision_pending(top_level_rel_identity_capture,
                 'separate ref lab compares how a top-level RHS relation scan may also bind the row identity').
decision_pending(rel_function_shape,
                 'fixpoint iteration compares rel A->B with ordinary relations plus Prolog-style modes across SQL, Rx clocks, and host bindings').
next_fixpoint(actual_world_baseline,
              'census current type dependencies, recover inline migration twin, then add a lab-only rel-surface normalization and compare real plans, emitted outputs, SQLite schemas, ticks, bytes, and statements').
observed(actual_type_census,
         '20 type declarations in 19 dl6 files; 18 struct conformance fixtures; six Prolog modules read type_decl; three compiler/runtime files call column_storage').
observed(actual_rel_surface_refusal,
         'current parser accepts rel span and rel mark(at: span), then the real checker refuses column_type_unknown(span) because only type_decl enters type_definitions').
observed(actual_rel_surface_normalization,
         'lab-only normalization of a referenced rel declaration to current type_decl IR yields identical real analyzer plans, lowered SQL plans, and emitted TypeScript bytes for the minimal pair').
next_fixpoint(queryable_runtime_representation,
              'retain rel span as a queryable relation while preserving value interning, rendering, boundary invisibility of support rows, actual schedule execution, and SQLite shape').

% User-directed removal of the unauthorized `type` surface word.
directive(remove_type_keyword,
          'remove it now, report the cost, and add no replacement constructs').
touched('v6/prolog/compile/parse_dl.pl',
        'deleted the type parser branch; a rel referenced from a rel, sh, or bind column domain normalizes to existing type_decl IR').
touched('v6/prolog/compile/print_dl.pl',
        'canonical internal type_decl rendering now spells rel').
touched('editors/vscode-dl/syntaxes/dl6.tmLanguage.json',
        'removed type from DL6 declaration highlighting').
touched('v6/prolog/compile/SYNTAX.md',
        'records referenced rel as the surface for existing value IR').
touched('v6/prolog/compile/test/plunit_tests.pl',
        'migrated host fixtures and added removal plus normalization coverage').
observed(type_removal_cost,
         'internal type_decl IR and dictionary implementation remain; a referenced rel is value-only, cannot simultaneously be a public queryable rel, and classification changes when another declaration names it as a column domain').
observed(type_removal_scope,
         '20 live DL6 declarations migrated; no ref, arrow, mode, host, clock, or runtime construct added').
verification(type_removal_focused, passed(6)).
verification(type_removal_plunit, passed(142)).
verification(type_removal_conformance, passed(163)).
verification(type_removal_roundtrip, passed(163)).
verification(type_removal_sweep,
             result(102, 100, 0, 2,
                    'compiled, identical, wrong, recorded run errors')).
next_fixpoint(queryable_relation_values,
              'measure how the existing kernel can expose one referenced relation for ordinary RHS querying without restoring a declaration split or adding surface syntax').

directive(reference_is_relation_edge,
          'entities and relations only; a typed relation reference is a graph edge; no dictionary or stored JSON representation').
discarded_implementation(value_only_rel_normalization,
                         'referenced rel disappeared from public RelPlans and retained dictionary storage').
observed(reference_relation_prototype,
         'one ordinary target table with hidden dense __id, parent INTEGER endpoint, temporary join view, direct RHS target query, and no __dict/__semantic/__rendered storage').
verification(reference_relation_hole_lab, passed(9)).
verification(reference_relation_sweep,
             result(102, 91, 9, 2,
                    'compiled, unchanged identical, expected old-oracle disagreements, pre-existing run errors')).
observed(reference_relation_holes,
         'existing key is not yet used as edge identity; old content-DAG cycle refusal still rejects a keyed entity cycle').
leading_hypothesis(no_new_concept,
                   existing_rel_key_and_rules,
                   'column domain names the target rel; key names semantic identity; unkeyed set defaults to full-row identity; existing rules and clocks govern edges and lifetime').
next_fixpoint(keyed_reference_identity,
              'make existing key positions drive insert, lookup, replacement, and cycle admissibility; test single and composite keys before changing any surface').
