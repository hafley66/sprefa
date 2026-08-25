% Generic expansion closes schema templates before enum expansion.
%
% The artifact table uses typed records.  `lower_artifacts/2` is the one
% boundary where a template's schema records become the program's Decl terms.
% Round one emits declarations only.  Rules remain author-written.
:- module(generic_expand,
          [ expand_generic_in_context/3,
            expand_generic_program/2,
            expand_generic_program_raw/2,
            canonical_type_name/2,
            canonical_type_encoding/2,
            generic_artifact_order/3,
            generated_generic_name/1,
            generic_type_ir/2,
            freeze_type_rows/2,
            normalize_key_wrappers/2,
            schema_member_rows/2,
            compiler_type_source_rows/3,
            type_relation_rows/2,
            schema_member_transport_rows/3,
            expand_generic_program_with_bindings/3,
            reset_type_row_memo/0
          ]).

:- use_module(library(apply)).
:- use_module(library(assoc)).
:- use_module(library(pairs), [group_pairs_by_key/2]).
:- use_module(library(crypto)).
:- use_module(library(lists)).
:- use_module('0_trace', [run_compile_step/4]).
:- use_module('0_option_expand', [expand_option_decls/2, scalar_element/1]).
:- use_module('0_enum_expand', [enum_type_rows/2]).
:- use_module('../0_dot_expand/0_type_plane', [unwrapped_column_type/2]).
:- use_module('0_anonymous_expand', [expand_anonymous_decls/2]).
:- use_module('0_annotation_expand', [elaborate_annotation/3]).
:- use_module('0_type_ids',
              [ decl_id/4, primitive_id/2, param_id/4, member_id/4,
                constraint_id/3, app_id/3, arg_id/3,
                id_kind_name/3 ]).
:- use_module('../0_compiler_relations',
              [ partition_compiler_program/5,
                evaluate_compiler_relations/3,
                compiler_type_apply_requests/3 ]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- discontiguous replace_generic_type/3.
:- discontiguous generated_decl_module/4.

:- include('../0_generic_expand/0_expand.pl').

:- include('../0_generic_expand/0a_type_apply_requests.pl').

:- include('../0_generic_expand/0b_expansion_pipeline.pl').

:- include('../0_generic_expand/1_annotations.pl').

:- include('../0_generic_expand/2_compiler_plane.pl').

:- include('../0_generic_expand/3_enum_templates.pl').

:- include('../0_generic_expand/4_type_views.pl').

:- include('../0_generic_expand/5_type_freeze.pl').

:- include('../0_generic_expand/6_type_conformance.pl').

:- include('../0_generic_expand/7_generic_instances.pl').

:- include('../0_generic_expand/8_type_rewrite.pl').

:- include('../0_generic_expand/8a_key_wrappers.pl').
