% use_resolve.pl : Door A. Resolve and splice `use "path".` imports, as v5's
% src/frontend.rs does, one module both doors call so imports cannot fork.

:- module(use_resolve,
          [ include_roots/2,
            resolve_use_path/3,
            expand_uses/6,
            reset_parse_counts/0,
            parse_count/2
          ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(filesex)).
:- use_module('compile/parse_dl', [use_item/3, parse_dl/4]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

:- dynamic(parse_count_fact/2).

%! include_roots(+EntryPath, -Roots) is det.
%  <crate>/std and <exe>/'..' are install layouts with no SWI equivalent.
include_roots(EntryPath, Roots) :-
    file_directory_name(EntryPath, Dir),
    (   catch(getenv('SPREFA_STD', Env), _, fail),
        Env \== ''
    ->  Roots = [Dir, Env]
    ;   Roots = [Dir]
    ).

%! resolve_use_path(+Roots, +UseText, -AbsPath) is semidet.
%  Throwing use_path_unresolved/2 when no root resolves is the caller's job.
resolve_use_path(Roots, UseText, AbsPath) :-
    member(Root, Roots),
    absolute_file_name(UseText, AbsPath,
                       [ relative_to(Root), access(read), file_errors(fail) ]),
    !.

%! expand_uses(+EntryPath, +OnStack, +Loaded0, -Loaded, -Prog, -ModuleTable) is det.
%  Spliced leaf-first; a canonical path already in Loaded0 is never re-parsed.
expand_uses(EntryPath, OnStack, Loaded0, Loaded, ProgOut, ModuleTable) :-
    collect_all(EntryPath, OnStack, Loaded0, Loaded, Pairs, ModuleTable),
    merge_pairs(Pairs, ProgOut).

% Loaded threads through the fold, so a diamond's second sight of a file is a
% cache hit; each child contributes the pairs and table of its whole subtree.
collect_children([], _, _, Loaded, Loaded, [], []).
collect_children([UseText | Rest], Roots, OnStack, Loaded0, Loaded,
                 Pairs, Tables) :-
    (   resolve_use_path(Roots, UseText, AbsPath)
    ->  true
    ;   throw(use_path_unresolved(UseText, Roots))
    ),
    (   memberchk(AbsPath, Loaded0)
    ->  FirstPairs = [], FirstTables = [], Loaded1 = Loaded0
    ;   collect_all(AbsPath, OnStack, Loaded0, Loaded1, FirstPairs, FirstTables)
    ),
    collect_children(Rest, Roots, OnStack, Loaded1, Loaded, MorePairs, MoreTables),
    append(FirstPairs, MorePairs, Pairs),
    append(FirstTables, MoreTables, Tables).

% One module's whole subtree: resolve its own imports, splice leaf-first, and
% thread its canonical path into Loaded.
collect_all(EntryPath, OnStack, Loaded0, Loaded, Pairs, Tables) :-
    canonical_abs(EntryPath, EntryAbs),
    (   memberchk(EntryAbs, OnStack)
    ->  (   OnStack = [EntryAbs | _]
        ->  throw(use_cycle([EntryAbs]))
        ;   throw(use_cycle([EntryAbs | OnStack]))
        )
    ;   true
    ),
    strip_entry(EntryPath, EntryAbs, UseTexts, prog(OwnDecls0, OwnRules0)),
    include_roots(EntryPath, Roots),
    collect_children(UseTexts, Roots, [EntryAbs | OnStack], Loaded0, Loaded1,
                     ChildPairs, ChildTables),
    append(ChildPairs, [(EntryAbs, prog(OwnDecls0, OwnRules0))], Pairs),
    module_name(EntryAbs, EntryName),
    module_hash(EntryName, EntryHash),
    append(ChildTables, [ module(EntryAbs, EntryName, EntryHash) ], Tables),
    Loaded = [EntryAbs | Loaded1].

% The parse counter is what a re-parsing loader trips; end-state equality on a
% diamond looks identical whether the shared file was read once or twice.
strip_entry(EntryPath, EntryAbs, UseTexts, prog(OwnDecls, OwnRules)) :-
    read_file_to_codes(EntryPath, Codes, []),
    bump_parse_count(EntryAbs),
    split_codes_lines(Codes, Lines),
    strip_use_lines(Lines, UseTexts, CoreLines),
    flatten(CoreLines, CoreCodes),
    parse_dl(CoreCodes, prog(OwnDecls, OwnRules), _, _).

%! split_codes_lines(+Codes, -Lines) is det.
%  Every line keeps its own newline, so flatten/2 rebuilds the original bytes.
split_codes_lines(Codes, Lines) :-
    string_codes(Text, Codes),
    split_string(Text, "\n", "", Parts),
    parts_to_lines(Parts, Lines).

% split_string yields one more part than there are newlines, so a text ending
% in a newline ends in an empty part that is not a line of its own.
parts_to_lines([], []).
parts_to_lines([Part | Rest], Lines) :-
    string_codes(Part, PartCodes),
    (   Rest == []
    ->  (   PartCodes == []
        ->  Lines = []
        ;   Lines = [PartCodes]
        )
    ;   append(PartCodes, [0'\n], Line),
        Lines = [Line | More],
        parts_to_lines(Rest, More)
    ).

% A stripped `use` line leaves its newline behind, so every line number the
% parser reports for the remainder still matches the file on disk.
strip_use_lines([], [], []).
strip_use_lines([Line | Rest], [Text | UseTexts], [[0'\n] | CoreLines]) :-
    use_item(use(Text), Line, After),
    ws_only(After),
    !,
    strip_use_lines(Rest, UseTexts, CoreLines).
strip_use_lines([Line | Rest], UseTexts, [Line | CoreLines]) :-
    strip_use_lines(Rest, UseTexts, CoreLines).

% code_type(_, space) already covers newline and carriage return.
ws_only([]).
ws_only([Code | Rest]) :-
    once(code_type(Code, space)),
    ws_only(Rest).

% col_type/3 dedupes by (Ref, Column): an equal type keeps one, a conflict
% hard-errors naming both paths. Every other decl and rule keeps load order.
merge_pairs(Pairs, prog(Decls, Rules)) :-
    pairs_attr_decls(Pairs, AttrDecls),
    merge_col(AttrDecls, [], Decls),
    pairs_rules(Pairs, Rules).

pairs_attr_decls([], []).
pairs_attr_decls([(Path, prog(D, _)) | Rest], Attr) :-
    attr_decls(D, Path, AttrHere),
    pairs_attr_decls(Rest, More),
    append(AttrHere, More, Attr).

attr_decls([], _, []).
attr_decls([Decl | Rest], Path, [Path-Decl | More]) :- attr_decls(Rest, Path, More).

pairs_rules([], []).
pairs_rules([(_, prog(_, R)) | Rest], RulesList) :-
    pairs_rules(Rest, More),
    append(R, More, RulesList).

merge_col([], Accum, Decls) :-
    strip_paths(Accum, DeclsRev),
    reverse(DeclsRev, Decls).
merge_col([Path-Decl | Rest], Accum, Decls) :-
    (   Decl = col_type(Ref, Column, Type),
        member(Path2-col_type(Ref, Column, Type2), Accum)
    ->  (   Type == Type2
        ->  merge_col(Rest, Accum, Decls)
        ;   throw(rel_col_conflict(Ref, Path2, Path))
        )
    ;   merge_col(Rest, [Path-Decl | Accum], Decls)
    ).

strip_paths([], []).
strip_paths([_-Term | Rest], [Term | More]) :- strip_paths(Rest, More).

canonical_abs(Path, Abs) :-
    absolute_file_name(Path, Abs, [expand(true)]).

module_name(Abs, Name) :-
    file_base_name(Abs, Base),
    (   file_name_extension(Stem, _, Base)
    ->  Name = Stem
    ;   Name = Base
    ).

module_hash(Name, Hash) :-
    crypto_data_hash(Name, Full, [algorithm(sha256)]),
    sub_atom(Full, 0, 16, _, Hash),
    !.

bump_parse_count(Path) :-
    (   retract(parse_count_fact(Path, N))
    ->  N1 is N + 1
    ;   N1 = 1
    ),
    assertz(parse_count_fact(Path, N1)),
    !.

reset_parse_counts :- retractall(parse_count_fact(_, _)).

parse_count(Path, N) :- parse_count_fact(Path, N).
