% Q3 probe: a stage that JOINS a second source. The header asks whether this
% breaks `|>`; the answer this file measures is that in a LEVEL rule it does
% not -- the minted rel just carries the columns the join bound and that are
% read later.
%
%   enriched(Sensor, Label, Value) <-
%       reading(Sensor, Raw), Doubled := Raw * 2
%    |> label(Sensor, Label)
%    |> Value := Doubled + 1.
%
% rx lowering: the second stage is `withLatestFrom(label$)` plus a filter on
% the key, which is the honest reading -- rx cannot write a JOIN inside pipe()
% either, and this is the closest operator.
sugar(prog(
  [ col_type(reading/2, sensor, text),
    col_type(reading/2, raw, int),
    col_type(label/2, sensor, text),
    col_type(label/2, label, text),
    col_type(enriched/3, sensor, text),
    col_type(enriched/3, label, text),
    col_type(enriched/3, value, int)
  ],
  [ (enriched(Sensor, Label, Value) <-
       ( reading(Sensor, Raw), Doubled := Raw * 2 )
     ~> ( label(Sensor, Label) )
     ~> ( Value := Doubled + 1 ))
  ])).
