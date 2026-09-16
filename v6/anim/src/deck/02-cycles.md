# Cycles

## the cycle

Three edges close a loop: run calls parse, parse calls lex, lex calls back into run. run, parse, and lex light up because they form a loop you can't leave. Nothing in the d2 below says "colour these" — the build detects the cycle and tints it.

```prolog
% three edges close a loop:
edge(run,   parse).
edge(parse, lex).
edge(lex,   run).
% run -> parse -> lex -> run -> ...
```

```d2 cycle
direction: right
main -> run
run -> parse
parse -> lex
lex -> run
run -> log
helper.class: dead
```

## mutual reachability = an SCC

Define the cycle precisely: two nodes are in the same strongly-connected component when each reaches the other. run, parse, and lex all qualify pairwise.

graph: cycle

```prolog
% an SCC: each node reaches the other, both ways
same_scc(X, Y) :- reaches(X, Y), reaches(Y, X).

% same_scc(run, parse).
% same_scc(parse, lex).
% same_scc(run, lex).
```

## condensation: one node per SCC

Pick a single representative for each component. The three cycle members fold into one super-node c1; the lone nodes get their own.

```prolog
same_scc(X, Y) :- reaches(X, Y), reaches(Y, X).

% assign every node to a component id
scc(run,  c1).  scc(parse, c1).  scc(lex, c1).
scc(main, c0).  scc(log,   c2).
```

```d2 scc
direction: right
main
scc: "run · parse · lex" { style.fill: "#a3be8c"; style.stroke: "#2e3440" }
log.class: sink
helper.class: dead
main -> scc
scc -> log
```

## the condensed graph is a DAG

Lift each edge onto components and drop the ones that stay inside a component. The self-loop vanishes, the cycle is gone, and what remains is a DAG you can topologically sort and stratify. The same trick on relations lives in [[03-relations]].

graph: scc

```prolog
scc(run,  c1).  scc(parse, c1).  scc(lex, c1).
scc(main, c0).  scc(log,   c2).

% lift edges to components; intra-component edges vanish
cedge(A, B) :- edge(X, Y), scc(X, A), scc(Y, B), A \= B.
% cedge(c0, c1).  cedge(c1, c2).   -- no cycles: a DAG
```
