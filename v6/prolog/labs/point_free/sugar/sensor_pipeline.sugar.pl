% corpus 10 sugar. M3: the two seam rels stop being named, declared and read.
% `~>` is the lab's stand-in glyph for `|>` (see expand.pl's header).
%
%   alert(Sensor, Value) <-
%       reading(Sensor, Raw), Doubled := Raw * 2
%    |> Shifted := Doubled + 10
%    |> Shifted > 50, Value := Shifted.
%
% rx lowering:
%   reading$.pipe(map(r => r.raw * 2), map(v => v + 10), filter(v => v > 50))
%
% The carry set at each cut is computed, not written: cut 1 carries
% (Sensor, Doubled) because both are read later; `Raw` is not, so it stops
% there.
sugar(prog(
  [ col_type(reading/2, sensor, text),
    col_type(reading/2, raw, int),
    col_type(alert/2, sensor, text),
    col_type(alert/2, value, int)
  ],
  [ (alert(Sensor, Value) <-
       ( reading(Sensor, Raw), Doubled := Raw * 2 )
     ~> ( Shifted := Doubled + 10 )
     ~> ( Shifted > 50, Value := Shifted ))
  ])).
