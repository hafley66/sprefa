# Ghcacher config-feeder golden

`6_gate.sh` compiles the text fixture through the current Prolog-to-TypeScript
compiler, replays one hermetic JSON schedule through both the Prolog oracle and
the emitted SQLite runtime, and byte-diffs both exact tick logs and the final
relation envelope against the checked-in goldens.

The config search order is DATA: candidate-path rows (`config_candidate`), a
host confirms which exist (`config_present`), `min/1` over the rank column picks
the winner (`best_rank`), `chosen_config` is the path at that rank. The host
response rows arrive through the same schedule seam as the clock golden; no
shell, network, or wall clock participates in the grade.

Defaulting of the search order is just defaulting of rows: four candidates
`(rank, path)` below map to the README's four levels. Reordering is reordering
rows; adding a fifth location is adding a row.

Surface adaptation forced by the compiler registry: the plan's
`path_exists(config_path, bucket)` and `read_org_config(config_path, bucket)`
are spelled as the registered `repos` (`org` identity, `bucket` freshness) and
`answer` (`name` identity, `bucket` freshness) hosts, because unregistered `sh`
names cannot carry a freshness salt. The compute rules and rel signatures are
the plan's verbatim.

| tick | committed batch | graded boundary result |
|---:|---|---|
| 1 | `interval(3600,1)`, four `config_candidate` rows (ranks 1-4) | existence demands for all four appear |
| 2 | four `__host_response_repos` (all exist=1) | `config_present` all four, `best_rank`=1, `chosen_config`=rank-1 path `flag.toml` |
| 3 | rank-1 org read | `want_org` appears |
| 4 | `interval(3600,2)` via the keyed `current_bucket` latch | the rank-1 present row retracts, `chosen_config` clears, fresh existence demands at bucket 2 |
| 5 | four `__host_response_repos` (rank-1 absent) | `config_present` ranks 2-4 only, `best_rank`=2, `chosen_config` MOVES to rank-2 path `env.toml`, tick-visible |
| 6 | rank-2 org read | `want_org` reappears for `env.toml` |
| 7 | `interval(3600,3)` | present rows retract, `chosen_config` clears |
| 8 | four `__host_response_repos` (none exist) | zero present, zero `chosen_config`, zero `best_rank`, zero `want_org`, no error |

Expected behaviors exercised: (1) all four present -> rank-1 path chosen; (2)
rank-1 absent -> falls to rank 2, and the move retracts/admits across ticks so
it is visible in the deltas; (3) no candidate present -> empty, zero rows, no
error; (4) tick logs byte-identical between oracle and emitted runtime.

Run from the repository root:

```bash
bash v6/tsv2/goldens/ghcacher_config_golden/6_gate.sh
```

Success is:

```text
GHCACHER_CONFIG_GOLDEN_HOLDS ticks=8 final=1
```
