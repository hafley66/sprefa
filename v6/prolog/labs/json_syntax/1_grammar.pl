% ╔══════════════════════════════════════════════════════════════════════════╗
% ║  PROTOTYPE DCG -- LAB ONLY, NOT PRODUCTION.                              ║
% ║  This file is NOT consulted by parse_dl.pl, print_dl.pl, registry.pl or  ║
% ║  any compile/ module. It exists to PROVE a grammar draft parses the real ║
% ║  archive text verbatim before any production parser is touched           ║
% ║  (no-unsighted-syntax law: the user rules on the spelling first).        ║
% ╚══════════════════════════════════════════════════════════════════════════╝
%
% Two dialects, ONE intermediate representation:
%
%   v5   -- the shipped brace-pattern surface (src/datapath.rs, 5 generations).
%           `$name` = hole, bare run in value position = literal.
%   dl6  -- the same grammar under the dl6 spelling law (bare identifier =
%           variable, quoted = constant). No `$` needed on the VALUE plane.
%
% The acceptance claim this file discharges: the flagship gh-cache pattern
% (examples/gh-cache.dl:114-124) parses VERBATIM in the v5 dialect, and its
% dl6-native transcription parses to the BYTE-IDENTICAL IR term.
%
% Grammar (11 productions; v5 shipped 9 -- the 2 extra are the literal/pattern
% split, which v5 did not have because v5 had no construction plane):
%
%   ── LITERAL (constructing) ──  head args, world/VALUES rows, host outputs
%   jsonlit  = objlit | arrlit | scalar
%   objlit   = "{" [ pairlit {"," pairlit} [","] ] "}"
%   pairlit  = keylit ":" jsonlit
%   keylit   = IDENT | STRING                     % unquoted when [A-Za-z_]\w*
%   arrlit   = "[" [ jsonlit {"," jsonlit} [","] ] "]"
%   scalar   = INT | FLOAT | STRING | "true" | "false"
%
%   ── PATTERN (matching) ──  rule bodies only
%   jsonpat  = objpat | arrpat | hole | scalar    % scalar = equality filter
%   objpat   = "{" [ pairpat {"," pairpat} [","] ] "}"     % OPEN: extra keys ok
%   pairpat  = keypat ":" jsonpat
%   keypat   = IDENT | STRING | keyhole | "**" | GLOB | "re:" REGEX
%   arrpat   = "[" "..." jsonpat "]"              % spread: ONE ROW PER ELEMENT
%            | "[" [jsonpat {"," jsonpat}] "]"    % positional, fixed arity
%   hole     = "$" IDENT | "${" IDENT ["?"] "}" | "$_"      % v5 dialect
%            | IDENT                                        % dl6 dialect
%
% LAW THE TWO PLANES DERIVE (§2 of the plan doc): quoting is the literal
% marker on the VALUE plane, bareness is the literal marker on the KEY plane.
% This is forced, not chosen -- JSON5 permits unquoted keys and forbids
% unquoted string values, so the value slot is free for dl6 variables while
% the key slot is not. Every key-axis production is therefore PATTERN-ONLY:
% constructing an object with a computed key is json_object(K,V), never a
% brace literal.

:- module(json_syntax_proto,
          [ parse_pattern/3,
            parse_literal/3,
            pattern_is_literal/1,
            pattern_to_literal/2,
            example/4,
            grammar_receipts/1
          ]).

:- use_module(library(lists)).

% ── entry points ─────────────────────────────────────────────────────────────

% parse_pattern(+Dialect, +Text, -PatternIR)
parse_pattern(Dialect, Text, IR) :-
    atom_codes(Text, Codes),
    phrase(top_pattern(Dialect, IR), Codes).

% parse_literal(+Dialect, +Text, -LiteralIR)
% The literal grammar is the pattern grammar RESTRICTED to the hole-free
% subset. Proving it that way (rather than writing a second DCG) is itself the
% receipt that `{` opens one grammar with two roles.
parse_literal(Dialect, Text, Literal) :-
    parse_pattern(Dialect, Text, IR),
    pattern_to_literal(IR, Literal).

% ── IR ───────────────────────────────────────────────────────────────────────
%
% pattern:  pat_obj([kp(KeyMatcher, Pat), ...])
%           pat_arr_spread(Pat) | pat_arr_fixed([Pat, ...])
%           pat_hole(Name) | pat_anon | pat_eq(Scalar) | pat_text(Template)
% key:      k_exact(Atom) | k_hole(Name) | k_anon
%           | k_glob(Text) | k_re(Regex) | k_descend
% literal:  lit_obj([Key-Lit, ...]) | lit_arr([Lit, ...]) | lit_scalar(V)

pattern_is_literal(IR) :- pattern_to_literal(IR, _).

pattern_to_literal(pat_obj(Members), lit_obj(Pairs)) :- !,
    maplist(member_literal, Members, Pairs).
pattern_to_literal(pat_arr_fixed(Items), lit_arr(Literals)) :- !,
    maplist(pattern_to_literal, Items, Literals).
pattern_to_literal(pat_eq(Value), lit_scalar(Value)).

member_literal(kp(k_exact(Key), Pat), Key-Literal) :-
    pattern_to_literal(Pat, Literal).

% ── DCG ──────────────────────────────────────────────────────────────────────

top_pattern(Dialect, IR) --> ws, pat_body(Dialect, IR), ws.

pat_body(Dialect, pat_obj(Members)) -->
    "{", !, ws, pat_members(Dialect, Members), ws, "}".
pat_body(Dialect, pat_arr_spread(Sub)) -->
    "[", ws, "...", !, ws, pat_body(Dialect, Sub), ws, "]".
pat_body(Dialect, pat_arr_fixed(Items)) -->
    "[", !, ws, pat_items(Dialect, Items), ws, "]".
pat_body(_, IR) --> sigil_hole(IR), !.
pat_body(Dialect, IR) --> scalar_pattern(Dialect, IR).

pat_members(Dialect, [Member | Rest]) -->
    pat_member(Dialect, Member), !, ws, pat_members_rest(Dialect, Rest).
pat_members(_, []) --> [].

pat_members_rest(Dialect, Rest) --> ",", !, ws, pat_members(Dialect, Rest).
pat_members_rest(_, []) --> [].

pat_member(Dialect, kp(Key, Value)) -->
    key_matcher(Key), ws, ":", ws, pat_body(Dialect, Value).

pat_items(Dialect, [Item | Rest]) -->
    pat_body(Dialect, Item), !, ws, pat_items_rest(Dialect, Rest).
pat_items(_, []) --> [].

pat_items_rest(Dialect, Rest) --> ",", !, ws, pat_items(Dialect, Rest).
pat_items_rest(_, []) --> [].

% ── the key axis (PATTERN ONLY) ──────────────────────────────────────────────

key_matcher(k_descend) --> "**", !.
key_matcher(k_anon)    --> "$_", !.
key_matcher(k_hole(Name)) -->
    "${", !, ident_codes(Codes), optional_question, "}",
    { hole_name(Codes, Name) }.
key_matcher(k_hole(Name)) -->
    "$", ident_codes(Codes), !, { hole_name(Codes, Name) }.
key_matcher(k_re(Regex)) -->
    "re:", !, key_run(Codes), { atom_codes(Regex, Codes) }.
% A QUOTED key is always literal. This is how v5 lets an OpenAPI/JSON-Schema
% document use a real "$ref" key without it reading as a capture
% (archive TASKS.md:168-171).
key_matcher(k_exact(Key)) --> quoted_text(Key), !.
key_matcher(Key) -->
    key_run(Codes), { key_run_matcher(Codes, Key) }.

key_run_matcher(Codes, Key) :-
    atom_codes(Atom, Codes),
    (   glob_codes(Codes)
    ->  Key = k_glob(Atom)
    ;   Key = k_exact(Atom)
    ).

glob_codes(Codes) :- memberchk(0'*, Codes), !.
glob_codes(Codes) :- memberchk(0'?, Codes), !.
glob_codes(Codes) :- memberchk(0'[, Codes).

% One key token: anything that is not a delimiter and not whitespace.
key_run([Code | Rest]) --> [Code], { key_code(Code) }, key_run_rest(Rest).
key_run_rest([Code | Rest]) --> [Code], { key_code(Code) }, !, key_run_rest(Rest).
key_run_rest([]) --> [].

key_code(Code) :-
    \+ memberchk(Code, [0':, 0',, 0'{, 0'}, 0'[, 0']]),
    \+ code_type(Code, space).

% ── the value plane ──────────────────────────────────────────────────────────

sigil_hole(pat_anon) --> "$_", !.
sigil_hole(pat_hole(Name)) -->
    "${", !, ident_codes(Codes), optional_question, "}",
    { hole_name(Codes, Name) }.
sigil_hole(pat_hole(Name)) -->
    "$", ident_codes(Codes), { hole_name(Codes, Name) }.

% v5: a quoted value containing `$` is a LeafPattern (a template over the value
% text); anything else quoted is an exact match. A bare run is an exact match.
scalar_pattern(v5, IR) -->
    quoted_text(Text), !,
    { sub_atom(Text, _, _, _, '$') -> IR = pat_text(Text) ; IR = pat_eq(Text) }.
scalar_pattern(v5, pat_eq(Value)) -->
    value_run(Codes), { value_atom(Codes, Value) }.

% dl6: quoting is the literal marker on the value plane; a bare identifier is a
% VARIABLE (the ruled dl6 spelling, SYNTAX.md). Numbers and true/false stay
% literal because they are not identifiers.
scalar_pattern(dl6, pat_eq(Text)) --> quoted_text(Text), !.
scalar_pattern(dl6, IR) -->
    value_run(Codes),
    { value_atom(Codes, Value),
      (   number(Value)
      ->  IR = pat_eq(Value)
      ;   memberchk(Value, [true, false])
      ->  IR = pat_eq(Value)
      ;   Value == '_'
      ->  IR = pat_anon
      ;   identifier_atom(Value)
      ->  IR = pat_hole(Value)
      ;   IR = pat_eq(Value)
      ) }.

value_run([Code | Rest]) --> [Code], { value_code(Code) }, value_run_rest(Rest).
value_run_rest([Code | Rest]) --> [Code], { value_code(Code) }, !, value_run_rest(Rest).
value_run_rest([]) --> [].

value_code(Code) :-
    \+ memberchk(Code, [0',, 0'}, 0'], 0'{, 0'[]),
    \+ code_type(Code, space).

value_atom(Codes, Value) :-
    atom_codes(Atom, Codes),
    (   atom_number(Atom, Number)
    ->  Value = Number
    ;   Value = Atom
    ).

identifier_atom(Atom) :-
    atom(Atom),
    atom_codes(Atom, [First | Rest]),
    ( code_type(First, lower) ; First == 0'_ ), !,
    forall(member(Code, Rest), ( code_type(Code, csym) )).

% ── lexical helpers ──────────────────────────────────────────────────────────

% Both quote characters are accepted: dl6 spells constants with single quotes,
% real JSON documents use double quotes, and the archive fixtures use both.
quoted_text(Text) --> "\"", !, quoted_body(0'\", Codes), { atom_codes(Text, Codes) }.
quoted_text(Text) --> "'", quoted_body(0'', Codes), { atom_codes(Text, Codes) }.

quoted_body(Quote, []) --> [Quote], !.
quoted_body(Quote, [Code | Rest]) --> [Code], quoted_body(Quote, Rest).

ident_codes([Code | Rest]) --> [Code], { code_type(Code, csym) }, ident_rest(Rest).
ident_rest([Code | Rest]) --> [Code], { code_type(Code, csym) }, !, ident_rest(Rest).
ident_rest([]) --> [].

% v3 recorded that `?` inside `${...}` is a diagnostic marker with NO runtime
% semantics: it is stripped before matching
% (v3/crates/pipeline/src/walk/brace_parse.rs:220-223). Accepted and dropped.
optional_question --> "?", !.
optional_question --> [].

% Capture names bind lowercase (archive TASKS.md:168 "Capture vars bind
% lowercase (dl convention)"). This is exactly what makes the v5 text and its
% dl6 transcription produce the same IR term.
hole_name(Codes, Name) :-
    atom_codes(Raw, Codes),
    downcase_atom(Raw, Name).

ws --> [Code], { code_type(Code, space) }, !, ws.
ws --> "#", !, line_comment, ws.
ws --> [].

line_comment --> [0'\n], !.
line_comment --> [_], !, line_comment.
line_comment --> [].

% ── the corpus: every example below is VERBATIM archive text ─────────────────
% example(Id, Dialect, Text, Source)

example(flat_and_nested_one_row, v5,
        '{ number: $n, user: { login: $a } }',
        'src/datapath.rs:1502-1509').
example(flat_and_nested_one_row, dl6,
        '{number: n, user: {login: a}}',
        'dl6 transcription of the same pattern').

example(gh_cache_flagship, v5,
        '[... { number: $num, title: $title, state: $state,\n                            user: { login: $author } } ]',
        'examples/gh-cache.dl:116-117').
example(gh_cache_flagship, dl6,
        '[... {number: num, title: title, state: state, user: {login: author}}]',
        'dl6 transcription of the flagship').

example(key_capture, v5, '{ $K: $V }', 'src/datapath.rs:1291-1298').
example(key_capture_program, v5, '{ $key: $value }', 'examples/type-from-json.dl:25').

example(recursive_descent, v5, '{ **: { image: $i } }', 'src/datapath.rs:1487-1494').
example(regex_key, v5, '{ re:^v: $val }', 'src/datapath.rs:1532-1542').
example(glob_key, v5, '{ *id: $v }', 'src/datapath.rs:1543-1549').
example(array_scalar_spread, v5, '{ tags: [...$t] }', 'src/datapath.rs:1526-1531').
example(optional_suffix, v5, '{ n: ${N?} }', 'src/datapath.rs:1368-1374').
example(value_template, v5, '{ image: "$REPO:$TAG" }', 'v3 ops/json.rs:60-74').
example(value_wildcard, v5, '{ x: $_ }', 'recovery doc §5.2 row 5').
example(quoted_dollar_key, v5, '{ "$ref": $v }', 'archive TASKS.md:168-171').

example(v1_workspace_members, v5,
        '{ workspace: { members: [...$MEMBER] } }',
        'archive-20260428/sprefa-rules.sprf:14').
example(v1_dep_regex_key, v5,
        '{ re:^(dev-)?dependencies: { $NAME: $_ } }',
        'archive-20260428/sprefa-rules.sprf:10').
example(v1_values_image, v5,
        '{ image: { repository: $REPO, tag: $TAG } }',
        'archive-20260428/EXAMPLES.sprf:13').
example(v3_cargo_package, v5,
        '{ package: { name: ${N?}, version: ${V?} } }',
        'v3/.../json_yaml_toml_smoke.sprf:7').
example(v3_openapi_path, v5,
        '{ paths: { /users: { get: { operationId: ${OPID?} } } } }',
        'v3/.../golden_kitchen_sink.sprf:40').
example(v4_toml_repos, v5,
        '{ repos: [ ... { slug: ${SLUG?}, root: ${ROOT?}, remote: ${REMOTE?} } ] }',
        'v4/tests/read_gates_bytes_smoke.rs:185').
example(v4_openapi_two_key_fanout, v5,
        '{ paths: { $PATH: { $METHOD: { operationId: $OP, summary: $SUMMARY } } } }',
        'v4/examples/openapi-cardinality-markdown.sprf').
example(v3_items_id, v5, '{ items: [...{ id: $ID }] }', 'v3 ops/json.rs:60-74').
example(v1_deep_image, v5,
        '{ **: { image: { repository: $REPO, tag: $TAG } } }',
        'archive-20260428/README.md:78').

% dl6 literal (construction) corpus: JSON5-shaped, quoted values, trailing
% comma, nesting, arrays, comment.
example(dl6_literal, dl6,
        '{name: \'cli\', stars: 4, ok: true, tags: [\'a\', \'b\'], nested: {x: 1}, }',
        'json5-ish literal draft').
example(dl6_literal_commented, dl6,
        '{ # the repo name\n  name: \'cli\',\n  stars: 4 }',
        'json5-ish literal with a dl6 comment').
example(dl6_empty_object, dl6, '{}', 'empty object literal').
example(dl6_empty_array, dl6, '[]', 'empty array literal').

% ── receipts ─────────────────────────────────────────────────────────────────

grammar_receipts(7) :-
    receipt_every_archive_example_parses(_),
    receipt_flagship_dialects_agree,
    receipt_key_axis_inventory,
    receipt_literal_is_hole_free_subset,
    receipt_dialect_divergence_on_bare_values,
    receipt_trailing_comma_and_comment,
    receipt_brace_tag_seam_is_free_in_prolog.

% R1 -- every verbatim archive example parses, none is left over.
receipt_every_archive_example_parses(Count) :-
    findall(Id-Dialect-Text, example(Id, Dialect, Text, _), All),
    length(All, Count),
    forall(member(Id-Dialect-Text, All),
           (   parse_pattern(Dialect, Text, _)
           ->  true
           ;   throw(grammar_rejects(Id, Dialect, Text))
           )),
    format("PASS R1 ~d verbatim archive/draft examples parse~n", [Count]).

% R2 -- THE ACCEPTANCE CASE. The flagship gh-cache pattern in v5 text and its
% dl6 transcription produce the identical IR term.
receipt_flagship_dialects_agree :-
    example(gh_cache_flagship, v5, V5Text, _),
    example(gh_cache_flagship, dl6, Dl6Text, _),
    parse_pattern(v5, V5Text, IR),
    parse_pattern(dl6, Dl6Text, IR),
    IR = pat_arr_spread(
             pat_obj([ kp(k_exact(number), pat_hole(num)),
                       kp(k_exact(title),  pat_hole(title)),
                       kp(k_exact(state),  pat_hole(state)),
                       kp(k_exact(user),
                          pat_obj([kp(k_exact(login), pat_hole(author))])) ])),
    example(flat_and_nested_one_row, v5, FlatV5, _),
    example(flat_and_nested_one_row, dl6, FlatDl6, _),
    parse_pattern(v5, FlatV5, FlatIR),
    parse_pattern(dl6, FlatDl6, FlatIR),
    format("PASS R2 flagship + flat/nested: v5 text and dl6 text give one IR~n").

% R3 -- all five key-axis forms reach distinct matchers. These are the five
% (d) rows of the recovery doc: everything v6 lost is "the key is data too".
receipt_key_axis_inventory :-
    parse_pattern(v5, '{ $K: $V }',
                  pat_obj([kp(k_hole(k), pat_hole(v))])),
    parse_pattern(v5, '{ **: { image: $i } }',
                  pat_obj([kp(k_descend, pat_obj([kp(k_exact(image), pat_hole(i))]))])),
    parse_pattern(v5, '{ re:^v: $val }',
                  pat_obj([kp(k_re('^v'), pat_hole(val))])),
    parse_pattern(v5, '{ *id: $v }',
                  pat_obj([kp(k_glob('*id'), pat_hole(v))])),
    parse_pattern(v5, '{ "$ref": $v }',
                  pat_obj([kp(k_exact('$ref'), pat_hole(v))])),
    parse_pattern(v5, '{ $_: $v }',
                  pat_obj([kp(k_anon, pat_hole(v))])),
    format("PASS R3 six key matchers: exact/hole/anon/glob/regex/descend~n").

% R4 -- the literal grammar IS the hole-free subset. One grammar, two roles.
receipt_literal_is_hole_free_subset :-
    example(dl6_literal, dl6, Text, _),
    parse_literal(dl6, Text,
                  lit_obj([ name-lit_scalar(cli),
                            stars-lit_scalar(4),
                            ok-lit_scalar(true),
                            tags-lit_arr([lit_scalar(a), lit_scalar(b)]),
                            nested-lit_obj([x-lit_scalar(1)]) ])),
    parse_literal(dl6, '{}', lit_obj([])),
    parse_literal(dl6, '[]', lit_arr([])),
    % A pattern with any hole, on either plane, is NOT a literal.
    forall(member(Holed, ['{name: n}', '{$k: \'v\'}', '{**: {a: \'b\'}}',
                          '[... {a: \'b\'}]']),
           ( parse_pattern(dl6, Holed, HoledIR),
             \+ pattern_is_literal(HoledIR) )),
    format("PASS R4 literal = pattern minus holes; 4 holed forms refuse~n").

% R5 -- the ONE place the dialects genuinely disagree, stated rather than
% hidden: a bare run in VALUE position. v5 reads it as a literal, dl6 reads it
% as a variable. JSON5 forbids unquoted string values, so dl6's reading costs
% no JSON5 compatibility -- which is what makes the migration total.
receipt_dialect_divergence_on_bare_values :-
    parse_pattern(v5,  '{ state: open }', pat_obj([kp(k_exact(state), pat_eq(open))])),
    parse_pattern(dl6, '{ state: open }', pat_obj([kp(k_exact(state), pat_hole(open))])),
    parse_pattern(dl6, '{ state: \'open\' }', pat_obj([kp(k_exact(state), pat_eq(open))])),
    % Numbers and booleans are not identifiers, so they never diverge.
    forall(member(Same, ['{ stars: 4 }', '{ ok: true }']),
           ( parse_pattern(v5, Same, IR), parse_pattern(dl6, Same, IR) )),
    format("PASS R5 bare-value divergence isolated to identifiers; numbers/bools agree~n").

% R6 -- json5 affordances the draft takes: trailing comma, `#` comment,
% unquoted keys. (Excluded, with reasons, in the plan doc: null, NaN/Infinity,
% hex/plus/leading-dot numbers, escapes-in-identifiers.)
receipt_trailing_comma_and_comment :-
    parse_literal(dl6, '{a: 1, b: 2, }', lit_obj([a-lit_scalar(1), b-lit_scalar(2)])),
    parse_literal(dl6, '[1, 2, ]', lit_arr([lit_scalar(1), lit_scalar(2)])),
    example(dl6_literal_commented, dl6, Commented, _),
    parse_literal(dl6, Commented, lit_obj([name-lit_scalar(cli), stars-lit_scalar(4)])),
    format("PASS R6 trailing commas + `#` comments + unquoted keys~n").

% R7 -- EXTENSIBILITY, the directive's "`{` will later be abused beyond json".
% The seam is free in the prolog term form: swipl already reads `tag{...}` as a
% native dict, a term shape that CANNOT unify with `{}/1`. So reserving
% `Tag{...}` today costs one refusal and zero ambiguity, and the later
% non-json brace form has a home that does not move the json spelling.
receipt_brace_tag_seam_is_free_in_prolog :-
    term_to_atom(Plain, '{a: 1}'),
    Plain = {}(_),
    \+ is_dict(Plain),
    term_to_atom(Tagged, 'point{x: 1, y: 2}'),
    is_dict(Tagged),
    \+ Tagged = {}(_),
    dict_pairs(Tagged, point, [x-1, y-2]),
    format("PASS R7 `tag{...}` is already a distinct term shape (dict) from `{...}`~n").
