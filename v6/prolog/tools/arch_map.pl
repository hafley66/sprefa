% arch_map.pl : emit the TRUE map of the v6 architecture as an atlas module.
%
% The map derives from the SAME facts ARCH.pl's `go` self-checks (graphs,
% refines, algorithms, tasks, sugar/kernel, tech) plus rulings.pl and the
% conformance corpus on disk, so it cannot drift from reality: if the facts
% lie, the build fails before the map does.
%
% Run:  swipl -q -l v6/prolog/tools/arch_map.pl -g emit -g halt
% Then: cd ~/projects/anim && npm run atlas:file atlases/sprefa-v6-arch.atlas.js
%
% Output: ~/projects/anim/atlases/sprefa-v6-arch.atlas.js

:- ensure_loaded('../ARCH.pl').
:- use_module('../conformance/rulings.pl').

out_path(Path) :-
    expand_file_name('~/projects/anim/atlases/sprefa-v6-arch.atlas.js', [Path]).

emit :-
    out_path(Path),
    setup_call_cleanup(open(Path, write, Stream),
                       emit_module(Stream),
                       close(Stream)),
    format("wrote ~w~n", [Path]).

emit_module(Stream) :-
    format(Stream, "export default {~n  d2: `~n", []),
    emit_d2(Stream),
    format(Stream, "`,~n  tours: {~n", []),
    emit_tours(Stream),
    format(Stream, "  },~n};~n", []).

% ── the d2 graph ────────────────────────────────────────────────────────────

emit_d2(Stream) :-
    format(Stream, "graphs: one graph five binding times {~n", []),
    forall(graph(Name, _, _, _), format(Stream, "  ~w~n", [Name])),
    format(Stream, "}~n", []),
    format(Stream, "algo: algorithms {~n", []),
    forall(algorithm(Name, _, _, _), format(Stream, "  ~w~n", [Name])),
    format(Stream, "}~n", []),
    format(Stream, "task: build order {~n", []),
    forall(task(Name, _, _), format(Stream, "  ~w~n", [Name])),
    format(Stream, "}~n", []),
    format(Stream, "lang: kernel and sugar {~n", []),
    forall(kernel(Name), format(Stream, "  ~w~n", [Name])),
    forall(sugar(Name, _), format(Stream, "  ~w~n", [Name])),
    format(Stream, "}~n", []),
    format(Stream, "tech: who may do what {~n", []),
    forall(tech(Name, _, _, _), format(Stream, "  ~w~n", [Name])),
    format(Stream, "}~n", []),
    format(Stream, "ruling: the user rulings {~n", []),
    forall(ruling(Id, _, _, _), format(Stream, "  ~w~n", [Id])),
    format(Stream, "}~n", []),
    format(Stream, "conformance: reference corpus {~n  engine~n  body~n  level_eval~n  runner~n", []),
    forall(fixture_file(Base), format(Stream, "  ~w~n", [Base])),
    format(Stream, "}~n", []),
    format(Stream, "made: things already built (the anti-forgetting ledger) {~n", []),
    forall(prior_art(Name, _, _, _), format(Stream, "  ~w~n", [Name])),
    forall(prior_art_target(Target), format(Stream, "  ~w~n", [Target])),
    format(Stream, "}~n", []),
    % edges
    forall(refines(Child, Parent),
           format(Stream, "graphs.~w -> graphs.~w~n", [Child, Parent])),
    forall(algorithm(Name, RunsOn, _, _),
           format(Stream, "algo.~w -> graphs.~w~n", [Name, RunsOn])),
    forall(( task(Name, _, Needs), member(Need, Needs) ),
           format(Stream, "task.~w -> task.~w~n", [Name, Need])),
    forall(( sugar(Name, Parts), member(Part, Parts) ),
           format(Stream, "lang.~w -> lang.~w~n", [Name, Part])),
    format(Stream, "conformance.engine -> conformance.body~n", []),
    format(Stream, "conformance.engine -> conformance.level_eval~n", []),
    format(Stream, "conformance.level_eval -> conformance.body~n", []),
    format(Stream, "conformance.runner -> conformance.engine~n", []),
    forall(fixture_file(Base),
           format(Stream, "conformance.~w -> conformance.engine~n", [Base])),
    format(Stream, "task.js_conformance_leg -> conformance.runner~n", []),
    forall(prior_art(Name, _, Feeds, _),
           format(Stream, "made.~w -> made.~w~n", [Feeds, Name])),
    format(Stream, "made.conformance_corpus -> conformance.runner~n", []),
    format(Stream, "algo.seminaive_eval -> tech.sqlite~n", []),
    format(Stream, "algo.count_ivm -> tech.sqlite~n", []),
    format(Stream, "conformance.engine -> tech.prolog~n", []),
    % annotations
    forall(graph(Name, Shape, Rows, Binding),
           ( sanitize(Rows, RowsText),
             format(Stream, "# @ graphs.~w : shape ~w, rows ~w, bound at ~w~n",
                    [Name, Shape, RowsText, Binding]) )),
    forall(graph(Name, _, _, Binding),
           format(Stream, "# tag graphs.~w : binding_~w~n", [Name, Binding])),
    forall(algorithm(Name, _, Species, Home),
           ( sanitize(Home, HomeText),
             format(Stream, "# @ algo.~w : species ~w -- home ~w~n",
                    [Name, Species, HomeText]),
             format(Stream, "# tag algo.~w : ~w~n", [Name, Species]),
             emit_src(Stream, algo, Name, Home) )),
    forall(task(Name, Status, _),
           ( format(Stream, "# tag task.~w : ~w~n", [Name, Status]),
             status_diff(Status, Diff)
             -> format(Stream, "# diff ~w task.~w~n", [Diff, Name]) ; true )),
    forall(kernel(Name),
           ( format(Stream, "# tag lang.~w : kernel~n", [Name]),
             format(Stream, "# diff add lang.~w~n", [Name]) )),
    forall(sugar(Name, _), format(Stream, "# tag lang.~w : sugar~n", [Name])),
    forall(tech(Name, Tier, Jobs, Note),
           ( sanitize(Note, NoteText),
             format(Stream, "# @ tech.~w : ~w -- jobs ~w -- ~w~n",
                    [Name, Tier, Jobs, NoteText]),
             format(Stream, "# tag tech.~w : ~w~n", [Name, Tier]) )),
    forall(ruling(Id, Choice, Status, Receipt),
           ( sanitize(Receipt, ReceiptText),
             format(Stream, "# @ ruling.~w : ~w -- ~w~n", [Id, Choice, ReceiptText]),
             format(Stream, "# tag ruling.~w : ~w~n", [Id, Status]) )),
    forall(fixture_file(Base),
           ( format(Stream, "# src conformance.~w = v6/prolog/conformance/fixtures/~w.pl:1~n",
                    [Base, Base]),
             format(Stream, "# tag conformance.~w : fixtures~n", [Base]) )),
    format(Stream, "# src conformance.engine = v6/prolog/conformance/engine.pl:1~n", []),
    format(Stream, "# src conformance.body = v6/prolog/conformance/body.pl:1~n", []),
    format(Stream, "# src conformance.level_eval = v6/prolog/conformance/level_eval.pl:1~n", []),
    format(Stream, "# src conformance.runner = v6/prolog/conformance/go.pl:1~n", []),
    forall(prior_art(Name, Path, _, Gift),
           ( sanitize(Gift, GiftText), sanitize(Path, PathText),
             format(Stream, "# @ made.~w : ~w -- lives at ~w~n", [Name, GiftText, PathText]),
             format(Stream, "# tag made.~w : prior_art~n", [Name]) )),
    forall(prior_art_target(Target),
           format(Stream, "# tag made.~w : inherited_into_v6~n", [Target])),
    format(Stream, "# view focus=graphs.ast mode=cone layout=dagre dir=LR~n", []).

emit_src(Stream, Container, Name, Home) :-
    (   atom(Home), sub_atom(Home, _, _, _, '/'),
        ( sub_atom(Home, _, _, _, '.pl') ; sub_atom(Home, _, _, _, '.ts') ),
        \+ sub_atom(Home, _, _, _, ' ')
    ->  format(Stream, "# src ~w.~w = ~w:1~n", [Container, Name, Home])
    ;   true ).

status_diff(done, add).
status_diff(labbed, mod).

prior_art_target(Target) :-
    findall(Feeds, prior_art(_, _, Feeds, _), AllFeeds),
    sort(AllFeeds, Targets),
    member(Target, Targets).

fixture_file(Base) :-
    expand_file_name('/Users/chrishafley/projects/sprefa/v6/prolog/conformance/fixtures/*.pl',
                     Paths),
    member(Path, Paths),
    file_base_name(Path, File),
    file_name_extension(Base, pl, File).

% Notes ride after a marker; strip characters the annotation scanner owns.
sanitize(Term, Text) :-
    format(atom(Raw), "~w", [Term]),
    atom_chars(Raw, Chars),
    maplist(safe_char, Chars, Safe),
    atom_chars(Text, Safe).

safe_char(':', '-') :- !.
safe_char('`', '\'') :- !.
safe_char('\n', ' ') :- !.
safe_char(Char, Char).

% ── tours, derived from the same facts ──────────────────────────────────────

emit_tours(Stream) :-
    % 1. the five binding times: the refines chain from delta_flow to ast.
    refine_chain(delta_flow, Chain),
    reverse(Chain, FromAst),
    tour_path_ids(graphs, FromAst, PathIds),
    format(Stream, "    \"one graph, five binding times\": [~n", []),
    format(Stream, "      { path: [~w], isolate: true, note: \"ast to delta_flow: refinements of ONE graph, not five designs. Compile owns the first three; sqlite owns the last two, on disk.\" },~n", [PathIds]),
    format(Stream, "    ],~n", []),
    % 2. build order: one focus step per task, topological, status in the note.
    findall(Name-Needs, task(Name, _, Needs), Pairs),
    topsort(Pairs, [], Order),
    format(Stream, "    \"build order (the roadmap, topsorted)\": [~n", []),
    forall(member(TaskName, Order),
           ( task(TaskName, Status, Needs2),
             sanitize(Needs2, NeedsText),
             format(Stream, "      { focus: \"task.~w\", isolate: false, note: \"~w -- needs ~w\" },~n",
                    [TaskName, Status, NeedsText]) )),
    format(Stream, "    ],~n", []),
    % 3. a feature grounding out: switch_map down to kernel.
    ground_chain(switch_map, GroundChain),
    tour_path_ids(lang, GroundChain, GroundIds),
    format(Stream, "    \"how a feature grounds out\": [~n", []),
    format(Stream, "      { path: [~w], isolate: true, note: \"switch_map desugars until every part is kernel; sugar_grounds_out enforces this for every feature.\" },~n", [GroundIds]),
    format(Stream, "    ],~n", []),
    % 4. the rulings, one step each.
    format(Stream, "    \"the rulings\": [~n", []),
    forall(ruling(Id, Choice, _, _),
           ( sanitize(Choice, ChoiceText),
             format(Stream, "      { focus: \"ruling.~w\", isolate: false, note: \"~w\" },~n",
                    [Id, ChoiceText]) )),
    format(Stream, "    ],~n", []).

refine_chain(Name, [Name | Rest]) :-
    ( refines(Name, Parent) -> refine_chain(Parent, Rest) ; Rest = [] ).

ground_chain(Name, [Name | Rest]) :-
    (   sugar(Name, [First | _])
    ->  ground_chain(First, Rest)
    ;   Rest = [] ).

tour_path_ids(Container, Names, Ids) :-
    findall(Quoted,
            ( member(Name, Names),
              format(atom(Quoted), "\"~w.~w\"", [Container, Name]) ),
            QuotedList),
    atomic_list_concat(QuotedList, ', ', Ids).
