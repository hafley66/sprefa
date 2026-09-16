# Glossary

Define a term once as `term :: definition`. Every occurrence in any frame's prose
becomes a hover card (the first occurrence per frame).

SCC :: Strongly-connected component — a maximal set of nodes where each reaches every other.
fixpoint :: The point where applying the rules adds nothing new; bottom-up evaluation runs to it.
closure :: The full set of derived facts, e.g. every (caller, reachable-callee) pair.
stratify :: Order relations so a negated relation is fully computed before it is used.
condensation :: Collapsing each SCC to one super-node, yielding a DAG.
Tarjan :: Linear-time algorithm that finds all SCCs in one DFS.
