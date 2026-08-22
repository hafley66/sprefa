% fixtures/8_json_flex.pl : the json state-machine coverage wave
% (plans/2026-07-30-json-flex-lab-header.md, verdict
% plans/2026-07-30-json-flex-verdict.md).
%
% The json arm landed with 23 fixture entries and none of them touched a
% control character, a unicode key, a non-ASCII sort pair, an integer past
% 2^53, an empty-string key, or a document whose top level is a scalar. Every
% fixture here is one of those, and each one exists because the lab measured
% a door disagreement or an untested agreement, never to restate a shape
% json_arm.pl already covers.
%
% Owner: the json_flex lab wave. Grading is the ordinary corpus grading: the
% oracle runs these through engine.pl, the sweep compiles them and diffs the
% tick log byte-for-byte.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% ═══ Q1 STRING ESCAPES ══════════════════════════════════════════════════════

% FAIL-FIRST at the wave's base (a116e3e9). The tick-log escape rule lived in
% two clause-for-clause mirrors, conformance/ticklog.pl:escape_json_codes/2 and
% 0_type_plane.pl:escape_json_codes/2, and both spelled the hex escape
% `format(atom(H), '\\u~`0t~16r~4|', [Code])`. `~4|` is a COLUMN stop measured
% from the start of the atom, and `\u` already occupies two of those columns,
% so the escape came out with TWO hex digits: code 12 rendered `\u0c`, and the
% next source character was then appended to it. Receipt, before the fix:
%
%   value_json('a\fb', J)  ->  J = '"a\u0cb"'
%   JSON.parse('"a\u0cb"') ->  SyntaxError: Bad Unicode escape in JSON
%
% The oracle's own tick log was not JSON. The emitter side is
% JSON.stringify, which is correct and also uses the SHORT escapes \b \f \r,
% so the two doors disagreed on every one of these bytes as well.
%
% Nothing in the 209-fixture corpus carried a control character, which is why
% a broken escape shipped through every byte-diff gate the project has.
fixture(json_string_control_escapes_are_valid_json,
  prog([col_type(note/1, body, text), col_type(seen/1, body, text)],
       [ (seen(Body) <- note(Body)) ]),
  [],
  [ [ +note('back\bspace'), +note('form\ffeed'), +note('carriage\rreturn'),
      +note('unit\x1\sep'), +note('tab\there'), +note('line\nfeed') ] ],
  [ final(seen/1, [ seen('back\bspace'), seen('carriage\rreturn'),
                    seen('form\ffeed'), seen('line\nfeed'),
                    seen('tab\there'), seen('unit\x1\sep') ]),
    ticks(1) ]).

% One program, three documents, one echo. Each seed row is a separate encoder
% edge and the rows are independent: `echoed(Body) <- raw_doc(Body)` copies the
% value, so the final set is the three canonicalized documents.
%   escapes    control characters INSIDE a document, where the text passes
%              through json_string_text/2 rather than the top-level
%              value_json/2 clause.
%   collation  non-ASCII keys and the SORT they land in. The cross-target
%              contract the canonical_json_text ruling fixed: prolog `keysort`
%              on atoms is code-POINT order, JS sort is UTF-16 code-UNIT order,
%              and the two agree everywhere except the astral plane. The BMP
%              half is pinned here; the astral half is the slot_key_collation
%              card, because no shipped program can produce an astral key.
%   containers empty object as a value, empty array as a value, both nested.
%              `{}` is the ATOM on both doors and `[]` is the empty list, the
%              pair most likely to fall through an encoder clause onto the
%              string path.
% folded 2026-08-20 from json_control_escapes_inside_a_document,
% json_non_ascii_keys_sort_by_code_point, json_empty_containers_nest.
fixture(json_document_encoder_edges_round_trip,
  prog([col_type(raw_doc/1, body, json), col_type(echoed/1, body, json)],
       [ (echoed(Body) <- raw_doc(Body)) ]),
  [ raw_doc({tab: 'a\tb', formfeed: 'a\fb', quote: 'a"b', solidus: 'a/b'}),
    raw_doc({'z': 1, 'é': 2, 'a': 3, 'Z': 4}),
    raw_doc({obj: {}, arr: [], nested: [{}, []], deep: {inner: {}}}) ],
  [],
  [ final(echoed/1,
          [ echoed(obj([ 'Z'-4, 'a'-3, 'z'-1, 'é'-2 ])),
            echoed(obj([ arr-[], deep-obj([inner-obj([])]),
                         nested-[obj([]), []], obj-obj([]) ])),
            echoed(obj([ formfeed-'a\fb', quote-'a"b',
                         solidus-'a/b', tab-'a\tb' ])) ]) ]).

% ═══ Q3 KEYS ════════════════════════════════════════════════════════════════


% NFC and NFD are DIFFERENT keys. Neither door normalizes, json1 does not
% normalize, and JSONTestSuite's own transform corpus
% (object_key_nfc_nfd.json) exists because implementations disagree. Pinned
% here so a future "helpful" normalization is a fixture failure.
%
% Both keys are spelled with explicit code escapes, never as literal source
% bytes: the composed and decomposed spellings LOOK identical in every editor
% and several tools in this repo's own authoring path silently normalize one
% into the other, which turns the fixture into `json_dup_key` with no visible
% cause. `\x` escapes are the only spelling that survives that.
fixture(json_nfc_and_nfd_keys_stay_distinct,
  prog([col_type(raw_doc/1, body, json), col_type(key_seen/1, name, text)],
       [ (key_seen(Key) <- raw_doc(Body), decode(Body, {$Key: _})) ]),
  [ raw_doc({'caf\xe9\': 1, 'cafe\x301\': 2}) ],
  [],
  % Standard order of atoms is code-point order, so the DECOMPOSED spelling
  % (`e` = U+0065 at position 3) sorts before the composed one (U+00E9).
  [ final(key_seen/1, [ key_seen('cafe\x301\'), key_seen('caf\xe9\') ]) ]).

% One program, two documents, one key capture. The rows are independent: each
% raw_doc contributes its own pairs, so the final set is their union.
%   empty key   legal JSON, legal here, and it survives key capture: the one
%               key spelling that cannot be written as a bare identifier.
%   marker keys a key that IS the `$` hole marker's spelling and a key that is
%               the `**` descent marker's spelling. Both are ordinary data on
%               the VALUE plane; the marker meaning lives only in a PATTERN.
%               The other half of that measurement is a named card: on the
%               pattern plane the two markers are unconditional, so a literal
%               `$k` or `**` key can never be matched by an exact-key pattern.
% folded 2026-08-20 from json_empty_string_key_round_trips,
% json_marker_shaped_keys_are_ordinary_data.
fixture(json_literal_keys_survive_capture,
  prog([col_type(raw_doc/1, body, json), col_type(pair/2, name, text),
        col_type(pair/2, value, int)],
       [ (pair(Key, Value) <- raw_doc(Body), decode(Body, {$Key: Value})) ]),
  [ raw_doc({'': 7, a: 8}),
    raw_doc({'$ref': 1, '**': 2, plain: 3}) ],
  [],
  [ final(pair/2, [ pair('', 7), pair('$ref', 1), pair('**', 2),
                    pair(a, 8), pair(plain, 3) ]) ]).


% ═══ Q1 NUMBERS ═════════════════════════════════════════════════════════════

% Integers at the SAFE-INTEGER boundary, both signs. The @libsql
% number->REAL corruption class bit this project twice already (the sweep's
% bigint-bind fix and the boot-bind fix); this pins where the seam actually
% ends so the next widening is a fixture failure and not a field report.
%
% THE EDGE IS EXACTLY ±(2^53 - 1) AND IT IS A READ-SIDE CLIFF, measured by the
% json_flex lab against @libsql 0.17.4 (intMode "number", runtime/rows.ts's
% own header states the choice):
%
%   INSERT 9007199254740992           -> ok, the row is in the table
%   SELECT "v" WHERE "v" = 9007199254740992
%                                     -> RangeError: Received integer which
%                                        cannot be safely represented as a
%                                        JavaScript number
%
% A program can store an integer it can never read back, and the failure is a
% driver RangeError naming no rel and no column. Same throw for a wide integer
% reached through `json_extract` inside a document. Beyond i64 the failure
% mode changes rather than stops: json1 keeps the source text in `json()` but
% `json_extract` hands back a REAL, and the tick-log canon (JSON.parse then
% JSON.stringify) rewrites 9223372036854775807 as 9223372036854776000 with no
% error at all. The oracle keeps every one of those exactly, so wide integers
% are a SILENT cross-door divergence above this boundary. Priced as
% slot_json_bignum in plans/2026-07-30-json-flex-verdict.md; deliberately not
% fixed here, because every option is either a driver-wide `intMode` change or
% a new dependency.
fixture(json_safe_integer_boundary_survives_both_doors,
  prog([col_type(measure/2, name, text), col_type(measure/2, value, int),
        col_type(carried/2, name, text), col_type(carried/2, value, int)],
       [ (carried(Name, Value) <- measure(Name, Value)) ]),
  [],
  [ [ +measure(max_safe, 9007199254740991),
      +measure(min_safe, -9007199254740991),
      +measure(small, 1) ] ],
  [ final(carried/2, [ carried(max_safe, 9007199254740991),
                       carried(min_safe, -9007199254740991),
                       carried(small, 1) ]),
    ticks(1) ]).

% ═══ Q1 CONTAINERS ══════════════════════════════════════════════════════════


% Deep nesting through decode. json1's own parser caps document depth
% (measured: json_valid accepts 1000 and refuses 2000 on both builds), so this
% sits an order of magnitude under the cap and pins that the accumulated
% `json_extract` path an exact-key chain builds is not itself the limit.
fixture(json_deep_exact_key_chain_binds,
  prog([col_type(raw_doc/1, body, json), col_type(found/1, leaf, int)],
       [ (found(Leaf) <-
            raw_doc(Body),
            decode(Body, {a: {b: {c: {d: {e: {f: {g: {h: Leaf}}}}}}}})) ]),
  [ raw_doc({a: {b: {c: {d: {e: {f: {g: {h: 9}}}}}}}}) ],
  [],
  [ final(found/1, [ found(9) ]) ]).

% FAIL-FIRST at the wave's base. A json DOCUMENT whose top level is a scalar
% is ordinary JSON (RFC 8259 §2: a value, not necessarily an object or array)
% and json1 accepts every one of these. The tick-log encoder on the tsv2 side
% decided whether a string was structure by SNIFFING ITS FIRST CHARACTER:
%
%   if (value[0] !== "{" && value[0] !== "[") return null;   // ticklog.ts
%
% so a json column holding `42` came back across the driver seam as the string
% "42", missed the sniff, and printed as the JSON STRING "42" while the oracle
% printed the NUMBER 42. Fifteen of the lab's twenty-three value kinds took
% that path, including `null`, `true` and every number.
%
% The fix is type-directed rather than another sniff: `json` stops collapsing
% to `text` at the driver seam (emit_ts.pl boundary_column_type/2) and the
% encoder renders a json-typed column as a json VALUE at any top level. That
% also closes the mirror-image hole the sniff had -- a `text` column whose
% value happens to start with `{` and parse as JSON was being rendered as
% structure -- which the second rule here pins: `label` is text, carries the
% same bytes, and must stay a string.
fixture(json_top_level_scalar_document_is_a_value,
  prog([col_type(payload/2, name, text), col_type(payload/2, body, json),
        col_type(label/2, name, text), col_type(label/2, body, text),
        col_type(echoed/2, name, text), col_type(echoed/2, body, json),
        col_type(labelled/2, name, text), col_type(labelled/2, body, text)],
       [ (echoed(Name, Body) <- payload(Name, Body)),
         (labelled(Name, Body) <- label(Name, Body)) ]),
  [],
  [ [ +payload(number, 42),
      +payload(negative, -7),
      +payload(text_scalar, '"quoted"'),
      +payload(object, {a: 1}),
      +label(looks_like_json, '{"a":1}'),
      +label(looks_like_number, '42') ] ],
  [ final(echoed/2, [ echoed(negative, -7), echoed(number, 42),
                      echoed(object, obj([a-1])),
                      echoed(text_scalar, '"quoted"') ]),
    final(labelled/2, [ labelled(looks_like_json, '{"a":1}'),
                        labelled(looks_like_number, '42') ]),
    ticks(1) ]).

% ═══ Q2 PRESENT-NULL vs ABSENT ══════════════════════════════════════════════

% The one behaviour both doors already agree on, pinned so the fix for the
% cards around it cannot move it by accident: an exact-key pattern binds a
% present value and yields NOTHING for a missing key, with no error either
% way. json_arm.pl's decode_missing_key_fails_quietly covers the same ground
% at the oracle only (empty Schedule, vacuous tick log); this one runs under a
% real arrival so the tick-log leg grades it too.
fixture(json_absent_key_yields_no_row_under_arrivals,
  prog([col_type(raw_doc/1, body, json), col_type(found/2, name, text),
        col_type(found/2, value, text)],
       [ (found(present, Value) <- raw_doc(Body), decode(Body, {present: Value})),
         (found(missing, Value) <- raw_doc(Body), decode(Body, {missing: Value})) ]),
  [],
  [ [ +raw_doc({present: here}) ] ],
  [ final(found/2, [ found(present, here) ]),
    ticks(1) ]).

% ═══ Q6 FAN-OUT ═════════════════════════════════════════════════════════════

% The three fanning productions in ONE rule, so the emitted plan has to carry
% one json_each for the spread, one json_each for the key capture, and one
% json_tree for the descent, and the row count is their product. The
% cardinality IS the receipt: an implementation that answered the first match
% only (a `memberchk` on the oracle, a correlated scalar subquery on the
% emitter) produces 1 row here instead of 4.
fixture(json_spread_and_capture_and_descent_multiply,
  prog([col_type(spec/1, body, json), col_type(hit/3, item, int),
        col_type(hit/3, name, text), col_type(hit/3, leaf, int)],
       [ (hit(Item, Key, Leaf) <-
            spec(Body),
            decode(Body, {items: spread({n: Item})}),
            decode(Body, {tags: {$Key: _}}),
            decode(Body, {'**': {leaf: Leaf}})) ]),
  [ spec({items: [{n: 1}, {n: 2}],
          tags: {red: 1, blue: 2},
          box: {leaf: 5}}) ],
  [],
  [ final(hit/3, [ hit(1, blue, 5), hit(1, red, 5),
                   hit(2, blue, 5), hit(2, red, 5) ]) ]).

% ═══ TYPED CAPTURES ═════════════════════════════════════════════════════════
%
% `{stars: Stars: int}`. The colon is already this language's type marker on
% the decl plane (ruling decl_column_spelling = colon_typed_ordered_columns);
% these fixtures are that marker one level down, inside a json pattern.
%
% FAIL-FIRST, at this wave's base (3d993e1e). The flagship below is the
% program a cold author wrote and the reason this lane exists. Without a
% typed capture it does not compile at all:
%
%   star_event(Repo, Stars) <- event(Payload), decode(Payload, {repo: Repo, stars: Stars}).
%   ...  |+> total(Repo, Next)
%   -> unsupported_construct(edge_head_column_type_mismatch(total/2,2,text,int))
%
% lower.pl types an untyped hole `text` (json_extract carries no declared
% column type and the clause says so), so star_event's second column took the
% zero-witness default and the `int` head column refused the rule by name. The
% SQL was never the problem: json1 hands back a real SQL INTEGER for a json
% number. The type pass had no way to be told.

% THE FLAGSHIP: a json event log folded into a keyed running total. Both arms
% of the match block read the same capture, the second one arithmetically.
% A json boolean inside a document captures as `bool` into a bool column. The
% live receipt: GitHub's `isDraft` is a json bool and a `: int` capture answered
% zero rows for every open PR (ghcache.dl6, 2026-08-22).
fixture(json_bool_capture_lands_in_a_bool_column,
  prog([col_type(event/1, payload, json), kind(event/1, log), keep(event/1, all),
        col_type(draft_flag/2, number, int), col_type(draft_flag/2, draft, bool)],
       [ (draft_flag(Number, Draft) <-
            event(Payload),
            decode(Payload, {number: Number: int, isDraft: Draft: bool})) ]),
  [],
  [ [ +event(obj([number-8, isDraft-bool_lit(false)])) ],
    [ +event(obj([number-9, isDraft-bool_lit(true)])) ],
    [ +event(obj([number-10, isDraft-1])) ] ],
  [ final(draft_flag/2, [ draft_flag(8, bool_lit(false)), draft_flag(9, bool_lit(true)) ]),
    deltas(draft_flag/2, [ [ +draft_flag(8, bool_lit(false)) ],
                           [ +draft_flag(9, bool_lit(true)) ],
                           [] ]),
    ticks(3) ]).

fixture(json_typed_capture_folds_into_a_keyed_int_total,
  prog([col_type(event/1, payload, json), kind(event/1, log), keep(event/1, all),
        col_type(total/2, repo, text), col_type(total/2, sum, int),
        keyed(total/2, [1])],
       [ (star_event(Repo, Stars) <-
            event(Payload),
            decode(Payload, {repo: Repo: text, stars: Stars: int})),
         match(star_event(Repo, Stars),
           ( (total(Repo, Stars) <+ not(total(Repo, _Prev)))
           ; (total(Repo, Next) <+ (pre(total(Repo, Prev)), Next := Prev + Stars))
           )) ]),
  [],
  [ [ +event(obj([repo-cli, stars-4])) ],
    [ +event(obj([repo-cli, stars-3])), +event(obj([repo-web, stars-10])) ],
    [ +event(obj([repo-cli, stars-1])) ] ],
  [ deltas(total/2,
           [ [ +total(cli, 4) ],
             [ -total(cli, 4), +total(cli, 7), +total(web, 10) ],
             [ -total(cli, 7), +total(cli, 8) ],
             [] ]),
    final(total/2, [ total(cli, 8), total(web, 10) ]) ]).

% THE GUARD IS NOT DECORATION. A declared type is ENFORCED, not assumed: the
% document whose `stars` is the STRING "many" contributes no row on either
% door, because the oracle checks integer/1 and the emitter emits
% `json_type(b0."payload", '$."stars"') = 'integer'` ahead of the extract.
%
% Deleting the guard does not fail loudly, which is exactly why this fixture
% exists. Measured against system sqlite3 3.43.2 over
% '{"repo":"cli","stars":4}' and '{"repo":"web","stars":"many"}', the two
% WHERE lists this compiler can emit, writing into `"stars" INTEGER NOT NULL`:
%
%   ... AND json_extract(b0."payload",'$."stars"') IS NOT NULL   (untyped)
%     -> cli|4|integer   AND   web|many|TEXT
%   ... AND json_type(b0."payload",'$."stars"') = 'integer'      (typed)
%     -> cli|4|integer
%
% INTEGER affinity keeps a non-numeric string as TEXT, so the untyped form
% writes text into an int column and the tick log prints "many" where the
% oracle prints nothing. The TEXT-collapse class again.
%
% It is also the answer to "why not just declare the intermediate rel". A
% declared `rel star_event(repo: text, stars: int)` DOES compile the flagship
% -- the head decl types the column and edge_head_column_type_mismatch stops
% firing -- but it emits the UNTYPED where list above, because the capture
% itself is still untyped. It buys the type and not the guard.
fixture(json_typed_capture_filters_a_wrong_typed_value,
  prog([col_type(event/1, payload, json), kind(event/1, log), keep(event/1, all),
        col_type(counted/2, repo, text), col_type(counted/2, stars, int)],
       [ (counted(Repo, Stars) <-
            event(Payload),
            decode(Payload, {repo: Repo: text, stars: Stars: int})) ]),
  [],
  [ [ +event(obj([repo-cli, stars-4])),
      +event(obj([repo-web, stars-many])),
      +event(obj([repo- 7, stars-1])) ] ],
  [ final(counted/2, [ counted(cli, 4) ]),
    ticks(1) ]).

% An UNTYPED capture is untouched by this wave: it still binds whatever sits
% at the key and still types `text`. The pair with the fixture above is the
% whole no-silent-widening argument -- the author says `int`, or the author
% gets exactly what was always there.
%
% TEXT values only, and the omission is a MEASURED finding rather than
% timidity: a json NUMBER read through an UNTYPED capture diverges today. The
% oracle binds the integer 4 and prints `4`; the emitter types the capture
% `text`, so the head column resolves to text and the log prints `"4"`. Ran
% as a fixture with `+event(obj([stars-4]))` in the batch:
%
%   actual  "seen":{"add":[["4"],["many"]]}
%   oracle  "seen":{"add":[["many"],[4]]}
%
% PRE-EXISTING and reachable without any of this lane's work -- json_arm.pl's
% own numeric fixtures avoid it by DECLARING the head column `int`, which is
% the shipped answer for a level head. Closing it means giving an untyped
% capture a type, which is the widening the typed capture exists instead of.
% Named here, not fixed here.
fixture(json_untyped_capture_binds_without_a_type,
  prog([col_type(event/1, payload, json), kind(event/1, log), keep(event/1, all),
        col_type(seen/1, value, text)],
       [ (seen(Value) <- event(Payload), decode(Payload, {stars: Value})) ]),
  [],
  [ [ +event(obj([stars-many])), +event(obj([stars-lots])) ] ],
  [ final(seen/1, [ seen(lots), seen(many) ]),
    ticks(1) ]).

% A capture type this plane does not define is a NAMED REFUSAL, never a
% pattern that quietly matches nothing.
fixture(json_capture_type_typo_is_refused,
  prog([col_type(event/1, payload, json)],
       [ (counted(Value) <- event(Payload), decode(Payload, {n: Value: itn})) ]),
  [ event(obj([n-4])) ],
  [],
  [ throws(json_capture_type_unknown(itn)) ]).

% ═══ Q6 DECODE IN AN EDGE BODY ══════════════════════════════════════════════
%
% FAIL-FIRST at this wave's base: every one of the four below compiled to
% unsupported_construct(edge_body_needs_json_destructure(...)) because
% analyze.pl branched on the rule KIND, never on the source column's type.
% The cost was a `_seen` level twin per fold, existing only to host a decode
% the edge body was not allowed to carry.

% A keyed set head folded straight from the document: ONE table, ONE
% INSERT ... ON CONFLICT DO UPDATE, and the second arrival overwrites the key.
fixture(json_decode_in_an_edge_body_folds_a_keyed_row,
  prog([col_type(config_doc/1, doc, json), kind(config_doc/1, log),
        keep(config_doc/1, all),
        col_type(global_setting/2, scope, text),
        col_type(global_setting/2, poll_interval_seconds, int),
        keyed(global_setting/2, [1])],
       [ (global_setting(Scope, PollIntervalSeconds) <+
            config_doc(Doc),
            decode(Doc, {scope: Scope: text,
                         poll_interval_seconds: PollIntervalSeconds: int})) ]),
  [],
  [ [ +config_doc(obj([scope-global, poll_interval_seconds-30])) ],
    [ +config_doc(obj([scope-global, poll_interval_seconds-90])) ] ],
  [ final(global_setting/2, [ global_setting(global, 90) ]),
    deltas(global_setting/2, [ [ +global_setting(global, 30) ],
                               [ -global_setting(global, 30),
                                 +global_setting(global, 90) ],
                               [] ]),
    ticks(3) ]).

% One document, several keyed rows: the spread is a json_each join inside the
% edge arm, which is the arrival shape a paginated API answer has.
fixture(json_decode_spread_in_an_edge_body_folds_many_keyed_rows,
  prog([col_type(pull_page/1, doc, json), kind(pull_page/1, log),
        keep(pull_page/1, all),
        col_type(pull_state/2, number, int),
        col_type(pull_state/2, title, text),
        keyed(pull_state/2, [1])],
       [ (pull_state(Number, Title) <+
            pull_page(Doc),
            decode(Doc, {pulls: spread({number: Number: int,
                                        title: Title: text})})) ]),
  [],
  [ [ +pull_page(obj([pulls-[obj([number-1, title-first]),
                             obj([number-2, title-second])]])) ],
    [ +pull_page(obj([pulls-[obj([number-2, title-renamed])]])) ] ],
  [ final(pull_state/2, [ pull_state(1, first), pull_state(2, renamed) ]),
    deltas(pull_state/2, [ [ +pull_state(1, first), +pull_state(2, second) ],
                           [ -pull_state(2, second), +pull_state(2, renamed) ],
                           [] ]),
    ticks(3) ]).

% A LOG head takes the same body. No key, so every derived row appends and
% the same document twice appends twice.
fixture(json_decode_in_an_edge_body_appends_to_a_log,
  prog([col_type(event_doc/1, doc, json), kind(event_doc/1, log),
        keep(event_doc/1, all),
        col_type(audit/1, action, text), kind(audit/1, log),
        keep(audit/1, all)],
       [ (audit(Action) <+ event_doc(Doc), decode(Doc, {action: Action: text})) ]),
  [],
  [ [ +event_doc(obj([action-open])) ],
    [ +event_doc(obj([action-open])), +event_doc(obj([action-close])) ] ],
  [ final(audit/1, [ audit(close), audit(open), audit(open) ]),
    deltas(audit/1, [ [ +audit(open) ],
                      [ +audit(open), +audit(close) ],
                      [] ]),
    ticks(3) ]).

% The type guard is the same guard the level arm emits, so a document whose
% value carries the wrong json type contributes no row on either door.
fixture(json_decode_in_an_edge_body_filters_a_wrong_typed_value,
  prog([col_type(config_doc/1, doc, json), kind(config_doc/1, log),
        keep(config_doc/1, all),
        col_type(global_setting/2, scope, text),
        col_type(global_setting/2, poll_interval_seconds, int),
        keyed(global_setting/2, [1])],
       [ (global_setting(Scope, PollIntervalSeconds) <+
            config_doc(Doc),
            decode(Doc, {scope: Scope: text,
                         poll_interval_seconds: PollIntervalSeconds: int})) ]),
  [],
  [ [ +config_doc(obj([scope-global, poll_interval_seconds-30])),
      +config_doc(obj([scope-repo, poll_interval_seconds-often])) ] ],
  [ final(global_setting/2, [ global_setting(global, 30) ]),
    ticks(2) ]).
