:- ensure_loaded('/private/tmp/claude-501/-Users-chrishafley-projects-sprefa/9236bad9-763d-49b4-bcdf-000c5fa97d8d/scratchpad/tc_emit.pl').

emit_lt :-
    scratch(Dir),
    atomic_list_concat([Dir, '/lt_probe.pl'], File),
    default_intern_mode(Mode0),
    sweep:read_all_fixtures(File, Entries),
    forall(member(entry(Name, Term, Bindings), Entries),
           emit_one(Dir, Mode0, Name, Term, Bindings)).
