# Module storage runtime proof

`0_module_storage_runtime.dl6` is compiled through the real Prolog compiler by
both runtime tests:

```text
v6/tsv2/tests/moduleStorageRuntime.test.ts
v6/sprefa-engine-rs/tests/module_storage_runtime.rs
```

The entry module imports `a/model.dl6` and `b/model.dl6`, which have the same
basename and distinct relation names. The entry module also declares `Person`
and `person`, exercising SQLite's case-folded collision suffix, plus a direct
`source` fact and the `First -> imported -> derived` rule chain.

Expected physical names:

```text
a_model_First
b_model_Second
0_module_storage_runtime_Person
0_module_storage_runtime_person_2
0_module_storage_runtime_source
0_module_storage_runtime_imported
0_module_storage_runtime_derived
```

The TypeScript test imports compiler output from `gen_emitted/` temporarily.
The Rust test parses temporary `emit_rust.pl` output into `ProgramJson`.
