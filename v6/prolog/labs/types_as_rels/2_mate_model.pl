% 2_mate_model.pl : content identity with a dense storage-key mate.
%
% Semantic identity is a full content hash. The intern dictionary assigns a
% dense integer used only by stored ref columns and indexes. Logical deltas
% print values. Removing the last support removes the dictionary row; a later
% reintern may assign a different dense integer to the same semantic value.
%
% Store term:
%   mate_store(NextDense, Entries)
%   Entries : mate(Hash, Dense, Type, Content, Support)

:- module(mate_model,
          [ empty_mate_store/1,
            semantic_id/3,
            intern_value/7, release_value/6,
            mate_assignments/2, mate_support/3, dense_for/3 ]).

:- use_module(library(lists)).

empty_mate_store(mate_store(1, [])).

semantic_id(Type, Content, Hash) :-
    variant_sha1(value(Type, Content), Digest),
    atom_concat('#', Digest, Hash).

intern_value(Type, Content,
             mate_store(NextDense, Entries0),
             mate_store(NextDense, Entries),
             Dense,
             Hash,
             []) :-
    semantic_id(Type, Content, Hash),
    select(mate(Hash, Dense, Type, Content, Support0), Entries0, Rest), !,
    Support is Support0 + 1,
    append(Rest, [mate(Hash, Dense, Type, Content, Support)], Entries).
intern_value(Type, Content,
             mate_store(NextDense, Entries0),
             mate_store(FollowingDense, Entries),
             NextDense,
             Hash,
             [+value(Type, Content)]) :-
    semantic_id(Type, Content, Hash),
    FollowingDense is NextDense + 1,
    append(Entries0, [mate(Hash, NextDense, Type, Content, 1)], Entries).

release_value(Type, Content,
              mate_store(NextDense, Entries0),
              mate_store(NextDense, Entries),
              Hash,
              Deltas) :-
    semantic_id(Type, Content, Hash),
    select(mate(Hash, Dense, Type, Content, Support0), Entries0, Rest),
    (   Support0 =:= 1
    ->  Entries = Rest,
        Deltas = [-value(Type, Content)]
    ;   Support is Support0 - 1,
        append(Rest, [mate(Hash, Dense, Type, Content, Support)], Entries),
        Deltas = []
    ).

mate_assignments(mate_store(_, Entries), Assignments) :-
    findall(assign(Hash, Dense),
            member(mate(Hash, Dense, _, _, _), Entries),
            Assignments0),
    sort(Assignments0, Assignments).

mate_support(mate_store(_, Entries), Hash, Support) :-
    memberchk(mate(Hash, _, _, _, Support), Entries).

dense_for(mate_store(_, Entries), Hash, Dense) :-
    memberchk(mate(Hash, Dense, _, _, _), Entries).
