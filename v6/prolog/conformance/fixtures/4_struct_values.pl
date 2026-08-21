% fixtures/4_struct_values.pl : the declared value plane (ruling
% compound_storage = struct_as_rows, arc header
% plans/2026-07-29-struct-as-rows-header.md).
%
% A declared struct value is a rel row referenced by content id. On the ORACLE
% side that is a statement about DECLARATIONS and REFUSALS only: engine.pl
% already holds real terms, so a struct-typed column holds the same canonical
% obj(...) it always did and ticklog.pl renders it as canonical JSON exactly as
% it always did. What the type decl adds here is the shape contract, the cycle
% unsupported construct, and the unknown-type unsupported construct. What it adds on the EMITTED side is
% the whole storage plane -- dictionary tables, intern-at-arrival, boundary
% render joins -- and the grade that the two sides still print the same bytes.
%
% Owner: coordinator (struct-as-rows arc).
%
% ── KNOWN: 18 of the 19 fixtures here fail roundtrip.sh's G1 ────────────────
% Measured 2026-07-30 at 6c3a7e2d+. G1 asserts parse_dl(print_dl(Term)) =@= Term
% and reports fail(not_variant) for every fixture below that declares a
% referenced relation. The one that passes (struct_column_type_unknown_rejected)
% is the one whose referenced name has no declaration at all.
%
% It is NOT a fixture spelling problem, and rewriting the terms alone makes it
% worse. The two doors disagree about where relation-reference normalization
% lives:
%
%   parse_dl.pl:574 normalize_relation_value_decls/2 synthesizes type_decl/2
%     from the referenced rel's own decl and KEEPS that rel's col_type/3 rows,
%     so the parser's output always carries BOTH forms.
%   print_dl.pl:209 decl_ref_order/2 emits a decl line for the type_decl AND a
%     second one for the same rel's col_type ref, so a term carrying both forms
%     prints `rel <target>(...)` TWICE.
%
% Every candidate fixture term therefore misses in one direction or the other:
%
%   type_decl alone (what is committed here)  parse adds the col_type rows
%   col_type rows alone                       parse adds the type_decl
%   both                                      print duplicates the decl line,
%                                             and reparsing the duplicate drops
%                                             the type entirely, because
%                                             parse_dl.pl:603 relation_schema/4
%                                             arity-checks the doubled column
%                                             list and stops synthesizing
%
% The fix is coupled and one half of it is compiler-side. Both halves measured
% together on scratch copies 2026-07-30:
%
%   (a) print_dl.pl decl_ref_order/2 skips the bare Name/Arity item when a
%       type_decl(Name, Specs) of the same arity is already printing that rel
%       (4 lines, one added guard predicate);
%   (b) every type_decl(N, Specs) here gains the explicit col_type(N/Arity, ...)
%       rows the parser produces, in place.
%
% Receipts: (a)+(b) together take this file to 19/19 on G1, and the printed
% .dl6 text is BYTE-IDENTICAL to today's for all 19 fixtures, so dl_view and
% the text door do not move. (b) alone duplicates a decl line in 18 of the 19
% rendered views and must not land by itself. (a) alone changes nothing.
%
% Held because print_dl.pl was outside this lane's file ownership.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% ═══ unsupported constructs (fail-first: each of these was ACCEPTED before this arc) ══════

% Content identity is computed FROM the children's content identity, so a
% cyclic type graph has no identity at all (types-as-rels verdict:
% interned_graph_is_a_dag). Refused on both doors; the entity plane, which
% permits cycles, is out of scope.
fixture(struct_type_cycle_rejected,
  prog([ type_decl(node, [col(next, node)]),
         col_type(node/1, next, node),
         col_type(holder/1, item, node) ],
       []),
  [],
  [ [ +holder(obj([next-nothing])) ] ],
  [ throws(type_cycle([node])) ]).

% Two types that reference each other. The same unsupported construct, one hop out: neither
% name can ever be placed, so both are named.
fixture(struct_type_mutual_cycle_rejected,
  prog([ type_decl(left, [col(other, right)]),
         col_type(left/1, other, right),
         type_decl(right, [col(other, left)]),
         col_type(right/1, other, left) ],
       []),
  [],
  [],
  [ throws(type_cycle([left, right])) ]).

% A bare identifier in type position is a REF to a declared type. A name no
% `type` decl introduces is a named unsupported construct, never a column that quietly holds
% whatever arrives -- the parser cannot tell a typo from a type.
% sibling folded 2026-08-20 (same throw column_type_unknown(spann)):
% struct_host_output_type_unknown_rejected, which reached the same stop
% through a decl-B host output column (sh_decl scan_span, at: spann). See git.
fixture(struct_column_type_unknown_rejected,
  prog([ col_type(finding/2, path, text),
         col_type(finding/2, at, spann) ],
       []),
  [],
  [],
  [ throws(column_type_unknown(spann)) ]).

% THE CHECKS-FIRST LOCK. Same rel, two live violations at once: a key position
% past the arity AND an unresolvable column type. Both doors run
% key_position_out_of_range first (analyze.pl:check_supported_subset_expanded/1,
% engine.pl:engine_check_order/1), and nothing that builds a column-type table
% earlier -- 0_rel_record.pl's rel/4 among them -- may move which class fires.
fixture(key_range_reported_before_unknown_column_type,
  prog([ col_type(finding/2, path, text),
         col_type(finding/2, at, spann),
         keyed(finding/2, [3]) ],
       []),
  [],
  [],
  [ throws(key_position_out_of_range(finding/2, 3, 2)) ]).

% SLOT-ARRIVAL-MALFORMED. A world row whose value is missing a declared field.
fixture(struct_arrival_missing_key_rejected,
  prog([ type_decl(span, [col(start, int), col(end, int)]),
         col_type(span/2, start, int), col_type(span/2, end, int),
         col_type(finding/2, path, text),
         col_type(finding/2, at, span) ],
       []),
  [],
  [ [ +finding('a.rs', obj([start-3])) ] ],
  [ throws(type_arrival_shape_mismatch(finding/2, at, span, missing_key(span, end))) ]).

% The same unsupported construct for a field whose declared type is int and whose arriving
% value is not. An int field silently storing text is exactly the TEXT-collapse
% class the expression lift spent an arc removing.
fixture(struct_arrival_field_type_rejected,
  prog([ type_decl(span, [col(start, int), col(end, int)]),
         col_type(span/2, start, int), col_type(span/2, end, int),
         col_type(finding/2, path, text),
         col_type(finding/2, at, span) ],
       []),
  [],
  [ [ +finding('a.rs', obj([end-nine, start-3])) ] ],
  [ throws(type_arrival_shape_mismatch(finding/2, at, span, field_not_int(span, end, nine))) ]).

% A key the type does not declare. Accepting it would put content in the value
% that the dictionary's column set cannot store, so the rendering and the row
% would disagree.
fixture(struct_arrival_unknown_key_rejected,
  prog([ type_decl(span, [col(start, int), col(end, int)]),
         col_type(span/2, start, int), col_type(span/2, end, int),
         col_type(finding/2, path, text),
         col_type(finding/2, at, span) ],
       []),
  [],
  [ [ +finding('a.rs', obj([end-9, extra-1, start-3])) ] ],
  [ throws(type_arrival_shape_mismatch(finding/2, at, span, unknown_key(span, extra))) ]).

% SLOT-ARRIVAL-CANONICAL-ORDER, RULED 2026-07-29 (struct_arrival_key_order,
% user: "we know the types order so we can induce it"): arrival key order is
% insignificant. The decl induces the canonical form; run_program rewrites
% every world row to sorted-key obj/1 before any store sees it
% (canonicalize_world_rows/3). FAIL-FIRST RECEIPT: this fixture's ancestor
% (struct_arrival_key_order_rejected, git history) pinned the old unsupported construct
% keys_not_sorted(span,[start,end]) on exactly this schedule; under the
% ruling the same out-of-order arrival is ACCEPTED and lands as the ONE
% canonical row -- byte-identical to the sorted-spelling twin above it.
fixture(struct_arrival_key_order_canonicalized,
  prog([ type_decl(span, [col(start, int), col(end, int)]),
         col_type(span/2, start, int), col_type(span/2, end, int),
         col_type(finding/2, path, text),
         col_type(finding/2, at, span) ],
       []),
  [],
  [ [ +finding('a.rs', obj([start-3, end-9])) ] ],
  [ final(finding/2, [ finding('a.rs', obj([end-9, start-3])) ]),
    ticks(1) ]).

% A struct-typed column does NOT accept a plain Prolog compound term.
% SLOT-TERM-STRUCT (0_type_plane.pl header): the oracle renders a compound
% term as canonical PROLOG text and a struct as canonical JSON, so accepting
% the functor spelling would silently change the graded bytes of a value that
% already has a meaning. The functor form keeps its current untyped behavior
% in undeclared columns; it is simply not a struct spelling.
fixture(struct_arrival_functor_term_rejected,
  prog([ type_decl(span, [col(start, int), col(end, int)]),
         col_type(span/2, start, int), col_type(span/2, end, int),
         col_type(finding/2, path, text),
         col_type(finding/2, at, span) ],
       []),
  [],
  [ [ +finding('a.rs', span(3, 9)) ] ],
  [ throws(type_arrival_shape_mismatch(finding/2, at, span, not_an_object(span, span(3, 9)))) ]).

% ═══ the value plane runs (oracle side: terms, unchanged) ═══════════════════

% EDGE 1, half one. A struct-typed column prints its VALUE, never an id, and
% the value prints as canonical JSON with sorted keys. On the oracle this is
% ticklog.pl's existing obj(...) rendering; the emitted side has to JOIN a
% dictionary and select a memoized rendering to say the same bytes.
fixture(struct_column_renders_canonical_json,
  prog([ type_decl(span, [col(start, int), col(end, int)]),
         col_type(span/2, start, int), col_type(span/2, end, int),
         col_type(finding/2, path, text),
         col_type(finding/2, at, span),
         col_type(touched/1, path, text) ],
       [ (touched(Path) <- finding(Path, _At)) ]),
  [],
  [ [ +finding('a.rs', obj([end-9, start-3])) ],
    [ +finding('b.rs', obj([end-4, start-1])) ] ],
  [ final(touched/1, [ touched('a.rs'), touched('b.rs') ]),
    final(finding/2, [ finding('a.rs', obj([end-9, start-3])),
                       finding('b.rs', obj([end-4, start-1])) ]),
    ticks(2) ]).

% EDGE 1, half two: BUILD ORDER INDEPENDENCE. The same two values arrive in
% opposite orders in the two halves of this pair. Dense storage ids are
% assigned in arrival order and therefore differ between the halves; the tick
% log must not. This is the lab's rendered_text_stable_under_both_policies as
% a real fixture pair -- the sibling is struct_intern_order_b below and the
% grade is the two emitted tick logs being identical after the rel names are
% aligned.
fixture(struct_intern_order_a,
  prog([ type_decl(span, [col(start, int), col(end, int)]),
         col_type(span/2, start, int), col_type(span/2, end, int),
         col_type(mark/1, at, span) ],
       []),
  [],
  [ [ +mark(obj([end-2, start-1])) ],
    [ +mark(obj([end-4, start-3])) ] ],
  [ final(mark/1, [ mark(obj([end-2, start-1])), mark(obj([end-4, start-3])) ]),
    ticks(2) ]).

fixture(struct_intern_order_b,
  prog([ type_decl(span, [col(start, int), col(end, int)]),
         col_type(span/2, start, int), col_type(span/2, end, int),
         col_type(mark/1, at, span) ],
       []),
  [],
  [ [ +mark(obj([end-4, start-3])) ],
    [ +mark(obj([end-2, start-1])) ] ],
  [ final(mark/1, [ mark(obj([end-2, start-1])), mark(obj([end-4, start-3])) ]),
    ticks(2) ]).

% THE MIGRATION RECEIPT (verdict Q6: "migrating a column from inline to ref
% changes the stored bytes and, under a counter policy, changes ids; it does
% not change the value"), MEASURED, and it does not say what the arc header
% expected.
%
% The twin of `struct_intern_order_a` with the type declaration REMOVED and
% nothing else touched -- same rel, same rows, same schedule -- was compiled
% and replayed on this arc's sweep. Its ORACLE log is byte-identical to the
% declared twin's, which is the half that holds: the declaration is inert on
% a door that stores terms, so the VALUE did not change. Its EMITTED log is
% not:
%
%   declared   actual = oracle
%              {"tick":1,"deltas":{"mark":{"add":[[{"end":2,"start":1}]],...
%   inline     actual {"tick":1,"deltas":{"mark":{"add":[["obj([|](-(end,2),[|](-(start,1),[])))"]],"del":[]}}}
%              oracle {"tick":1,"deltas":{"mark":{"add":[[{"end":2,"start":1}]],"del":[]}}}
%
% An untyped compound value ARRIVES as canonical prolog term text while the
% emitter's own compound encoding is the json1 tagged form -- the "compound
% arrival text vs json1 match" mismatch SCOREBOARD.md has carried since phase
% C. So the honest statement is stronger than "byte-identical either way":
% the ref side is byte-identical to the oracle and the inline side never was.
% The migration is the fix, not a wash.
%
% The twin is NOT kept as a fixture, deliberately: it would be a corpus entry
% whose emitted log is knowingly wrong, and the sweep's wrong bucket is a gate
% that must stay at zero. Making it live in the `unsupported` bucket instead
% means turning the untyped-compound-arrival encoding into a NAMED unsupported construct --
% real work, real blast radius across every fixture that arrives a compound
% into a text column, and SLOT-TERM-STRUCT's question first. Named follow-up.

% Nesting costs no new syntax: a struct field may itself be a declared type,
% and the parent's rendering is one concat over the child's.
fixture(struct_nested_value_renders_whole_tree,
  prog([ type_decl(span, [col(start, int), col(end, int)]),
         col_type(span/2, start, int), col_type(span/2, end, int),
         type_decl(place, [col(file, text), col(at, span)]),
         col_type(place/2, file, text), col_type(place/2, at, span),
         col_type(diag/2, where, place),
         col_type(diag/2, message, text),
         col_type(diag_file/1, file, text) ],
       [ (diag_file(File) <- diag(Where, _Message), decode(Where, {file: File})) ]),
  [],
  [ [ +diag(obj([at-obj([end-9, start-3]), file-'a.rs']), 'unused') ] ],
  [ final(diag_file/1, [ diag_file('a.rs') ]),
    ticks(1) ]).

% THE ACCEPTANCE CASE (arc header scope item 5): ghcacher's stars
% normalization. `ghcacher_json_normalization` reads the same value out of an
% UNTYPED json column and is refused by name (decode_source_not_struct on the
% first rule, json_each on the second). Declaring the response body's shape is
% the whole difference: decode becomes a dictionary join and the rule compiles.
%
%   rel repo_body(full_name: text, stargazers_count: int).
%   rel current_body(ep: text, body: repo_body).
%   stars(ep, n) <- current_body(ep, body), decode(body, {stargazers_count: n}).
%
%   const stars$ = combineLatest([currentBody$, repoBodyDict$]).pipe(
%     map(([bodies, dict]) => bodies.flatMap((row) => {
%       const body = dict.get(row.body);          // one keyed read, no parse
%       return body ? [{ ep: row.ep, n: body.stargazers_count }] : [];
%     })),
%     distinctUntilChanged(sameRowSet),
%   );
fixture(struct_ghcacher_stars_normalization,
  prog([ type_decl(repo_body, [col(full_name, text), col(stargazers_count, int)]),
         col_type(repo_body/2, full_name, text), col_type(repo_body/2, stargazers_count, int),
         col_type(current_body/2, ep, text),
         col_type(current_body/2, body, repo_body),
         col_type(stars/2, ep, text),
         col_type(stars/2, n, int) ],
       [ (stars(Ep, N) <-
            current_body(Ep, Body),
            decode(Body, {stargazers_count: N})) ]),
  [],
  [ [ +current_body(repo, obj([full_name-cli, stargazers_count-17])) ],
    [ +current_body(other, obj([full_name-hub, stargazers_count-4])) ] ],
  [ deltas(stars/2, [ [ +stars(repo, 17) ], [ +stars(other, 4) ] ]),
    final(stars/2, [ stars(other, 4), stars(repo, 17) ]),
    ticks(2) ]).

% A key the declared type does not have is a NAMED unsupported construct, where the untyped
% arm's own answer is "fails quietly" (json_arm.pl
% decode_missing_key_fails_quietly). That difference is the point of declaring
% a type: a typo in a field name stops being a rule that derives nothing.
fixture(struct_decode_field_unknown_rejected,
  prog([ type_decl(span, [col(start, int), col(end, int)]),
         col_type(span/2, start, int), col_type(span/2, end, int),
         col_type(mark/1, at, span),
         col_type(seen/1, start, int) ],
       [ (seen(Start) <- mark(At), decode(At, {beginning: Start})) ]),
  [],
  [ [ +mark(obj([end-2, start-1])) ] ],
  [ final(seen/1, []), ticks(1) ]).

% SPANS (arc header scope item 5). `sprefa-extract` emits byte spans as a
% NESTED json object -- `SpanOut { start: u32, end: u32 }` in
% v6/sprefa-extract/src/types.rs, serialized `"span":{"start":..,"end":..}` --
% and both flagship-callgraph.dl6 and diag-rail.dl6 dropped line numbers
% entirely rather than fake them, because a nested field could not reach a
% declared int column. Under struct-as-rows the shape is expressible exactly:
% the span is a declared type, its fields ARE int columns, and decode/2 gets
% them out by join. This fixture is that shape end to end with the value
% arriving as an ordinary world row.
%
%   rel span(end: int, start: int).
%   rel node_fact(path: text, name: text, at: span) log keep(all).
%   rel def_start(path: text, offset: int).
%   def_start(path, offset) <- node_fact(path, name, at), decode(at, {start: offset}).
%
%   const defStart$ = combineLatest([nodeFact$, spanDict$]).pipe(
%     map(([facts, spans]) => facts.flatMap((fact) => {
%       const at = spans.get(fact.at);
%       return at ? [{ path: fact.path, offset: at.start }] : [];
%     })),
%     distinctUntilChanged(sameRowSet),
%   );
%
% The host-output seam fixture below closes the remaining arrival spelling:
% live sh JSON crosses serve/1_hosts.ts as text, then StructPlane parses and
% interns that text for the response relation's emitted ref column.
fixture(struct_span_columns_are_int_after_decode,
  prog([ type_decl(span, [col(end, int), col(start, int)]),
         col_type(span/2, end, int), col_type(span/2, start, int),
         col_type(node_fact/3, path, text),
         col_type(node_fact/3, name, text),
         col_type(node_fact/3, at, span),
         kind(node_fact/3, log), keep(node_fact/3, all),
         col_type(def_start/2, path, text),
         col_type(def_start/2, offset, int) ],
       [ (def_start(Path, Offset) <-
            node_fact(Path, _Name, At),
            decode(At, {start: Offset})) ]),
  [],
  [ [ +node_fact('a.rs', main, obj([end-42, start-17])) ],
    [ +node_fact('b.rs', helper, obj([end-9, start-3])) ] ],
  [ deltas(def_start/2, [ [ +def_start('a.rs', 17) ], [ +def_start('b.rs', 3) ] ]),
    final(def_start/2, [ def_start('a.rs', 17), def_start('b.rs', 3) ]),
    ticks(2) ]).

% HOST-OUTPUT-SEAM FAIL-FIRST RECEIPT, acceptance direction:
% the term/oracle door already accepted this program and its canned response.
% Printing the same program to .dl6 and compiling the text refused `at: span`
% as unsupported_surface(column_type_wrapper(scan_span,at,none)). This fixture
% keeps the oracle and emitter on the same arc: the host answer is schedule-fed
% in the canned-rows form, its struct value is interned before arrival SQL, and
% the boundary renders the canonical value in both emitter modes.
fixture(struct_host_output_schedule_answer_interned,
  program(
    [ type_decl(span, [col(end, int), col(start, int)]),
      col_type(span/2, end, int), col_type(span/2, start, int),
      col_type(source_path/1, path, text),
      col_type(host_span/2, path, text),
      col_type(host_span/2, at, span),
      col_type(host_start/2, path, text),
      col_type(host_start/2, start, int),
      sh_decl(scan_span,
              [col(path, text)],
              [col(at, span)],
              template(""))
    ],
    [ (host_span(Path, At) <-
          (source_path(Path),
           probe(scan_span, [Path], [At], []))),
      (host_start(Path, Start) <-
          (host_span(Path, At),
           decode(At, {start: Start})))
    ],
    [query(host_start(Path, Start))]),
  [source_path('a.rs')],
  [ [ +'__host_response_scan_span'(
          'witness|scan_span|path:text=a.rs',
          0, 'a.rs', obj([end-42, start-17]))
    ]
  ],
  [ final(host_span/2,
          [host_span('a.rs', obj([end-42, start-17]))]),
    final(host_start/2, [host_start('a.rs', 17)]),
    ticks(1) ]).

% Two parents reference the same public target row. Parent retraction does not
% cascade into target membership.
fixture(struct_shared_child_survives_one_release,
  prog([ type_decl(span, [col(start, int), col(end, int)]),
         col_type(span/2, start, int), col_type(span/2, end, int),
         col_type(hit/2, owner, text),
         col_type(hit/2, at, span) ],
       []),
  [],
  [ [ +hit(left, obj([end-2, start-1])), +hit(right, obj([end-2, start-1])) ],
    [ -hit(left, obj([end-2, start-1])) ],
    [ -hit(right, obj([end-2, start-1])) ] ],
  [ deltas(hit/2, [ [ +hit(left, obj([end-2, start-1])),
                      +hit(right, obj([end-2, start-1])) ],
                    [ -hit(left, obj([end-2, start-1])) ],
                    [ -hit(right, obj([end-2, start-1])) ] ]),
    final(hit/2, []),
    ticks(3) ]).

% Nested ingress becomes a target arrival before the parent. A level rule
% reading the target therefore settles at the same external tick.
fixture(relation_reference_target_and_parent_share_tick,
  prog([ type_decl(span, [col(start, int), col(end, int)]),
         col_type(span/2, start, int), col_type(span/2, end, int),
         col_type(finding/1, at, span),
         col_type(span_seen/2, start, int),
         col_type(span_seen/2, end, int) ],
       [ (span_seen(Start, End) <- span(Start, End)) ]),
  [],
  [ [ +finding(obj([end-9, start-3])) ] ],
  [ deltas(span/2, [ [ +span(3, 9) ] ]),
    deltas(finding/1, [ [ +finding(obj([end-9, start-3])) ] ]),
    deltas(span_seen/2, [ [ +span_seen(3, 9) ] ]),
    final(span/2, [ span(3, 9) ]),
    final(finding/1, [ finding(obj([end-9, start-3])) ]),
    final(span_seen/2, [ span_seen(3, 9) ]),
    ticks(1) ]).
