# Storage-selection probe: an application target in the selection's scalar field

Lane `chore-dl6-integration-review-20260910`. Read-only over the worktree except
this file. Every source file the probe compiled lives under `/tmp/dl6probe2`.
No worktree source was authored and the emitter copy is byte-identical to the
shipped one.

## Contents

- [Question](#question)
- [What ran](#what-ran)
- [Result: all six storage artifacts](#result-all-six-storage-artifacts)
- [The two field rows, in full](#the-two-field-rows-in-full)
- [Companion artifact: measured empty](#companion-artifact-measured-empty)
- [Incidental measurement: probe sources must live under the project root](#incidental-measurement-probe-sources-must-live-under-the-project-root)
- [What this probe does not say](#what-this-probe-does-not-say)

## Question

Can a field annotated `(Interned text)` reach `"dictionary-local-id"` with the
**unmodified** emitter, no wrapper classifier branch, no kernel change, no new
relation schema, no `intern_snapshot` at the consumer?

Mechanism under test: `storage_policy_dictionary` at
`v7/emitters/2_interned_storage.dl7:106-111` binds `?Scalar` from
`(edge_snapshot ?Selection ?ScalarLabel ?Scalar 1)` and never constrains that
target to a primitive. Point the selection's `scalar` field at the application
instead of at `text`, and the existing dictionary rule at `:162-168` matches the
wrapped field directly.

## What ran

| item | value |
|---|---|
| driver | `/tmp/dl6probe2/probe.pl` |
| command | `timeout 300 swipl -q -s /tmp/dl6probe2/probe.pl -g go -t halt` |
| exit code | `rc=0`, terminal marker `PROBE_DONE` |
| raw output | `/tmp/dl6probe2/run2.out` |
| project root | `/tmp/dl6probe2` |
| sources | `/tmp/dl6probe2/emitters/2_interned_storage.dl7`, `/tmp/dl6probe2/application/probe.dl7` |
| emitter sha256 | `b8bfb858a897205bb928d9b8857503f95c82b5254b29ed5a6c040fa4f1cba2d4`, equal to `v7/emitters/2_interned_storage.dl7` |
| runtime slice | `compiler_closed_runtime/2`, copied verbatim from `v7/test/15_interned_storage.test.pl:138-140` |
| `compile_diagnostics` | `[]` |
| `storage_emit_diagnostics` | `[]` |
| `companion_emit_diagnostics` | `[]` |

Application source, one file, dictionary identity authored, infix colon only:

```lisp
(Interned : (* (source : type)
               (return : type)))

(<- (Interned ?Source ?Result)
    (nil ?Empty)
    (cons ?Source ?Empty ?Arguments)
    (intern Interned ?Arguments ?Result))

(Owner : (* (wrapped : (Interned text))
            (plain : text)))

(SharedTextDictionary : (*))

(OwnerStorageRoot : (* (capability : "storage-root-v1")
                       (target : Owner)))

(DictionarySelection : (* (capability : "scalar-dictionary-v1")
                          (scalar : (Interned text))
                          (dictionary : SharedTextDictionary)))

(ProbePolicy : (* (capability : "interned-storage-policy-v1")
                  (root : OwnerStorageRoot)
                  (dictionary : DictionarySelection)))

(InternedCompanionEmitter : (*))

(emits InternedCompanionEmitter "interned" Interned)
```

Short names used below for the reader. Each is one `owner(file(..),
expansion_node(..))` term in `run2.out`.

| short name | reader node in `application/probe.dl7` |
|---|---|
| `Interned` | `expansion_node(reader_node(..,0),infix_colon,1,3)` |
| `Owner` | `expansion_node(reader_node(..,32),infix_colon,1,3)` |
| `Dict` | `expansion_node(reader_node(..,47),infix_colon,1,3)`, the `SharedTextDictionary` node |
| `Policy` | `expansion_node(reader_node(..,84),infix_colon,1,3)`, the `ProbePolicy` node |
| `App(Interned,[text])` | `application(Interned, [primitive(text)])` |

The two edge sets the whole result rests on, verbatim in short form:

```
owner_edges     = [edge(0, wrapped, ref(App(Interned,[text]))),
                   edge(1, plain,   ref(primitive(text)))]

selection_edges = [edge(0, capability, const("scalar-dictionary-v1")),
                   edge(1, scalar,     ref(App(Interned,[text]))),
                   edge(2, dictionary, ref(Dict))]
```

An application term sits in the selection's `scalar` slot with zero
diagnostics. The lowerer accepts it because the label is an ATOM and atom
labels route through `lower_bind/5` and its `expression_bind_target/1` branch
at `v7/src/2_comptime/0_lowerer.pl:425-429`.

## Result: all six storage artifacts

Full artifact API from `emit_compiled(dl7(InternedStorageEmitter), ..)`.
Every count is exact and measured, not inferred.

| artifact | count | rows |
|---|---|---|
| `types` | 1 | `[Policy, Owner, "product"]` |
| `fields` | 2 | one per field, listed below |
| `dependencies` | 1 | `[Policy, Owner, Dict, wrapped, 0, "dictionary"]` |
| `projections` | 2 | the two field rows re-ordered by `storage_projection` |
| `dictionaries` | 1 | `[Policy, App(Interned,[text]), Dict]` |
| `identities` | 2 | `[Policy, Owner, "structural", "catalog-local-intern-id", "constructor-and-ordered-fields"]` and `[Policy, Dict, "dictionary", "catalog-local-dictionary-id", "scalar-content"]` |

`identities` measured at 2, not more. One scalar maps to one dictionary in this
probe, so no duplicate-row question arises here; a two-scalar selection set is
NOT measured by this probe and stays open.

## The two field rows, in full

```
[Policy, Owner, wrapped, 0, App(Interned,[text]), "dictionary-local-id", Dict]
[Policy, Owner, plain,   1, primitive(text),      "scalar", primitive(text)]
```

Exactly one row per field. Both expectations hold:

| field | expected | measured |
|---|---|---|
| `wrapped : (Interned text)` | `dictionary-local-id` -> `SharedTextDictionary` | matches |
| `plain : text` | ordinary `scalar`, domain `primitive(text)` | matches |

No double-match. The scalar fallback at
`v7/emitters/2_interned_storage.dl7:170-176` is blocked for `wrapped` by its own
`(not (storage_dictionary_target ?Policy ?Target))` guard, because
`App(Interned,[text])` IS the policy's dictionary target. `plain` reaches the
fallback because `primitive(text)` is no longer a dictionary target in this
program.

The `dictionaries` row carries `App(Interned,[text])` as the scalar column. A
consumer reading that row learns the storage domain but not the underlying
primitive; that join is what the companion artifact was meant to supply.

### What this replaces from receipt 1

Receipt 1 F1 reported `(Interned text)` landing on `"scalar"` with an opaque
application storage domain. That measurement was taken against a fixture whose
selection pointed `scalar` at `text`. Moving the selection target is enough;
F1's finding is a property of that fixture's selection, not of the emitter.

Receipt 1 F2 predicted two rows for one field once a wrapper is honoured. This
route produces one row, because it adds no rule and so adds no second match.

## Companion artifact: empty without the mirror, exact with it

The companion needs the `intern_snapshot` replay clause that every prelude
constructor already carries. Nothing else. No `node` rule, no `product` rule,
no new relation.

### The A/B, one variable

Both runs compile the same emitter and the same application, except that the
second adds the replay clause copied goal-for-goal from the `Option` mirror at
`v7/prelude/2_constructor_rules.dl7:17-20`:

```lisp
(<- (Interned ?Source ?Result)
    (intern_snapshot Interned ?Arguments ?Result)
    (nil ?Empty)
    (cons ?Source ?Empty ?Arguments))
```

| measurement | one rule only | plus mirror clause |
|---|---|---|
| driver | `/tmp/dl6probe2/probe.pl` | `/tmp/dl6probe2/probe4.pl` |
| source | `application/probe.dl7` | `application/probe_mirror.dl7` |
| raw output | `/tmp/dl6probe2/run2.out` | `/tmp/dl6probe2/run4.out` |
| exit code | `rc=0`, `PROBE_DONE` | `rc=0`, `PROBE4_DONE` |
| `compile_diagnostics` | `[]` | `[]` |
| compiler rows carrying relation `Interned` | not captured | **1** |
| runtime rules heading `Interned` | not captured | **2** |
| `Interned` declared in runtime | not captured | yes |
| companion artifact `interned`, closed slice | **0** | **1** |
| companion artifact `interned`, full runtime | not captured | **1** |

The single companion row, identical under both the `compiler_closed_runtime/2`
slice and the unsliced `RuntimeProgram`:

```
[ref(primitive(text)), ref(application(Interned, [primitive(text)]))]
```

That is the join a consumer needs: wrapped storage domain on the right, the
underlying primitive on the left. It arrives as ordinary `emits` rows, with no
`intern_snapshot` goal at the consumer and no new relation schema.

### Why one rule was not enough

The constructor rule ends in `(intern Interned ?Arguments ?Result)`. It answers
demand: an authored `(Interned text)` target gets an identity. It does not
publish an ordinary `Interned` row for the closure to read back. The replay
clause reads the interning that already happened through `intern_snapshot` and
republishes it as an ordinary row, which is what `dl7_emitter_rows/4`
(`v7/src/3_emit/1_artifact_emitter.pl:118-136`) collects.

Necessity is measured, not argued: the replay clause is the only delta between
the two runs, and the companion count moves 0 -> 1.

### The mirror does not disturb the storage layout

Re-emitting all six storage artifacts from the mirror-carrying program returns
the same counts and the same rows as the run without it:

| artifact | without mirror | with mirror |
|---|---|---|
| `types` | 1 | 1 |
| `fields` | 2 | 2 |
| `dependencies` | 1 | 1 |
| `projections` | 2 | 2 |
| `dictionaries` | 1 | 1 |
| `identities` | 2 | 2 |

`storage_emit_diagnostics` stays `[]`. The closed slice is sufficient for both
the storage six and the companion; the full runtime was measured and adds
nothing, so a consumer has no reason to carry it.

## Incidental measurement: probe sources must live under the project root

First run, with `Root` set to the worktree `v7` and the application file in
`/tmp`:

```
compile_diagnostics =
  [diagnostic(module,
              filesystem('/tmp/dl6probe2/app.dl7'),
              outside_project_root(
                '<worktree>/v7', '/tmp/dl6probe2/app.dl7'))]
```

Throw site `v7/src/2_comptime/0b_filesystem_grapher.pl:69-77`, predicate
`outside_project_root/1` at `:110-112`. The prelude is unaffected:
`type_prelude_paths/1` at `v7/src/2_comptime/2_compiler.pl:307-316` resolves
from the compiler module's own source path, so a scratch root outside the repo
still loads the real prelude. Any out-of-repo probe must mirror both source
files under the scratch root.

## What this probe does not say

- Nothing about `(Interned SomeProduct)`. `storage_reachable` at
  `v7/emitters/2_interned_storage.dl7:131-141` still requires `(product ?Child)`
  on the raw edge target; receipt 1's C4 stands unmeasured here.
- Nothing about two selections in one policy, so no duplicate-row measurement
  for `identities` or `dictionaries`.
- Nothing about `(Key ..)` compound labels or expression-alias references. This
  probe used atom labels only, by design.
- Nothing about test 15's shipped counts. `22/48/41/48/4/26` were not re-run in
  this lane.
- Nothing about whether this selection shape is the shape the implementation
  lane should ship. It is a measurement that the existing emitter already
  admits it.
