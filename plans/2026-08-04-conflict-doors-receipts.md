# Same-tick multi-writer conflict: the doors, verified against primary sources

Opus research 2026-08-04. Verbatim quotes extracted from the cited PDFs locally
(pdftotext), page numbers printed. Companion to plans/2026-08-04-rxprim-duel-verdict.md
and the tick_boundary ruling. Coordinator's five-door sketch graded; TWO additional
doors found.

## 0. Verdict summary

| # | claim | verdict | correction |
|---|---|---|---|
| 1 | Lustre: one defining equation per flow; merge needs complementary clocks | verified, citation corrected | single-definition rule is Halbwachs 1991 §IV-A p.1310 (compiler check, listed first); the V6 manual carries complementarity as TYPING on the branch arguments (`i1: int when clk; i2: int when not clk`), so exclusivity is a caller-visible signature contract |
| 2 | Signal: `default` = deterministic left-wins priority merge | verified, INCOMPLETE | Signal has a second door: `::=` partial definitions permit MULTIPLE writers under a static agreement-on-overlap obligation (V4 ref man §VI-1.1 p.96-97) |
| 3 | Esterel: multi-emit needs declared commutative-associative combine | verified, error class corrected | v5: static but only under `-Icheck` flag (primer §4.7.3 p.57); v7: a RUN-TIME error "detected at run-time, compile-time, or verification time, according to the programming environment" (§5.6 p.66); commutativity/associativity required and explicitly UNCHECKED |
| 4 | VHDL: multi-driver needs resolution fn; Verilog last-write-wins in a block | verified, scoped | VHDL error fires "after the ELABORATION of a description" (1076-2008 §6.4.2.3 p.69); Verilog last-write-wins holds within ONE sequential block only, across blocks the LRM says "indeterminate" (1364-2005 §9.2.2 p.121) |
| 5 | superdense time (t,n) orders simultaneous events by microstep | PARTIALLY REFUTED | microsteps separate weakly simultaneous events only; strongly simultaneous ones fall to topological level, then a MANUAL Priority parameter; "without the use of priorities, the order would be nondeterminate" (Ptolemaeus §7.3.1 p.254-255) |
| 6 | (new) Bloom/BloomL lattice merge | REAL SIXTH DOOR | conflict is defined away: set union (or any bounded join semilattice) is commutative associative idempotent, writer order unobservable. Bloom's deferred-merge operator is spelled `<+`, same as ours |
| 7 | (new) Statecharts scope priority | REAL SEVENTH DOOR | STATEMATE resolves outside-in (higher scope wins, TOSEM 5(4) §7 p.310-311); SCXML resolves inside-out + document order (§3.13). The two standards CONTRADICT each other on direction |

## 1. Lustre (door 1: forbid)

Halbwachs et al 1991, p.1310: "Definition checking: any local and output variable
should have one and only one equational definition." First of five static checks.
Clock equality is deliberately syntactic: "two boolean expressions define the same
clock if and only if these can be unified by means of syntactical substitutions"
(decidability retreat, p.1310-1311).

Lustre V6 ref man §3.2 p.27-28, merge with typed complementarity:

```lustre
node merge_bool_alt(clk : bool ;
                    i1 : int when clk ;
                    i2 : int when not clk)
returns (y: int);
let
      y = merge clk (true -> i1) (false-> i2);
tel
```

Engine mapping: full door 1 = at most one `<+` arm per head. Our
retention_head_conflict_risk already does this for bounded logs with the measured
receipt "ZERO tracked programs carry the shape" (rulings.pl). The gap vs Lustre:
arms on DIFFERENT triggers colliding in one drain, which is exactly what
edge_head_conflict_risk's shared-trigger condition misses (analyze.pl:1334-1353).

## 2. Signal (door 2: declared priority, PLUS partial definitions)

`default` is left-biased by definition (SCP paper §2.8 p.15; V4 ref man §III-7 p.43):

```signal
x := y default z    (* y wins at common instants; clock: x ^= y ^+ z *)
```

THE MISSED DOOR: partial definition `::=` (V4 ref man §VI-1.1 1-c p.96-97).
"Equations of partial definition of a signal are a way to avoid the syntactic
single assignment rule, even if semantically, this rule applies." Desugaring:

```
( | X ::= E1 | ... | X ::= En |)
=
( | X := E1 default X | ... | X := En default X | X ^= E1 ^+ ... ^+ En ^+ X |)
```

with the static obligation: "any two different expressions Ei must have the same
value at their common instants if they have such common instants." The agreement
obligation is what buys order-independence back from the source-order fold.
Also p.96: a signal is `:=`-shaped or `::=`-shaped, never both, which mirrors our
one-rel-one-rule-kind law. Caveat (SCP §3.5 p.23-24): "determinism is not stable
by composition and restriction."

Engine mapping: `::=` needs an overlap-emptiness test between rule bodies;
undecidable in the value algebra, which is why Signal restricts it to the clock
algebra. For datalog bodies that is a join-emptiness proof.

## 3. Esterel (door 3: declared combine)

v5 primer §4.4.2 p.49: single signals "cannot be emitted twice in the same
instant"; combined signals gather simultaneous emissions "using the specified
binary function or operator that must be commutative and associative", which "obviously
cannot be checked by Esterel". Syntax (p.45,48):

```esterel
output YesVotes := 0 : combine integer with +;
output Beeper : combine Beep with CombineBeeps;
```

v7 ref man §5.6 p.66: double-emit on single signals is "a run-time error ...
detected at run-time, compile-time, or verification time, according to the
programming environment"; fold shape fixed as f(v1, f(v2, ...f(vn-1, vn))).

Two facts that matter for us:
- COMBINE IS STRICT (v5 §5.2.5 p.90): "the value is known only when all emitters
  are either executed or discarded ... the computation of the value cannot be
  lazy." A combine-declared rel is a per-tick synchronization barrier, colliding
  with our laziness arc (subscribed_reset_pole, zero_query_semantics).
- Esterel contains door 1 AND door 3 in one language, split by object kind:
  variables refuse concurrent writes outright ("There is no way to give decent
  meaning to such statements", §3.7 p.26); signals combine.

## 4. VHDL / Verilog (door 4: resolution function)

IEEE 1076-2008 §6.4.2.3 p.69: "It is an error if, after the elaboration of a
description, a signal has multiple sources and it is not a resolved signal."
§4.6: the resolution function is PURE and takes the WHOLE multiset of driver
values as one unconstrained array. Associativity/commutativity are NOT required;
order-independence is the author's problem.

Design to copy or reject, the std_logic lattice: explicit conflict top `X`,
no-writer bottom `Z`, uninitialized `U`, folded from `Z` (IEEE 1164 package body).

Verilog contrast (1364-2005): equal-strength wire conflict = `x` (§4.6.1 p.26);
last-write-wins is scoped to ONE sequential block (§9.2.1 p.117); two blocks
writing one variable = "indeterminate" (§9.2.2 Example 5 p.121); "active events
can be taken off the queue and processed in any order" (§11.4.2 p.160).

Engine mapping: VHDL's whole-multiset signature = a GROUP BY user aggregate over
the tick's arrivals, evaluated once at settle. That composes with SQL storage
better than Esterel's pairwise fold. Error timing (elaboration) = our
0_program_check assembly-time slot.

## 5. Superdense time (door 5: serialize by index), partially refuted

Ptolemaeus ch.7: timestamp = (model time, microstep); strongly simultaneous =
same both. §7.3 p.254: same-timestamp events ordered by TOPOLOGICAL LEVEL.
§7.3.1 p.254-255: remaining ties need a manual Priority parameter; "two actors
can affect each other even though there is no direct communication between them
in the model. They are interacting under the table"; "Without the use of
priorities, the order would be nondeterminate."

Two `<+` arms on one head ARE the under-the-table case (the shared rel is the
file both FileWriters write). Arrival order resolves it only if arrival order is
itself deterministic across runs, a property of the task queue, never of the tick
definition. COMPOSE.md's four-run race table is our measurement of that.

Practical serializers: Elm (one update per Msg), Redux (reentrancy throw:
"Reducers may not dispatch actions."). Actor mailboxes are WEAKER: Erlang
guarantees FIFO per sender only; cross-sender order explicitly unspecified.
Our tick_boundary ruling (ingress transaction, list of one) is this door with
the Elm/Redux single-channel property, which is what neutralizes the Ptolemy
caveat for unrelated sources.

## 6. Bloom / BloomL (door 6: lattice merge), the closest cousin

Bloom (CIDR 2011 §3.1): timesteps; facts derived "either in the current
timestep, at the very next timestep, or at some non-deterministic time in the
future at a remote node"; "collections in Bloom provide set semantics". Two
rules writing one collection = set union: commutative, associative, idempotent,
writer order unobservable. THE CONFLICT DOES NOT ARISE.

Operator table (SoCC 2012): `<=` merge now, `<+` DEFERRED MERGE NEXT TIMESTEP,
`<-` deferred delete, `<~` async. Our `<+` is Bloom's spelling with Bloom's
meaning.

BloomL: any bounded join semilattice ("commutative, associative, and idempotent
merge functions") replaces set union; CALM certifies confluence. Priced
constraints: `<-` unsupported for lattices (idempotence and retraction are
mutually exclusive in BloomL); non-monotone reads need computed-to-completion
(the strictness barrier again).

Differential dataflow (CIDR 2013 §3): Z-multisets, A_t = sum of deltas,
commutative associative NOT idempotent, negative diffs = retraction. The
algebra table:

| system | structure | comm | assoc | idem | retraction |
|---|---|---|---|---|---|
| Bloom sets | union | y | y | y | via `<-` next step |
| BloomL | join semilattice | y | y | y | no |
| Dedalus | union, minimal model | y | y | y | via p_neg |
| differential | Z-multiset | y | y | NO | yes, diff<0 |
| Esterel combine | binary fn | required unchecked | required unchecked | no | no |
| VHDL resolution | fn on multiset | optional | optional | no | null transaction |

Idempotent-or-not is the axis deciding whether retraction is expressible. Our
engine sits on the differential side (visible minus marks) and the
one_admission_no_lockout ruling already recognizes two sound folds.

## 7. Statecharts (door 7: priority by structural scope)

STATEMATE (TOSEM 5(4) 1996 §7 p.310-311): conflict = shared exited state;
priority to the HIGHER scope (outside-in); equal scopes = "nondeterminism
occurs". SCXML (W3C §3.13): conflict = non-null exit-set intersection; priority
to the DESCENDANT source (inside-out), then document order. The standards
contradict each other on direction; the resolution is a language-owner choice,
never a discoverable fact. Transferable bit: SCXML's exit-set intersection test
is structurally analyze.pl:1352's `intersection(RefsA, RefsB, Shared)`.

## 8. Céu (variant: deterministic by fiat plus a lint)

TECS 16(4) 2017 §2.3 p.A:8-A:9: trails scheduled in LEXICAL ORDER; "enforcing an
arbitrary execution order can be misleading"; an "apparently innocuous change in
the order of trails modifies the semantics"; compile-time conflict detection
warns on write/write and write/read of shared vars awakened by one event, and
"the static checks are optional and do not affect the semantics". This is
emitter-arm-order elevated to doctrine plus a warning; our one_pick_order ruling
explicitly rejected that axis.

## 9. What this buys our open decisions

| our open item | door evidence |
|---|---|
| one() = arrival order per tick (ruled) | door 5 with the Ptolemy warning; neutralized for unrelated sources by tick_boundary (one ingress transaction per tick = single channel, Elm-style); residual: order WITHIN a deliberate batch is submitter-defined, which is fine because batching is now opt-in |
| any = merge (ruled) | door 6: set-union semantics, conflict defined away; our `<+` is literally Bloom's operator |
| declared-combine door (unexplored) | Esterel strictness receipt says it fights the laziness arc; VHDL's GROUP-BY-shaped signature is the SQL-friendly variant if ever wanted |
| Signal `::=` agreement obligation | the only static-multi-writer design in the field; needs join-emptiness proofs; park |
| keyed-vs-log revisit (standing note) | the idempotence axis (section 6 table) is the principled frame for that revisit: keyed = last-write fold (order-sensitive), log = monoid append, set = idempotent union; retraction expressibility follows the algebra |
