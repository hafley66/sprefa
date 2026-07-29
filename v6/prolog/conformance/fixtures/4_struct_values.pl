% fixtures/4_struct_values.pl : the declared value plane (ruling
% compound_storage = struct_as_rows, arc header
% plans/2026-07-29-struct-as-rows-header.md).
%
% A declared struct value is a rel row referenced by content id. On the ORACLE
% side that is a statement about DECLARATIONS and REFUSALS only: engine.pl
% already holds real terms, so a struct-typed column holds the same canonical
% obj(...) it always did and ticklog.pl renders it as canonical JSON exactly as
% it always did. What the type decl adds here is the shape contract, the cycle
% refusal, and the unknown-type refusal. What it adds on the EMITTED side is
% the whole storage plane -- dictionary tables, intern-at-arrival, boundary
% render joins -- and the grade that the two sides still print the same bytes.
%
% Owner: coordinator (struct-as-rows arc).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% ═══ refusals (fail-first: each of these was ACCEPTED before this arc) ══════

% Content identity is computed FROM the children's content identity, so a
% cyclic type graph has no identity at all (types-as-rels verdict:
% interned_graph_is_a_dag). Refused on both doors; the entity plane, which
% permits cycles, is out of scope.
fixture(struct_type_cycle_rejected,
  prog([ type_decl(node, [col(next, node)]),
         col_type(holder/1, item, node) ],
       []),
  [],
  [ [ +holder(obj([next-nothing])) ] ],
  [ throws(type_cycle([node])) ]).

% Two types that reference each other. The same refusal, one hop out: neither
% name can ever be placed, so both are named.
fixture(struct_type_mutual_cycle_rejected,
  prog([ type_decl(left, [col(other, right)]),
         type_decl(right, [col(other, left)]) ],
       []),
  [],
  [],
  [ throws(type_cycle([left, right])) ]).

% A bare identifier in type position is a REF to a declared type. A name no
% `type` decl introduces is a named refusal, never a column that quietly holds
% whatever arrives -- the parser cannot tell a typo from a type.
fixture(struct_column_type_unknown_rejected,
  prog([ col_type(finding/2, path, text),
         col_type(finding/2, at, spann) ],
       []),
  [],
  [],
  [ throws(column_type_unknown(spann)) ]).

% SLOT-ARRIVAL-MALFORMED. A world row whose value is missing a declared field.
fixture(struct_arrival_missing_key_rejected,
  prog([ type_decl(span, [col(start, int), col(end, int)]),
         col_type(finding/2, path, text),
         col_type(finding/2, at, span) ],
       []),
  [],
  [ [ +finding('a.rs', obj([start-3])) ] ],
  [ throws(type_arrival_shape_mismatch(finding/2, at, span, missing_key(span, end))) ]).

% The same refusal for a field whose declared type is int and whose arriving
% value is not. An int field silently storing text is exactly the TEXT-collapse
% class the expression lift spent an arc removing.
fixture(struct_arrival_field_type_rejected,
  prog([ type_decl(span, [col(start, int), col(end, int)]),
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
         col_type(finding/2, path, text),
         col_type(finding/2, at, span) ],
       []),
  [],
  [ [ +finding('a.rs', obj([end-9, extra-1, start-3])) ] ],
  [ throws(type_arrival_shape_mismatch(finding/2, at, span, unknown_key(span, extra))) ]).

% SLOT-ARRIVAL-CANONICAL-ORDER. Two spellings of one value would be two rows
% on the oracle (term identity) and ONE dictionary row on the emitted side
% (same canonical content), so the non-canonical spelling is refused rather
% than left to diverge. Lifting this needs canonicalization inside the
% oracle's own absorb path, which is an oracle semantics ruling.
fixture(struct_arrival_key_order_rejected,
  prog([ type_decl(span, [col(start, int), col(end, int)]),
         col_type(finding/2, path, text),
         col_type(finding/2, at, span) ],
       []),
  [],
  [ [ +finding('a.rs', obj([start-3, end-9])) ] ],
  [ throws(type_arrival_shape_mismatch(finding/2, at, span, keys_not_sorted(span, [start, end]))) ]).

% A struct-typed column does NOT accept a plain Prolog compound term.
% SLOT-TERM-STRUCT (0_type_plane.pl header): the oracle renders a compound
% term as canonical PROLOG text and a struct as canonical JSON, so accepting
% the functor spelling would silently change the graded bytes of a value that
% already has a meaning. The functor form keeps its current untyped behavior
% in undeclared columns; it is simply not a struct spelling.
fixture(struct_arrival_functor_term_rejected,
  prog([ type_decl(span, [col(start, int), col(end, int)]),
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
         col_type(mark/1, at, span) ],
       []),
  [],
  [ [ +mark(obj([end-2, start-1])) ],
    [ +mark(obj([end-4, start-3])) ] ],
  [ final(mark/1, [ mark(obj([end-2, start-1])), mark(obj([end-4, start-3])) ]),
    ticks(2) ]).

fixture(struct_intern_order_b,
  prog([ type_decl(span, [col(start, int), col(end, int)]),
         col_type(mark/1, at, span) ],
       []),
  [],
  [ [ +mark(obj([end-4, start-3])) ],
    [ +mark(obj([end-2, start-1])) ] ],
  [ final(mark/1, [ mark(obj([end-2, start-1])), mark(obj([end-4, start-3])) ]),
    ticks(2) ]).

% Nesting costs no new syntax: a struct field may itself be a declared type,
% and the parent's rendering is one concat over the child's.
fixture(struct_nested_value_renders_whole_tree,
  prog([ type_decl(span, [col(start, int), col(end, int)]),
         type_decl(place, [col(file, text), col(at, span)]),
         col_type(diag/2, where, place),
         col_type(diag/2, message, text),
         col_type(diag_file/1, file, text) ],
       [ (diag_file(File) <- diag(Where, _Message), decode(Where, {file: File})) ]),
  [],
  [ [ +diag(obj([at-obj([end-9, start-3]), file-'a.rs']), 'unused') ] ],
  [ final(diag_file/1, [ diag_file('a.rs') ]),
    ticks(1) ]).

% THE SHARED CHILD (types-as-rels verdict Q3, domination_shared_child_survives
% / domination_sole_owner_cascades) as a compiled fixture. Two parents hold the
% SAME child value; releasing one leaves the survivor's rendering intact, and
% releasing the last one removes both parents. What the tick log shows is the
% VALUE plane only -- the dictionary is boundary-invisible (Edge 2), which is
% exactly why GC timing on it cannot be observed here.
fixture(struct_shared_child_survives_one_release,
  prog([ type_decl(span, [col(start, int), col(end, int)]),
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
