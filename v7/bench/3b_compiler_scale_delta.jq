def mean($values):
  ($values | add) / ($values | length);

def phase_mean($runs; $phase; $field):
  mean([$runs[].phases[] | select(.phase == $phase) | .[$field]]);

def aggregate_runs:
  . as $runs
  | ($runs[0]) as $first
  | ([$runs[].phases[].phase] | unique) as $phase_names
  | {
      files: $first.files,
      types_per_file: $first.types_per_file,
      fields_per_type: $first.fields_per_type,
      total_types: $first.total_types,
      total_fields: $first.total_fields,
      source_bytes: $first.source_bytes,
      repetitions: ($runs | length),
      wall_ms: mean([$runs[].wall_ms]),
      inferences: mean([$runs[].inferences]),
      compiler_rows: mean([$runs[].compiler_rows]),
      phases: [
        $phase_names[] as $phase
        | {
            phase: $phase,
            wall_ms: phase_mean($runs; $phase; "wall_ms"),
            inferences: phase_mean($runs; $phase; "inferences")
          }
      ]
    };

def phase_value($measurement; $phase; $field):
  first($measurement.phases[] | select(.phase == $phase) | .[$field]) // 0;

def ratio($after; $before):
  if $before == 0 then null else $after / $before end;

def percent($part; $whole):
  if $whole == 0 then null else ($part * 10000 / $whole | round) / 100 end;

sort_by(.files)
| group_by(.files)
| map(aggregate_runs)
| . as $measurements
| range(1; $measurements | length) as $index
| $measurements[$index - 1] as $before
| $measurements[$index] as $after
| ($after.wall_ms - $before.wall_ms) as $wall_delta
| ($after.inferences - $before.inferences) as $inference_delta
| ([($before.phases[].phase), ($after.phases[].phase)] | unique) as $phase_names
| ([
    $phase_names[] as $phase
    | (phase_value($after; $phase; "inferences")
       - phase_value($before; $phase; "inferences")) as $phase_inference_delta
    | (phase_value($after; $phase; "wall_ms")
       - phase_value($before; $phase; "wall_ms")) as $phase_wall_delta
    | {
        phase: $phase,
        inference_delta: $phase_inference_delta,
        inference_share_pct: percent($phase_inference_delta; $inference_delta),
        wall_delta_ms: $phase_wall_delta,
        wall_share_pct: percent($phase_wall_delta; $wall_delta)
      }
  ]) as $phase_pressure
| ($phase_pressure | map(.inference_delta) | add) as $attributed_inferences
| ($phase_pressure | map(.wall_delta_ms) | add) as $attributed_wall
| ({
    phase: "outside_traced_phases",
    inference_delta: ($inference_delta - $attributed_inferences),
    inference_share_pct:
      percent(($inference_delta - $attributed_inferences); $inference_delta),
    wall_delta_ms: ($wall_delta - $attributed_wall),
    wall_share_pct: percent(($wall_delta - $attributed_wall); $wall_delta)
  }) as $outside_pressure
| {
    kind: "differential",
    from_files: $before.files,
    to_files: $after.files,
    workload_ratio: ratio($after.total_fields; $before.total_fields),
    wall: {
      before_ms: $before.wall_ms,
      after_ms: $after.wall_ms,
      delta_ms: $wall_delta,
      ratio: ratio($after.wall_ms; $before.wall_ms)
    },
    inference: {
      before: $before.inferences,
      after: $after.inferences,
      delta: $inference_delta,
      ratio: ratio($after.inferences; $before.inferences),
      marginal_per_added_field:
        ($inference_delta / ($after.total_fields - $before.total_fields))
    },
    rows: {
      before: $before.compiler_rows,
      after: $after.compiler_rows,
      delta: ($after.compiler_rows - $before.compiler_rows),
      ratio: ratio($after.compiler_rows; $before.compiler_rows)
    },
    pressure_by_phase:
      (($phase_pressure + [$outside_pressure])
       | sort_by(-.inference_delta))
  }
