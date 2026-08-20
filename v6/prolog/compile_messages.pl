% compile_messages.pl : the dl6 compiler's ONE observability door.
%
% Three things live here and nowhere else:
%
%   1. the debug-topic registry (dl6_debug_topic/2) and the DL6_DEBUG parser,
%   2. every prolog:message//1 clause the compile pipeline renders,
%   3. the message_hook that writes the JSON diagnostic channel's bytes.
%
% Topics are dl6(<name>). A topic that is off costs one dynamic lookup and
% writes nothing. Drivers turn them on: DL6_DEBUG=plan,expand or DL6_DEBUG=all.

:- module(compile_messages,
          [ dl6_debug/3,
            dl6_debug_topic/2,
            dl6_checkpoint/1,
            dl6_last_checkpoint/1,
            dl6_reset_checkpoint/0,
            dl6_debug_from_env/0,
            dl6_debugging/1,
            dl6_program_sizes/3
          ]).

:- use_module(library(debug)).
:- use_module(library(lists), [memberchk/2]).
:- use_module(library(apply), [exclude/3]).
:- use_module(library(http/json), [json_write_dict/3]).

% ═══ the registry ══════════════════════════════════════════════════════════
% One row per phase or subsystem. The second argument is the docs text; the
% report table and `DL6_DEBUG=all` both read this table rather than a list
% written twice.

dl6_debug_topic(parse,  'text-door read: source file, decls, rules, surface findings').
dl6_debug_topic(plan,   'plan phase: expanded sizes, rel plans, arrival targets, subscribed rels').
dl6_debug_topic(expand, 'one line per 1_expansion.pl phase: decls and rules in/out').
dl6_debug_topic(check,  'plan-time checks: supported subset, clock, world shapes, arity, edge head types').
dl6_debug_topic(lower,  'lower phase: rule statements and level statements produced').
dl6_debug_topic(boot,   'boot phase: boot statements produced from seed rows').
dl6_debug_topic(emit,   'emit phase: emitter seam and emitted character count').
dl6_debug_topic(write,  'write phase: output path and byte count').
dl6_debug_topic(sweep,  'corpus sweep: per-fixture bucket, then the run total').

% library(debug) warns on enabling a topic no debug/3 goal has registered, and
% dl6_debug/3 passes its topic as a variable, so nothing registers by goal
% expansion. Registering here keeps DL6_DEBUG=all silent about its own topics.
:- forall(dl6_debug_topic(Topic, _), (Term = dl6(Topic), nodebug(Term))).

% ═══ the call site ═════════════════════════════════════════════════════════

% Record the checkpoint whether or not the topic is on: a phase that FAILS has
% to name where it got to, and a failure diagnosis that only works under
% DL6_DEBUG is no diagnosis.
dl6_debug(Topic, Format, Args) :-
    dl6_checkpoint(Topic-Format-Args),
    Term = dl6(Topic),
    (   debugging(Term)
    ->  atomics_to_string(['dl6(', Topic, ') ', Format], Prefixed),
        debug(Term, Prefixed, Args)
    ;   true
    ).

% The guard a call site puts in front of a count it would otherwise walk a
% list to get. With the topic off nothing is computed and nothing is written.
dl6_debugging(Topic) :-
    Term = dl6(Topic),
    debugging(Term).

dl6_checkpoint(Term) :-
    nb_setval(dl6_checkpoint, Term).

dl6_last_checkpoint(Term) :-
    (   nb_current(dl6_checkpoint, Stored)
    ->  Term = Stored
    ;   Term = none
    ).

dl6_reset_checkpoint :-
    nb_setval(dl6_checkpoint, none).

% Both surface program shapes, counted in one place so no instrumentation site
% has to know which door produced its term.
dl6_program_sizes(prog(Decls, Rules), DeclCount, RuleCount) :-
    !,
    length(Decls, DeclCount),
    length(Rules, RuleCount).
dl6_program_sizes(program(Decls, Rules, _), DeclCount, RuleCount) :-
    !,
    length(Decls, DeclCount),
    length(Rules, RuleCount).
dl6_program_sizes(_, 0, 0).

% ═══ DL6_DEBUG, parsed once ════════════════════════════════════════════════

dl6_debug_from_env :-
    (   getenv('DL6_DEBUG', Spec)
    ->  dl6_debug_enable(Spec)
    ;   true
    ).

dl6_debug_enable(Spec) :-
    split_string(Spec, ",", " \t\n", Parts),
    exclude(==(""), Parts, Named),
    (   memberchk("all", Named)
    ->  forall(dl6_debug_topic(Topic, _), dl6_topic_on(Topic))
    ;   forall(member(Part, Named),
               ( atom_string(Topic, Part), dl6_topic_on(Topic) ))
    ).

dl6_topic_on(Topic) :-
    (   dl6_debug_topic(Topic, _)
    ->  Term = dl6(Topic),
        debug(Term)
    ;   print_message(warning, dl6_unknown_debug_topic(Topic))
    ).

% ═══ the JSON diagnostic channel ═══════════════════════════════════════════
%
% diag.pl builds the record; the bytes on the wire are JSON that LSP and CI
% parse, so the line renderer must not touch them. translate_message yields no
% lines and the hook writes the record itself, which is why print_message/2 can
% carry a channel whose output is not message-shaped.

:- multifile user:message_hook/3.

user:message_hook(dl6_diag(Stream, Record), _Kind, _Lines) :-
    json_write_dict(Stream, Record, [width(0)]),
    nl(Stream).

% The CLI error line carries no `ERROR: ` kind prefix, so it renders through
% print_message_lines/3 with an empty prefix rather than the default printer.
user:message_hook(dl6_cli_error(_, _), _Kind, Lines) :-
    print_message_lines(user_error, '', Lines).

% ═══ messages ══════════════════════════════════════════════════════════════

:- multifile prolog:message//1.

prolog:message(dl6_diag(_, _)) --> [].

prolog:message(dl6_unknown_debug_topic(Topic)) -->
    [ 'DL6_DEBUG names no such topic: ~w'-[Topic], nl,
      '    known topics: '-[] ],
    dl6_topic_list.

dl6_topic_list -->
    { findall(Topic, dl6_debug_topic(Topic, _), Topics),
      atomic_list_concat(Topics, ',', Spelling) },
    [ '~w'-[Spelling] ].

% The thrown ball. Text unchanged: `broken:` lines and error output carrying it
% are matched elsewhere, and the detail below is printed additively.
prolog:message(compile_phase_failed(Phase)) -->
    [ 'compile phase ~w failed and threw no ball'-[Phase] ].

% Printed at the failing phase, while the checkpoint is still the one the phase
% reached. Phase, program, checkpoint: the three facts a wedge report needs.
prolog:message(compile_phase_failed(Phase, Program, Checkpoint)) -->
    [ 'dl6: phase ~w failed on program ~w (failure, not a thrown ball)'-[Phase, Program], nl ],
    dl6_checkpoint_line(Checkpoint),
    [ '    re-run with DL6_DEBUG=all for the per-phase log'-[] ].

dl6_checkpoint_line(none) -->
    !,
    [ '    last checkpoint: none reached'-[], nl ].
% A renderer that throws turns a diagnosis into a second incident, so a format
% that does not fit its arguments falls back to the raw format string.
dl6_checkpoint_line(Topic-Format-Args) -->
    !,
    { (   catch(format(atom(Rendered), Format, Args), _, fail)
      ->  true
      ;   Rendered = Format
      ) },
    [ '    last checkpoint: ~w / ~w'-[Topic, Rendered], nl ].
dl6_checkpoint_line(Checkpoint) -->
    [ '    last checkpoint: ~w'-[Checkpoint], nl ].

% dl6c.pl and compile/scripts/bop_check.pl render one line per refused or
% broken compile. The bytes are `<prefix>: <text>`, unchanged from the
% format(user_error, ...) call this replaced.
prolog:message(dl6_cli_error(Prefix, Text)) --> [ '~w: ~w'-[Prefix, Text] ].
