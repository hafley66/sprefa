# DL6 userland and binding integration

## Reading order

1. [Catalog source](../applications/dl6/0_catalog.dl7): `Interned` constructor,
   authored products, dictionary selections, policies and companion emitter.
2. [Storage rules](../emitters/2_interned_storage.dl7): existing graph-derived
   layout; the only change excludes plain-selected fields from scalar fallback.
3. [Demo](../applications/dl6/1_demo.pl): public compilation and artifact APIs,
   full runtime input, diagnostics on stderr and nonzero failure status.
4. [Binding lowerer](../src/2_comptime/0_lowerer.pl): compound expression targets
   and shared partial-call rules with constant or computed labels.
5. Tests [16](../test/16_storage_plain_field.test.pl),
   [17](../test/17_dl6_interned.test.pl),
   [18](../test/18_binding_symmetry.test.pl): exact layouts, identity and origins.

## Authored input and emitted output

```lisp
(: WrappedTextSelection
   (* (: capability "scalar-dictionary-v1")
      (: scalar (Interned text))
      (: dictionary SharedTextDictionary)))
```

| SelectivePolicy field | Representation | Storage domain |
| --- | --- | --- |
| SourceFile.path: Interned(text) | dictionary-local-id | SharedTextDictionary |
| SourceFile.language: text | scalar | text |
| Symbol.name: Interned(text) | dictionary-local-id | SharedTextDictionary |
| Symbol.file: SourceFile | reference-local-id | SourceFile |

The application reuses the existing scalar-dictionary selection. Its logical
target remains the wrapper identity. A companion artifact joins that identity
to its underlying type. Forward plus intern-snapshot replay rules produce this
ordinary relation row while the snapshot is present. Full and compiler-closed
storage artifacts are asserted equal; the demo uses the full public API.

The shared-dictionary policy selects both primitive text and its wrapper into
one authored dictionary. The test asserts one dictionary identity row. This
describes dictionary identity; no runtime values are allocated or persisted.

## Binding behavior

```lisp
(field : (Option text))
((Key "field" KeyOptions) : (Option text))
```

Both target the same canonical application. Compound labels formerly rejected
application targets. Both label forms now use expression lowering and shared
Curry rules for partial targets. The last partial binding rule carries the
computed label's goals and their source origins; ordinary labels retain an empty
body and their existing origins. Colon syntax is unchanged.

Compound deferral requires a nonempty diagnostic list containing only deferrable
diagnostics. Mixed diagnostics surface a non-deferrable target error. This is
tested through the existing `lower_datalog_deferred/5` entrypoint. The worker
did not reproduce the deferral branch through a complete `compile_dl7` file
invocation; generated-callable file tests establish a separate behavior.

## Verification

Run from the repository root, matching the new CI job:

```sh
swipl -q -g "load_files(['v7/test/2_module_system.test.pl','v7/test/15_interned_storage.test.pl','v7/test/16_storage_plain_field.test.pl','v7/test/17_dl6_interned.test.pl','v7/test/18_binding_symmetry.test.pl']),(run_tests -> halt(0); halt(1))"
just --justfile v7/justfile dl6-demo
```

Integrated execution on September 10: 22/22 tests passed, process exit 0.
The demo exited 0 and printed 16 layout rows plus the Interned(text) mapping.

The new GitHub Actions job runs those tests in the official SWI-Prolog 10.0.2
linux/amd64 container, pinned by manifest digest. Its initial Ubuntu package
setup failed because `library(tableutil)` was absent. The container matches the
locally tested version and explicitly checks this import before running tests.
It performs no Rust or extractor build. Remote execution after the pin is pending.

The separate release plan job failed to read the `hafley-observe` dependency
manifest under `hafley-rs`. This checkpoint does not change release dependencies.

The binding worker also ran the source-query suite: two tests passed and one
failed because its required `v6/sprefa-extract/target/debug/extract` binary was
absent. No source-query execution success is claimed for this PR.

## Remaining contracts and consumers

| Area | State |
| --- | --- |
| Selective typed storage layout | Implemented in userland |
| Compound-label application and partial targets | Implemented in lowerer |
| Named expression aliases | Approved follow-up; absent from this checkpoint |
| Nearest binding and transitive reference lookup | Approved follow-up; absent from this checkpoint |
| Authored Key metadata enforcement | Unchanged; metadata does not enforce keys |
| Option representation | Unchanged |
| Multiple-output expressions and modes | Discussion only |
| Runtime value dictionary allocation, reopening, batching | Not implemented by this application |
| SQLite IVM, DD and Rust artifact consumers | Separate integration work |

No new kernel relation, hosted facility, Rust adapter or storage representation
was added. Historical review reports remain in their original worktrees; their
early claims about broken full-runtime artifacts and mandatory wrapper-specific
classifiers were superseded by the executable application tests.
