% fixtures/json_arm.pl : the json arm (plans/2026-07-27-json-arm.md) plus the
% q7 bag-aggregate fixtures. Owner: coordinator (delegated lab promotions must
% not duplicate these).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% ═══ json values: braces are the one grammar ════════════════════════════════

% A braces literal in a body binding canonicalizes: keys sort, nesting keeps.
fixture(braces_literal_canonicalizes,
  prog([],
       [ (doc(Value) <- seed(Name), Value := {stars: 4, name: Name}) ]),
  [ seed(cli) ],
  [],
  [ final(doc/1, [ doc(obj([ name-cli, stars-4 ])) ]) ]).

% A braces literal in HEAD position is the same expression grammar.
fixture(braces_in_head_position,
  prog([],
       [ (doc_out({repo: Name}) <- seed(Name)) ]),
  [ seed(cli) ],
  [],
  [ final(doc_out/1, [ doc_out(obj([ repo-cli ])) ]) ]).

% decode: open object patterns, nested holes bind.
fixture(decode_open_pattern_binds_nested,
  prog([],
       [ (repo_name(Name)   <- raw_doc(Body), decode(Body, {name: Name})),
         (repo_owner(Login) <- raw_doc(Body), decode(Body, {owner: {login: Login}})) ]),
  [ raw_doc({name: cli, owner: {login: octo}, langs: [go, rust]}) ],
  [],
  [ final(repo_name/1,  [ repo_name(cli) ]),
    final(repo_owner/1, [ repo_owner(octo) ]) ]).

% Missing key and null both fail a bare field pattern: no rows, no error.
fixture(decode_missing_key_fails_quietly,
  prog([],
       [ (probe(Value) <- raw_doc(Body), decode(Body, {absent_key: Value})),
         (probe(Value) <- raw_doc(Body), decode(Body, {nullish: Value})) ]),
  [ raw_doc({name: cli, nullish: none}) ],
  [],
  [ final(probe/1, []) ]).

% json_each: element fan-out over a decoded array.
fixture(json_each_fans_out,
  prog([],
       [ (repo_lang(Lang) <- raw_doc(Body), decode(Body, {langs: Langs}), json_each(Langs, Lang)) ]),
  [ raw_doc({name: cli, langs: [go, rust]}) ],
  [],
  [ final(repo_lang/1, [ repo_lang(go), repo_lang(rust) ]) ]).

% ═══ aggregate heads (q9 reserved forms), bag multiplicity (q7) ═════════════

% R8's fail-pre-fix fixture: two hits on ONE line count 2 under bag.
% REJECTED READING (timeless_rail interpreter): set-of-projected-values gave 1.
fixture(count_is_bag_of_derivations,
  prog([],
       [ (hits(Path, count(Line)) <- hit(Path, Line, _)) ]),
  [ hit(main_rs, 1, 3), hit(main_rs, 1, 7), hit(lib_rs, 9, 1) ],
  [],
  [ final(hits/2, [ hits(lib_rs, 1), hits(main_rs, 2) ]) ]).

fixture(sum_min_max_group_by_plain_columns,
  prog([],
       [ (stat(Repo, sum(Stars), min(Stars), max(Stars)) <- star_row(Repo, Stars)) ]),
  [ star_row(cli, 4), star_row(cli, 10), star_row(shell, 7) ],
  [],
  [ final(stat/4, [ stat(cli, 14, 4, 10), stat(shell, 7, 7, 7) ]) ]).

% json_array: the bag in canonical (msort) order; duplicates SURVIVE (q7).
fixture(json_array_keeps_bag_duplicates,
  prog([],
       [ (star_bag(json_array(Stars)) <- repo(_, Stars)) ]),
  [ repo(alpha, 4), repo(beta, 4), repo(gamma, 9) ],
  [],
  [ final(star_bag/1, [ star_bag([4, 4, 9]) ]) ]).

% json_array groups by the plain columns and nests values.
fixture(json_array_groups_and_nests,
  prog([],
       [ (repo_langs(Repo, json_array(Lang)) <- repo_lang(Repo, Lang)) ]),
  [ repo_lang(cli, go), repo_lang(cli, rust), repo_lang(shell, sh) ],
  [],
  [ final(repo_langs/2, [ repo_langs(cli, [go, rust]), repo_langs(shell, [sh]) ]) ]).

% json_object: one document per group, keys sorted.
fixture(json_object_builds_document,
  prog([],
       [ (repo_meta(Repo, json_object(Key, Value)) <- repo_kv(Repo, Key, Value)) ]),
  [ repo_kv(cli, name, cli), repo_kv(cli, stars, 4) ],
  [],
  [ final(repo_meta/2, [ repo_meta(cli, obj([ name-cli, stars-4 ])) ]) ]).

% One key, two values in a group = the aggregate twin of the FD law.
fixture(json_object_dup_key_rejected,
  prog([],
       [ (repo_meta(json_object(Key, Value)) <- repo_kv(Key, Value)) ]),
  [ repo_kv(name, cli), repo_kv(name, shell) ],
  [],
  [ throws(json_object_dup_key([name, name])) ]).

% ═══ aggregates ACROSS TICKS (added by the expression+aggregate lift arc) ═══
% Every aggregate fixture above has an EMPTY Schedule, so the whole q7/q9
% aggregate family was graded only at the t=0 level closure -- the tick log is
% empty on both sides and says nothing (the vacuous-pass class SCOREBOARD.md
% Finding 2 names). These two put an aggregate under real arrivals AND a real
% retraction, which is what actually exercises a compiled program's
% group-scoped maintenance path.
%
% Both grade three things the empty-schedule fixtures cannot: the PER-TICK
% delta of an aggregate row (a group whose value moves must show -old then
% +new at the boundary, never just +new), the SILENCE of a group that did not
% move, and the DISAPPEARANCE of a group whose last member is retracted.

% star_row is an unkeyed Set rel, so `-star_row(...)` is an ordinary
% exact-row removal. Tick 1 seeds two repos; tick 2 adds a row to an existing
% group (cli's max moves 4 -> 10, count 1 -> 2) and opens a new group; tick 3
% retracts shell's only row, which must take the whole shell group away.
fixture(aggregate_count_min_max_track_arrivals_and_retraction,
  prog([],
       [ (stat(Repo, count(Stars), min(Stars), max(Stars)) <- star_row(Repo, Stars)) ]),
  [],
  [ [ +star_row(cli, 4), +star_row(shell, 7) ],
    [ +star_row(cli, 10), +star_row(docs, 2) ],
    [ -star_row(shell, 7) ] ],
  [ deltas(stat/4,
           [ [ +stat(cli, 1, 4, 4), +stat(shell, 1, 7, 7) ],
             [ -stat(cli, 1, 4, 4), +stat(cli, 2, 4, 10), +stat(docs, 1, 2, 2) ],
             [ -stat(shell, 1, 7, 7) ] ]),
    final(stat/4, [ stat(cli, 2, 4, 10), stat(docs, 1, 2, 2) ]) ]).

% The retraction case min/max cannot decompose (the match-frontier lab's rx
% table records incremental min/max over a retractable set as IMPOSSIBLE):
% removing the CURRENT minimum tells you nothing about the next one without
% re-reading the group. Tick 2 retracts cli's min (4) with 10 and 6 still
% present, so min must move 4 -> 6 and that can only come from a recompute.
% Tick 3 retracts a NON-extreme row (6), which changes count and nothing else,
% and tick 4 touches an unrelated group so the untouched one stays silent.
fixture(aggregate_min_recomputes_when_the_minimum_is_retracted,
  prog([],
       [ (stat(Repo, count(Stars), min(Stars), max(Stars)) <- star_row(Repo, Stars)) ]),
  [ star_row(cli, 4), star_row(cli, 10), star_row(cli, 6), star_row(docs, 3) ],
  [ [ -star_row(cli, 4) ],
    [ -star_row(cli, 6) ],
    [ +star_row(docs, 9) ] ],
  [ deltas(stat/4,
           [ [ -stat(cli, 3, 4, 10), +stat(cli, 2, 6, 10) ],
             [ -stat(cli, 2, 6, 10), +stat(cli, 1, 10, 10) ],
             [ -stat(docs, 1, 3, 3), +stat(docs, 2, 3, 9) ] ]),
    final(stat/4, [ stat(cli, 1, 10, 10), stat(docs, 2, 3, 9) ]) ]).

% sum over a group that both grows and shrinks, with a second grouped column
% carried through untouched -- the decomposable half of the family, graded on
% the same across-ticks shape so count/sum and min/max share one receipt
% style.
fixture(aggregate_sum_tracks_a_growing_and_shrinking_group,
  prog([],
       [ (budget(Team, sum(Cost)) <- spend(Team, _Item, Cost)) ]),
  [ spend(core, disk, 10), spend(core, cpu, 5) ],
  [ [ +spend(core, net, 7) ],
    [ -spend(core, disk, 10) ] ],
  [ deltas(budget/2,
           [ [ -budget(core, 15), +budget(core, 22) ],
             [ -budget(core, 22), +budget(core, 12) ] ]),
    final(budget/2, [ budget(core, 12) ]) ]).

% The whole arm end to end: decode a raw doc, fan out, re-aggregate into a
% NEW document. Construction was the missing half in v5 (gh-cache.dl stores
% bodies as opaque Str). Note the q9 consequence: an aggregate form sits at a
% HEAD COLUMN position only, so nesting one inside a braces literal takes a
% second rule (aggregate first, wrap after).
fixture(json_round_trip_decode_to_document,
  prog([],
       [ (repo_lang(Name, Lang) <- raw_doc(Body), decode(Body, {name: Name, langs: Langs}),
                                    json_each(Langs, Lang)),
         (lang_list(Name, json_array(Lang)) <- repo_lang(Name, Lang)),
         (lang_doc(Name, Doc) <- lang_list(Name, Langs), Doc := {langs: Langs}) ]),
  [ raw_doc({name: cli, langs: [go, rust]}) ],
  [],
  [ final(lang_doc/2, [ lang_doc(cli, obj([ langs-[go, rust] ])) ]) ]).
