% use_resolve.pl : Door A. Resolve and splice `use "path".` imports, as v5's
% src/frontend.rs does, one module both doors call so imports cannot fork.

:- module(use_resolve,
          [ include_roots/2,
            resolve_use_path/3,
            expand_uses/6,
            expand_uses/8,
            short_hash/2,
            reset_parse_counts/0,
            parse_count/2
          ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(crypto)).
:- use_module(library(filesex)).
:- use_module('compile/parse_dl', [use_item/3, parse_dl_source/5]).
:- use_module('0_dot_expand', [declared_path/3]).

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
    expand_uses(EntryPath, OnStack, Loaded0, Loaded, ProgOut, ModuleTable,
                _Bindings, _Findings).

% Concatenating two files' Bindings is safe because
% analyze.pl:column_name_at/4 selects by ==/2, never by name.
expand_uses(EntryPath, OnStack, Loaded0, Loaded, ProgOut, ModuleTable,
            Bindings, Findings) :-
    entry_base_dir(EntryPath, BaseDir),
    collect_all(EntryPath, BaseDir, OnStack, Loaded0, Loaded, Files,
                ModuleTable, _Paths),
    merge_files(Files, ProgOut, Bindings, Findings).

% relative_file_name/3 reads a slashless second argument as a FILE and drops
% its last segment, so the base always carries its trailing slash.
entry_base_dir(EntryPath, BaseDir) :-
    canonical_abs(EntryPath, EntryAbs),
    file_directory_name(EntryAbs, Dir),
    atom_concat(Dir, '/', BaseDir).

% Loaded threads through the fold, so a diamond's second sight of a file is a
% cache hit; the cached entry carries the subtree's paths so a MOUNT of an
% already-loaded file still grafts the same tree.
collect_children([], _, _, _, _, _, Loaded, Loaded, [], [], []).
collect_children([Spec | Rest], Roots, BaseDir, OnStack, OwnerName, OwnerHash,
                 Loaded0, Loaded, Files, Tables, EdgeDecls) :-
    use_spec_parts(Spec, UseText, AliasOrNone),
    (   resolve_use_path(Roots, UseText, AbsPath)
    ->  true
    ;   throw(use_path_unresolved(UseText, Roots))
    ),
    (   memberchk(loaded(AbsPath, CachedPaths), Loaded0)
    ->  FirstFiles = [], FirstTables = [], Loaded1 = Loaded0,
        ChildPaths = CachedPaths
    ;   collect_all(AbsPath, BaseDir, OnStack, Loaded0, Loaded1, FirstFiles,
                    FirstTables, ChildPaths)
    ),
    edge_decls_for(AliasOrNone, AbsPath, BaseDir, OwnerName, OwnerHash,
                   ChildPaths, HereEdges),
    collect_children(Rest, Roots, BaseDir, OnStack, OwnerName, OwnerHash,
                     Loaded1, Loaded, MoreFiles, MoreTables, MoreEdges),
    append(FirstFiles, MoreFiles, Files),
    append(FirstTables, MoreTables, Tables),
    append(HereEdges, MoreEdges, EdgeDecls).

use_spec_parts(use(Text), Text, none).
use_spec_parts(use(Text, Alias), Text, alias(Alias)).

% The dependency edge is minted for every use; an alias ADDS the mount edge
% beside it (ruling mount_alias_additive) rather than replacing it.
edge_decls_for(AliasOrNone, AbsPath, BaseDir, OwnerName, OwnerHash, ChildPaths,
               EdgeDecls) :-
    canonical_abs(AbsPath, ChildAbs),
    module_name(ChildAbs, ChildName),
    module_hash(BaseDir, ChildAbs, ChildHash),
    UseEdge = module_edge_decl(OwnerHash, ChildHash, use, ChildName),
    (   AliasOrNone = alias(Alias)
    ->  EdgeDecls = [ UseEdge,
                      module_edge_decl(OwnerHash, ChildHash, mount, Alias),
                      mount_decl(Alias, ChildName, OwnerName, ChildPaths) ]
    ;   EdgeDecls = [UseEdge]
    ).

% One module's whole subtree. The ENTRY parses LAST: parse_dl_source/5 retracts
% its statement table per call, and the diag channel reads the entry's.
collect_all(EntryPath, BaseDir, OnStack, Loaded0, Loaded, Files, Tables,
            SubtreePaths) :-
    canonical_abs(EntryPath, EntryAbs),
    (   memberchk(loaded(EntryAbs, _), OnStack)
    ->  on_stack_paths(OnStack, StackPaths),
        (   StackPaths = [EntryAbs | _]
        ->  throw(use_cycle([EntryAbs]))
        ;   throw(use_cycle([EntryAbs | StackPaths]))
        )
    ;   true
    ),
    strip_entry(EntryPath, EntryAbs, UseSpecs, CoreCodes),
    include_roots(EntryPath, Roots),
    module_name(EntryAbs, EntryName),
    module_hash(BaseDir, EntryAbs, EntryHash),
    collect_children(UseSpecs, Roots, BaseDir, [loaded(EntryAbs, []) | OnStack],
                     EntryName, EntryHash, Loaded0, Loaded1, ChildFiles,
                     ChildTables, EdgeDecls),
    parse_dl_source(EntryPath, CoreCodes, OwnProg, OwnBindings, OwnFindings),
    prog_parts(OwnProg, OwnDecls0, OwnRules, OwnQueries),
    rel_module_decls(OwnDecls0, EntryHash, RelModuleDecls),
    append([OwnDecls0, [module_decl(EntryName, EntryHash)], RelModuleDecls,
            EdgeDecls],
           OwnDecls),
    append(ChildFiles,
           [file(EntryAbs, OwnDecls, OwnRules, OwnQueries, OwnBindings,
                 OwnFindings)],
           Files),
    append(ChildTables, [ module(EntryAbs, EntryName, EntryHash) ], Tables),
    subtree_paths(Files, SubtreePaths),
    Loaded = [loaded(EntryAbs, SubtreePaths) | Loaded1].

% Read off the file's OWN decls, mounts excluded: a mounted rel keeps the
% identity of the module that declared it, never the module that grafted it.
rel_module_decls(OwnDecls, Hash, RelModuleDecls) :-
    findall(Name, declared_path(OwnDecls, _Segments, Name), Names0),
    sort(Names0, Names),
    findall(rel_module_decl(Name, Hash), member(Name, Names), RelModuleDecls).

on_stack_paths(OnStack, Paths) :-
    findall(Path, member(loaded(Path, _), OnStack), Paths).

% parse_dl_source/5 picks prog/2 or program/3 per FILE; the merge re-picks it
% over the whole spliced program by the same test.
prog_parts(prog(Decls, Rules), Decls, Rules, []).
prog_parts(program(Decls, Rules, Queries), Decls, Rules, Queries).

merged_prog(Decls, Rules, Queries, Prog) :-
    (   Queries == [],
        \+ member(sh_decl(_, _, _, _), Decls),
        \+ member(bind_decl(_, _), Decls)
    ->  Prog = prog(Decls, Rules)
    ;   Prog = program(Decls, Rules, Queries)
    ).

% The graft's raw material: every path the subtree declares, mounts included,
% read off the CONCATENATED decls (sort/2 collapses what merge_col would).
subtree_paths(Files, SubtreePaths) :-
    findall(Decl,
            ( member(file(_, Decls, _, _, _, _), Files), member(Decl, Decls) ),
            AllDecls),
    findall(Segments-Name, declared_path(AllDecls, Segments, Name), Paths0),
    sort(Paths0, SubtreePaths).

% The parse counter is what a re-parsing loader trips; end-state equality on a
% diamond looks identical whether the shared file was read once or twice.
strip_entry(EntryPath, EntryAbs, UseSpecs, CoreCodes) :-
    read_file_to_codes(EntryPath, Codes, []),
    bump_parse_count(EntryAbs),
    split_codes_lines(Codes, Lines),
    strip_use_lines(Lines, UseSpecs, CoreLines),
    flatten(CoreLines, CoreCodes).

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
strip_use_lines([Line | Rest], [Spec | UseSpecs], [[0'\n] | CoreLines]) :-
    use_item(Spec, Line, After),
    ws_only(After),
    !,
    strip_use_lines(Rest, UseSpecs, CoreLines).
strip_use_lines([Line | Rest], UseSpecs, [Line | CoreLines]) :-
    strip_use_lines(Rest, UseSpecs, CoreLines).

% code_type(_, space) already covers newline and carriage return.
ws_only([]).
ws_only([Code | Rest]) :-
    once(code_type(Code, space)),
    ws_only(Rest).

% col_type/3 dedupes by (Ref, Column): an equal type keeps one, a conflict
% hard-errors naming both paths. Every other decl and rule keeps load order.
merge_files(Files, Prog, Bindings, Findings) :-
    files_attr_decls(Files, AttrDecls),
    merge_col(AttrDecls, [], Decls),
    files_field(3, Files, Rules),
    files_field(4, Files, Queries),
    files_field(5, Files, Bindings),
    files_field(6, Files, Findings),
    merged_prog(Decls, Rules, Queries, Prog).

files_attr_decls([], []).
files_attr_decls([file(Path, D, _, _, _, _) | Rest], Attr) :-
    attr_decls(D, Path, AttrHere),
    files_attr_decls(Rest, More),
    append(AttrHere, More, Attr).

attr_decls([], _, []).
attr_decls([Decl | Rest], Path, [Path-Decl | More]) :- attr_decls(Rest, Path, More).

files_field(_, [], []).
files_field(Position, [File | Rest], List) :-
    arg(Position, File, Here),
    files_field(Position, Rest, More),
    append(Here, More, List).

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

% Identity is the path relative to the ENTRY's directory, extension dropped:
% equal basenames stay distinct and an entry hashes exactly its module name.
module_hash(BaseDir, Abs, Hash) :-
    relative_file_name(Abs, BaseDir, Relative),
    module_name(Relative, BaseStem),
    file_directory_name(Relative, Dir),
    (   Dir == '.'
    ->  Stem = BaseStem
    ;   atomic_list_concat([Dir, '/', BaseStem], Stem)
    ),
    short_hash(Stem, Hash).

% SHA-256 truncated to 16 hex characters. The compiler's one hash: module
% identity, rel h_id and the schema/rule digests all read it.
short_hash(Text, Hash) :-
    crypto_data_hash(Text, Full, [algorithm(sha256)]),
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
