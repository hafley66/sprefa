% registry.pl: the compiler's surface construct inventory.
%
% surface(Functor/Arity, Axis, AnalyzeRole, LowerRole, Status)
%
% Arity is an integer except for combine/variadic. Status is live, reserved,
% or refused. LowerRole carries both the textual body shape and the gate role
% so the parser, printer, analyzer, and supported-subset gate can project from
% these rows without maintaining their own functor lists.

:- module(registry,
          [ surface/5,
            surface_for_term/6,
            body_surface_for_term/6,
            wrapper_lower_role/3
          ]).

surface(latest/1,       sample,    refs_of_arg(1, pos, sampled), wrapper(rel_atom, lower),              live).
surface(finalize/1,     time,      refs_of_arg(1, pos, trigger), wrapper(rel_atom, refuse(goal)),       refused).
surface(next/1,         time,      splice_bare,                  wrapper(rel_atom, lower),              live).
surface(combine/variadic, join,    splice_bare,                  wrapper(atom_list, lower),             live).
surface(zip/2,          join,      splice_bare,                  wrapper(atom_list, refuse(functor)),   reserved).

surface(unsubscribe/1,  time,      refs_of_arg(1, pos, trigger), wrapper(rel_atom, refuse(lifecycle)), reserved).
surface(complete/1,     time,      refs_of_arg(1, pos, trigger), wrapper(rel_atom, refuse(lifecycle)), reserved).
surface(subscribe/1,    time,      refs_of_arg(1, pos, trigger), wrapper(rel_atom, refuse(lifecycle)), reserved).
surface(error/1,        time,      refs_of_arg(1, pos, trigger), wrapper(rel_atom, refuse(lifecycle)), reserved).

surface(not/1,          sign,      arm(neg),                     wrapper(body_item, lower),             live).
surface(pre/1,          sample,    refs_of_arg(1, pos, sampled), wrapper(rel_atom, refuse(goal)),       refused).
surface(now/1,          time,      no_refs,                      wrapper(expr, refuse(goal)),           refused).
surface(decode/2,       guard,     no_refs,                      wrapper(expr_pair, refuse(goal)),      refused).
surface(json_each/2,    guard,     no_refs,                      wrapper(expr_pair, refuse(goal)),      refused).
surface(true/0,         guard,     no_refs,                      word(lower),                           live).

surface(':='/2,         bind,      no_refs,                      infix(refuse(goal)),                   refused).
surface(is/2,           bind,      no_refs,                      infix(refuse(goal)),                   refused).

surface('<'/2,          guard,     no_refs,                      infix(refuse(comparison)),             refused).
surface('=<'/2,         guard,     no_refs,                      infix(refuse(comparison)),             refused).
surface('>'/2,          guard,     no_refs,                      infix(refuse(comparison)),             refused).
surface('>='/2,         guard,     no_refs,                      infix(refuse(comparison)),             refused).
surface('=='/2,         guard,     no_refs,                      infix(refuse(comparison)),             refused).
surface('\\=='/2,       guard,     no_refs,                      infix(refuse(comparison)),             refused).

surface(count/1,        aggregate, no_refs,                      head(refuse(aggregate)),               refused).
surface(sum/1,          aggregate, no_refs,                      head(refuse(aggregate)),               refused).
surface(min/1,          aggregate, no_refs,                      head(refuse(aggregate)),               refused).
surface(max/1,          aggregate, no_refs,                      head(refuse(aggregate)),               refused).
surface(json_array/1,   aggregate, no_refs,                      head(refuse(aggregate)),               refused).
surface(json_object/2,  aggregate, no_refs,                      head(refuse(aggregate)),               refused).

surface(enum_decl/2,     decl,      no_refs,                      decl(enum_variants),                    live).
surface(';' /2,          decl,      no_refs,                      decl(enum_variant_separator),           live).
surface(col_type/3,      decl,      no_refs,                      decl(column_type),                      live).
surface(set/0,           decl,      no_refs,                      decl(refuse(removed_word)),            refused).

surface_for_term(Term, Functor/Arity, Axis, AnalyzeRole, LowerRole, Status) :-
    nonvar(Term),
    functor(Term, Functor, Arity),
    ( surface(Functor/Arity, Axis, AnalyzeRole, LowerRole, Status)
    ; surface(Functor/variadic, Axis, AnalyzeRole, LowerRole, Status)
    ).

body_surface_for_term(Term, Signature, Axis, AnalyzeRole, LowerRole, Status) :-
    surface_for_term(Term, Signature, Axis, AnalyzeRole, LowerRole, Status),
    body_lower_role(LowerRole).

body_lower_role(wrapper(_, _)).
body_lower_role(word(_)).
body_lower_role(infix(_)).

wrapper_lower_role(wrapper(Shape, GateRole), Shape, GateRole).
