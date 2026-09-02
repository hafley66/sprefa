# Soopy calls, generics, and higher types

## Contents

1. Existing Soopy boundary
2. One callable model
3. Generics
4. Higher types and partial calls
5. Compile and runtime clocks
6. Logical and execution IR
7. Open implementation decisions

## 1. Existing Soopy boundary

The `soopy` crate in `~/projects/hafley-rs/crates/soopy` already owns typed
filesystem and Git coordinates plus typed operations. Its README records these
principal shapes:

```text
SourceQuery  -> SourceSnapshot
ReadRequest  -> SourceBytes
WatchQuery   -> RepositoryDelta stream
StageRequest -> sealed mutation stage
StageId      -> CommitReceipt
```

The stable identities include `RepositoryId`, `WorktreeId`, `RevisionId`,
`RepoPath`, `ContentId`, `SourceRef`, and `SourceSpan`. Soopy remains the Rust
implementation. DL7 exposes abstract `source.*` and `git.*` callables whose
host implementation is Soopy.

```text
DL7 name             Soopy implementation shape

source.snapshot      SourceQuery -> SourceSnapshot
source.read          ReadRequest -> SourceBytes
source.watch         WatchQuery -> RepositoryDelta*
git.resolve          repository + revision -> RevisionId
git.refs             ref query -> ref observations
source.stage         StageRequest -> sealed stage
source.commit        StageId -> CommitReceipt
```

This keeps target IR independent of the Rust crate name while retaining one
implementation in both compiler and runner.

## 2. One callable model

TSI describes the type of every callable:

```text
tsi.callable(Callee)
tsi.input(Callee, Position, InputType)
tsi.output(Callee, Position, OutputType)
```

Calling a type constructor produces a semantic result node:

```text
tsi.called(ResultType, CalleeType, ArgumentTypes)
```

Executing a program call uses a reified logical-IR call so asynchronous and
relational results have their own identity:

```text
call(CallId, Callee, Arguments)
call_result(CallId, Result)
```

`tsi.called/3` describes type construction. `call/3` describes one program
execution. Both use the same callee, ordered argument-list representation, and
TSI input/output checks.

A callee can have either implementation source:

```text
rules(Callee)                 DL7 rules derive its results
host(Callee, soopy)           Soopy derives its results
```

The type checker operates before that implementation choice matters.

## 3. Generics

A generic type constructor is a callable whose inputs and output are types.

```text
tsi.callable(option)
tsi.input(option, 0, type)
tsi.output(option, 0, type)
```

`Option(int)` becomes:

```text
tsi.called(option_int, option, [int])
```

Userland compiler rules define the result graph:

```text
tsi.sum(?Result) <-
    tsi.called(?Result, option, [?Element]).

tsi.edge(?SomeEdge, ?Result, some, ?Element, 1) <-
    tsi.called(?Result, option, [?Element]).
```

The result identity is interned from the callee and ordered argument IDs.
Repeated calls with the same closed arguments therefore name the same type.
The generic body remains ordinary DL7 rules over type facts.

## 4. Higher types and partial calls

A higher type accepts or returns another callable type. Its constraint is an
ordinary callable contract.

```text
tsi.callable(type_unary)
tsi.input(type_unary, 0, type)
tsi.output(type_unary, 0, type)

tsi.conforms(option, type_unary, option_type_unary_witness)
```

One constructor can then accept another constructor:

```text
tsi.callable(compose)
tsi.input(compose, 0, type_unary)
tsi.input(compose, 1, type_unary)
tsi.output(compose, 0, type_unary)
```

This represents the traditional higher-kinded shape `(Type -> Type) -> Type`
through normal callable types. Callable contracts carry the role traditionally
assigned to kinds.

A call supplying any proper subset of the input slots produces a canonical
partial callable. The remaining input rows are copied to the partial result,
and bound argument rows retain their original positions.

```text
tsi.called(option_composer, compose, [option])

tsi.callable(option_composer)
tsi.input(option_composer, 0, type_unary)
tsi.output(option_composer, 0, type_unary)
```

Supplying the final argument closes the call:

```text
tsi.called(list_of_optional, option_composer, [list])
```

Named argument and punning syntax can lower to the same ordered argument rows
after the callee's labeled input edges resolve positions.

## 5. Compile and runtime clocks

Soopy is available behind the same abstract callables in both clocks.

```text
COMPILE CLOCK

ground source/git call
        |
        v
generic SWI host predicate
        |
        v
Rust foreign-library adapter
        |
        v
Soopy read/resolve/snapshot
        |
        v
typed result facts enter the next comptime round


RUNTIME CLOCK

logical source/git call
        |
        v
execution IR host operation
        |
        v
Rust runner calls Soopy directly
        |
        v
typed result rows arrive on the next runtime tick
```

Compile-time reads are memoized by operation, ground arguments, Soopy version,
and source identities. Immutable Git operations carry revision and content
identity. Worktree reads carry the observed `WorktreeId`, revision observation,
path, and expected `ContentId` where available.

The first compiler host surface contains read-only operations: discovery,
snapshot, enumeration, reading, revision resolution, and ref observation.
Staging and commit remain runtime operations until compiler mutation effects
receive an explicit phase and approval contract.

## 6. Logical and execution IR

The logical IR carries abstract operation identity and typed arguments:

```text
host_call(CallId, source.read, Arguments, ResultRelation)
host_call(CallId, source.watch, Arguments, ResultRelation)
```

The execution IR adds clock and delivery mechanics:

```text
host_operation(
    CallId,
    Host,
    Operation,
    ArgumentLayout,
    ResultLayout,
    DeliveryClock
)
```

The SQLite physicalizer sees arrival relations produced by the operation. The
Rust runner routes `source.*` and `git.*` operations to Soopy. Another runner
can implement the same operation protocol.

## 7. Open implementation decisions

1. SWI bridge implementation: direct SWI foreign predicate in a Rust `cdylib`,
   or a small C FLI shim calling a Rust C ABI.
2. Argument and result transport across the SWI boundary: direct Prolog-term
   construction, a stable binary row codec, or both.
3. Partial-call argument representation for named, positional, and omitted
   slots.
4. Exact compiler-effect cache key for mutable worktree observations.
5. Whether `call_result/2` remains one relation for deterministic and
   nondeterministic calls or expands to result ordinal and support identity.
