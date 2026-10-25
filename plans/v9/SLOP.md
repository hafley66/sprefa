# v9 slop log

Slop accepted at review: file, what, why accepted.

| punch | file | what | why accepted |
| --- | --- | --- | --- |
| P1 | `v9/src/_3_plan.rs` `check` | `Plan::hash` is `id_of(format!("{program:?}"))`; derived `Debug` text could shift across rustc versions and make a reopen report `Mismatch` | one impl, local use; replace with a canonical encoder when plans persist across toolchains |
| P1 | `v9/src/_3_plan.rs` | a Derived rel with no rules gets an empty stratum | harmless to the engine; filter when a reader minds |
| P1 | `v9/src/skeleton/` | walking-skeleton modules kept under `skeleton::` until P6 ports reach onto Core | removed in P6 |
