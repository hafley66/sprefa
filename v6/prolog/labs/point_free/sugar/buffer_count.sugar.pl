% corpus 4 sugar. M2 collapses the four-rule cursor template to one bind.
%
%   rel numbered(ordinal: int, payload: text) log keep(all).
%   numbered(Ordinal, Payload) <+ arrival(Payload), Ordinal := seq('q').
%   batch(Bucket, Payload) <- numbered(Ordinal, Payload), Bucket := (Ordinal - 1) / 3.
%
% The cursor rel is not declared and not named by the author; the expansion
% mints it. `seq('q')` with an ATOM is one global order; `seq(Partition)` with
% a variable is one order per value of that variable -- that argument is the
% whole of slot_seq_scope.
%
% rx lowering:
%   arrival$.pipe(scan((ordinal, a) => [ordinal[0] + 1, a.payload], [0, null]),
%                 map(([ordinal, payload]) => ({ ordinal, payload })),
%                 map(row => ({ bucket: ((row.ordinal - 1) / 3) | 0, payload: row.payload })))
sugar(prog(
  [ col_type(arrival/1, payload, text),
    col_type(numbered/2, ordinal, int),
    col_type(numbered/2, payload, text),
    kind(numbered/2, log),
    keep(numbered/2, all),
    col_type(batch/2, bucket, int),
    col_type(batch/2, payload, text)
  ],
  [ (numbered(Ordinal, Payload) <+ arrival(Payload), Ordinal := seq('q')),
    (batch(Bucket, Payload) <- numbered(Ordinal, Payload), Bucket := (Ordinal - 1) / 3)
  ])).
