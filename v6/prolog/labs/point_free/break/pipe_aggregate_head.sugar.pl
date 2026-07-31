% BREAK M3-2. `|>` under an AGGREGATE head. An aggregate's group key is
% exactly the head's non-aggregate columns, and under `|>` the columns that
% survive to the head are computed by liveness -- so adding a later stage that
% stops reading a column silently REGROUPS the aggregate.
% Expected refusal: pipe_head_is_aggregate(count/1).
sugar(prog(
  [ col_type(visit/2, page, text),
    col_type(visit/2, visitor, text),
    col_type(popular/2, page, text),
    col_type(popular/2, hits, int)
  ],
  [ (popular(Page, count(Visitor)) <-
       ( visit(Page, Visitor) )
     ~> ( Page \== 'admin' ))
  ])).
