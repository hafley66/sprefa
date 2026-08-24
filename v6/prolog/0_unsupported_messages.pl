% 0_unsupported_messages.pl: one-line presentation for compiler unsupported constructs.
%
% FAIL-FIRST RECEIPT:
%   ERROR: Unknown message:
%     unsupported_construct(finalize_in_level_rule(gone/1))
%
% The text parser does not retain token positions. Unsupported terms therefore
% carry rule-index granularity only; the fallback states that residue instead
% of manufacturing a FILE:LINE. The at(File, Line, Reason) arm is ready for a
% future text-door position wrapper without changing the message shape.

:- module(unsupported_messages,
          [ unsupported_inventory/1,
            unsupported_inventory_forget/0,
            unsupported_message_clause_count/1,
            unsupported_renderer_counts/2
          ]).

:- use_module('compile/registry', [surface/5]).
:- use_module('3_clock_check', [clock_unsupported_reason/1]).

:- multifile prolog:message//1.

prolog:message(unsupported_construct(WrappedReason)) -->
    { unsupported_context(WrappedReason, Reason, Location),
      unsupported_reason_text(Reason, ReasonText)
    },
    [ '~w: unsupported_construct: ~w'-
      [Location, ReasonText]
    ].

% The text is deliberately assembled from names and typed payload categories.
% Payload terms never pass through ~q, so a unsupported construct remains one readable line.
% Inventory members use the specific renderer; a reason outside the inventory
% keeps the generic fallback and still preserves its functor in parentheses.
unsupported_reason_text(Reason, Text) :-
    reason_name(Reason, ReasonName),
    ( ( unsupported_inventory_name(ReasonName)
      ; Reason = registered_surface(Signature),
        unsupported_inventory_signature(Signature)
      )
    -> specific_reason_text(ReasonName, Reason, Text)
    ;  fallback_reason_text(ReasonName, Text)
    ).

% A REMOVED word renders its replacement, because the whole point of keeping a
% registry row for a deleted spelling is to answer "then what do I write". The
% generic renderer below would print the removed word twice and name nothing.
specific_reason_text(removed_word, removed_word(Word), Text) :-
    !,
    removed_word_replacement(Word, Replacement),
    format(atom(Text),
           "compiler refused rule 'removed_word': '~w' is not a construct, ~w (removed_word)",
           [Word, Replacement]).
specific_reason_text(ReasonName, Reason, Text) :-
    Reason = registered_surface(Signature),
    !,
    format(atom(Text), "compiler refused surface '~w' (~w)",
           [Signature, ReasonName]).
specific_reason_text(ambiguous_type_projection,
                     ambiguous_type_projection(Owner, Name, Targets), Text) :-
    !,
    projection_type_label(Owner, OwnerLabel),
    maplist(projection_type_label, Targets, TargetLabels),
    atomic_list_concat(TargetLabels, ', ', TargetList),
    format(atom(Text),
           "compiler refused projection '~w.~w': name resolves to [~w] (ambiguous_type_projection)",
           [OwnerLabel, Name, TargetList]).
specific_reason_text(ReasonName, Reason, Text) :-
    reason_subject_text(Reason, Subject),
    ( Subject == ''
    -> format(atom(Text), "compiler refused rule '~w' (~w)",
               [ReasonName, ReasonName])
    ;  format(atom(Text), "compiler refused rule '~w'~w (~w)",
               [ReasonName, Subject, ReasonName])
    ).

projection_type_label(primitive(Name), Name) :- !.
projection_type_label(named(_, _, Name), Name) :- !.
projection_type_label(parameter(_, _, Name), Name) :- !.
projection_type_label(application(Constructor, Arguments), Text) :-
    !,
    projection_type_label(Constructor, ConstructorLabel),
    maplist(projection_type_label, Arguments, ArgumentLabels),
    atomic_list_concat(ArgumentLabels, ', ', ArgumentList),
    format(atom(Text), '~w(~w)', [ConstructorLabel, ArgumentList]).
projection_type_label(Type, Text) :-
    format(atom(Text), '~w', [Type]).

% One line per removed word. The catch-all keeps a word that loses its row
% before it gains a sentence here from printing a dangling comma.
removed_word_replacement(scan,
    'file enumeration is files(glob, path, digest) over the worktree and files_at(rev, glob, path, digest) over a pinned revision') :- !.
removed_word_replacement(set,
    'a bare rel declaration is already a set table') :- !.
removed_word_replacement(list,
    'the json array column type is json_list(T), e.g. json_list(text)') :- !.
removed_word_replacement(_, 'the language assigns it no meaning').

fallback_reason_text(ReasonName, Text) :-
    format(atom(Text), "compiler refused rule '~w' (~w)",
           [ReasonName, ReasonName]).

unsupported_inventory_name(Name) :-
    unsupported_inventory(Inventory),
    member(Name/_Arity-_, Inventory),
    !.

unsupported_inventory_signature(Signature) :-
    unsupported_inventory(Inventory),
    member(Signature-_, Inventory),
    !.

reason_subject_text(Reason, Subject) :-
    findall(Rel,
            reason_relation_reference(Reason, Rel),
            Relations0),
    sort(Relations0, Relations),
    ( Relations == []
    -> Subject = ''
    ;  maplist(reason_relation_text, Relations, RelationTexts),
       atomic_list_concat(RelationTexts, ', ', RelationList),
       format(atom(Subject), " for rel ~w", [RelationList])
    ).

reason_relation_reference(Reason, Name/Arity) :-
    sub_term(Name/Arity, Reason),
    atom(Name),
    integer(Arity).

reason_relation_text(Name/Arity, Text) :-
    format(atom(Text), "'~w/~w'", [Name, Arity]).

unsupported_context(at(File, Line, Reason), Reason, Location) :-
    !,
    format(atom(Location), '~w:~w', [File, Line]).
unsupported_context(Reason, Reason, 'rule-index unavailable').

reason_name(Reason, Name) :-
    ( compound(Reason)
    -> functor(Reason, Name, _)
    ;  Name = Reason
    ).

% The coverage inventory is derived from the two unsupported construct sources:
%
%   1. registry rows whose status or lowering role says refused;
%   2. loaded compiler clauses that construct unsupported_construct/1 reasons,
%      including the shared program-check mapping and edge-shape producers.
%
% No message-name list is duplicated here. A new registry unsupported construct or named
% compiler reason enters this inventory from its defining clause.
% MEMOIZED: the scan walks every clause of all thirteen unsupported_source_module/1
% modules, and unsupported_inventory_name/1 sits under the umbrella renderer, so
% unmemoized it runs once per rendered unsupported construct. The answer is a function of
% loaded clause source alone. A process that loads another unsupported construct source after
% a unsupported construct has rendered must call unsupported_inventory_forget/0.
:- dynamic unsupported_inventory_memo/1.

unsupported_inventory(Inventory) :-
    (   unsupported_inventory_memo(Memo)
    ->  Inventory = Memo
    ;   unsupported_inventory_scan(Scanned),
        assertz(unsupported_inventory_memo(Scanned)),
        Inventory = Scanned
    ).

unsupported_inventory_forget :-
    retractall(unsupported_inventory_memo(_)).

unsupported_inventory_scan(Inventory) :-
    findall(Signature,
            unsupported_inventory_entry(Signature, _),
            Signatures0),
    sort(Signatures0, Signatures),
    maplist(unsupported_inventory_example, Signatures, Inventory).

unsupported_inventory_example(Signature, Signature-Example) :-
    once(unsupported_inventory_entry(Signature, Example)).

unsupported_inventory_entry(Signature, registered_surface(Signature)) :-
    surface(Signature, _, _, LowerRole, Status),
    ( Status == refused
    ; sub_term(refuse(_), LowerRole)
    ).
unsupported_inventory_entry(Signature, Example) :-
    unsupported_source_module(Module),
    current_predicate(Module:Name/Arity),
    functor(Head, Name, Arity),
    catch(clause(Module:Head, Body), _, fail),
    sub_term(unsupported_construct(Reason), Body),
    nonvar(Reason),
    reason_signature(Reason, Signature),
    Signature \== at/3,
    copy_term(Reason, Example).
unsupported_inventory_entry(Signature, Example) :-
    unsupported_reason_producer(Example),
    nonvar(Example),
    reason_signature(Example, Signature).

% parse_dl is a unsupported construct source as of the json wiring arc: `tagged_brace_
% reserved` is thrown at the LEXER, before any analyzer stage sees a term, and
% a unsupported construct the inventory cannot see is a unsupported construct nothing checks renders.
unsupported_source_module(parse_dl).
unsupported_source_module(enum_expand).
unsupported_source_module(match_expand).
unsupported_source_module(coalesce_expand).
unsupported_source_module(ast_expand).
unsupported_source_module(dot_expand).
unsupported_source_module(option_expand).
unsupported_source_module(generic_expand).
unsupported_source_module(type_plane).
unsupported_source_module(expansion).
unsupported_source_module(host_expand).
unsupported_source_module(program_check).
unsupported_source_module(compile).
unsupported_source_module(analyze).
unsupported_source_module(strat).
unsupported_source_module(lower).

unsupported_reason_producer(Reason) :-
    clause(analyze:compiler_unsupported(_, _, Reason), _).
% The reserved-word arm of compiler_unsupported/3 hands its reason term back
% through this predicate, so its own head carries an unbound variable and the
% clause above sees nothing. Reading the naming clauses directly is what puts
% lifecycle_arm/1 and removed_word/1 in the inventory.
unsupported_reason_producer(Reason) :-
    clause(analyze:reserved_construct_name(_, _, Reason), _).
unsupported_reason_producer(Reason) :-
    clause(analyze:edge_goal_unsupported(_, _, _, Reason), _).
unsupported_reason_producer(Reason) :-
    clause(analyze:edge_trigger_shape(_, unsupported(Reason)), _).
unsupported_reason_producer(Reason) :-
    clock_unsupported_reason(Reason).

reason_signature(Reason, Name/Arity) :-
    ( compound(Reason)
    -> functor(Reason, Name, Arity)
    ;  Name = Reason, Arity = 0
    ).

unsupported_message_clause_count(Count) :-
    findall(Clause,
            ( clause(prolog:message(unsupported_construct(_), _, _), Clause),
              strip_module(Clause, Module, _),
              Module == unsupported_messages
            ),
            Clauses),
    length(Clauses, Count).

unsupported_renderer_counts(Specific, Fallback) :-
    unsupported_inventory(Inventory),
    findall(Name/Arity,
            ( member(Name/Arity-_, Inventory),
              unsupported_inventory_name(Name) ),
            SpecificSignatures0),
    sort(SpecificSignatures0, SpecificSignatures),
    length(SpecificSignatures, Specific),
    length(Inventory, InventoryCount),
    Fallback is InventoryCount - Specific.
