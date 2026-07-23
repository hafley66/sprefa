/**
 * prolog.ts -- the STUDY lab: Prolog's core resolution algorithm in literal rxjs/TS,
 * two ways, golden-gated against fixpoint.ts's bottom-up datalog closure. Both
 * engines share the term/unify core imported from fixpoint.ts (Value/Fact/Var/Const/
 * Term/Atom/Rule/Subst, matchFact/instantiate/applySubst/factKey) -- Prolog and datalog
 * are the SAME logical fragment (function-free Horn clauses) read in opposite
 * directions: bottom-up saturate-everything (fixpoint.ts) vs top-down demand-driven
 * proof search (this file). The duality is the point; the golden proves it.
 *
 * -- ENGINE 1: SLG (tabled resolution) -- WHY `expand` --------------------------
 * SLG is top-down but answers a SUBGOAL at most once per call pattern (tabling), so
 * it terminates on cycles and only expands rels the query actually demands. That is
 * a QUEUE of pending work (new subgoals, new answers) drained round after round until
 * no round adds anything new -- structurally identical to `datalogExpand` in
 * fixpoint.ts: same self-feeding `expand` + seen-set shape, different item type
 * (SlgItem instead of Fact). `expand` earns its keep here for the same reason it does
 * there: the hop is synchronous but the ACCUMULATION needs a driver that restarts
 * itself on its own output, which is exactly what `expand` is for.
 *
 * -- ENGINE 2: SLD (Prolog's real default) -- WHY a generator, not `expand` -----
 * SLD is depth-first with backtracking: try the first clause that matches, recurse
 * into ITS body before trying the next clause, and on failure backtrack to the next
 * alternative. That is a STACK, not a queue -- `expand` cannot express it, because
 * `expand` always drains the FULL current queue breadth-first before touching what
 * that queue produced. A JS generator (`function*` + `yield*`) IS a stack: each
 * recursive call is a stack frame, `yield` suspends mid-search and resumes exactly
 * where it left off, and unification failure just falls through to the next
 * alternative in the same frame. `from(generator)` bridges it into rxjs without
 * bending the algorithm to fit an operator it does not fit. Because SLD has no
 * tabling, it re-explores identical subgoals forever on a cyclic/left-recursive
 * program -- a hard step/depth budget is required for the demo to terminate, and
 * hitting that budget IS the observable difference from SLG (test 5).
 *
 * Both engines are restricted to the same function-free fragment as fixpoint.ts
 * (no compound terms), so "unification" here is flat var/const matching, not the
 * general Robinson algorithm with occurs-check.
 */

import { of, from, expand, EMPTY, type Observable } from "rxjs";
import {
  type Fact,
  type Atom,
  type Rule,
  type Subst,
  type Value,
  type Term,
  atom,
  vr,
  con,
  rule,
  fact,
  factKey,
  matchFact,
  instantiate,
  applySubst,
  datalogWhile,
} from "./fixpoint.ts";

// re-export the shared constructors/types so a consumer of prolog.ts need not also
// import fixpoint.ts directly for the common vocabulary.
export { atom, vr, con, rule, fact, factKey, matchFact, instantiate, applySubst, datalogWhile };
export type { Fact, Atom, Rule, Subst, Value };

// =============================================================================
// SHARED -- the reference oracle used by the golden (datalog closure filtered to a
// query's demand set, i.e. the ANSWER an all-knowing bottom-up saturation would give).
// =============================================================================

/** The facts a bottom-up closure produces that also match the query pattern. */
export function closureDemand(edb: readonly Fact[], rules: readonly Rule[], query: Atom): Fact[] {
  return datalogWhile(edb, rules).filter((candidateFact) => matchFact(query, candidateFact, new Map()) !== null);
}

/** Set-of-factKey comparison helper (both engines and the oracle return Fact[]s). */
export function factKeySet(facts: readonly Fact[]): Set<string> {
  return new Set(facts.map(factKey));
}

// =============================================================================
// ENGINE 1 -- SLG (tabled resolution, BFS via `expand`)
// =============================================================================

export type SlgItem =
  | { readonly kind: "subgoal"; readonly key: string; readonly call: Atom }
  | { readonly kind: "answer"; readonly subgoal: string; readonly fact: Fact };

/**
 * VARIANT canonical key for a call pattern: constants stay literal, variables are
 * renumbered by first occurrence within the atom (v0, v1, ...) so `path(a,X)` and
 * `path(a,Y)` table together -- tabling is keyed on the CALL PATTERN, not the exact
 * variable names, since the names are just local bookkeeping for the caller.
 */
export function subgoalKey(callAtom: Atom): string {
  const seen = new Map<string, number>();
  const parts = callAtom.args.map((term) => {
    if (term.kind === "const") return `c:${term.value}`;
    let n = seen.get(term.name);
    if (n === undefined) {
      n = seen.size;
      seen.set(term.name, n);
    }
    return `v${n}`;
  });
  return `${callAtom.pred}(${parts.join(",")})`;
}

/**
 * Call-pattern tabling: bind a rule's head vars to the CALLER's constants only.
 * Where the call arg is itself a variable, the head var stays free (unbound) -- the
 * resulting subst is the head-side view of "what the caller already fixed".
 * Returns null on pred/arity mismatch or a const/const clash.
 */
export function unifyHeadCall(head: Atom, callAtom: Atom): Subst | null {
  if (head.pred !== callAtom.pred || head.args.length !== callAtom.args.length) return null;
  const subst = new Map<string, Value>();
  for (let i = 0; i < head.args.length; i++) {
    const headTerm = head.args[i]!;
    const callTerm = callAtom.args[i]!;
    if (callTerm.kind === "const") {
      if (headTerm.kind === "const") {
        if (headTerm.value !== callTerm.value) return null;
      } else {
        const bound = subst.get(headTerm.name);
        if (bound !== undefined) {
          if (bound !== callTerm.value) return null; // shared head-var consistency
        } else {
          subst.set(headTerm.name, callTerm.value);
        }
      }
    }
    // callTerm.kind === "var": the caller left this position open; head stays free.
  }
  return subst;
}

/** Stable identity of a queue item, for the `expand` seen-set. */
function slgItemKey(item: SlgItem): string {
  return item.kind === "subgoal" ? `G:${item.key}` : `A:${item.subgoal}${factKey(item.fact)}`;
}

/**
 * Drive SLG to its fixpoint via `expand` + a stream seen-set, exactly like
 * `datalogExpand` in fixpoint.ts: each hop reads the CLOSURE-so-far state
 * (`registered`/`answers`), emits only NOVEL items, and an empty novel batch stops
 * `expand` = the tabled fixpoint (no more subgoals to register, no more answers to
 * derive for any registered call pattern).
 */
export function prologSolveSlg(edb: readonly Fact[], rules: readonly Rule[], query: Atom): Observable<SlgItem[]> {
  const edbByPred = new Map<string, Fact[]>();
  for (const edbFact of edb) {
    const bucket = edbByPred.get(edbFact.pred);
    if (bucket) bucket.push(edbFact);
    else edbByPred.set(edbFact.pred, [edbFact]);
  }

  const registered = new Map<string, Atom>(); // subgoal key -> its call atom
  const answers = new Map<string, Fact[]>(); // subgoal key -> its answer facts (in order)
  const answerSeen = new Map<string, Set<string>>(); // subgoal key -> factKeys already admitted
  const seen = new Set<string>(); // expand-level seen-set (dedups subgoal/answer items)

  const addAnswer = (subgoal: string, answerFact: Fact): SlgItem | null => {
    let seenSet = answerSeen.get(subgoal);
    if (!seenSet) {
      seenSet = new Set<string>();
      answerSeen.set(subgoal, seenSet);
    }
    const key = factKey(answerFact);
    if (seenSet.has(key)) return null;
    seenSet.add(key);
    const bucket = answers.get(subgoal);
    if (bucket) bucket.push(answerFact);
    else answers.set(subgoal, [answerFact]);
    return { kind: "answer", subgoal, fact: answerFact };
  };

  const admit = (items: readonly SlgItem[]): SlgItem[] => {
    const out: SlgItem[] = [];
    for (const item of items) {
      const key = slgItemKey(item);
      if (!seen.has(key)) {
        seen.add(key);
        out.push(item);
      }
    }
    return out;
  };

  const hop = (delta: readonly SlgItem[]): SlgItem[] => {
    const produced: SlgItem[] = [];

    // (1) NEW subgoals in this delta: register + EDB base case + rule demand.
    for (const item of delta) {
      if (item.kind !== "subgoal") continue;
      if (registered.has(item.key)) continue; // guard: already registered elsewhere
      registered.set(item.key, item.call);
      answers.set(item.key, []);
      answerSeen.set(item.key, new Set());

      for (const edbFact of edbByPred.get(item.call.pred) ?? []) {
        if (matchFact(item.call, edbFact, new Map()) !== null) {
          const answerItem = addAnswer(item.key, edbFact);
          if (answerItem) produced.push(answerItem);
        }
      }

      for (const candidateRule of rules) {
        if (candidateRule.head.pred !== item.call.pred) continue;
        const sigma = unifyHeadCall(candidateRule.head, item.call);
        if (sigma === null) continue;
        for (const bodyAtom of candidateRule.body) {
          const subCall = applySubst(bodyAtom, sigma);
          produced.push({ kind: "subgoal", key: subgoalKey(subCall), call: subCall });
        }
      }
    }

    // (2) EVERY registered subgoal, every round: re-run its rules' body joins against
    // the tables-so-far. Monotone (answers only grow) + dedup-guarded (addAnswer), so
    // re-running is safe and is how new sub-answers propagate up to dependents.
    for (const [key, call] of registered) {
      for (const candidateRule of rules) {
        if (candidateRule.head.pred !== call.pred) continue;
        const sigma = unifyHeadCall(candidateRule.head, call);
        if (sigma === null) continue;

        let substs: Subst[] = [sigma];
        for (const bodyAtom of candidateRule.body) {
          const boundBodyAtom = applySubst(bodyAtom, sigma);
          const tableKey = subgoalKey(boundBodyAtom);
          const tabledAnswers = answers.get(tableKey) ?? [];
          const nextSubsts: Subst[] = [];
          for (const partial of substs) {
            for (const answerFact of tabledAnswers) {
              const extended = matchFact(bodyAtom, answerFact, partial);
              if (extended) nextSubsts.push(extended);
            }
          }
          substs = nextSubsts;
          if (substs.length === 0) break;
        }

        for (const finalSubst of substs) {
          const answerItem = addAnswer(key, instantiate(candidateRule.head, finalSubst));
          if (answerItem) produced.push(answerItem);
        }
      }
    }

    return produced;
  };

  const seedItem: SlgItem = { kind: "subgoal", key: subgoalKey(query), call: query };
  const seed0 = admit([seedItem]);
  return of(seed0).pipe(
    expand((delta) => {
      const novel = admit(hop(delta));
      return novel.length ? of(novel) : EMPTY; // EMPTY -> expand stops = the SLG fixpoint
    }),
  );
}

/** Subscribe + collect SLG's delta batches (sync exhaust -- same posture as datalogExpandDeltas). */
export function slgDeltas(edb: readonly Fact[], rules: readonly Rule[], query: Atom): SlgItem[][] {
  const batches: SlgItem[][] = [];
  prologSolveSlg(edb, rules, query).subscribe((batch) => batches.push(batch));
  return batches;
}

/** Answer facts for exactly this query's subgoal, across all collected delta batches. */
export function queryAnswers(deltas: readonly SlgItem[][], query: Atom): Fact[] {
  const wantSubgoal = subgoalKey(query);
  const out: Fact[] = [];
  for (const batch of deltas) {
    for (const item of batch) {
      if (item.kind === "answer" && item.subgoal === wantSubgoal) out.push(item.fact);
    }
  }
  return out;
}

/** Every subgoal key SLG's demand-driven search actually reached (the pruning evidence). */
export function tabledSubgoals(deltas: readonly SlgItem[][]): Set<string> {
  const out = new Set<string>();
  for (const batch of deltas) {
    for (const item of batch) {
      if (item.kind === "subgoal") out.add(item.key);
    }
  }
  return out;
}

// =============================================================================
// ENGINE 2 -- SLD (DFS backtracking) via a generator, bridged with `from(gen)`
// =============================================================================

/** Thrown when the DFS step budget is exhausted mid-search (the non-termination witness). */
export class SldBudgetExceeded extends Error {
  constructor(steps: number) {
    super(`SLD step budget exceeded after ${steps} steps (no tabling: a cycle loops forever)`);
    this.name = "SldBudgetExceeded";
  }
}

// A var<->var unification (e.g. a query's free variable meeting a fresh rule
// variable) cannot bind to a concrete Value yet: neither side has one. It is
// recorded as an ALIAS -- a Value-typed string carrying a sentinel prefix plus
// the other variable's name. The prefix is a plain identifier no real test
// value collides with (node names are single lowercase letters, statuses are
// numbers). `walkVar` chases a variable through zero or more alias hops until
// it hits a real Value (grounded) or an unbound variable (still free): the
// union-find "resolve" step of unification, done lazily at read time instead
// of via mutable parent pointers.
const ALIAS_PREFIX = "PROLOG_ALIAS__";

function aliasOf(varName: string): Value {
  return ALIAS_PREFIX + varName;
}

function walkVar(name: string, s: Subst): Term {
  const bound = s.get(name);
  if (bound === undefined) return vr(name);
  if (typeof bound === "string" && bound.startsWith(ALIAS_PREFIX)) {
    return walkVar(bound.slice(ALIAS_PREFIX.length), s);
  }
  return con(bound);
}

/** Resolve a term to its most-chased form: a Const if grounded (possibly
 * through an alias chain), else the canonical still-free Var. The unifyTerm/
 * resolve helper both engines' unification sits on. */
export function resolveTerm(term: Term, s: Subst): Term {
  return term.kind === "const" ? term : walkVar(term.name, s);
}

/** Substitute a goal atom by fully chasing each arg through `s` (a Const if
 * grounded, the canonical free-var name otherwise). SLD's chase-aware analogue
 * of fixpoint.ts's `applySubst`: plain `applySubst` treats ANY map entry as a
 * grounded Value, which corrupts an alias entry (a sentinel string, not real
 * data) into a bogus constant the moment an aliased variable reappears in a
 * later goal. `chaseAtom` is required wherever var<->var aliasing is live;
 * `applySubst` stays correct (and is what SLG uses) only because
 * `unifyHeadCall`'s substitutions are always real ground consts, never aliases. */
function chaseAtom(goalAtom: Atom, s: Subst): Atom {
  return { pred: goalAtom.pred, args: goalAtom.args.map((term) => resolveTerm(term, s)) };
}

/**
 * Full unification of two atoms (either side may carry unbound vars, unlike
 * matchFact which requires a GROUND right side). No function symbols: a term is
 * var|const, so var<->const binds; const<->const must already be equal; var<->var
 * aliases the left variable to the right one (chased later via `walkVar`/
 * `resolveTerm` whenever either side eventually grounds).
 */
export function unifyAtoms(atomA: Atom, atomB: Atom, s: Subst): Subst | null {
  if (atomA.pred !== atomB.pred || atomA.args.length !== atomB.args.length) return null;
  let subst = s;
  for (let i = 0; i < atomA.args.length; i++) {
    const left = resolveTerm(atomA.args[i]!, subst);
    const right = resolveTerm(atomB.args[i]!, subst);
    if (left.kind === "const" && right.kind === "const") {
      if (left.value !== right.value) return null;
    } else if (left.kind === "var" && right.kind === "const") {
      const next = new Map(subst);
      next.set(left.name, right.value);
      subst = next;
    } else if (left.kind === "const" && right.kind === "var") {
      const next = new Map(subst);
      next.set(right.name, left.value);
      subst = next;
    } else if (left.kind === "var" && right.kind === "var") {
      if (left.name === right.name) continue;
      const next = new Map(subst);
      next.set(left.name, aliasOf(right.name));
      subst = next;
    }
  }
  return subst;
}

let renameCounter = 0;

/** Rename every variable in a rule with a fresh unique suffix, so two USES of the
 * same rule never share a variable identity (standard SLD clause renaming). */
export function renameRule(sourceRule: Rule, tag: number): Rule {
  const renameAtom = (sourceAtom: Atom): Atom => ({
    pred: sourceAtom.pred,
    args: sourceAtom.args.map((term) => (term.kind === "var" ? vr(`${term.name}#${tag}`) : term)),
  });
  return { head: renameAtom(sourceRule.head), body: sourceRule.body.map(renameAtom) };
}

/**
 * SLD resolution: depth-first, backtracking, no tabling. `goals` is the remaining
 * conjunction to prove; DFS order comes directly from the recursion + `yield*` --
 * try every EDB fact then every rule for the first goal, and for EACH alternative
 * recurse fully into the rest before trying the next alternative (a stack, not a
 * queue). `budget.steps` is a hard step cap shared across the whole search
 * (mutated by reference) so a cyclic/left-recursive program cannot spin forever
 * in the demo. Uses `chaseAtom` (not fixpoint.ts's `applySubst`) to compute the
 * grounded current goal, because SLD's var<->var aliasing (see `unifyAtoms`)
 * requires a chase, not a flat substitution -- see `chaseAtom`'s comment.
 */
export function* solve(
  edbByPred: ReadonlyMap<string, Fact[]>,
  rules: readonly Rule[],
  goals: readonly Atom[],
  s: Subst,
  budget: { steps: number },
): Generator<Subst> {
  if (goals.length === 0) {
    yield s;
    return;
  }
  if (budget.steps <= 0) throw new SldBudgetExceeded(budget.steps);
  budget.steps -= 1;

  const [currentGoal, ...restGoals] = goals as [Atom, ...Atom[]];
  const groundGoal = chaseAtom(currentGoal, s);

  // Alternative 1: EDB facts for this predicate.
  for (const edbFact of edbByPred.get(groundGoal.pred) ?? []) {
    const extended = matchFact(groundGoal, edbFact, s);
    if (extended) yield* solve(edbByPred, rules, restGoals, extended, budget);
  }

  // Alternative 2: rules for this predicate, each freshly renamed for this use.
  for (const candidateRule of rules) {
    if (candidateRule.head.pred !== groundGoal.pred) continue;
    renameCounter += 1;
    const renamed = renameRule(candidateRule, renameCounter);
    const unified = unifyAtoms(renamed.head, groundGoal, s);
    if (unified) yield* solve(edbByPred, rules, [...renamed.body, ...restGoals], unified, budget);
  }
}

/**
 * Bridge SLD's generator into rxjs via `from(generator)` (a plain Iterable source
 * pulled to completion) -- the DFS/backtracking algorithm is unchanged by the
 * bridge; `from` just drains the generator's `yield`s as emissions. Answers are
 * deduped by factKey inside the bridge (SLD can re-derive the same ground fact
 * along different proof paths). `capped()` reports whether the step budget tripped.
 */
export function prologSolveSld(
  edb: readonly Fact[],
  rules: readonly Rule[],
  query: Atom,
  maxSteps: number,
): { answers$: Observable<Fact[]>; capped: () => boolean } {
  const edbByPred = new Map<string, Fact[]>();
  for (const edbFact of edb) {
    const bucket = edbByPred.get(edbFact.pred);
    if (bucket) bucket.push(edbFact);
    else edbByPred.set(edbFact.pred, [edbFact]);
  }
  const budget = { steps: maxSteps };
  let capped = false;

  function* answerGen(): Generator<Fact> {
    try {
      for (const finalSubst of solve(edbByPred, rules, [query], new Map(), budget)) {
        yield instantiate(query, finalSubst);
      }
    } catch (err) {
      if (err instanceof SldBudgetExceeded) {
        capped = true;
        return;
      }
      throw err;
    }
  }

  const seen = new Set<string>();
  const dedup: Fact[] = [];
  for (const answerFact of answerGen()) {
    const key = factKey(answerFact);
    if (!seen.has(key)) {
      seen.add(key);
      dedup.push(answerFact);
    }
  }

  return { answers$: from([dedup]), capped: () => capped };
}

/** Simple collector: subscribe/collect prologSolveSld's single emission. */
export function sldAnswers(
  edb: readonly Fact[],
  rules: readonly Rule[],
  query: Atom,
  maxSteps: number,
): { answers: Fact[]; capped: boolean } {
  const { answers$, capped } = prologSolveSld(edb, rules, query, maxSteps);
  let answers: Fact[] = [];
  answers$.subscribe((batch) => {
    answers = batch;
  });
  return { answers, capped: capped() };
}
