% FAIL-FIRST: query_stmt//1 read `)` then `.`, so every `order by` source below
% threw dl_parse_error(statement, _) before an assertion ran, and final_select
% carried no ORDER BY for any rel.
:- module(query_order_tail_tests, []).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- use_module(library(plunit)).
:- use_module(library(lists)).
:- use_module('../../7_lower/parse_dl_dcg', [parse_dl_dcg_entry/5]).
:- use_module('../../print_dl', [print_dl_program/3]).
:- use_module('../../compile', [read_fixture_term/4, program_plan/2]).
:- use_module('../../7_lower/lower', [lower_program/2, query_order_by_map/3]).

:- dynamic(order_tail_test_dir/1).
:- prolog_load_context(directory, Here), assertz(order_tail_test_dir(Here)).

:- begin_tests(query_order_tail).

ordered_source(Source) :-
    Source = "rel score(player: int, points: int).\c
\n? score(player, points) order by points desc, player.\n".

tailless_source(Source) :-
    Source = "rel score(player: int, points: int).\c
\n? score(player, points).\n".

parsed(Source, Prog, Bindings) :-
    string_codes(Source, Codes),
    parse_dl_dcg_entry(order_tail_test, Codes, Prog, Bindings, Findings),
    Findings == [].

% The position, not the word, is what the tail carries: it indexes the rel's
% own column list, so no emitter repeats the name lookup.
test(order_tail_resolves_each_column_to_its_argument_position) :-
    ordered_source(Source),
    parsed(Source, program(_, _, Queries), _),
    Queries = [query(Atom, order(OrderCols))],
    functor(Atom, score, 2),
    OrderCols == [order_col(2, desc), order_col(1, asc)].

test(a_tailless_query_keeps_the_query_1_term) :-
    tailless_source(Source),
    parsed(Source, program(_, _, Queries), _),
    Queries = [query(Atom)],
    functor(Atom, score, 2).

test(asc_is_the_direction_a_column_without_a_word_takes) :-
    parsed("rel score(player: int, points: int).\c
\n? score(player, points) order by points.\n",
           program(_, _, Queries), _),
    Queries = [query(_, order([order_col(2, asc)]))].

% The name the query never gave an argument is unknown, and the error says
% which query and which word.
test(an_unknown_order_column_is_a_parse_error_naming_the_query) :-
    string_codes("rel score(player: int, points: int).\c
\n? score(player, points) order by defs desc.\n", Codes),
    catch(parse_dl_dcg_entry(order_tail_test, Codes, _, _, _), Ball, true),
    Ball = dl_parse_error(Reason, _Position),
    Reason == order_column_unknown(score, defs).

test(an_anonymous_argument_names_no_order_column) :-
    string_codes("rel score(player: int, points: int).\c
\n? score(_, points) order by player.\n", Codes),
    catch(parse_dl_dcg_entry(order_tail_test, Codes, _, _, _), Ball, true),
    Ball = dl_parse_error(order_column_unknown(score, player), _).

% The text door has to hand the tail back verbatim or a rendered corpus loses
% every ordered read.
test(the_tail_round_trips_through_the_text_door) :-
    ordered_source(Source),
    parsed(Source, Prog, Bindings),
    print_dl_program(Prog, Bindings, Text),
    once(sub_atom(Text, _, _, _,
                  '? score(player, points) order by points desc, player.')).

score_relplans([rel(score/2, set,
                    [col(player, inferred, int), col(points, inferred, int)],
                    none)]).

% ONE definition of the clause, read by the emitter that appends it.
test(the_order_clause_is_sql_over_the_rels_own_column_names) :-
    ordered_source(Source),
    parsed(Source, program(Decls0, _, Queries), _),
    append(Decls0, Queries, Decls),
    score_relplans(RelPlans),
    query_order_by_map(Decls, RelPlans, Map),
    Map == [score-' ORDER BY "points" DESC, "player" ASC'].

test(a_tailless_query_puts_no_clause_in_the_map) :-
    tailless_source(Source),
    parsed(Source, program(Decls0, _, Queries), _),
    append(Decls0, Queries, Decls),
    score_relplans(RelPlans),
    query_order_by_map(Decls, RelPlans, Map),
    Map == [].

lowered_fixture(Name, Ddl, DeltaStatements) :-
    order_tail_test_dir(Dir),
    atomic_list_concat([Dir, '/../../conformance/fixtures/order_by_read.pl'],
                       File),
    once(( read_fixture_term(File, Name, Term, Bindings),
           program_plan(Term-Bindings, Plan),
           lower_program(Plan,
                         lowered(_, Ddl, _, _, _, DeltaStatements, _, _)) )).

% deltastmt's SelectSql is the tick path's own snapshot read; the tail lands on
% the emitter's final_select copy and this one has to stay clean.
test(the_tick_paths_snapshot_read_carries_no_order_clause) :-
    lowered_fixture(order_by_int_columns_read_the_base_table, _, DeltaStatements),
    forall(member(deltastmt(_, SelectSql, _, _, _), DeltaStatements),
           \+ sub_atom(SelectSql, _, _, _, 'ORDER BY')).

% All-int columns read the base table and the ordering is not a prefix of the
% all-columns UNIQUE, so the read earns one index.
test(an_ordered_base_table_read_mints_one_order_index) :-
    lowered_fixture(order_by_int_columns_read_the_base_table, Ddl, _),
    include(is_order_index_ddl, Ddl, [IndexDdl]),
    once(sub_atom(IndexDdl, _, _, _, '("points" DESC, "player" ASC)')).

is_order_index_ddl(Ddl) :- once(sub_atom(Ddl, _, _, _, '__order_1')).

% A text column reads through the intern VIEW, so an index on the base table
% would copy ids the ordered read never looks at.
test(an_ordered_read_over_the_intern_view_mints_no_index) :-
    lowered_fixture(order_by_desc_with_a_tie_break, Ddl, _),
    include(is_order_index_ddl, Ddl, []).

test(a_tailless_query_mints_no_index) :-
    lowered_fixture(query_without_an_order_tail_is_unmoved, Ddl, _),
    include(is_order_index_ddl, Ddl, []).

:- end_tests(query_order_tail).
