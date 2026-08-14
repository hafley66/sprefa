% Semantic-graph row ids.  Construction and deconstruction live here and
% nowhere else; `id_kind_name/3` is the only inverse any consumer may use.
:- module(type_ids,
          [ decl_id/3,
            param_id/4,
            member_id/4,
            constraint_id/3,
            impl_id/3,
            app_id/3,
            arg_id/3,
            id_kind_name/3
          ]).

%! decl_id(?Kind, ?Name, ?Id) is det.
decl_id(Kind, Name, Id) :- atomic_list_concat([decl, Kind, Name], ':', Id).

%! param_id(+Owner, +Ordinal, +Name, -Id) is det.
param_id(Owner, Ordinal, Name, Id) :-
    atomic_list_concat([Owner, param, Ordinal, Name], ':', Id).

%! member_id(+Owner, +Ordinal, +Name, -Id) is det.
member_id(Owner, Ordinal, Name, Id) :-
    atomic_list_concat([Owner, member, Ordinal, Name], ':', Id).

%! constraint_id(+Subject, +Interface, -Id) is det.
constraint_id(Subject, Interface, Id) :-
    atomic_list_concat([Subject, constraint, Interface], ':', Id).

%! impl_id(+Subject, +Interface, -Id) is det.
impl_id(Subject, Interface, Id) :-
    atomic_list_concat([Subject, impl, Interface], ':', Id).

%! app_id(+Constructor, +Arguments, -Id) is det.
app_id(Constructor, Arguments, Id) :-
    term_to_atom(Arguments, Encoded),
    atomic_list_concat([Constructor, app, Encoded], ':', Id).

%! arg_id(+Application, +Ordinal, -Id) is det.
arg_id(Application, Ordinal, Id) :-
    atomic_list_concat([Application, arg, Ordinal], ':', Id).

% A declaration name may itself carry ':', so a bound Kind strips a prefix.
%! id_kind_name(?Id, ?Kind, ?Name) is semidet.
id_kind_name(Id, Kind, Name) :-
    nonvar(Kind),
    !,
    atomic_list_concat([decl, Kind, ''], ':', Prefix),
    atom_concat(Prefix, Name, Id).
id_kind_name(Id, Kind, Name) :-
    atom_concat('decl:', Rest, Id),
    sub_atom(Rest, Before, 1, _, ':'),
    !,
    sub_atom(Rest, 0, Before, _, Kind),
    After is Before + 1,
    sub_atom(Rest, After, _, 0, Name).
