% 1_entity_model.pl : reference semantics for the entity policy.
%
% Entity identity is an extrinsic integer. Current rows may change while the
% identity stays fixed. Every create and update appends an immutable history
% row. Lifetime ends only through an explicit retire operation.
%
% Store term:
%   entity_store(NextId, NextTick, CurrentRows, HistoryRows)
%   CurrentRows : entity(Type, Id, Args)
%   HistoryRows : version(Tick, Type, Id, Args)
%   Args        : text(Atom) | int(Number) | id(Id)

:- module(entity_model,
          [ empty_entity_store/1,
            create_entity/6, update_entity/5, retire_entities/4,
            entity_rows/2, entity_history/2, entity_row_count/2,
            entity_refs/3 ]).

:- use_module(library(lists)).

empty_entity_store(entity_store(1, 1, [], [])).

entity_rows(entity_store(_, _, Rows, _), Rows).
entity_history(entity_store(_, _, _, History), History).
entity_row_count(Store, Count) :-
    entity_rows(Store, Rows),
    length(Rows, Count).

create_entity(Type, Args,
              entity_store(NextId, NextTick, Rows0, History0),
              entity_store(FollowingId, FollowingTick, Rows, History),
              NextId,
              [+row(Type, NextId, Args)]) :-
    FollowingId is NextId + 1,
    FollowingTick is NextTick + 1,
    append(Rows0, [entity(Type, NextId, Args)], Rows),
    append(History0, [version(NextTick, Type, NextId, Args)], History).

update_entity(Id, Args,
              entity_store(NextId, NextTick, Rows0, History0),
              entity_store(NextId, FollowingTick, Rows, History),
              [-row(Type, Id, PreviousArgs), +row(Type, Id, Args)]) :-
    select(entity(Type, Id, PreviousArgs), Rows0, RemainingRows),
    FollowingTick is NextTick + 1,
    append(RemainingRows, [entity(Type, Id, Args)], Rows),
    append(History0, [version(NextTick, Type, Id, Args)], History).

% A retire set is accepted only when no live entity outside the set points
% into it. This permits a cycle to retire as one explicit lifetime operation
% while refusing a partial retirement that would leave a dangling ref.
retire_entities(Ids,
                entity_store(NextId, NextTick, Rows0, History),
                entity_store(NextId, NextTick, Rows, History),
                Deltas) :-
    sort(Ids, RetiringIds),
    RetiringIds \== [],
    forall(( member(entity(_, OutsideId, OutsideArgs), Rows0),
             \+ memberchk(OutsideId, RetiringIds),
             member(id(ReferencedId), OutsideArgs) ),
           \+ memberchk(ReferencedId, RetiringIds)),
    findall(-row(Type, Id, Args),
            ( member(Id, RetiringIds),
              member(entity(Type, Id, Args), Rows0) ),
            Deltas0),
    length(Deltas0, RetiringCount),
    length(RetiringIds, RetiringCount),
    exclude(entity_has_id(RetiringIds), Rows0, Rows),
    msort(Deltas0, Deltas).

entity_has_id(Ids, entity(_, Id, _)) :-
    memberchk(Id, Ids).

entity_refs(Store, ParentId, ChildIds) :-
    entity_rows(Store, Rows),
    memberchk(entity(_, ParentId, Args), Rows),
    findall(ChildId, member(id(ChildId), Args), ChildIds).
