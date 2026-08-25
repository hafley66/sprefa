% Semantic type identities are compiler values.  They remain ground Prolog
% terms until an artifact boundary asks for their SHA-256 text form.
:- module(type_ids,
          [ decl_id/4,
            primitive_id/2,
            app_id/3,
            param_id/4,
            member_id/4,
            constraint_id/3,
            arg_id/3,
            id_kind_name/3,
            semantic_type_id_text/2
          ]).

:- use_module(library(crypto)).

%! decl_id(+ModuleHash, +Kind, +Name, -SemanticTypeId) is det.
decl_id(ModuleHash, Kind, Name, named(ModuleHash, Kind, Name)).

%! primitive_id(+Name, -SemanticTypeId) is det.
primitive_id(Name, primitive(Name)).

%! app_id(+ConstructorSemanticTypeId, +ArgumentSemanticTypeIds,
%!        -SemanticTypeId) is det.
app_id(Constructor, Arguments, application(Constructor, Arguments)).

%! param_id(+OwnerId, +Ordinal, +Name, -SemanticNodeId) is det.
param_id(Owner, Ordinal, Name, parameter(Owner, Ordinal, Name)).

%! member_id(+OwnerId, +Ordinal, +Name, -SemanticNodeId) is det.
member_id(Owner, Ordinal, Name, member(Owner, Ordinal, Name)).

%! constraint_id(+SubjectNodeId, +InterfaceApplicationId, -SemanticNodeId) is det.
constraint_id(Subject, InterfaceApplication, constraint(Subject, InterfaceApplication)).

%! arg_id(+ApplicationId, +Ordinal, -SemanticNodeId) is det.
arg_id(Application, Ordinal, argument(Application, Ordinal)).

%! id_kind_name(+SemanticTypeId, ?Kind, ?Name) is semidet.
id_kind_name(named(_, Kind, Name), Kind, Name).

%! semantic_type_id_text(+SemanticTypeId, -Sha256Text) is det.
%  The encoding carries each atom's UTF-8 byte length and each list's element
%  count, preserving application nesting and argument order without delimiter
%  ambiguity.  The SHA-256 input is the same UTF-8 byte sequence used for
%  those lengths.  This conversion is reserved for catalog and emitted
%  artifacts.
semantic_type_id_text(Id, Text) :-
    ground(Id),
    semantic_type_id_encoding(Id, Encoding),
    string_bytes(Encoding, Bytes, utf8),
    crypto_data_hash(Bytes, Text, [algorithm(sha256), encoding(octet)]).

semantic_type_id_encoding(named(Module, Kind, Name), Encoding) :-
    atom_encoding(Module, ModuleEncoding),
    atom_encoding(Kind, KindEncoding),
    atom_encoding(Name, NameEncoding),
    string_concat("N", ModuleEncoding, A),
    string_concat(A, KindEncoding, B),
    string_concat(B, NameEncoding, Encoding).
semantic_type_id_encoding(primitive(Name), Encoding) :-
    atom_encoding(Name, NameEncoding),
    string_concat("P", NameEncoding, Encoding).
semantic_type_id_encoding(any_pattern, "W").
semantic_type_id_encoding(application(Constructor, Arguments), Encoding) :-
    semantic_type_id_encoding(Constructor, ConstructorEncoding),
    maplist(semantic_type_id_encoding, Arguments, ArgumentEncodings),
    length(ArgumentEncodings, Arity),
    format(string(ArityEncoding), "~d:", [Arity]),
    string_concat("A", ConstructorEncoding, A),
    string_concat(A, ArityEncoding, B),
    foldl(append_encoding, ArgumentEncodings, B, Encoding).
semantic_type_id_encoding(parameter(Owner, Ordinal, Name), Encoding) :-
    semantic_type_id_encoding(Owner, OwnerEncoding),
    atom_encoding(Name, NameEncoding),
    format(string(OrdinalEncoding), "~d:", [Ordinal]),
    string_concat("R", OwnerEncoding, A),
    string_concat(A, OrdinalEncoding, B),
    string_concat(B, NameEncoding, Encoding).
semantic_type_id_encoding(member(Owner, Ordinal, Name), Encoding) :-
    semantic_type_id_encoding(Owner, OwnerEncoding),
    atom_encoding(Name, NameEncoding),
    format(string(OrdinalEncoding), "~d:", [Ordinal]),
    string_concat("M", OwnerEncoding, A),
    string_concat(A, OrdinalEncoding, B),
    string_concat(B, NameEncoding, Encoding).
semantic_type_id_encoding(constraint(Subject, Interface), Encoding) :-
    semantic_type_id_encoding(Subject, SubjectEncoding),
    semantic_type_id_encoding(Interface, InterfaceEncoding),
    string_concat("C", SubjectEncoding, A),
    string_concat(A, InterfaceEncoding, Encoding).
semantic_type_id_encoding(argument(Application, Ordinal), Encoding) :-
    semantic_type_id_encoding(Application, ApplicationEncoding),
    format(string(OrdinalEncoding), "~d:", [Ordinal]),
    string_concat("G", ApplicationEncoding, A),
    string_concat(A, OrdinalEncoding, Encoding).
semantic_type_id_encoding(anonymous(Owner, Path, Shape), Encoding) :-
    semantic_type_id_encoding(Owner, OwnerEncoding),
    path_encoding(Path, PathEncoding),
    type_term_encoding(Shape, ShapeEncoding),
    string_concat("O", OwnerEncoding, A),
    string_concat(A, PathEncoding, B),
    string_concat(B, ShapeEncoding, Encoding).
semantic_type_id_encoding(anonymous_placeholder(Type), Encoding) :-
    type_term_encoding(Type, TypeEncoding),
    string_concat("U", TypeEncoding, Encoding).

% A site path is a list of member-name atoms and wrapper/application ordinals.
path_encoding(Path, Encoding) :-
    maplist(path_element_encoding, Path, Encodings),
    length(Encodings, Arity),
    format(string(ArityEncoding), "~d:", [Arity]),
    string_concat("[", ArityEncoding, A),
    foldl(append_encoding, Encodings, A, B),
    string_concat(B, "]", Encoding).

path_element_encoding(Element, Encoding) :-
    integer(Element),
    !,
    format(string(Encoding), "~d:", [Element]).
path_element_encoding(Element, Encoding) :-
    atom_encoding(Element, Encoding).

% A structural type term encoding shared with the anonymous shape: length-prefix
% atoms and constructor/arity-delimited compounds, so nesting and argument order
% are unambiguous.
type_term_encoding(Term, Encoding) :-
    atom(Term),
    !,
    atom_encoding(Term, Encoding).
type_term_encoding([], Encoding) :-
    !,
    Encoding = "0:".
type_term_encoding(Term, Encoding) :-
    compound(Term),
    Term =.. [Constructor | Args],
    atom_encoding(Constructor, ConstructorEncoding),
    maplist(type_term_encoding, Args, ArgEncodings),
    length(Args, Arity),
    format(string(ArityEncoding), "~d:", [Arity]),
    string_concat("(", ConstructorEncoding, A),
    string_concat(A, ArityEncoding, B),
    foldl(append_encoding, ArgEncodings, B, C),
    string_concat(C, ")", Encoding).

atom_encoding(Atom, Encoding) :-
    atom_string(Atom, Text),
    string_bytes(Text, Bytes, utf8),
    length(Bytes, Length),
    format(string(Prefix), "~d:", [Length]),
    string_concat(Prefix, Text, Encoding).

append_encoding(Part, Prefix, Encoding) :- string_concat(Prefix, Part, Encoding).
