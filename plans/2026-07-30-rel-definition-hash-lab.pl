% Consultable result for:
%   swipl -q -l plans/2026-07-30-rel-definition-hash-lab.pl
%
% Runnable receipts:
%   swipl -q -l v6/prolog/labs/rel_definition_hash/0_receipts.pl -g go -g halt

plan(rel_definition_hash_lab, completed).
receipt_file('v6/prolog/labs/rel_definition_hash/0_receipts.pl').
receipt_result(11, pass).

context(current_ir, 'expanded prog/2 plus plan/6').
context(expansion_order,
        'host preparation, enum, match, and relation-edge expansion precede hashing').
context(lowering,
        'plan/6 lowers to fixed SQL text and physical relation names').

locked(hash_point,
       'hash the expanded checked candidate before SQL lowering; match sugar and handwritten rules then share identity').
locked(variable_identity,
       'copy the clause and number variables; source variable spelling does not enter a hash').
locked(relation_rename,
       'relation symbols become dependency slots in content hashes; authored names remain storage or binding identities').
locked(column_names,
       'declared column names remain in the shape hash because they are boundary and host-template names').
locked(order,
       'preserve source rule order and conjunction order in the exact-code hash').
locked(hosts,
       'normalize compiler-generated helper names and digest prefixes; include input/output schemas and exact template bytes').
locked(recursion,
       'hash a recursive SCC as one canonically labelled graph plus outgoing dependency closure hashes').

hash_layer(shape_hash,
           [storage_kind, columns_with_names, column_types, key, plane,
            clock_signature]).
hash_layer(direct_definition_hash,
           [shape_hash, alpha_normalized_expanded_clauses_in_order,
            dependency_slots_with_shape_hashes, host_contract]).
hash_layer(dependency_closure_hash,
           [canonical_scc_graph, direct_definition_content,
            outgoing_dependency_closure_hashes]).
hash_layer(stable_storage_identity,
           [authored_relation_ref_or_stable_call_site_id]).
hash_layer(specialization_code_key,
           [algorithm_name, algorithm_version, argument_content_hashes]).
hash_layer(specialization_instance_key,
           [specialization_code_key, storage_bindings, host_bindings]).

invalidation(shape_hash, column_name_change).
invalidation(shape_hash, column_type_change).
invalidation(shape_hash, key_change).
invalidation(shape_hash, storage_kind_change).
invalidation(shape_hash, plane_or_clock_change).
invalidation(direct_definition_hash, clause_change).
invalidation(direct_definition_hash, rule_order_change).
invalidation(direct_definition_hash, body_order_change).
invalidation(direct_definition_hash, host_template_byte_change).
invalidation(dependency_closure_hash, transitive_dependency_change).
invalidation(specialization_code_key, algorithm_version_change).
invalidation(specialization_instance_key, storage_or_host_binding_change).

compatibility(sqlite_layout_reuse, equal(shape_hash)).
compatibility(state_semantic_reuse, equal(dependency_closure_hash)).
compatibility(durable_table_location, equal(stable_storage_identity)).
compatibility(prepared_sql_reuse,
              'code key and physical storage bindings both equal').
compatibility(abstract_sql_template_reuse,
              'code key equal; physical identifiers may differ and are rendered per instance').

measured(alpha_equivalent_variable_names, equal_hash).
measured(systematic_relation_rename, equal_content_hash).
measured(declared_column_rename, different_shape_hash).
measured(rule_reorder, different_direct_hash).
measured(body_reorder, different_direct_hash).
measured(match_and_hand_desugaring, equal_direct_and_closure_hash).
measured(generated_host_rename, equal_content_hash).
measured(host_template_change, different_direct_hash).
measured(recursive_scc_rename, equal_closure_hash).
measured(recursive_scc_body_change, different_closure_hash).
measured(same_layout_changed_reducer,
         'equal shape hash and different closure hash').
measured(specialization_cache,
         calls(6, code_templates(3), storage_instances(6))).
measured(lowered_sql,
         'renamed raw SQL hashes differ; identifier-slot-normalized SQL hashes equal').

stateful_rule('identical scan and switchMap calls may reuse expansion, checking, and an abstract SQL template').
stateful_rule('each call site keeps a separate table and prepared statement unless it explicitly binds the same named state relation').
pure_rule('an identical pure specialization with identical physical bindings may reuse the prepared SQL statement').

collision_storage(algorithm, sha256).
collision_storage(database_encoding, '32-byte BLOB with UNIQUE constraint').
collision_storage(debug_encoding, '64-character lowercase hex outside hot rows').
collision_storage(on_digest_conflict,
                  'compare canonical payload bytes; reject unequal payloads under one digest').
collision_storage(row_policy,
                  'store one canonical payload per cached program object, not beside every fact row').

production_seam('v6/prolog/1_expansion.pl',
                'canonical program boundary after all sugar expansion').
production_seam('v6/prolog/compile/compile.pl',
                'plan/6 supplies relplan shape and ordered concrete rules').
production_seam('v6/prolog/compile/analyze.pl',
                'body_ref_uses/2 supplies dependency polarity and sampling roles').
production_seam('v6/prolog/1_host_expand.pl',
                'host_plan supplies typed templates and generated-helper ownership').
production_seam('v6/prolog/compile/lower.pl',
                'render physical identifiers from a cached abstract specialization').
production_seam(new_module,
                'numbered dependency-order module before compile/compile.pl').

production_sequence(0, 'add canonical shape and direct-definition terms').
production_sequence(1, 'add SCC closure hashing and cache records').
production_sequence(2, 'key scan and switchMap specialization by code key').
production_sequence(3, 'render one physical instance per stable storage binding').
production_sequence(4, 'expose hashes in checker reports before using them for durable migration').

known_limit(mutual_recursion,
            'production currently refuses recursive_stratum; the isolated SCC receipt hashes the same prog/2 term before that refusal').
known_limit(sql_identifier_binding,
            'SQLite statement parameters cannot bind table names, so renamed instances require rendered SQL text').

open_choice(scc_canonical_label_backend,
            'The lab uses exhaustive permutation, which is factorial in SCC size.',
            [ route(exhaustive_with_size_cap,
                    'zero dependency; refuse hashing above the cap; rename-stable only below the cap'),
              route(partition_refinement_with_backtracking,
                    'new canonical-label implementation and adversarial symmetry tests'),
              route(graph_canonicalization_library,
                    'library dependency and adapter from relation SCC terms')
            ]).

next_task(scan_specialization,
          'use code key for the compiled reducer template and instance key for its named keyed state relation').
next_task(switch_map_expansion,
          'use the same split for reusable scope logic and separate active-scope tables').
