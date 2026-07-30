% 0_refusal_messages.pl: one-line presentation for compiler refusals.
%
% FAIL-FIRST RECEIPT:
%   ERROR: Unknown message:
%     unsupported_construct(finalize_in_level_rule(gone/1))
%
% The text parser does not retain token positions. Unsupported terms therefore
% carry rule-index granularity only; the fallback states that residue instead
% of manufacturing a FILE:LINE. The at(File, Line, Reason) arm is ready for a
% future text-door position wrapper without changing the message shape.

:- module(refusal_messages,
          [ refusal_inventory/1,
            refusal_message_clause_count/1
          ]).

:- use_module('compile/registry', [surface/5]).
:- use_module('compile/3_clock_check', [clock_refusal_reason/1]).

:- multifile prolog:message//1.

prolog:message(unsupported_construct(WrappedReason)) -->
    { refusal_context(WrappedReason, Reason, Location),
      reason_name(Reason, ReasonName)
    },
    [ 'compiler refusal unsupported_construct(~q); reason=~w; location=~w'-
      [Reason, ReasonName, Location]
    ].

refusal_context(at(File, Line, Reason), Reason, Location) :-
    !,
    format(atom(Location), '~w:~w', [File, Line]).
refusal_context(Reason, Reason, 'rule-index unavailable').

reason_name(Reason, Name) :-
    ( compound(Reason)
    -> functor(Reason, Name, _)
    ;  Name = Reason
    ).

% The coverage inventory is derived from the two refusal sources:
%
%   1. registry rows whose status or lowering role says refused;
%   2. loaded compiler clauses that construct unsupported_construct/1 reasons,
%      including the shared program-check mapping and edge-shape producers.
%
% No message-name list is duplicated here. A new registry refusal or named
% compiler reason enters this inventory from its defining clause.
refusal_inventory(Inventory) :-
    findall(Signature,
            refusal_inventory_entry(Signature, _),
            Signatures0),
    sort(Signatures0, Signatures),
    maplist(refusal_inventory_example, Signatures, Inventory).

refusal_inventory_example(Signature, Signature-Example) :-
    once(refusal_inventory_entry(Signature, Example)).

refusal_inventory_entry(Signature, registered_surface(Signature)) :-
    surface(Signature, _, _, LowerRole, Status),
    ( Status == refused
    ; sub_term(refuse(_), LowerRole)
    ).
refusal_inventory_entry(Signature, Example) :-
    refusal_source_module(Module),
    current_predicate(Module:Name/Arity),
    functor(Head, Name, Arity),
    catch(clause(Module:Head, Body), _, fail),
    sub_term(unsupported_construct(Reason), Body),
    nonvar(Reason),
    reason_signature(Reason, Signature),
    copy_term(Reason, Example).
refusal_inventory_entry(Signature, Example) :-
    refusal_reason_producer(Example),
    nonvar(Example),
    reason_signature(Example, Signature).

% parse_dl is a refusal source as of the json wiring arc: `tagged_brace_
% reserved` is thrown at the LEXER, before any analyzer stage sees a term, and
% a refusal the inventory cannot see is a refusal nothing checks renders.
refusal_source_module(parse_dl).
refusal_source_module(enum_expand).
refusal_source_module(match_expand).
refusal_source_module(coalesce_expand).
refusal_source_module(type_plane).
refusal_source_module(expansion).
refusal_source_module(host_expand).
refusal_source_module(program_check).
refusal_source_module(compile).
refusal_source_module(analyze).
refusal_source_module(strat).
refusal_source_module(lower).

refusal_reason_producer(Reason) :-
    clause(analyze:compiler_refusal(_, _, Reason), _).
refusal_reason_producer(Reason) :-
    clause(analyze:edge_goal_refusal(_, _, _, Reason), _).
refusal_reason_producer(Reason) :-
    clause(analyze:edge_trigger_shape(_, unsupported(Reason)), _).
refusal_reason_producer(Reason) :-
    clock_refusal_reason(Reason).

reason_signature(Reason, Name/Arity) :-
    ( compound(Reason)
    -> functor(Reason, Name, Arity)
    ;  Name = Reason, Arity = 0
    ).

refusal_message_clause_count(Count) :-
    findall(Clause,
            ( clause(prolog:message(unsupported_construct(_), _, _), Clause),
              strip_module(Clause, Module, _),
              Module == refusal_messages
            ),
            Clauses),
    length(Clauses, Count).
