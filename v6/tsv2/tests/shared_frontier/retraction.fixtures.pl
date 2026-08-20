% Retraction battery for frontier(shared), plan step 5. One source per case:
% compile_fixture/5 compiles this term for both arms, conformance/ticklog.pl
% runs the same term through the oracle.
%
% Every schedule ENDS on a tick carrying a retraction, because the recount
% verb runs only on such a tick (1_incremental.ts recompute_levels_after_edges
% reads retraction_guard_sql first). The shared support ledger is written by
% that verb, so its rows are current exactly at the end of these runs.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

% Current retraction: the deleted person is the row the rule read, so
% resident/1 loses its only support and is retracted through the recount.
fixture(sf_retract_current,
        prog([ col_type(person/2, name, text),
               col_type(person/2, town, text),
               keyed(person/2, [1]),
               col_type(town/1, name, text),
               keyed(town/1, [1]),
               col_type(resident/1, name, text) ],
             [ (resident(Name) <- person(Name, Town), town(Town)) ]),
        [],
        [ [ +town(nyc), +person(ann, nyc), +person(bob, nyc) ],
          [ +person(cy, nyc) ],
          [ -person(bob, nyc) ] ],
        []).

% Stale retraction: tick 2 replaces the keyed row, so tick 3's delete names a
% row that is no longer stored and must move nothing.
fixture(sf_retract_stale,
        prog([ col_type(setting/2, key, text),
               col_type(setting/2, depth, int),
               keyed(setting/2, [1]),
               col_type(deep/1, key, text) ],
             [ (deep(Key) <- setting(Key, Depth), Depth >= 5) ]),
        [],
        [ [ +setting(width, 9), +setting(depth, 1) ],
          [ +setting(depth, 9) ],
          [ -setting(depth, 1) ] ],
        []).

% Negation support counts: sold/1 arriving takes available/1's support away,
% sold/1 leaving gives it back, both through the recount.
fixture(sf_negation_support,
        prog([ col_type(item/1, name, text),
               keyed(item/1, [1]),
               col_type(sold/1, name, text),
               keyed(sold/1, [1]),
               col_type(available/1, name, text) ],
             [ (available(Name) <- item(Name), not(sold(Name))) ]),
        [],
        [ [ +item(hat), +item(mug) ],
          [ +sold(hat) ],
          [ -sold(hat), +sold(mug) ],
          [ -sold(mug) ] ],
        []).

% Two rules feeding one head: the ledger carries a row per rule id, so a
% retraction that empties one arm leaves the other arm's count standing.
fixture(sf_two_rule_support,
        prog([ col_type(hired/1, name, text),
               keyed(hired/1, [1]),
               col_type(founder/1, name, text),
               keyed(founder/1, [1]),
               col_type(staff/1, name, text) ],
             [ (staff(Name) <- hired(Name)),
               (staff(Name) <- founder(Name)) ]),
        [],
        [ [ +hired(ann), +founder(ann), +hired(bob) ],
          [ -hired(ann) ],
          [ -hired(bob) ] ],
        []).
