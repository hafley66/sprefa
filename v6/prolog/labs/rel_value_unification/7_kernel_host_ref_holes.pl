% Actual parser/compiler boundary checks for file spans, host work, and ref.
%
% Run:
%   swipl -q -l v6/prolog/labs/rel_value_unification/7_kernel_host_ref_holes.pl -g go -g halt

:- use_module('../../src/grader.pl').
:- use_module('../../compile/parse_dl.pl', [parse_dl_file/4, parse_dl/4]).
:- use_module('../../compile/compile.pl', [program_plan/2]).
:- use_module('../../compile/lower.pl', [lower_program/2]).
:- use_module('../../compile/registry.pl', [expression/5]).

:- dynamic lab_directory/1.
:- prolog_load_context(directory, Directory), assertz(lab_directory(Directory)).

go :- run(check).

parse_text(Text, Program, Bindings) :-
    string_codes(Text, Codes),
    parse_dl(Codes, Program, Bindings, []).

plan_text(Name, Text, Plan) :-
    parse_text(Text, Program, Bindings),
    program_plan(fixture(Name, Program, [], [], [])-Bindings, Plan).

lower_text(Name, Text, Plan, Lowered) :-
    plan_text(Name, Text, Plan),
    lower_program(Plan, Lowered).

file_span_lowered(Plan, Lowered) :-
    lab_directory(Directory),
    directory_file_path(Directory, '6_file_span_kernel.dl6', Path),
    parse_dl_file(Path, Program, Bindings, []),
    program_plan(fixture(file_span_kernel, Program, [], [], [])-Bindings, Plan),
    lower_program(Plan, Lowered).

check(slice_is_existing_relational_arithmetic,
      ( file_span_lowered(_, lowered(_, _, _, _, Statements, _, _, _)),
        member(levelstmt(sliced_span/4, _, Inserts, _, _, _), Statements),
        member(Sql, Inserts),
        sub_atom(Sql, _, _, _, '"start" +'),
        sub_atom(Sql, _, _, _, '"relative_end"') )).

check(line_ordinal_is_existing_count_aggregate,
      ( file_span_lowered(_, lowered(_, _, _, _, Statements, _, _, _)),
        member(levelstmt(preceding_newline_count/2, _, _, _, _, aggsql(_, _, _, _, _, _)),
               Statements) )).

check(column_anchor_is_existing_max_aggregate,
      ( file_span_lowered(_, lowered(_, _, _, _, Statements, _, _, _)),
        member(levelstmt(preceding_newline_max/2, _, _, _, _, aggsql(_, _, _, _, _, _)),
               Statements) )).

check(content_slice_has_no_current_pure_expression,
      \+ expression(substring/3, _, _, _, _)).

check(content_slice_candidate_reaches_real_refusal,
      ( Text = "rel blob_bytes(blob_id: int, bytes: text).\nrel file_span(file_span_id: int, blob_id: int, start: int, end: int).\nrel span_text(file_span_id: int, text: text).\nspan_text(FileSpanId, Text) <- file_span(FileSpanId, BlobId, Start, End), blob_bytes(BlobId, Bytes), substring(Bytes, Start, End).\n",
        plan_text(content_slice_hole, Text, Plan),
        catch(lower_program(Plan, _),
              unsupported_construct(_),
              Refused = yes),
        Refused == yes )).

check(relation_shaped_reference_constructor_is_currently_stored_as_json_term,
      ( Text = "rel span(start: int, end: int).\nrel finding(at: span).\nfinding(span(Start, End)) <- span(Start, End).\n",
        lower_text(reference_constructor_hole, Text, _, Lowered),
        Lowered = lowered(_, _, _, _, Statements, _, _, _),
        member(levelstmt(finding/1, _, Inserts, _, _, _), Statements),
        member(Sql, Inserts),
        sub_atom(Sql, _, _, _, 'json_object'),
        sub_atom(Sql, _, _, _, '''span''') )).

check(implicit_reference_identity_capture_is_unbound,
      ( Text = "rel span(start: int, end: int).\nrel finding(at: span).\nfinding(SpanRef) <- span(Start, End).\n",
        parse_text(Text, Program, Bindings),
        program_plan(fixture(implicit_reference_capture_hole,
                             Program, [], [], [])-Bindings,
                     Plan),
        catch(lower_program(Plan, _),
              unsupported_construct(unbound_head_var(_)),
              Refused = yes),
        Refused == yes )).

check(ref_has_no_registered_semantics,
      ( \+ expression(ref/1, _, _, _, _),
        Text = "rel span(start: int, end: int).\nrel finding(at: span).\nfinding(ref(span(Start, End))) <- span(Start, End).\n",
        lower_text(ref_surface_hole, Text, _, Lowered),
        Lowered = lowered(_, _, _, _, Statements, _, _, _),
        member(levelstmt(finding/1, _, Inserts, _, _, _), Statements),
        member(Sql, Inserts),
        sub_atom(Sql, _, _, _, '''ref''') )).

check(interned_string_is_expressible_as_an_ordinary_referenced_rel,
      ( Text = "rel string(content: text).\nrel path(text: string).\n",
        lower_text(string_reference, Text, Plan, Lowered),
        Plan = plan(_, _, RelPlans, _, _, _),
        memberchk(relplan(string/1, set, [content], none, [text]), RelPlans),
        memberchk(relplan(path/1, set, [text], none, [ref(string)]), RelPlans),
        Lowered = lowered(_, Ddl, _, _, _, _, _, _),
        atomic_list_concat(Ddl, '\n', Sql),
        sub_atom(Sql, _, _, _, 'CREATE TABLE "string"'),
        \+ sub_atom(Sql, _, _, _, '__dict_') )).
