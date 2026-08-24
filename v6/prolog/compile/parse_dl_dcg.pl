
:- module(parse_dl_dcg,
          [ parse_dl_dcg_entry/5,
            parse_dl/4,
            parse_dl_file/4,
            parse_dl_line_for_reason/2,
            remaining_line_column/3,
            statement_location_for_reason/3,
            statement_location_for_reference/4,
            use_item/3,
            parse_dl_source/5
          ]).

:- set_prolog_flag(back_quotes, codes).

% lists/apply/pairs ride SWI autoloading; both module imports take the whole
% export list because no name here collides with theirs.
:- use_module(registry).
:- use_module('../0_cst_query').
:- use_module('../0_type_plane', [type_wrapper/2, column_element_type_name/2]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% Terminal sigils: a module-local prefix-operator DSL. @Codes matches the
% literal right here, ~Codes adds a word boundary, #Codes skips ws then @Codes.
:- op(200, fy, [#, @, ~]).

:- thread_local finding_fact/1, rel_column_order_fact/2,
                host_signature_fact/3, host_path_fact/2,
                source_statement_fact/3,
                parse_marks_on/0.

% THREAD_LOCAL, not dynamic: parse_dl_source/5 retracts all four at entry and
% reads them back at exit, so two parses running at once on shared clauses would
% each erase the other's findings. The plunit battery runs units on parallel
% workers, and every unit parses.

% lex_token/2 rows sit beside the escape decoders they mirror, so the clauses
% are spread across the file on purpose.
:- discontiguous type_base/3.

% Editor CST boundaries this parser erases: Nonterminal -> Node-FieldNames,
% bare = shape from clauses, ref = name only, repeat = item only, '-' = unnamed.

:- include('parse_dl_dcg/0_cst_shapes.pl').

:- include('parse_dl_dcg/1_entry.pl').

:- include('parse_dl_dcg/2_lexer.pl').

:- include('parse_dl_dcg/3_use_and_router.pl').

:- include('parse_dl_dcg/4_rel_decl.pl').

:- include('parse_dl_dcg/5_name_resolution.pl').

:- include('parse_dl_dcg/6_host_and_template.pl').

:- include('parse_dl_dcg/7_query_and_match.pl').

:- include('parse_dl_dcg/8_rule_and_args.pl').

:- include('parse_dl_dcg/9_body.pl').

:- include('parse_dl_dcg/10_expr.pl').
