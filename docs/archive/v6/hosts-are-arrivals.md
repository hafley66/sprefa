# Hosts are arrivals

| declaration or row | execution |
| --- | --- |
| `sh` | `shell`, with `sh -c` over its validated template |
| sidecar `sprefa_extract` row | argv process for the demand relation |
| sidecar `soopy` row | Soopy mutation adapter |
| sidecar `boop` row | `boop host oneshot`, JSON on stdin |

`IProcessAdapter` has `name`, `applicative`, `command(demand)`, and
`decode(stdout, plan)`. `command` returns `{ argv, env, stdin? }`. Only the
shell adapter reads `plan.template`.

`<program>.adapters.json` is an array of rows:

```json
[{"adapter":"sprefa_extract","demand_rel":"extract_ask","response_rel":"extract"}]
```

Before the sidecar, a matching extractor template selected the extractor path
and its frontier fold. After the sidecar, the same fold comes from the adapter
row; every `sh` plan emits `execution: shell`.
