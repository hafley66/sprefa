% Content-addressed, process-local caches for immutable compiler results.

:- module(dl7_compiler_cacher,
          [ with_prelude_cache/5,
            with_compilation_cache/5,
            with_rule_check_cache/7,
            clear_compiler_caches/0
          ]).

:- meta_predicate with_prelude_cache(+, 2, -, -, -).
:- meta_predicate with_compilation_cache(+, 2, -, -, -).
:- meta_predicate with_rule_check_cache(+, +, 3, -, -, -, -).

:- dynamic cached_prelude/3.
:- dynamic cached_compilation/3.
:- dynamic cached_rule_check/5.

%% with_prelude_cache(+Text, :Producer, -Unit, -Diagnostics, -Hit) is det.
with_prelude_cache(Text, Producer, Unit, Diagnostics, Hit) :-
    (   with_mutex(
            dl7_compiler_cache,
            cached_prelude(Text, Unit, Diagnostics))
    ->  Hit = hit
    ;   call(Producer, Unit0, Diagnostics0),
        must_cache_ground(prelude, Unit0, Diagnostics0),
        with_mutex(
            dl7_compiler_cache,
            store_prelude(Text, Unit0, Diagnostics0)),
        Unit = Unit0,
        Diagnostics = Diagnostics0,
        Hit = miss
    ).

store_prelude(Text, Unit, Diagnostics) :-
    (   cached_prelude(Text, _, _)
    ->  true
    ;   assertz(cached_prelude(Text, Unit, Diagnostics))
    ).

%% with_compilation_cache(+Key, :Producer,
%%                        -Compiled, -Diagnostics, -Hit) is det.
with_compilation_cache(Key, Producer, Compiled, Diagnostics, Hit) :-
    (   with_mutex(
            dl7_compiler_cache,
            cached_compilation(Key, Compiled, Diagnostics))
    ->  Hit = hit
    ;   call(Producer, Compiled0, Diagnostics0),
        must_cache_ground(compilation, Compiled0, Diagnostics0),
        with_mutex(
            dl7_compiler_cache,
            store_compilation(Key, Compiled0, Diagnostics0)),
        Compiled = Compiled0,
        Diagnostics = Diagnostics0,
        Hit = miss
    ).

store_compilation(Key, Compiled, Diagnostics) :-
    (   cached_compilation(Key, _, _)
    ->  true
    ;   assertz(cached_compilation(Key, Compiled, Diagnostics))
    ).

with_rule_check_cache(Relations, Rules, Producer,
                      Depends, Strata, Diagnostics, Hit) :-
    (   with_mutex(
            dl7_compiler_cache,
            cached_rule_check(Relations, Rules,
                              Depends, Strata, Diagnostics))
    ->  Hit = hit
    ;   call(Producer, Depends0, Strata0, Diagnostics0),
        must_cache_ground(rule_check,
                          Depends0-Strata0, Diagnostics0),
        with_mutex(
            dl7_compiler_cache,
            store_rule_check(Relations, Rules,
                             Depends0, Strata0, Diagnostics0)),
        Depends = Depends0,
        Strata = Strata0,
        Diagnostics = Diagnostics0,
        Hit = miss
    ).

store_rule_check(Relations, Rules, Depends, Strata, Diagnostics) :-
    (   cached_rule_check(Relations, Rules, _, _, _)
    ->  true
    ;   assertz(cached_rule_check(Relations, Rules,
                                 Depends, Strata, Diagnostics))
    ).

must_cache_ground(Kind, Value, Diagnostics) :-
    (   ground(Value-Diagnostics)
    ->  true
    ;   throw(error(
            instantiation_error,
            context(dl7_compiler_cacher, non_ground_cache_value(Kind))))
    ).

clear_compiler_caches :-
    with_mutex(
        dl7_compiler_cache,
        ( retractall(cached_prelude(_, _, _)),
          retractall(cached_compilation(_, _, _)),
          retractall(cached_rule_check(_, _, _, _, _))
        )).
