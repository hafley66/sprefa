% fixtures/14_option_wrapper_walk.pl : `option` walks like every other type
% wrapper (0_type_plane.pl type_wrapper/2 + column_element_type_name/2), and
% the type_decl/2 mirror is re-read from the expanded columns
% (0_generic_expand.pl expanded_relation_specs/3).
%
% Both directions in one file. The four accepting fixtures each round-trip an
% ABSENT and a PRESENT value; the four stopping fixtures name the term the
% spelling threw before this walk existed and still throws after it.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% ═══ accepted ═══════════════════════════════════════════════════════════════

% option(list(<rel>)): the option desugars to a companion split rel whose
% element is the minted list rel, the list member's value column is the rel,
% and an absent list is a missing companion row. Squad 1 is present, squad 2
% absent.
fixture(option_list_of_rel_round_trips_absent_and_present,
  prog([ type_decl(fighter_summary, [col(name, text), col(url, text)]),
         col_type(fighter_summary/2, name, text),
         col_type(fighter_summary/2, url, text),
         col_type(squad/2, id, int),
         col_type(squad/2, members, option(list(fighter_summary))),
         keyed(squad/2, [1]) ],
       []),
  [],
  [ [ +squad(1),
      +squad__members(1, 100),
      +'__gen__list_fighter_summary_b424a4b49951eef7'('[{"name":"ada","url":"ada.io"},{"name":"bo","url":"bo.io"}]'),
      +'__gen__list_fighter_summary_b424a4b49951eef7__member'(100, 0, obj([name-ada, url-'ada.io'])),
      +'__gen__list_fighter_summary_b424a4b49951eef7__member'(100, 1, obj([name-bo, url-'bo.io'])) ],
    [ +squad(2) ] ],
  [ final(squad/1, [ squad(1), squad(2) ]),
    final(squad__members/2, [ squad__members(1, 100) ]),
    final('__gen__list_fighter_summary_b424a4b49951eef7__member'/3,
          ['__gen__list_fighter_summary_b424a4b49951eef7__member'(100, 0, obj([name-ada, url-'ada.io'])),
           '__gen__list_fighter_summary_b424a4b49951eef7__member'(100, 1, obj([name-bo, url-'bo.io']))]),
    final(fighter_summary/2,
          [ fighter_summary(ada, 'ada.io'), fighter_summary(bo, 'bo.io') ]),
    ticks(2) ]).

% option(<rel>) on a rel that is ITSELF a reference target. The mirror commit
% carries at parse still lists reviewed_by; the desugar removes that column
% from commit/2 and mints commit__reviewed_by/2, so the mirror has to lose it
% too or a later check reads option(person), which no rel declares.
fixture(option_rel_on_a_reference_target_round_trips_absent_and_present,
  prog([ col_type(person/2, id, int),
         col_type(person/2, name, text),
         keyed(person/2, [1]),
         type_decl(commit, [col(id, int), col(reviewed_by, option(person))]),
         col_type(commit/2, id, int),
         col_type(commit/2, reviewed_by, option(person)),
         col_type(audit/2, audit_id, int),
         col_type(audit/2, at_commit, commit),
         col_type(reviewed/2, commit_id, int),
         col_type(reviewed/2, reviewer_name, text) ],
       [ (reviewed(CommitId, ReviewerName) <-
              commit__reviewed_by(CommitId, PersonId),
              person(PersonId, ReviewerName)) ]),
  [],
  [ [ +person(7, "ada"), +audit(1, obj([id-101])), +audit(2, obj([id-102])) ],
    [ +commit__reviewed_by(101, 7) ] ],
  [ final(commit/1, [ commit(101), commit(102) ]),
    final(commit__reviewed_by/2, [ commit__reviewed_by(101, 7) ]),
    final(reviewed/2, [ reviewed(101, "ada") ]),
    final(audit/2, [ audit(1, obj([id-101])), audit(2, obj([id-102])) ]),
    ticks(2) ]).

% The same walk over the other value-storing wrappers: option in front of the
% dense-sequence and linked-sequence flavors reaches the element exactly as the
% bare flavor does.
fixture(option_dense_sequence_of_rel_round_trips_absent_and_present,
  prog([ type_decl(fighter_summary, [col(name, text), col(url, text)]),
         col_type(fighter_summary/2, name, text),
         col_type(fighter_summary/2, url, text),
         col_type(squad/2, id, int),
         col_type(squad/2, members,
                  option(list_entity_dense_sequence(fighter_summary))),
         keyed(squad/2, [1]) ],
       []),
  [],
  [ [ +squad(1),
      +squad__members(1, 100),
      +'__gen__list_entity_dense_sequence_fighter_summary_bb78bd1b4eb62d42'(100),
      +'__gen__list_entity_dense_sequence_fighter_summary_bb78bd1b4eb62d42__member'(100, 0, obj([name-ada, url-'ada.io'])) ],
    [ +squad(2) ] ],
  [ final(squad/1, [ squad(1), squad(2) ]),
    final(squad__members/2, [ squad__members(1, 100) ]),
    final(fighter_summary/2, [ fighter_summary(ada, 'ada.io') ]),
    ticks(2) ]).

% option(list(<scalar>)) is unchanged by the walk: 13_option_list_columns.pl
% holds the round-trip, and this one holds the shape beside a rel-element
% sibling so one program carries both element families.
fixture(option_list_of_scalar_and_of_rel_in_one_rel,
  prog([ type_decl(fighter_summary, [col(name, text), col(url, text)]),
         col_type(fighter_summary/2, name, text),
         col_type(fighter_summary/2, url, text),
         col_type(squad/3, id, int),
         col_type(squad/3, members, option(list(fighter_summary))),
         col_type(squad/3, ranks, option(list(int))),
         keyed(squad/3, [1]) ],
       []),
  [],
  [ [ +squad(1),
      +squad__ranks(1, 200),
      +'__gen__list_int_798e673312e7575f'('[3]'),
      +'__gen__list_int_798e673312e7575f__member'(200, 0, 3) ],
    [ +squad(2),
      +squad__members(2, 100),
      +'__gen__list_fighter_summary_b424a4b49951eef7'('[{"name":"ada","url":"ada.io"}]'),
      +'__gen__list_fighter_summary_b424a4b49951eef7__member'(100, 0, obj([name-ada, url-'ada.io'])) ] ],
  [ final(squad/1, [ squad(1), squad(2) ]),
    final(squad__ranks/2, [ squad__ranks(1, 200) ]),
    final(squad__members/2, [ squad__members(2, 100) ]),
    final(fighter_summary/2, [ fighter_summary(ada, 'ada.io') ]),
    ticks(2) ]).

% ═══ still stopping, same term as before the walk ═══════════════════════════

% json_list/1 is not a wrapper: its element domain is the closed scalar set, so
% the walk never reaches `fighter_summary` through it and the stop keeps naming
% which of the two json_list reasons it was.
fixture(option_of_json_list_keeps_its_stop,
  prog([ col_type(squad/2, id, int),
         col_type(squad/2, ranks, option(json_list(int))),
         keyed(squad/2, [1]) ],
       []),
  [],
  [],
  [ throws(unsupported_construct(
             option_element_type_unknown(json_list(int)))) ]).

% Nested scalar options are two enum layers. The outer some payload is an
% inner option id, preserving none, some(none), and some(some(value)).
fixture(option_of_option_of_scalar_keeps_its_stop,
  prog([ col_type(squad/2, id, int),
         col_type(squad/2, rank, option(option(int))),
         keyed(squad/2, [1]) ],
       []),
  [],
  [],
  []).

% The interned-set value dictionary keys on scalar content whatever wraps it.
% Before the walk this spelling hid the rel element from
% check_interned_set_rel_elements/1 and stopped as column_type_unknown; it now
% carries the same term the bare spelling in 10_list_elements.pl throws.
fixture(option_of_interned_set_of_rel_is_refused,
  prog([ type_decl(fighter_summary, [col(name, text), col(url, text)]),
         col_type(fighter_summary/2, name, text),
         col_type(fighter_summary/2, url, text),
         col_type(squad/2, id, int),
         col_type(squad/2, members, option(list_interned_set(fighter_summary))),
         keyed(squad/2, [1]) ],
       []),
  [],
  [],
  [ throws(unsupported_construct(
             list_interned_set_relation_element(fighter_summary))) ]).

% The reference-option desugar mints `<parent>__<column>` into the author
% namespace. A program that already declares that name used to surface as a
% bare rel_arity_collision naming neither the option nor the column.
fixture(option_companion_name_collision_is_named,
  prog([ col_type(pair_holder__before/1, probe_value, int),
         col_type(pair_holder/2, label, text),
         col_type(pair_holder/2, before, option(pair_holder__before)) ],
       []),
  [],
  [],
  [ throws(unsupported_construct(
             option_companion_name_collision(pair_holder__before/1,
                                             pair_holder/2, before))) ]).

% Every column of a reference target moving to a companion split rel leaves it
% with no stored columns, and identity is key(...) or the full row, so a
% zero-column row can never be told from another. Same rel WITHOUT the
% reference-target use compiles: see option_list_of_rel_round_trips above.
fixture(reference_target_emptied_by_option_split_is_named,
  prog([ type_decl(fighter_summary, [col(name, text), col(url, text)]),
         col_type(fighter_summary/2, name, text),
         col_type(fighter_summary/2, url, text),
         type_decl(squad, [col(members, option(list(fighter_summary)))]),
         col_type(squad/1, members, option(list(fighter_summary))),
         col_type(roster/2, roster_id, int),
         col_type(roster/2, at_squad, squad) ],
       []),
  [],
  [],
  [ throws(unsupported_construct(
             reference_target_has_no_columns(squad/0))) ]).

% A name no rel declares is still a name no rel declares, at any wrapper depth.
fixture(option_list_of_unknown_name_keeps_its_stop,
  prog([ col_type(squad/2, id, int),
         col_type(squad/2, members, option(list(fighter_summry))),
         keyed(squad/2, [1]) ],
       []),
  [],
  [],
  [ throws(column_type_unknown(fighter_summry)) ]).
