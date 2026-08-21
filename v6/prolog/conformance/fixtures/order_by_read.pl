% order_by_read.pl : the `order by` tail on a `?` read.
%
% Order is a property of the READ, so the oracle's answer is unchanged: a rel
% is still a set and `final/2` still grades one. What each fixture pins is that
% the tail compiles, survives the text door, and lands on final_select alone.
%
% FAIL-FIRST RECEIPT (order-tail arc): before order_tail//3 existed,
% parse_dl_dcg.pl:query_stmt//1 read `)` then `.` and every program below
% stopped at dl_parse_error(statement, ...) on the word `order`.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).


% A text ordering column reads through the intern VIEW (`__txt_*`), so the
% clause orders the DECODED characters and no base-table index can serve it.
% Two modules tie at 5 defs and `path` breaks the tie.
fixture(order_by_desc_with_a_tie_break,
  program(
    [ col_type(module_defs/2, path, text),
      col_type(module_defs/2, defs, int) ],
    [],
    [ query(module_defs(_Path, _Defs),
            order([order_col(2, desc), order_col(1, asc)])) ]),
  [ module_defs('b.rs', 5), module_defs('a.rs', 5), module_defs('c.rs', 9) ],
  [],
  [ final(module_defs/2,
          [ module_defs('a.rs', 5),
            module_defs('b.rs', 5),
            module_defs('c.rs', 9) ]) ]).

% All-int columns read the BASE table, and the ordering columns are not a
% prefix of the all-columns UNIQUE, so this one mints its own order index.
fixture(order_by_int_columns_read_the_base_table,
  program(
    [ col_type(score/2, player, int),
      col_type(score/2, points, int) ],
    [],
    [ query(score(_Player, _Points),
            order([order_col(2, desc), order_col(1, asc)])) ]),
  [ score(3, 5), score(1, 5), score(2, 9) ],
  [],
  [ final(score/2, [ score(1, 5), score(2, 9), score(3, 5) ]) ]).

% The control: same shape, no tail. Its final_select carries no ORDER BY and
% its DDL carries no order index.
fixture(query_without_an_order_tail_is_unmoved,
  program(
    [ col_type(tally/2, player, int),
      col_type(tally/2, points, int) ],
    [],
    [ query(tally(_Player, _Points)) ]),
  [ tally(3, 5), tally(1, 5), tally(2, 9) ],
  [],
  [ final(tally/2, [ tally(1, 5), tally(2, 9), tally(3, 5) ]) ]).
