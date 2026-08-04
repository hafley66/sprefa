# Ghcacher env / config-as-json golden

`6_gate.sh` compiles the text fixture through the current Prolog-to-TypeScript
compiler, replays one hermetic JSON schedule through both the Prolog oracle and
the emitted SQLite runtime, and byte-diffs both exact tick logs and final
relations against the checked-in goldens.

The schedule uses the host-response seam. It does not run `printenv`, `python3`,
`pwd`, `git`, a shell host, wall time, or the network. The graded behavior is:

1. env override present, rank 0 wins: a `GHCACHE_CONFIG` env answer becomes the
   rank-0 candidate and `chosen_config` selects it over every file candidate.
2. env override absent, falls to the file candidates: with an unset var (no
   `env_var` answer for the new bucket) the rank-0 row disappears and the lowest
   file rank wins.
3. the decoded nested value lands as a typed column: `decode` over the json `doc`
   binds `global.db_path` into `db_path(path: text)`.
4. tick logs are byte-identical oracle vs emitted, both doors.

The winner's path is fed to the `toml_json` host, whose response `doc` is the
json decode plane's input. Absent semantics: `printenv '{name}' || true` prints
nothing for an unset var, which is zero response rows, exactly the "absent"
meaning rule 2 relies on; `git_toplevel`'s not-a-repo case is the same zero-row
shape (registered, not exercised here).

The schedule carries the json `doc` as canonical json text. The oracle door
host-expands the program (generating the response rel) before reading the
schedule so the response `doc` column resolves to `json` and the seam injects a
real obj term that `decode` can match; the emitted door parses the same text
into its json value.

| tick | committed batch | graded boundary result |
|---:|---|---|
| 1 | `interval(3600,1)`, `env_lookup(GHCACHE_CONFIG)` | bucket-1 env demand appears; no file candidates yet |
| 2 | env answer `GHCACHE_CONFIG=/env/config.toml` | rank-0 override wins: `chosen_config(/env/config.toml)`; toml demand switches to it |
| 3 | toml answer for `/env/config.toml` | `config_doc` fed, `decode` binds `db_path(/env/db.sqlite)` |
| 4 | `interval(3600,2)` + file candidates `flag.toml`, `~/user.toml` | env var unset at bucket 2: override gone, `chosen_config(flag.toml)` |
| 5 | toml answer for `flag.toml` | `db_path(/repo/db.sqlite)` |

The tilde candidate `~/user.toml` is spelled with a leading `~/`; the `~`
resolves to `$HOME` inside the path-taking host body (`toml_json`), never as a
language construct, and here the seam injects the doc directly.

## Fail-first receipt

Graded expectation 3 (the decoded value) was broken in a scratch copy by
repointing the second toml answer's doc at a wrong path. The gate's first diff
went red with the expected value instead of a pass:

```diff
--- /dev/fd/63
+++ /Users/chrishafley/projects/sprefa-lab-ghenv/v6/tsv2/.ghcacher-env-golden.NjZl6u/oracle.jsonl
@@ -2,5 +2,5 @@
-{"tick":5,"deltas":{"__host_response_toml_json":{"add":[["witness|toml_json|config_path:text=flag.toml|bucket=2",0,"flag.toml",{"global":{"db_path":"/repo/db.sqlite"}}]],"del":[]},"config_doc":{"add":[[{"global":{"db_path":"/repo/db.sqlite"}}]],"del":[]},"db_path":{"add":[["/repo/db.sqlite"]],"del":[]}}}
+{"tick":5,"deltas":{"__host_response_toml_json":{"add":[["witness|toml_json|config_path:text=flag.toml|bucket=2",0,"flag.toml",{"global":{"db_path":"/WRONG/db.sqlite"}}]],"del":[]},"config_doc":{"add":[[{"global":{"db_path":"/WRONG/db.sqlite"}}]],"del":[]},"db_path":{"add":[["/WRONG/db.sqlite"]],"del":[]}}}
```

The scratch copy was removed; the checked-in golden restores the `/repo/db.sqlite`
value and passes.

Run from the repository root:

```bash
bash v6/tsv2/goldens/ghcacher_env_golden/6_gate.sh
```

Success is:

```text
GHCACHER_ENV_GOLDEN_HOLDS ticks=5 final=1
```
