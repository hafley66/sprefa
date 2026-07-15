# Reactivity micro-probe

This is deliberately small. It generates 10, 100, and 1,000 Rust files under
`target/reactivity/probe`, then drives the prebuilt Rust example directly.

The probe records cold, unchanged, one-file edit, and fresh rebuild time plus
physical parse counts and a canonical call-graph digest. It fails if the edit
silently widens to a full tick or differs from the rebuild.

```text
just perf-reactivity-build
just perf-reactivity
```

The run command never builds, invokes `dl`, starts a daemon, or writes outside
this repository. Remove `target/reactivity/probe` explicitly before rerunning.
