ported: cascade.rs, reconcile.rs, sqlite_exp.rs, reach_dred.rs, reach_exp.rs, reach_inc.rs, sqlmem.rs
unported: none

`cargo build`
```
Finished `dev` profile [unoptimized + debuginfo]
```
`cargo build --features with-dd,with-salsa`
```
Finished `dev` profile [unoptimized + debuginfo]
```
`cargo run --bin 0_unified`
```
UNIFIED-REPORT.md written; 8/8 reported cells correct=true
```
`rg -l rusqlite v6/labkit/`
```
v6/labkit/Cargo.toml
v6/labkit/EXPERIMENT-G4V2-UNIFY.md
v6/labkit/examples/ram_wall.rs
```
