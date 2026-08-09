% @comment-ok: the rel record's field contract, its single documentation site
% ═══ the rel record : one term per relation, read by every phase after `plan`
%
%   relplan(Ref, Kind, Columns, KeyOrNone, ColumnTypes)
%     Ref          Name/Arity.
%     Kind         log | set.
%     Columns      one name per argument position, in position order.
%     KeyOrNone    key(Positions) | none.
%     ColumnTypes  storage kind per Columns position: int | float | bool |
%                  text | json | list(T) | ref(TypeName). lower.pl:column_def/3
%                  is the only SQL storage reader.
%
% The relplan_ prefix names the LIST plan/9 carries, not a second record; the
% bare rel_ prefix is taken by analyze.pl (rel_kind/3, rel_columns/4).

:- module(rel_record,
          [ relplan_parts/6,
            relplan_of/3,
            relplan_shape/6,
            relplan_kind/3,
            relplan_columns/3,
            relplan_column_types/3,
            relplan_key/3
          ]).

:- use_module(library(lists)).

% Bidirectional: the one place the record's field order is written down.
relplan_parts(relplan(Ref, Kind, Columns, KeyOrNone, ColumnTypes),
              Ref, Kind, Columns, KeyOrNone, ColumnTypes).

% ═══ lookup in plan/9's RelPlans ═════════════════════════════════════════════

relplan_of(RelPlans, Ref, Rel) :-
    relplan_parts(Rel, Ref, _, _, _, _),
    memberchk(Rel, RelPlans).

relplan_shape(RelPlans, Ref, Kind, Columns, KeyOrNone, ColumnTypes) :-
    relplan_of(RelPlans, Ref, Rel),
    relplan_parts(Rel, Ref, Kind, Columns, KeyOrNone, ColumnTypes).

relplan_kind(RelPlans, Ref, Kind) :- relplan_shape(RelPlans, Ref, Kind, _, _, _).

relplan_columns(RelPlans, Ref, Columns) :-
    relplan_shape(RelPlans, Ref, _, Columns, _, _).

relplan_column_types(RelPlans, Ref, ColumnTypes) :-
    relplan_shape(RelPlans, Ref, _, _, _, ColumnTypes).

relplan_key(RelPlans, Ref, KeyOrNone) :-
    relplan_shape(RelPlans, Ref, _, _, KeyOrNone, _).
