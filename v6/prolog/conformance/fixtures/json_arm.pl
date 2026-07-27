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
