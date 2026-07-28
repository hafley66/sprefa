# SQLite UDF graft lab

The executable lab checks the v5 registration inventory, the hermetic Rust
capture, the Node driver probes, the graft parity receipt, and the existing
Prolog conformance suite.

Run:

```text
swipl -q -l sqlite_udf_lab.pl -g go -g halt
```

The v5 capture uses `SPREFA_CONFIG=/nonexistent/x.toml`, `DL_NO_DAEMON=1`, and
the scratch database in this directory. The bare rusqlite connection and the
v5-opened connection are both recorded because custom registration changes
the function inventory.

The named `node-sqlite3` slot is represented by the npm package `sqlite3`.
Its native binding did not load under Node 24.15.0 on darwin arm64, so the
receipt records the load failure and does not claim a UDF result for that
driver.

The compile roundtrip script writes `v6/prolog/compile/dl_view/`, which is
outside this lab's write fence. The lab therefore runs the read-only
conformance runner and records the roundtrip limitation in the verdict.
