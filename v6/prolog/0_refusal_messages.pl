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
            refusal_inventory_forget/0,
            refusal_message_clause_count/1,
            refusal_renderer_counts/2
          ]).

:- use_module('compile/registry', [surface/5]).
:- use_module('3_clock_check', [clock_refusal_reason/1]).

:- multifile prolog:message//1.

prolog:message(unsupported_construct(WrappedReason)) -->
    { refusal_context(WrappedReason, Reason, Location),
      refusal_reason_text(Reason, ReasonText)
    },
    [ '~w: unsupported_construct: ~w'-
      [Location, ReasonText]
    ].

% The text is deliberately assembled from names and typed payload categories.
% Payload terms never pass through ~q, so a refusal remains one readable line.
% Inventory members use the specific renderer; a reason outside the inventory
% keeps the generic fallback and still preserves its functor in parentheses.
refusal_reason_text(Reason, Text) :-
    reason_name(Reason, ReasonName),
    ( ( refusal_inventory_name(ReasonName)
      ; Reason = registered_surface(Signature),
        refusal_inventory_signature(Signature)
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
specific_reason_text(ReasonName, Reason, Text) :-
    reason_subject_text(Reason, Subject),
    ( Subject == ''
    -> format(atom(Text), "compiler refused rule '~w' (~w)",
               [ReasonName, ReasonName])
    ;  format(atom(Text), "compiler refused rule '~w'~w (~w)",
               [ReasonName, Subject, ReasonName])
    ).

% One line per removed word. The catch-all keeps a word that loses its row
% before it gains a sentence here from printing a dangling comma.
removed_word_replacement(scan,
    'file enumeration is files(glob, path, digest) over the worktree and files_at(rev, glob, path, digest) over a pinned revision') :- !.
removed_word_replacement(set,
    'a bare rel declaration is already a set table') :- !.
removed_word_replacement(_, 'the language assigns it no meaning').

fallback_reason_text(ReasonName, Text) :-
    format(atom(Text), "compiler refused rule '~w' (~w)",
           [ReasonName, ReasonName]).

refusal_inventory_name(Name) :-
    refusal_inventory(Inventory),
    member(Name/_Arity-_, Inventory),
    !.

refusal_inventory_signature(Signature) :-
    refusal_inventory(Inventory),
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
% MEMOIZED: the scan walks every clause of all thirteen refusal_source_module/1
% modules, and refusal_inventory_name/1 sits under the umbrella renderer, so
% unmemoized it runs once per rendered refusal. The answer is a function of
% loaded clause source alone. A process that loads another refusal source after
% a refusal has rendered must call refusal_inventory_forget/0.
:- dynamic refusal_inventory_memo/1.

refusal_inventory(Inventory) :-
    (   refusal_inventory_memo(Memo)
    ->  Inventory = Memo
    ;   refusal_inventory_scan(Scanned),
        assertz(refusal_inventory_memo(Scanned)),
        Inventory = Scanned
    ).

refusal_inventory_forget :-
    retractall(refusal_inventory_memo(_)).

refusal_inventory_scan(Inventory) :-
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
    Signature \== at/3,
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
refusal_source_module(dot_expand).
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
% The reserved-word arm of compiler_refusal/3 hands its reason term back
% through this predicate, so its own head carries an unbound variable and the
% clause above sees nothing. Reading the naming clauses directly is what puts
% lifecycle_arm/1 and removed_word/1 in the inventory.
refusal_reason_producer(Reason) :-
    clause(analyze:reserved_construct_name(_, _, Reason), _).
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

refusal_renderer_counts(Specific, Fallback) :-
    refusal_inventory(Inventory),
    findall(Name/Arity,
            ( member(Name/Arity-_, Inventory),
              refusal_inventory_name(Name) ),
            SpecificSignatures0),
    sort(SpecificSignatures0, SpecificSignatures),
    length(SpecificSignatures, Specific),
    length(Inventory, InventoryCount),
    Fallback is InventoryCount - Specific.
