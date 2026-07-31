% xref_facts.pl : the DATAFLOW ATLAS's PROLOG fact plane. One .pl path in, JSON
% lines of predicate-level call-graph facts out.
%
% Run:  swipl -q -l v6/prolog/tools/xref_facts.pl -g main -t halt -- <path.pl>
%       (v6/dl/fixtures/dataflow-atlas.dl6 drives it through an `sh` host)
%
% ── why this file exists at all ──────────────────────────────────────────────
%
% The atlas draws four language planes and every plane's facts must be EXTRACTED
% rather than described. TypeScript has `sprefa-extract`; shell and SQL are
% regular enough for a regex host; prolog had nothing. SWI ships the answer --
% `library(prolog_xref)` is the cross-referencer the IDE and `make/0` already
% use -- so this file is a thin JSON-lines adapter over it and writes no analysis
% of its own. That is the same trade `tools/self_map_facts.pl` makes when it
% reads `analyze.pl`'s own predicates: the facts are the tool's tables, not a
% second opinion about them.
%
% XREF READS, IT DOES NOT RUN. `xref_source/2` parses and expands a file without
% executing its directives, so pointing it at `ARCH.pl` or `compile.pl` costs a
% parse and never a load. That is why the atlas can cover 38 prolog files in one
% pass without consulting a single one of them.
%
% ── output contract ──────────────────────────────────────────────────────────
%
% One JSON object per line. The key SETS are pairwise non-containing so a
% projection host declaring one set never picks up another record's line
% (v6/tsv2/serve/1_hosts.ts:decodeObjectItems drops any object missing a declared
% output column, and drops any object whose declared column is null).
%
%   {"record":"pred_defined", "file","name","arity","how"}
%   {"record":"pred_called",  "file","caller","caller_arity","callee","callee_arity"}
%
% `name`/`arity` appear only on the first shape and `caller`/`callee` only on the
% second, so each projection selects exactly its own lines.
%
% Rows are SORTED before printing, because the atlas rail's byte-stability
% receipt (two runs of `just atlas` diff clean) rests on it. The dl6 engine's row
% order is not a guarantee this file may lean on.
%
% ── what is deliberately NOT emitted ─────────────────────────────────────────
%
% IMPORTED definitions (`xref_defined(_, _, imported(_))`) are dropped: the atlas
% wants "which file OWNS this predicate", and the importing file does not. A call
% that crosses files is recovered in the dl6 program instead, by joining a
% `pred_called` row's callee against every `pred_defined` row -- which is the
% ordinary relational join the atlas would have to write anyway, and it keeps the
% cross-file edge derivable rather than asserted here.
%
% Callers that are DIRECTIVES rather than predicates (`xref_called/3` reports
% those as `'<directive>'(Line)`) are dropped for the same reason: a directive is
% not a node the call graph can draw an arrow out of.

:- module(xref_facts, [ emit_for/1, main/0 ]).

:- use_module(library(lists)).
:- use_module(library(prolog_xref)).

% ── entry ────────────────────────────────────────────────────────────────────
%
% The path arrives in argv rather than spliced into a `-g` goal, for the reason
% tools/self_map_facts.pl states: a goal string would put the path through the
% shell's quoting AND prolog's reader, two escaping layers for a value the host
% template already quotes once.
%
% prolog-lint reports `unused_export_candidate main/0` and that report is CORRECT
% AND EXPECTED -- a `-g` entry is indistinguishable from a dead export at the
% source level. The caller is the `sh pl_def` / `sh pl_call` template in
% v6/dl/fixtures/dataflow-atlas.dl6.

main :-
    current_prolog_flag(argv, Argv),
    (   Argv = [Path]
    ->  emit_for(Path)
    ;   throw(error(xref_facts_expects_one_path(Argv), _))
    ).

% `silent(true)` keeps xref's own "unknown module" chatter off stderr, which
% otherwise interleaves with the host's process output for no reader's benefit.
% Absolute path first: xref keys its tables on the source it was handed, and the
% atlas's host passes a repo-relative path, so the two have to be reconciled
% before any table is read.
emit_for(Path) :-
    absolute_file_name(Path, Absolute),
    xref_source(Absolute, [silent(true)]),
    emit_defined(Absolute, Path),
    emit_called(Absolute, Path).

% ── the definitions this file owns ───────────────────────────────────────────

emit_defined(Absolute, Path) :-
    findall(row(Name, Arity, How),
            ( xref_defined(Absolute, Callable, HowTerm),
              owned_definition(HowTerm, How),
              callable_name_arity(Callable, Name, Arity)
            ),
            Rows0),
    sort(Rows0, Rows),
    forall(member(row(Name, Arity, How), Rows),
           print_json([ 'record'-'pred_defined',
                        'file'-Path,
                        'name'-Name,
                        'arity'-Arity,
                        'how'-How ])).

% The `How` inventory is prolog_xref's own; an imported definition is the one
% shape this atlas drops, and it is dropped by NAME rather than by a catch-all
% so a How this file has never seen becomes a visible failure rather than a
% silent omission.
owned_definition(local(_),        local).
owned_definition(dynamic(_),      dynamic).
owned_definition(multifile(_),    multifile).
owned_definition(thread_local(_), thread_local).
owned_definition(public(_),       public).
owned_definition(foreign(_),      foreign).
owned_definition(constraint(_),   constraint).

% ── the calls this file makes ────────────────────────────────────────────────
%
% `By` is the CALLING predicate. Both ends are reported as name + arity in their
% own columns rather than as `name/arity` text, because the dl6 program joins the
% callee against `pred_defined` and a join on two int/text columns is the shape
% the emitted SQL indexes.

emit_called(Absolute, Path) :-
    findall(row(CallerName, CallerArity, CalleeName, CalleeArity),
            ( xref_called(Absolute, Callee, By),
              callable_name_arity(By, CallerName, CallerArity),
              callable_name_arity(Callee, CalleeName, CalleeArity)
            ),
            Rows0),
    sort(Rows0, Rows),
    forall(member(row(CallerName, CallerArity, CalleeName, CalleeArity), Rows),
           print_json([ 'record'-'pred_called',
                        'file'-Path,
                        'caller'-CallerName,
                        'caller_arity'-CallerArity,
                        'callee'-CalleeName,
                        'callee_arity'-CalleeArity ])).

% Module qualification is stripped: the atlas node is the predicate, and the
% same predicate reached through `lists:member/2` and through `member/2` is one
% node. A caller that is not a predicate at all (xref reports directives as
% `'<directive>'(Line)`) fails here and the row is dropped, which is the
% intended reading -- see the header.
callable_name_arity(_Module:Callable, Name, Arity) :-
    !,
    callable_name_arity(Callable, Name, Arity).
callable_name_arity(Name/Arity, Name, Arity) :-
    !,
    atom(Name),
    integer(Arity).
callable_name_arity(Callable, Name, Arity) :-
    atom(Callable),
    !,
    Name = Callable,
    Arity = 0.
callable_name_arity(Callable, Name, Arity) :-
    compound(Callable),
    functor(Callable, Name, Arity),
    Name \== '<directive>'.

% ── printing ─────────────────────────────────────────────────────────────────
%
% Hand-rolled rather than library(http/json), for the reason self_map_facts.pl
% gives: the output is a flat object of atoms and integers, the escape set is
% four characters wide, and pulling the http bundle into a one-shot that a
% subprocess host spawns per file is cost with no benefit. `print_json/1` refuses
% anything but atom/integer values, so a future record shape carrying a compound
% cannot silently print as `foo(bar)` inside a JSON string.

print_json(Pairs) :-
    maplist(json_pair_text, Pairs, PairTexts),
    atomic_list_concat(PairTexts, ',', Body),
    format("{~w}~n", [Body]).

json_pair_text(Key-Value, Text) :-
    json_string(Key, KeyText),
    (   integer(Value)
    ->  format(atom(Text), '~w:~w', [KeyText, Value])
    ;   atom(Value)
    ->  json_string(Value, ValueText),
        format(atom(Text), '~w:~w', [KeyText, ValueText])
    ;   throw(error(xref_facts_unprintable_value(Key, Value), _))
    ).

json_string(Value, Text) :-
    atom_codes(Value, Codes),
    json_escape(Codes, Escaped),
    atom_codes(EscapedAtom, Escaped),
    format(atom(Text), '"~w"', [EscapedAtom]).

json_escape([], []).
json_escape([Code | Rest], Out) :-
    (   Code =:= 0'\\ -> Escaped = [0'\\, 0'\\]
    ;   Code =:= 0'"  -> Escaped = [0'\\, 0'"]
    ;   Code =:= 10   -> Escaped = [0'\\, 0'n]
    ;   Code =:= 13   -> Escaped = [0'\\, 0'r]
    ;   Code =:= 9    -> Escaped = [0'\\, 0't]
    ;   Escaped = [Code]
    ),
    json_escape(Rest, RestEscaped),
    append(Escaped, RestEscaped, Out).
