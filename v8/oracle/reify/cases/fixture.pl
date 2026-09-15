% One tiny checked program plus the prelude bindings every reified row functor
% needs. Authored so each diagnostic reason gets a committed case that does not
% carry a megabyte of compiler rows.

tiny_runtime(Seeds, Keys, Runtime) :-
    Runtime = checked_datalog(
        root_graph([], []),
        datalog_program(
            [ relation(ref(source_rel), 2, Keys),
              relation(ref(output_rel), 2, [])
            ],
            Seeds,
            [ rule(call(ref(output_rel), [var(left), var(right)]),
                   [ checked_goal(
                         positive,
                         call(ref(source_rel), [var(left), var(right)]))
                   ])
            ]),
        [depends(ref(output_rel), ref(source_rel), positive)],
        [stratum(ref(source_rel), 0), stratum(ref(output_rel), 1)]).

tiny_runtime(Runtime) :-
    tiny_runtime([call(ref(source_rel), [const(1), const(alpha)])],
                 [[0]], Runtime).

% One ':' row per logical-program row functor, spelled the way the prelude
% publishes a public program relation.
prelude_bindings(Bindings) :-
    findall(call(ref(kernel(':')),
                 [ref(module(prelude)), const(Name),
                  ref(prelude_relation(Name)), const(0)]),
            logical_row_functor(Name),
            Bindings).

logical_row_functor(program_relation).
logical_row_functor(program_key).
logical_row_functor(program_key_position).
logical_row_functor(program_seed).
logical_row_functor(program_rule).
logical_row_functor(program_rule_kind).
logical_row_functor(program_goal).
logical_row_functor(program_apply).
logical_row_functor(program_argument).
logical_row_functor(program_edge).
logical_row_functor(program_dependency).
logical_row_functor(program_stratum).

emits_binding(call(ref(kernel(':')),
                   [ref(module(prelude)), const(emits),
                    ref(emits_rel), const(0)])).

emits_row(Name, Output,
          call(ref(emits_rel), [ref(the_emitter), const(Name), ref(Output)])).

tiny_unit(ExtraFacts, Unit) :-
    tiny_runtime(Runtime),
    prelude_bindings(Bindings),
    append(Bindings, ExtraFacts, Facts),
    Unit = compiled_unit([], Runtime, Facts).

% A second program whose arguments are a reference and an aggregate, so the
% reference, aggregate and input edge labels all appear. The reifier walks
% structure and never rechecks, so this program needs no checker agreement.
argument_shapes_runtime(Runtime) :-
    Runtime = checked_datalog(
        root_graph([], []),
        datalog_program(
            [ relation(ref(source_rel), 2, []),
              relation(ref(output_rel), 2, [])
            ],
            [call(ref(source_rel), [ref(primitive(text)), const(alpha)])],
            [ rule(call(ref(output_rel),
                        [var(left), aggregate(count, var(right))]),
                   [ checked_goal(
                         positive,
                         call(ref(source_rel), [var(left), var(right)])),
                     checked_goal(
                         negative,
                         call(ref(output_rel),
                              [ref(primitive(text)), const(3)]))
                   ])
            ]),
        [ depends(ref(output_rel), ref(source_rel), positive),
          depends(ref(output_rel), ref(output_rel), negative)
        ],
        [stratum(ref(source_rel), 0), stratum(ref(output_rel), 1)]).
