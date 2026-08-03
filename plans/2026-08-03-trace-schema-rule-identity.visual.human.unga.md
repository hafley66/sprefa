# trace + rule names, in plain words

## the thing you have today

```
  .dl6 file  --compiler-->  emitted .ts  --runtime-->  SQLite
                                              |
                                              v
                                     one log line per tick

  {"tick":7, "rels":4, "rows":312, "statements":18, "ms":41}
                                                ^
                                                |
                            18 statements ran. which ones? no idea.
```

the log knows HOW MANY rules fired. it does not know WHICH.

## the thing you asked for

```
  {"tick":7, "rels":4, "rows":312, "statements":18,
   "rules":[{"rule":"golden-flex:pickable/3#2","rows":12},
            {"rule":"golden-flex:lane_gap/2#1","rows":4}],
   "wall_ms":41}
```

same flag (DL_PERF_LOG), same file, one new list.

## why a rust emitter can copy it

the log line is not written by hand anywhere. it is a SHAPE:

```
   trace_schema.pl              <- one file, says what a line looks like
        |
        +-----------------+-----------------+
        |                 |                 |
     typescript         prolog            rust
     pino               json_write        tracing
        |                 |                 |
        +-----------------+-----------------+
                          |
                  same bytes come out
```

each language uses its OWN normal logger. nobody writes a logger.
they just agree on the field names and the order.

## the catch, stated up front

a clock is not repeatable. 41ms here, 43ms there. so every field is
one of two kinds:

```
   STABLE  tick, rels, rows, statements, rules   <- must match, every time
   TIMING  wall_ms                                <- stripped before compare
```

the receipt strips the timing and diffs the rest. that is the whole test:

```
   run golden-flex --> strip clocks --> diff against pinned file
                                             |
                                    empty = the emitter is right
```

## the drift nobody noticed

two files already claim to follow "the DL_PERF_LOG convention".
they disagree with each other:

```
   prolog says:   wall_ms   cpu_ms   gc_left_bytes    (snake, *_ms)
   typescript:    ms        witnessDigest             (neither)
```

a rust lane told "follow the same bytes" would follow whichever file
it happened to open first. so step one is picking one. snake_case and
*_ms wins, because the prolog side already wrote it down as the rule.

## what has to change, smallest first

```
  A   rename ms -> wall_ms, witnessDigest -> witness_digest
      write the schema down as data instead of a comment
      4 places read the old names, they move too
                                                        [small]

  B   give every emitted rule a name: "module:rel/arity#n"
      the runtime already loops over the statements,
      it just never knew what to call them
      197 generated files regenerate (bytes change, both doors
      regenerate together, so the byte-identity gate stays green)
                                                        [medium]

  C   pin the golden trace file
      that file IS the spec a rust emitter is graded on
                                                        [small]
```

## what is NOT in this

line numbers. "which rule" is easy, "which LINE of your file"
is not, because the rule that runs is not the rule you wrote:

```
   you wrote:        pickable(T, S, L) <- tree(T, S), L := ...
   match expansion:  rewrites it
   host expansion:   rewrites it again
   what runs:        a rule with no memory of where it came from
```

threading that memory through both expanders is a separate job.
there is already a branch for it (codex/rel-ref-file-span-lab).

## the one number to remember

when off, the cost is ONE `if` per statement. the channel is asked
"is anyone listening" and the answer is no.
