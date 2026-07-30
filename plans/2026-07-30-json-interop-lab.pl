% JSON interop lab record.
%
% Consult:
%   swipl -q -l plans/2026-07-30-json-interop-lab.pl
%
% Validate this record:
%   swipl -q -l plans/2026-07-30-json-interop-lab.pl -g go -g halt
%
% Execute current-world receipts:
%   swipl -q -l v6/prolog/labs/json_interop/0_receipts.pl -g go -g halt
%
% This lab makes no new decision. The one-rel boundary is an inherited locked
% constraint. No candidate surface spelling is proposed. Cards enumerate
% bounded questions for a later user ruling.

:- module(json_interop_lab,
          [ go/0,
            plan_section/2,
            goal/2,
            current_fact/3,
            boundary/3,
            receipt/3,
            gap/3,
            open_card/2,
            card_option/4
          ]).

:- use_module(library(aggregate), [aggregate_all/3]).

:- discontiguous open_card/2.
:- discontiguous card_option/4.

plan_section(context,
             'Current JSON behavior spans the oracle, struct/reference lowering, host expansion, SQLite json1, and named compiler refusals').
plan_section(decisions,
             'No new decisions; inherit one rel model, one checker, integer reference edges, generated joins, and no stored nested JSON').
plan_section(verification,
             'Run the 12 executable receipts and the plan integrity gate; both commands must exit zero').
plan_section(staffing,
             'Single Codex lane, shared worktree, base 9aa853bf76ac, two focused SWI-Prolog commands, no production ownership').

goal(current_world,
     'measure the existing oracle, compiler, SQLite, host, reference, and clock behavior without adding a language form').
goal(relational_storage,
     'normalize every queryable object and array into ordinary target rows plus integer reference edges; JSON text remains interchange only').
goal(optional_interop,
     'price additive JSON capability and opt-out while recording that V6 currently has no module import').
goal(stop_condition,
     'stop at metadata and decision cards if any route needs a new surface spelling').

% Current implementation facts

current_fact(value_representation,
             'v6/prolog/conformance/body.pl',
             'objects are obj(sorted key-value pairs); arrays are Prolog lists; scalars remain ordinary values').
current_fact(typed_object_storage,
             'v6/prolog/0_type_plane.pl and compile/lower.pl',
             'a rel column naming another rel stores a dense integer ref; the target rel stores fields; boundary rendering uses a __ref join view').
current_fact(untyped_json_storage,
             'v6/prolog/0_type_plane.pl:column_storage/3',
             'json currently maps to TEXT on the inline json1 path; the locked one-rel boundary classifies this as migration debt, not a target storage model').
current_fact(array_storage,
             'v6/prolog/0_type_plane.pl',
             'typed list/array storage is absent; finite untyped arrays remain inline list values; list(text) is an unknown column type').
current_fact(enum_storage,
             'v6/prolog/0_enum_expand.pl',
             'each variant becomes a relation and a derived tag relation; no JSON tag object is stored').
current_fact(null,
             'body.pl and 0_type_plane.pl',
             'none and null are ordinary atoms rendered as JSON strings; a bare object decode rejects none; SQLite NULL is not a language value').
current_fact(clock,
             'v6/prolog/conformance/engine.pl',
             'JSON aggregate rows use the same B-set boundary deltas and retractions as other level relations').
current_fact(recursion,
             'v6/prolog/0_type_plane.pl:type_cycle_witness/2',
             'finite nested JSON renders recursively; content-addressed relation values form a DAG; cyclic value-type references are refused').
current_fact(host,
             'v6/prolog/1_host_expand.pl',
             'host inputs and outputs keep declared column types on demand/response relations; a typed object output reaches ref storage').
current_fact(module_import,
             'v6/prolog/compile/parse_dl.pl',
             'V6 has no module-import production; JSON oracle semantics are globally present').
current_fact(schema_import,
             'v6/prolog/ARCH.pl',
             'schema_import_epic is unbuilt; the parser has no schema-import production').
current_fact(sqlite,
             '/usr/bin/sqlite3 json1',
             'json_object, json_group_object, json_extract, and json_each execute in the system SQLite').
current_fact(compiler_gap,
             'v6/prolog/compile/registry.pl and SCOREBOARD.md',
             'json_array/json_object aggregate heads are named refusals in the compiler even though the oracle executes them').

% Boundaries

boundary(language_relations,
         accepts,
         'entities, enum variants, declared object fields, reference edges, joins, antijoins, clocks, and retractions').
boundary(untyped_json,
         transient_only,
         'host, SQLite, and schema adapters may parse or construct interchange text; stored program state must be target rows and integer reference edges').
boundary(host,
         accepts,
         'typed demand/response target rows; JSON text may cross the executor wire and must normalize before relational storage').
boundary(sqlite_json1,
         accepts,
         'transient construction, extraction, array fan-out, and wire encoding; json1 values do not define a second stored value model').
boundary(semantic_model,
         locked,
         'one rel model and one checker: a rel column naming another rel lowers to the target row plus an integer reference edge and generated joins').
boundary(schema_loader,
         absent,
         'no JSON Schema, OpenAPI, or TypeSpec import reaches program metadata today').
boundary(module_loader,
         absent,
         'no optional JSON module can currently be imported or omitted').

% Executed receipts

receipt(relational_object_array, pass,
        'decode and json_each derive ordinary rows; oracle json_array re-aggregates them').
receipt(no_null, pass,
        'none/null atoms render as strings; none fails a bare field match').
receipt(enum_variants, pass,
        'enum expansion produces variant relations and a tag relation').
receipt(relation_reference, pass,
        'lowering emits INTEGER NOT NULL parent refs, a typed value table, and a render view').
receipt(host_boundary, pass,
        'host output response relation retains the declared relation-valued column type').
receipt(sqlite_json1, pass,
        'system SQLite constructs, extracts, groups, and explodes JSON').
receipt(clock_retraction, pass,
        'aggregate value replacement emits -old,+new and exact final state').
receipt(recursive_values, pass,
        'finite nesting renders; cyclic typed value refs and typed lists are current refusals').
receipt(storage_amplification, pass,
        '10,000 repeated object values occupy more than five times the SQLite bytes of one relational value row plus 10,000 dense refs').
receipt(optional_module_absence, pass,
        'the existing V5 use form is refused by V6 while JSON remains globally executable in the oracle').
receipt(schema_import_absence, pass,
        'ARCH records unbuilt and parser source contains no schema_import production').
receipt(opt_out, pass,
        'a scalar-only program emits no reference target table/view; aggregate JSON remains a named compiler refusal').

% Gaps exposed by the receipts

gap(array_relation,
    current,
    'the prior cons-cell design is documented but absent from 0_type_plane.pl and the compiler').
gap(json_aggregate_parity,
    current,
    'oracle supports json_array/json_object; compiler keeps four aggregate-head fixtures unsupported').
gap(untyped_decode_parity,
    current,
    'untyped decode/json_each still account for the JSON destructure unsupported family').
gap(null_ingress,
    boundary,
    'external JSON null needs a rejection or explicit relational mapping before host/schema import can be total').
gap(schema_mapping,
    boundary,
    'required/optional fields, unions, recursive schemas, numeric precision, additionalProperties, and references lack program-metadata mappings').
gap(module_semantics,
    boundary,
    'there is no import graph, capability closure, version identity, or absence behavior to test at the DL level').
gap(storage_policy,
    migration,
    'relation refs and inline JSON TEXT coexist today; the locked boundary requires stored inline JSON to be normalized or refused').

% Open cards, with no selection.

open_card(json_residency,
          'Where does JSON capability reside once module support exists?').
card_option(json_residency, core_global,
            'Keep a global JSON wire adapter that emits ordinary relations.',
            'No import graph cost; every implementation carries adapter behavior while the checker sees only rels').
card_option(json_residency, optional_additive_module,
            'A module contributes JSON-to-rel adapter metadata and lowering hooks.',
            'Requires a general module system and absence tests; generated output still enters the one rel checker').
card_option(json_residency, host_only,
            'Hosts own JSON parsing and emit normalized rows and reference edges.',
            'Shrinks kernel adapter work; loses in-language json_each/decode and SQL fusion').

open_card(array_storage,
          'How are queryable arrays represented in the one rel model?').
card_option(array_storage, cons_relations,
            'Use fixed-arity ordinary cons/nil rels from the prior lab.',
            'Content identity and sharing work through ordinary rows; traversal adds joins and implementation is currently absent').
card_option(array_storage, indexed_elements,
            'Use array header plus (array_id,index,value) rows.',
            'Direct positional queries; whole-array content identity and mutation policy need separate rules').
card_option(array_storage, host_flattened,
            'The boundary emits only application-specific element relations.',
            'No generic array relations; each host/schema adapter owns the projection and loses generic reconstruction').
card_option(array_storage, refuse_arrays,
            'Reject queryable arrays until a relational mapping is declared.',
            'Keeps storage and checker singular; external array inputs require preprocessing').

open_card(null_and_optional,
          'How does external JSON null map into the no-null relational world?').
card_option(null_and_optional, reject_null,
            'Reject null at host/schema boundaries.',
            'Keeps the language total and null-free; external schemas containing null need preprocessing').
card_option(null_and_optional, row_absence,
            'Map null or missing optional fields to absence of an edge/row.',
            'Uses current relational absence; missing and explicit null become indistinguishable').
card_option(null_and_optional, explicit_variant,
            'Map nullable T to variant relations such as value/none.',
            'Preserves explicit null; adds rows and requires schema-generated variant identity').

open_card(schema_import_boundary,
          'What may schema import contribute without changing runtime semantics?').
card_option(schema_import_boundary, metadata_only,
            'Generate relation, column, variant, ref, and boundary-check metadata.',
            'Reuses the one checker/lowerer; unsupported schema features must produce named findings').
card_option(schema_import_boundary, metadata_plus_rules,
            'Generate metadata and ordinary normalization/validation rules.',
            'Can express field projections and unions; generated rule volume and clocks enter grading').
card_option(schema_import_boundary, host_pre_normalized,
            'External tooling emits already normalized relation rows.',
            'Small compiler surface; schema identity and validation move to the host contract while storage stays relational').

open_card(recursive_identity,
          'How do recursive schemas cross the value-DAG boundary?').
card_option(recursive_identity, refuse_cycles,
            'Accept only finite acyclic value schemas.',
            'Content identity and structural sharing remain simple; recursive entity documents are excluded').
card_option(recursive_identity, entity_relations,
            'Map recursive nodes to extrinsic entity ids and ordinary reference edges.',
            'Cycles become queryable; identity, lifetime, mutation, and retraction must be explicit').
card_option(recursive_identity, bounded_unroll,
            'Emit relational rows to a configured depth plus an explicit truncation relation.',
            'Finite storage with no nested blob fallback; queries and completeness become depth-dependent').

go :-
    forall(member(Section, [context, decisions, verification, staffing]),
           plan_section(Section, _)),
    aggregate_all(count, open_card(_, _), CardCount),
    CardCount =< 5,
    forall(
        open_card(Card, _),
        ( aggregate_all(count, card_option(Card, _, _, _), OptionCount),
          OptionCount >= 2,
          OptionCount =< 5
        )),
    aggregate_all(count, receipt(_, pass, _), ReceiptCount),
    ReceiptCount =:= 12,
    \+ current_predicate(selected/2),
    format("JSON_INTEROP_LAB_VALID cards=~d receipts=~d decisions=0~n",
           [CardCount, ReceiptCount]).
