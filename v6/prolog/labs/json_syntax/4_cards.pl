% ╔══════════════════════════════════════════════════════════════════════════╗
% ║  CARD RECONCILIATION -- the user-facing deliverable.                     ║
% ║  No decision is taken here. Cards the three ruled directives ANSWERED    ║
% ║  are marked with the directive that answered them; everything else is    ║
% ║  an open card carrying EXACT spellings, because no syntax lands before   ║
% ║  the user has seen the exact text (no-unsighted-syntax law).             ║
% ╚══════════════════════════════════════════════════════════════════════════╝

:- module(json_syntax_cards,
          [ directive/2,
            card_answered/4,
            card_open/3,
            spelling/4,
            spelling_free/2,
            card_receipts/1
          ]).

:- use_module(library(aggregate), [aggregate_all/3]).
:- use_module(library(lists)).

% ── the three directives being applied ───────────────────────────────────────
% Verbatim from chat_log/20260730.1.fable-opus-storm-lab-assimilation.pl.

directive(json_as_rel_type,
          'json becomes a rel column type that LOWERS TO SQLITE JSON1; typed refs and json columns coexist').
directive(json_syntax_native,
          'json natively expressable 1-1 in the language; unquoted keys when valid (json5-ish); HOLES like v3/v4 brace patterns; the { opening will later be abused beyond json, FOR NOW { means json').
directive(list_types_and_generics,
          'list types and list generics -- designed twice, implemented never; list(text) is today an unknown column type; arrays are half of json').

% ── ANSWERED ─────────────────────────────────────────────────────────────────
% card_answered(Card, Origin, ByDirective, Answer).

card_answered('CARD-KEY-CAPTURE', recovery_doc, json_syntax_native,
  'holes ARE the directive; the lowering is json_each(key,value), zero new SQL machinery (receipt L3). Residue is one spelling pick: CARD-KEY-HOLE-SPELLING.').
card_answered('CARD-ARRAY-FANOUT', recovery_doc, json_as_rel_type,
  'q:[... {...}] executes as one json_each join over a json column; the gh-cache flagship runs end to end (receipt L2). Lifting the json_each compiler unsupported construct is a dispatch, not a ruling.').
card_answered('CARD-CONSTRUCTION', recovery_doc, json_as_rel_type,
  '"lowers to sqlite json1" IS json_group_array/json_group_object; already ruled emittable by json_ticklog_encoding. Dispatch, not a ruling.').
card_answered('CARD-RECURSIVE-KEY', recovery_doc, json_syntax_native,
  '** was live in v1..v5 so the directive covers it; the lowering is json_tree, and json_tree.fullkey ALSO supplies v4 $$${PATH?}, the one construct v5 dropped with no successor (receipt L4). Residue: CARD-DESCENT-DEPTH.').
card_answered('CARD-SUBTREE-CAPTURE', recovery_doc, json_as_rel_type,
  'THE OLDEST OPEN JSON QUESTION IN THE PROJECT CLOSES. A value-position hole with no sub-pattern already means "bind this node, do not descend" (v5 Step::Leaf); what v5 lacked was a typed place to put the subtree, which a json column now supplies. human-goals.md:693 answered.').
card_answered('CARD-PATTERN-KEY(glob)', recovery_doc, json_syntax_native,
  'a glob key is one json_each join plus SQL GLOB, which is CORE SQLite (receipt L5). The regex half is not core and splits off as CARD-REGEX-KEY.').
card_answered(json_residency, json_interop_lab, json_as_rel_type,
  'core_global. A column TYPE plus a literal in the base grammar is as core as a construct gets; optional_additive_module and host_only are both contradicted by making json a type word.').
card_answered(array_storage, json_interop_lab, list_types_and_generics,
  'the four options collapse into a two-plane statement: json carrier for VALUES (best on all five graded axes, 3_lists.pl), ordinary rows for queryable element SETS. cons_relations and indexed_elements lose; refuse_arrays is contradicted by five generations of shipped [...] patterns.').
card_answered(recursive_identity, json_interop_lab, json_as_rel_type,
  'split, not chosen: refuse_cycles stands for REF columns (content ids cannot express a cycle, type_cycle_witness/2 unchanged); a json column is acyclic by construction because text cannot cycle. Recursive JSON documents become expressible with no change to the value-DAG rule.').

% ── OPEN ─────────────────────────────────────────────────────────────────────
% card_open(Card, Origin, Question).

card_open('CARD-KEY-HOLE-SPELLING', new_from_directives,
  'in KEY position a bare identifier is a literal label today; what marks a key as a variable? (This is the only thing standing between v6 and the highest-leverage v5 construct.)').
card_open('CARD-PATTERN-GOAL-SPELLING', new_from_directives,
  'how does a brace pattern attach to its source column in a rule body? Note: directive json_as_rel_type takes the word `json` as a TYPE, so v5''s own op name json(body, q:{...}) is no longer available.').
card_open('CARD-LIST-SPELLING', new_from_directives,
  'the surface for the one parametric type. The checker delta is measured at four clauses (receipt T6); only the spelling is open.').
card_open('CARD-BRACE-TAG', new_from_directives,
  'do we reserve `Tag{...}` NOW for the later non-json abuse of `{`? swipl already reads it as a distinct term shape (receipt R7), so reserving costs one unsupported construct clause and buys the directive''s stated future.').
card_open('CARD-JSON5-SUBSET', new_from_directives,
  'exactly which json5 affordances the literal takes. The draft takes unquoted keys, trailing commas and `#` comments (receipt R6) and excludes null, NaN/Infinity, hex/plus/leading-dot numbers.').
card_open('CARD-STRING-QUOTE', new_from_directives,
  'single-quoted (dl6 atom law, current fixtures) or double-quoted (real JSON, copy-paste from a document) as the canonical json string spelling in dl6 source. Printer question; both already parse (receipt R1).').
card_open('CARD-DESCENT-DEPTH', new_from_directives,
  '`**` has never had a depth cap in any generation (archive TASKS.md T9 asks for one and it was never built).').
card_open('CARD-REGEX-KEY', new_from_directives,
  'REGEXP is SYNTAX in core SQLite with NO implementation. The sqlite3 CLI and @libsql each supply one (measured, receipt L5); rusqlite by default does not, and directive rust_flip_soon means that matters.').
card_open(null_and_optional, json_interop_lab,
  'SHARPENED by json_as_rel_type: there are now TWO null questions, not one. (i) `null` INSIDE a json column value -- it is just text and survives a round trip. (ii) `null` as a column value -- still no such language value. Both need a word, and the recovery evidence (row_absence) only speaks to (ii).').
card_open(schema_import_boundary, json_interop_lab,
  'untouched by these directives, and INVERTED by the same session''s openapi_codegen_spine directive: the spec is to be GENERATED from prolog facts, so the import direction may never be needed at all.').
card_open('CARD-EDGE-BODY-JSON', recovery_doc,
  'HALF ANSWERED: the encoding half (SLOT-TERM-STRUCT -- what a compound arrival into an untyped column stores) is answered, because a json column stores canonical JSON text. The frontier-staging half of edge_body_needs_json_destructure is a runtime arc and stays open.').
card_open('CARD-FORMAT-DISPATCH', recovery_doc,
  'v5 read JSON, JSONL, YAML and TOML through ONE grammar by extension dispatch. In v6 this is a host-decl question, not a syntax one; it needs a yes/no on whether the alpha wants it.').
card_open('CARD-VALUE-PATTERN', recovery_doc,
  'q:{ image: "$REPO:$TAG" }. v5 parsed it and matched it LITERALLY -- archive TASKS.md T7 is still open, so this never shipped its semantics. The lab refuses it by name (value_template_never_shipped) rather than inventing a lowering.').

% ── EXACT SPELLINGS ──────────────────────────────────────────────────────────
% spelling(Card, Option, ExactText, Cost).

spelling('CARD-KEY-HOLE-SPELLING', sigil,
  'kv(key, value) <- sample(body), decode(body, {$key: $value}).',
  'five generations of precedent; `$` appears nowhere else in dl6, so it costs one lexer branch confined to key position. Reads foreign next to bare-identifier value holes.').
spelling('CARD-KEY-HOLE-SPELLING', parens,
  'kv(key, value) <- sample(body), decode(body, {(key): value}).',
  'no new sigil; `(` in key position is unambiguous because a key is never an expression today. Visually quiet, and quiet is the failure mode -- a typo`d paren silently changes a label into a capture.').
spelling('CARD-KEY-HOLE-SPELLING', brackets,
  'kv(key, value) <- sample(body), decode(body, {[key]: value}).',
  'the JS computed-key spelling, instantly readable to anyone who writes JS. Collides with `[` as the array opener when a future form allows a pattern in key position.').
spelling('CARD-KEY-HOLE-SPELLING', invert_quoting,
  'kv(key, value) <- sample(body), decode(body, {key: value}).   % and a LITERAL key becomes {''name'': value}',
  'the only option fully consistent with the ruled dl6 law (bare = variable, quoted = constant). Costs a hard migration of every brace in the corpus and makes copy-pasted JSON stop meaning itself.').

spelling('CARD-PATTERN-GOAL-SPELLING', decode_keyword,
  'pull(num, author) <- resp(ep, body), decode(body, {number: num, user: {login: author}}).',
  'zero change: shipped today, registry row live, oracle solves it (body.pl json_decode/2). The word `decode` is not an rx/prolog/SQL word, which the language-design review already flagged.').
spelling('CARD-PATTERN-GOAL-SPELLING', unification,
  'pull(num, author) <- resp(ep, body), body = {number: num, user: {login: author}}.',
  'reads as what it is and needs no keyword; `=` is not currently a body operator, and adding it means ruling on whether `=` is unification or the SQL equality the comparison family already spells `==`.').
spelling('CARD-PATTERN-GOAL-SPELLING', match_keyword,
  'pull(num, author) <- resp(ep, body), match(body, {number: num, user: {login: author}}).',
  'the word already exists in the language as the match/2 block sugar; reusing it for a second idea is exactly the two-constructs-one-word hazard decode_field_unknown was built to prevent.').

spelling('CARD-LIST-SPELLING', list_of,
  'rel repo(name: text, tags: list(text)).\nrepo(''cli'', [''go'', ''rust'']).',
  'four checker clauses, measured (receipt T6). `list(` is a function-call shape the decl grammar already parses for keep(count) and key(...).').
spelling('CARD-LIST-SPELLING', postfix_brackets,
  'rel repo(name: text, tags: text[]).\nrepo(''cli'', [''go'', ''rust'']).',
  'shorter and familiar from TS/rust; introduces postfix type syntax where every other type is a bare word, and `[` in type position is new.').
spelling('CARD-LIST-SPELLING', json_only,
  'rel repo(name: text, tags: json).\nrepo(''cli'', [''go'', ''rust'']).',
  'zero checker delta -- but nothing then states that tags is an array of text, so a malformed arrival is stored rather than named. This is the do-nothing option and it is honest to price it.').

spelling('CARD-BRACE-TAG', reserve_now,
  'rel diag(at: json).\ndiag(point{x: 1, y: 2}).    % => refused: tagged_brace_reserved(point)',
  'one unsupported construct clause. swipl already reads `point{...}` as a dict term that cannot unify with {}/1 (receipt R7), so the seam exists before we write anything.').
spelling('CARD-BRACE-TAG', do_not_reserve,
  'diag(point{x: 1, y: 2}).    % => refused: unexpected token after identifier',
  'nothing to build today; the later non-json brace form arrives as a breaking parse change instead of an already-named unsupported construct.').

spelling('CARD-JSON5-SUBSET', json5_draft,
  '{ # the repo name\n  name: ''cli'',\n  stars: 4,\n  tags: [''go'', ''rust''],\n}',
  'unquoted keys + trailing comma + `#` comments; all three parse in the prototype (receipt R6) and `#` is already the dl6 comment.').
spelling('CARD-JSON5-SUBSET', strict_json,
  '{ "name": "cli", "stars": 4, "tags": ["go", "rust"] }',
  'copy-paste from any real document works unchanged; loses the unquoted keys the directive explicitly asked for.').

spelling('CARD-STRING-QUOTE', single,
  'repo(''cli'', [''go'', ''rust'']).',
  'consistent with the ruled dl6 atom spelling and with every current fixture.').
spelling('CARD-STRING-QUOTE', double,
  'repo("cli", ["go", "rust"]).',
  'a pasted JSON document is a valid dl6 literal with zero edits, which is most of what "1-1 expressable" means.').
spelling('CARD-STRING-QUOTE', both_print_single,
  'repo("cli", [''go'', ''rust'']).    % both parse; print_dl emits single',
  'zero migration and paste-friendly; two spellings for one value is the thing the roundtrip door exists to catch.').

spelling('CARD-DESCENT-DEPTH', unbounded,
  'image(i) <- doc(body), decode(body, {**: {image: i}}).',
  'exactly v5 behaviour. A pathological document is a pathological query and no diagnostic names it.').
spelling('CARD-DESCENT-DEPTH', capped,
  'image(i) <- doc(body), decode(body, {**(3): {image: i}}).',
  'a depth argument the emitter turns into a WHERE on json_tree''s own depth-bearing path; costs one production and makes the cap visible at the call site.').
spelling('CARD-DESCENT-DEPTH', bind_the_path,
  'image(path, i) <- doc(body), decode(body, {$$path: {image: i}}).',
  'v4''s dropped $$${PATH?}, free in the lowering (json_tree.fullkey, receipt L4). Does not cap anything; lets the program filter depth itself with an ordinary guard.').

spelling('CARD-REGEX-KEY', ship_it,
  'dep(name, version) <- manifest(body), decode(body, {re:^(dev-)?dependencies: {$name: $version}}).',
  'exactly the v1 spelling (archive-20260428/sprefa-rules.sprf:10), and it runs today in both SQLite instances we measured. Not core SQLite: the rust target must register a REGEXP function or the program breaks on the flip.').
spelling('CARD-REGEX-KEY', compose_instead,
  'dep(name, version) <- manifest(body), decode(body, {$section: {$name: $version}}), section =~ ''^(dev-)?dependencies''.',
  'one extra body goal, no new key production, and the guard is an ordinary comparison the checker already sees. The recovery doc already recommended this as "subsumed if key capture lands".').

spelling(null_and_optional, two_plane_split,
  'rel resp(body: json).\nresp({name: ''cli'', parent: null}).      % (i) null INSIDE a json value: stored, round-trips\nrel repo(parent: text).\nrepo(null).                             % (ii) => refused: field_not_text(repo, parent, null)',
  'the split the storage forces: text can carry null, a typed column cannot. Missing and explicit null stay indistinguishable at the FIELD level, which five generations already chose (missing_key_yields_no_match).').
spelling(null_and_optional, reject_at_ingress,
  'resp({name: ''cli'', parent: null}).   % => refused: json_null_at_ingress(parent)',
  'keeps one answer for one word; every real GitHub/OpenAPI payload then needs preprocessing before it can enter a json column, which is most of the point of having one.').
spelling(null_and_optional, explicit_variant,
  'rel parent(some: text; none).\nrepo(''cli'', none).',
  'preserves the distinction with the enum machinery that already exists; costs a variant rel per nullable field and does nothing for null inside a json blob.').

spelling('CARD-FORMAT-DISPATCH', per_format_host,
  'sh read_toml(path) -> (body: json) = ''toml2json {path}''.',
  'no new syntax at all; one host decl per format, and the pattern grammar never learns about formats.').
spelling('CARD-FORMAT-DISPATCH', extension_dispatch,
  'rel doc(path: text, body: json).\ndoc(path, body) <- watch(''**/*.{json,yaml,toml}'', path, _), parse_doc(path, body).',
  'v5''s own behaviour (one grammar, extension sniffing) rebuilt as an ordinary host; the format knowledge lives in the extractor, not the language.').

spelling('CARD-VALUE-PATTERN', not_wanted,
  'image(repo, tag) <- doc(body), decode(body, {image: image_text}), image_text =~ ''^(.+):(.+)$''.',
  'recommended. Composes from constructs that already exist and does not resurrect semantics v5 never shipped.').
spelling('CARD-VALUE-PATTERN', ship_it,
  'image(repo, tag) <- doc(body), decode(body, {image: "$repo:$tag"}).',
  'the v3 documented form. Needs a template-to-regex lowering plus a rule for what a `$` inside a real JSON string value means, which is why v5 left T7 open.').

% Cards with no spelling to pick: they are dispatches or runtime arcs.
spelling_free('CARD-EDGE-BODY-JSON',
  'a runtime arc (frontier staging), not a spelling. Nothing for the user to choose here beyond scheduling it.').
spelling_free(schema_import_boundary,
  'blocked on the openapi_codegen_spine direction; if the spec is generated FROM prolog facts, the import half may never be needed.').

% ── receipts ─────────────────────────────────────────────────────────────────

card_receipts(4) :-
    receipt_every_origin_card_classified,
    receipt_no_card_is_both,
    receipt_open_cards_carry_exact_spellings,
    receipt_directive_attribution_complete.

% C1 -- all 14 inherited cards (9 recovery doc + 5 json-interop) are accounted
% for, and nothing was quietly dropped.
receipt_every_origin_card_classified :-
    findall(Card, ( card_answered(Card, recovery_doc, _, _)
                  ; card_open(Card, recovery_doc, _) ), RecoveryCards),
    length(RecoveryCards, RecoveryCount),
    findall(Card, ( card_answered(Card, json_interop_lab, _, _)
                  ; card_open(Card, json_interop_lab, _) ), InteropCards),
    length(InteropCards, 5),
    aggregate_all(count, card_answered(_, _, _, _), Answered),
    aggregate_all(count, card_open(_, _, _), Open),
    format("PASS C1 origin cards classified: recovery ~d, json-interop 5 (answered ~d / open ~d)~n",
           [RecoveryCount, Answered, Open]).

% C2 -- no card is both answered and open. A half-answered card is recorded as
% OPEN with the answered half named in its question text, never as both.
receipt_no_card_is_both :-
    \+ ( card_answered(Card, _, _, _), card_open(Card, _, _) ),
    format("PASS C2 no card is simultaneously answered and open~n").

% C3 -- THE NO-UNSIGHTED-SYNTAX GATE. Every open card either carries at least
% two exact spellings the user can read, or declares itself spelling-free.
receipt_open_cards_carry_exact_spellings :-
    forall(card_open(Card, _, _),
           (   spelling_free(Card, _)
           ->  true
           ;   aggregate_all(count, spelling(Card, _, _, _), Count),
               (   Count >= 2
               ->  true
               ;   throw(card_without_spellings(Card, Count))
               )
           )),
    % Every spelling is real text a user can read, not a placeholder.
    forall(spelling(_, _, Text, Cost),
           ( atom_length(Text, TextLength), TextLength > 15,
             atom_length(Cost, CostLength), CostLength > 40 )),
    aggregate_all(count, spelling(_, _, _, _), Spellings),
    format("PASS C3 every open card carries >=2 exact spellings (~d total) or is spelling-free~n",
           [Spellings]).

% C4 -- every answered card names WHICH directive answered it. An answer with
% no directive behind it would be this lab making a decision, which it may not.
receipt_directive_attribution_complete :-
    forall(card_answered(_, _, By, _), directive(By, _)),
    aggregate_all(count, directive(_, _), 3),
    format("PASS C4 all answers attributed to one of the 3 ruled directives~n").
