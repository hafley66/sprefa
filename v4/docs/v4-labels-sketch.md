# Labels in Pipeline Position — Hold-Onto-It Sketch

Working title for the idea. Not a plan yet; just the shape pinned down
so you can put it down and pick it back up.

## The one primitive

```
label(:name, PIPE)
```

Declares a named arm in the current block. Atom `:name` becomes a
runtime-addressable handle into PIPE.

Sugar:

```
{ name: PIPE; }                     →    block(label(:name, PIPE))
{ a: P; b: Q; }                     →    block(label(:a, P), label(:b, Q))
```

Brace block desugars to `block(label, label, …)`. Brace is sugar for
`block`. `block` is a normal op. The grammar already lets you write
`{}`; this gives it runtime meaning.

## Two semantic knobs

Everything else falls out of how you set these.

### Knob 1: Block discriminator

How does an incoming cursor pick which arm(s) fire?

| Mode | Rule | Reads as |
| --- | --- | --- |
| **by-atom** | arm fires if `cursor.value` matches its atom name | match on symbol |
| **by-type** | arm fires if `cursor.CursorValue` variant matches its type op | match on type |
| **by-predicate** | first arm whose body's first op returns truthy | if/cond |
| **all-fire** | every arm receives the cursor | merge / fork |
| **by-name** | only fires when cursor was sent via `> :name` | select / dispatch |

Pick one as the default. The others become op variants:
`block_match(...)`, `block_fork(...)`, etc. The `>` operator's RHS atom
form (`cursor > :name`) is always available regardless of default.

### Knob 2: Arm lifetime

| Mode | Rule | Reads as |
| --- | --- | --- |
| **per-cursor** | arm runs for each cursor that arrives and exits | normal arm |
| **per-block-instance** | arm holds state across visits to this block instance | scan, accumulator, switchMap |

Mark stateful arms syntactically. Suggested: `!` suffix on the arm name
(matches the existing `NAME!` convention). Default per-cursor; `!` opts
into per-block-instance.

## Worked examples

### Conditional (by-atom)

```
get_status > {
  :ok: process_record;
  :err: log_and_skip;
}
```

`get_status` emits a cursor whose `value` is one of the atoms. The arm
whose name matches fires.

### Match on type

```
fetch > {
  :String: parse_text;
  :Number: passthrough;
  :default: error("unknown type");
}
```

Default arm `:default` catches the residual. Same shape as conditional;
the discriminator is the `CursorValue` variant tag.

### Recursion (lexical, terminating)

```
{
  walk: list_entries > {
    :file: yield;
    :dir:  > :walk;
  };
} > :walk
```

The `:dir` arm sends the cursor back into `:walk`. Lexical resolution:
`:walk` inside `:walk`'s body resolves to the same block instance.
Termination happens when `list_entries` produces only `:file` cursors.

### Loop

```
{
  step: check > {
    :continue: body > :step;
    :done:     yield;
  };
} > :step
```

Same shape as recursion. The fixed point is the `:done` arm with no
back-edge.

### Accumulation / scan

```
{
  acc!:  state(0);
  step:  add(${&}, read(:acc)) > write(:acc);
  total: read(:acc) > yield;
}

items     > :step    // for each input, update :acc
end_token > :total   // when done, emit accumulator
```

`acc!` is per-block-instance, so it survives across the visits that
`:step` makes. `:total` reads the final value.

### switchMap / cancel-on-new-input

```
{
  run!: do_long_work;
}

inputs > :run
```

`run!` is per-block-instance. When a new input arrives, the prior
`:run`'s output rows are owned by `(block_instance, :run, prior_input_key)`;
the new input lands with a fresh `input_key`, retracting the prior
owner's outputs through the support table. Cancellation is automatic.

This is the connection to the retraction plan — switchMap reduces to
"per-block-instance arm + owner-keyed retraction".

### Merge

```
sources > {
  :a: filter_a;
  :b: filter_b;
  :c: filter_c;
}_fork
```

`_fork` suffix → `block_fork(...)` variant — all arms fire, outputs
interleave. Without `_fork`, the default discriminator decides.

### Select

```
cursor > :a
```

Direct address. Works whether or not the block uses a discriminator,
because `> :name` is the name-dispatch primitive.

## What `>` means now

| Form | Meaning |
| --- | --- |
| `> next_op` | sequence |
| `> :name` | dispatch to named arm |
| `> {a: P; b: Q;}` | dispatch via block discriminator |
| `> :self_name` (inside arm) | recursion |

One operator. Four readings. No new operators; `>` already does "send
forward" and the receiver type fans the meaning.

## Open knobs

Decide these before implementation, in this order:

1. **Default discriminator** — by-atom or by-name? By-atom reads
   naturally as if/match; by-name forces explicit dispatch. Probably
   by-atom default, by-name explicit.
2. **State arm marker** — `!` suffix, `?` suffix, `state(...)` op-style,
   or a `let` keyword? `!` is consistent with the existing CAPS! /
   NAME! conventions.
3. **`:name` resolution scope** — lexical (innermost enclosing `block`
   that defines `:name`) vs dynamic (current cursor address). Lexical is
   the safe default. A `dyn(:name)` form can opt into dynamic.
4. **Output ordering when multiple arms fire** — enqueue-order
   interleave (per-arm rows interleaved as produced) vs per-arm-batched
   (drain arm A fully, then arm B). Interleave is simpler.
5. **Discriminator failure** — silent drop, `:default` arm required,
   diagnostic on no-match? Probably `:default` optional, diagnostic when
   absent and no arm matches.

## Why this fits sprefa today

- Cursor `Ref` is already the runtime addressing scheme; labels are a
  named handle on top of the same machinery.
- Brace blocks already parse as op bodies (`v4/crates/tree-sitter-sprefa/grammar.js`);
  this gives them runtime meaning without grammar churn.
- The retraction plan's `support`-count rule
  ([v4-effect-output-retraction-plan.md](v4-effect-output-retraction-plan.md))
  gives state arms and switchMap their cancellation primitive for free.
- Task 3 of the foundations plan
  ([v4-runtime-foundations-plan.md](v4-runtime-foundations-plan.md))
  plumbs `instance_id` onto `RenderCtx`; that's exactly the handle an
  arm uses to reference itself.

## Minimum implementation surface

1. **Parse** — brace block with `name: pipe;` arms (grammar already
   accepts; binding-graph needs to handle the new desugar).
2. **Lower** — `{n: P; m: Q;}` → `block(label(:n, P), label(:m, Q))`.
3. **Runtime** — `label(:name, PIPE)` op registers a handle on the
   parent block's scope.
4. **Dispatch** — `> :name` resolves to the registered handle and
   enqueues into that arm's pipe.
5. **State marker** — `!` suffix on arm name flips arm lifetime to
   per-block-instance.

Five touchpoints. The semantic knobs decide everything else, and they
are not architectural — they are syntactic defaults you can tune
without touching the runtime.

## Holding it down in one sentence

> A block is a bag of named arms; `>` sends a cursor into the bag; the
> bag's discriminator picks the arm(s); an arm marked `!` keeps state
> between visits; an arm names itself for recursion and the foundations
> plan + retraction plan already handle the runtime support.
