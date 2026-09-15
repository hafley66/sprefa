# Effects

`effect` is a kernel relation that records the goals a program evaluated on a served relation (`src/_6_eval/_5_evaluate.rs:244-249`). A host process serves a relation, the program expresses interest by evaluating a goal on it, and the `effect` row is that interest, one row per evaluation. A loading rule reads the row back and names the arguments the program still needs from the outside.

## What the evaluator writes

The served set is a runtime input. `--serve a,b` on `dl8 eval` or `dl8 run` names the relations the outside settles, resolved against the program's declared names (`src/bin/dl8.rs:318-323`). An unknown name is the diagnostic `served_relation_unknown` and exit 1 (`src/_6_eval/_6_json.rs:345-366`). Without the flag the served set is empty.

On every evaluation of a goal, the evaluator takes these branches:

- A positive goal on a served relation that heads no rule writes one row `(effect Relation Application)`, whether or not a data row matched (`src/_6_eval/_5_evaluate.rs:244-249`, `:261-262`).
- A positive goal on a served relation that heads a rule writes no `effect` row (`_5_evaluate.rs:246`).
- A negative goal writes no `effect` row (`_5_evaluate.rs:175-180`).
- An aggregate body writes no `effect` row; aggregate mode matches stored rows only (`_5_evaluate.rs:147-149`).
- A served goal with nothing bound is a source: the evaluator writes the row with `none` at every unbound position, and no rule demanded anything.

`Application` is the `intern` of the relation over the goal's arguments in position order, the atom `none` at each unbound position (`_5_evaluate.rs:263-276`). The same interned row is written to `intern_snapshot`, so a rule can open the argument list (`:272-273`, `:826-829`).

The `effect` relation itself is a kernel relation, arity 2, keys `[0,1]` (`src/_3_check/_5_kernel.rs:29`, `:56`). A goal on `effect` reads the round's new rows like a current-stratum goal (`_5_evaluate.rs:341-360`).

## What contradicts the plans

Five statements in the plans disagree with the code. The code wins in every case (`plans/v8/2026-09-14-v8-effect-demand.brief.md:24`, `:25`, `:79`; `plans/v8/2026-09-14-v8-store.PLAN.md:906`):

| plan | code | winner |
|---|---|---|
| `plans/v8/2026-09-14-v8-effect-demand.brief.md:24`: a row only when no row matched | `_5_evaluate.rs:261-262`: every evaluation; the brief's own amendment at `:79` agrees | code |
| `effect-demand.brief.md:79`: every goal on a served relation | `_5_evaluate.rs:246`: not when the served relation heads a rule | code |
| `plans/v8/2026-09-14-v8-store.PLAN.md:906`: `(effect ?Host ?Key ?Rule)` | `_3_check/_5_kernel.rs:29`: arity 2 | code |
| `effect-demand.brief.md:25`: `effect` never enters `KERNEL_RELATIONS` | `_3_check/_5_kernel.rs:29`: it is there; amendment `:84` agrees | code |
| `src/bin/dl8.rs:73` help: "a miss on one writes an `effect` row" | `_5_evaluate.rs:261-262` | code |

## Why

Any relation is settable from outside. Nothing in the program marks a relation as hosted; the outside says what it serves, and loading, failed and ready are ordinary rules (`plans/v8/2026-09-14-v8-effect-demand.brief.md:11`, decision of 2026-09-14). The row is live interest: it exists while a rule evaluates that goal, and its argument list reads back with `intern_snapshot` then `cons` the way `Partial` does (`effect-demand.brief.md:76-81`, amendment 3 of 2026-09-14).

## When to use

Use `effect` when:

- a relation's rows come from a process outside the program: `--serve`, `fixtures/host_effect/0_pending.dl7`
- a program shows what is in flight: a loading rule, `fixtures/host_effect/3_loading.dl7:21-26`
- a stream with no request arguments: a source, `fixtures/host_effect/2_source.dl7`

Do not use `effect` when:

- the served relation also heads a rule: no `effect` row is written, `_5_evaluate.rs:246`
- interest should end when the reading rule stops: rows are never retracted, [Not built yet](../../../../16_not_built.md)
- the program only runs `dl8 eval`: nobody answers the row, use `dl8 run`, [Executors](../../../../11_executors.md)

## Example

A settled row feeds the reader like any fact; the miss stays for the other url. The program is `v8/fixtures/host_effect/1_settled.dl7`:

```dl7
; A settled row feeds the reader like any fact; the miss stays for the other url.
(: fetch_json
   (* (: url text)
      (: body text)))

(: Watch (* (: url text)))

(Watch "https://a")

(Watch "https://b")

(fetch_json "https://a" "hello")

(: Body
   (* (: url text)
      (: body text)))

(<- (Body ?Url ?Body)
    (Watch ?Url)
    (fetch_json ?Url ?Body))
```

```console
$ bash book/show.sh eval fixtures/host_effect/1_settled.dl7 --serve fetch_json
(effect fetch_json ref(application(fetch_json, ["https://a" none])))
(effect fetch_json ref(application(fetch_json, ["https://b" none])))
(fetch_json "https://a" "hello")
(Watch "https://a")
(Watch "https://b")
(Body "https://a" "hello")
exit 0
```

Both goals were evaluated, so both have an `effect` row; the fixture's comment says "the miss stays for the other url", which was the first design (`effect-demand.brief.md:47`) before amendment 3.

Loading as a rule. The program is `v8/fixtures/host_effect/3_loading.dl7`:

```dl7
; Loading is an ordinary rule over effect; the pattern opens with
; intern_snapshot then cons, the way Partial reads back in the prelude.
(: fetch_json
   (* (: url text)
      (: body text)))

(: Watch (* (: url text)))

(Watch "https://a")

(Watch "https://b")

(: Body
   (* (: url text)
      (: body text)))

(<- (Body ?Url ?Body)
    (Watch ?Url)
    (fetch_json ?Url ?Body))

(: Loading (* (: url text)))

(<- (Loading ?Url)
    (effect fetch_json ?App)
    (intern_snapshot fetch_json ?Args ?App)
    (cons ?Url ?_ ?Args))
```

```console
$ bash book/show.sh eval fixtures/host_effect/3_loading.dl7 --serve fetch_json
(effect fetch_json ref(application(fetch_json, ["https://a" none])))
(effect fetch_json ref(application(fetch_json, ["https://b" none])))
(Watch "https://a")
(Watch "https://b")
(Loading "https://a")
(Loading "https://b")
exit 0
```

A source, and the same program with nothing served. The program is `v8/fixtures/host_effect/2_source.dl7`:

```dl7
; Nothing is bound, so the whole pattern is free: a source.
(: tick (* (: at int)))

(: Seen (* (: at int)))

(<- (Seen ?At)
    (tick ?At))
```

```console
$ bash book/show.sh eval fixtures/host_effect/2_source.dl7 --serve tick
(effect tick ref(application(tick, [none])))
exit 0
```

```console
$ bash book/show.sh eval fixtures/host_effect/2_source.dl7
exit 0
```

A name the program does not declare:

```console
$ bash book/show.sh eval fixtures/host_effect/0_pending.dl7 --serve fetch_jsonn
diagnostic served_relation_unknown(fetch_jsonn)
exit 1
```

## What proves it

| claim | path | command |
|---|---|---|
| pending, settled, source, loading, unserved | `fixtures/host_effect/*.expected.json`, `tests/_14_host_effect.rs:17-24` | `cargo test --test _14_host_effect` |
| an unknown served name | `tests/_14_host_effect.rs:123` | `cargo test --test _14_host_effect serving_a_name_the_program_does_not_declare_is_a_diagnostic` |
| every declared relation reaches `program.names` | `tests/_14_host_effect.rs:142` | `cargo test --test _14_host_effect every_declared_relation_reaches_the_names_table` |
| the effect branch | `src/_6_eval/_5_evaluate.rs:244-276` | `sed -n 244,276p src/_6_eval/_5_evaluate.rs` |
