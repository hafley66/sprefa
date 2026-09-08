---
created: 2026-09-08
updated: 2026-09-08
type: feature
status: open
priority: normal
labels:
- pkg:extract
- size:med
---

# extract --reach ENTRY --sink NAME: call-path reachability with loop flags, native

## Description

## Ask

`extract --reach ENTRY --sink NAME [--via-loop]`: from a named entrypoint, stream every call path over `resolved_edge` that reaches a function containing a `site callee=NAME`, flagging edges whose call site is a `df_nest` row. Today the facts exist (`resolved_edge`, `df_nest`, `cfg_scope`, `site`) but the join lives outside the binary; the caller writes python.

## Receipt

hafley-rs `boop-store/src/_0_session_graph.rs:343-345`: `for lane in lanes { store.query_trace_events(Some(&lane), ..) }` prepares a 9-join statement 5,212 times per graph read (one per session + shell). 55% of a 7 s request under `sample`. The attached `loop_reach.py` finds it in one run:

```
load_agent_session_graph_with_runtime -> load_agent_session_graph -> query_trace_events -> query_trace_events [LOOP] -> prepare()
load_agent_session_graph_with_runtime -> query_trace_events -> query_trace_events [LOOP] -> prepare()
```

Cost: three extract passes (`--family call,df` per file, `--resolve --family call` over the set), 60 lines of join.

## Shape

- Input: `--reach ENTRY` (fn name, optional `path::name`), `--sink NAME` (callee name at a site), `--via-loop` (require at least one `df_nest` edge on the path), `--skip-cfg test` (drop edges from `cfg_scope` spans).
- Output: one `reach_path` record per distinct path: `path=[{file, fn, in_loop}]`, `sink_site={file, span}`.
- Cycle guard: a fn appears once per path.

## Acceptance Criteria

- [ ] `extract --resolve --reach load_agent_session_graph_with_runtime --sink prepare --via-loop *.rs` in boop-store/src emits the two paths above and nothing from `cfg(test)` modules
- [ ] `--sink execute` on the same entry emits zero rows
- [ ] `--via-loop` omitted emits every path, loop edges still flagged

## Tests Run

- [ ] fixture: three files, one loop, one adaptor-free direct call, one cfg(test) loop
