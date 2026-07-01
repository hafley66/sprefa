# Reachability

## the call graph, as facts

Start from the ground truth: five call edges over the running example. main calls run, run calls parse and log, parse calls lex, and lex calls back into run, a cycle. helper sits alone.

```prolog
% caller -> callee, one fact per edge
edge(main,  run).
edge(run,   parse).
edge(parse, lex).
edge(lex,   run).
edge(run,   log).
```

```d2 plain
direction: right
main -> run
run -> parse
parse -> lex
lex -> run
run -> log
log.class: sink
helper.class: dead
```

## the base rule

One rule turns a raw edge into a reach: if X calls Y directly, then X reaches Y. This is the one-hop case, the seed of the closure.

graph: plain

```prolog
edge(main,  run).
edge(run,   parse).
edge(parse, lex).
edge(lex,   run).
edge(run,   log).

% base: a direct edge is a one-step reach
reaches(X, Y) :- edge(X, Y).
```

## the recursive step

The second rule does the walking: follow one edge to Y, then keep reaching from Y. Watch the tokens slide in: only the new clause moves, the rest holds still. See it close up in [[02-cycles]].

```prolog
edge(main,  run).
edge(run,   parse).
edge(parse, lex).
edge(lex,   run).
edge(run,   log).

% base: a direct edge is a one-step reach
reaches(X, Y) :- edge(X, Y).
% step: follow an edge, then keep reaching
reaches(X, Z) :- edge(X, Y), reaches(Y, Z).
```

## the query

Ask who main reaches. Bottom-up, the fixpoint saturates and the lex->run cycle is no trouble: it just stops adding facts once everything reachable is found.

graph: plain

```prolog
reaches(X, Y) :- edge(X, Y).
reaches(X, Z) :- edge(X, Y), reaches(Y, Z).

% who can main reach, transitively?
?- reaches(main, What).
% What = run ; parse ; lex ; log
```
