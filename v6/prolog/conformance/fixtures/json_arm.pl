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

% ═══ the json wiring arc acceptance corpus (2026-07-30) ═════════════════════
%
% One fixture per production added by the json grammar wiring, each carrying
% the archive source the json_syntax lab drew it from. Every one of these is
% a form five generations shipped and v6 had no spelling for.

% THE FLAGSHIP, examples/gh-cache.dl:116-117, transcribed to dl6. Array spread
% over an array of objects, siblings correlated, one row per element. The
% recovery doc graded this row "(c) blocked on storage"; the lab's L2 receipt
% executes it as ONE json_each join.
fixture(json_array_spread_fans_out_correlated_siblings,
  prog([],
       [ (pull_request(Number, Title, Author) <-
            resp(Body),
            decode(Body, spread({number: Number, title: Title,
                                 user: {login: Author}}))) ]),
  [ resp([ {number: 1, title: first,  user: {login: octo}},
           {number: 2, title: second, user: {login: hubot}} ]) ],
  [],
  [ final(pull_request/3, [ pull_request(1, first,  octo),
                           pull_request(2, second, hubot) ]) ]).

% Spread over an element that does not match binds nothing and raises nothing:
% the missing-key silence, one level down inside the fan-out.
fixture(json_array_spread_skips_non_matching_elements,
  prog([],
       [ (numbered(Number) <- resp(Body), decode(Body, spread({number: Number}))) ]),
  [ resp([ {number: 1}, {title: no_number}, {number: 3} ]) ],
  [],
  [ final(numbered/1, [ numbered(1), numbered(3) ]) ]).

% KEY CAPTURE (ruling json_key_hole_marker = dollar), examples/type-from-json.dl:25.
% The VALUE hole is a bare variable, which is the canonical term form: `$` is
% the marker on the key plane, where a bare identifier is a literal label, and
% a TEXT-door alias on the value plane, where a bare identifier is already a
% variable. `{$Key: Value}` and the text `{$key: $value}` are the same term.
% The single most-used v5 form, and the lowering is json_each's own (key,value)
% columns -- zero new SQL machinery (lab receipt L3).
fixture(json_key_capture_binds_key_and_value,
  prog([],
       [ (field(Key, Value) <- raw_doc(Body), decode(Body, {$Key: Value})) ]),
  [ raw_doc({name: cli, stars: 4}) ],
  [],
  [ final(field/2, [ field(name, cli), field(stars, 4) ]) ]).

% Two key holes nested: v4/examples/openapi-cardinality-markdown.sprf, the
% path x method fan-out. Cardinality is the product, which is what makes this
% the test that key capture really is a join and not a lookup.
fixture(json_key_capture_nests_and_fans_out,
  prog([],
       [ (operation(Path, Method, Id) <-
            spec(Body),
            decode(Body, {paths: {$Path: {$Method: {operationId: Id}}}})) ]),
  [ spec({paths: {'/users': {get:  {operationId: list_users},
                             post: {operationId: create_user}},
                  '/pets':  {get:  {operationId: list_pets}}}}) ],
  [],
  [ final(operation/3, [ operation('/pets',  get,  list_pets),
                        operation('/users', get,  list_users),
                        operation('/users', post, create_user) ]) ]).

% `**` DESCENT (ruling descent_depth_cap = uncapped), archive-20260428/
% README.md:78. Unbounded like the CSS descendant combinator; lowers to
% json_tree, whose first row is the root, so the root is a candidate too.
fixture(json_descent_matches_at_any_depth,
  prog([],
       [ (image(Repository, Tag) <-
            chart(Body),
            decode(Body, {'**': {image: {repository: Repository, tag: Tag}}})) ]),
  [ chart({spec: {template: {image: {repository: nginx, tag: '1.2'}}},
           sidecar: {image: {repository: envoy, tag: '2.0'}}}) ],
  [],
  [ final(image/2, [ image(envoy, '2.0'), image(nginx, '1.2') ]) ]).

% Descending into a SCALAR is a silent non-match, never an error. This is the
% oracle half of the emitted `type = 'object'` guard: without that guard SQLite
% raises `malformed JSON` and kills the whole statement (lab finding 6).
fixture(json_descent_into_scalars_is_silent,
  prog([],
       [ (found(Value) <- doc(Body), decode(Body, {'**': {leaf: Value}})) ]),
  [ doc({a: 1, b: text, c: {leaf: here}, d: [1, 2]}) ],
  [],
  [ final(found/1, [ found(here) ]) ]).

% The EMPTY object, the atom `{}` on both doors. An open pattern with no
% members: matches any object, binds nothing, and does NOT match a scalar.
fixture(json_empty_object_pattern_matches_any_object,
  prog([],
       [ (is_object(Name) <- entry(Name, Value), decode(Value, {})) ]),
  [ entry(first, {a: 1}), entry(second, 4), entry(third, {}) ],
  [],
  [ final(is_object/1, [ is_object(first), is_object(third) ]) ]).

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
