//! Rules as rows: one IR program per strata bucket evaluates any rule set given as source
//! rows. Design and row shapes: `plans/v8/2026-10-05-rule-interpreter.findings.md`.

use super::kernel::Kernel;
use super::program::{next_inner, Arg, Polarity, Program, Row};
use super::stratify::stratify;
use super::term::{TermId, Universe};
use ivm_engine::Engine;
use ivm_ir::{self as ir, Cell};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::OnceLock;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum RulesMode {
    Interpret,
    Compile,
}

static RULES_MODE: OnceLock<Result<RulesMode, String>> = OnceLock::new();

fn read_rules_mode() -> Result<RulesMode, String> {
    match std::env::var("DL8_RULES").as_deref() {
        Ok("interpret") => Ok(RulesMode::Interpret),
        Err(std::env::VarError::NotPresent) | Ok("compile") => Ok(RulesMode::Compile),
        Ok(other) => Err(format!(
            "unknown DL8_RULES {other:?}; expected interpret or compile"
        )),
        Err(error) => Err(format!("DL8_RULES {error}; expected interpret or compile")),
    }
}

/// The binary reads `DL8_RULES` here, once, at start; a library caller that never did reads it
/// on first use.
pub fn init_rules_mode() -> Result<RulesMode, String> {
    RULES_MODE.get_or_init(read_rules_mode).clone()
}

pub fn rules_mode() -> RulesMode {
    init_rules_mode().unwrap_or(RulesMode::Compile)
}

pub const NIL: Cell = -1;
const POSITIVE: Cell = 0;
const NEGATIVE: Cell = 1;

/// `ref(kernel(intern | effect | intern_snapshot))`: relations the compiled program fills from
/// intern requests, served effects and readbacks as well as from rules.
fn special_relation(u: &Universe, rel: TermId) -> Option<&str> {
    let inner = u.unary(rel, "ref")?;
    let label = u.unary(inner, "kernel")?;
    match u.functor_or_atom(label)? {
        (name @ ("intern" | "effect" | "intern_snapshot"), []) => Some(name),
        _ => None,
    }
}

/// Why the interpreter cannot run `rule`, if it cannot.
pub fn rule_reason(u: &Universe, program: &Program, rule: &super::program::Rule) -> Option<String> {
    if next_inner(u, rule.rel).is_some() {
        return Some("next head (waves)".into());
    }
    if let Some(name) = special_relation(u, rule.rel) {
        return Some(format!("{name} head"));
    }
    if program.served.contains(&rule.rel) {
        return Some("served relation head".into());
    }
    if rule.is_aggregate() {
        return Some("head aggregate or fold".into());
    }
    for goal in &rule.body {
        if let Some(kernel) = Kernel::of(u, goal.rel) {
            return Some(format!("kernel goal {kernel:?}"));
        }
        if let Some(name) = special_relation(u, goal.rel) {
            return Some(format!("{name} goal"));
        }
        if program.served.contains(&goal.rel) {
            return Some("served relation goal".into());
        }
        if goal
            .args
            .iter()
            .any(|arg| !matches!(arg, Arg::Var(_) | Arg::Ground(_)))
        {
            return Some("goal aggregate or fold".into());
        }
    }
    let positive: BTreeSet<u32> = rule
        .body
        .iter()
        .filter(|goal| goal.polarity == Polarity::Positive)
        .flat_map(|goal| goal.args.iter())
        .filter_map(|arg| match arg {
            Arg::Var(v) => Some(v.0),
            _ => None,
        })
        .collect();
    if rule
        .head
        .iter()
        .any(|arg| matches!(arg, Arg::Var(v) if !positive.contains(&v.0)))
    {
        return Some("head variable no positive goal binds".into());
    }
    None
}

/// The rules of one program split in two: `compiled` runs first on the compiled program, with
/// every seed and served relation; `interpreted` reads its closure.
pub struct Split {
    pub compiled: Program,
    pub interpreted: Program,
    /// `(relation, arity)` the interpreted rules read or head: the compiled rows they are given.
    pub feeds: std::collections::HashSet<(u32, usize)>,
    /// Compiled rules by their reason, and the interpretable rules a compiled rule reads.
    pub reasons: BTreeMap<String, usize>,
}

/// Interpreted rules: every rule of a relation component whose rules are all interpretable and
/// that no compiled rule reads, directly or through another component. `Err` when no rule is.
pub fn split(u: &mut Universe, program: &Program) -> Result<Split, String> {
    let (_, diagnostics) = stratify(u, program);
    if !diagnostics.is_empty() {
        return Err("stratification diagnostic".into());
    }
    let mut index: HashMap<TermId, usize> = HashMap::new();
    for rule in &program.rules {
        let next = index.len();
        index.entry(rule.rel).or_insert(next);
        for goal in &rule.body {
            let next = index.len();
            index.entry(goal.rel).or_insert(next);
        }
    }
    // The compiled program derives demand rules for these relations from every caller's bound
    // arguments (`with_demand_in_strata`): their callers stay compiled.
    let snapshots = super::kernel::kernel_ref(u, "intern_snapshot");
    let demanded: std::collections::HashSet<TermId> = program
        .rules
        .iter()
        .filter(|rule| super::evaluate::needs_bound_head(u, rule, snapshots))
        .map(|rule| rule.rel)
        .collect();
    let mut edges = vec![Vec::new(); index.len()];
    let mut capable = vec![true; index.len()];
    let mut reasons: BTreeMap<String, usize> = BTreeMap::new();
    let mut ok = Vec::with_capacity(program.rules.len());
    for rule in &program.rules {
        let head = index[&rule.rel];
        let reason = rule_reason(u, program, rule).or_else(|| {
            rule.body
                .iter()
                .any(|goal| goal.polarity == Polarity::Positive && demanded.contains(&goal.rel))
                .then(|| "calls a demand-driven relation".to_string())
        });
        capable[head] &= reason.is_none();
        ok.push(reason.is_none());
        if let Some(reason) = reason {
            *reasons.entry(reason).or_default() += 1;
        }
        for goal in &rule.body {
            edges[head].push((index[&goal.rel], 0));
        }
    }
    let component = super::stratify::components(&edges);
    let count = component.iter().max().map_or(0, |c| c + 1);
    let mut interpreted = vec![true; count];
    for (rel, c) in component.iter().enumerate() {
        interpreted[*c] &= capable[rel];
    }
    // A component a compiled rule reads is compiled; its rules read further components.
    let mut todo: Vec<usize> = program
        .rules
        .iter()
        .filter(|rule| !interpreted[component[index[&rule.rel]]])
        .map(|rule| index[&rule.rel])
        .collect();
    let mut seen = vec![false; index.len()];
    while let Some(rel) = todo.pop() {
        if std::mem::replace(&mut seen[rel], true) {
            continue;
        }
        for (body, _) in &edges[rel] {
            let c = component[*body];
            if interpreted[c] {
                interpreted[c] = false;
                todo.extend(
                    component
                        .iter()
                        .enumerate()
                        .filter(|(_, x)| **x == c)
                        .map(|(r, _)| r),
                );
            }
            todo.push(*body);
        }
    }
    let mut compiled = Program {
        rules: Vec::new(),
        seeds: program.seeds.clone(),
        served: program.served.clone(),
        int_columns: program.int_columns.clone(),
    };
    let mut rules = Program::default();
    let mut feeds = std::collections::HashSet::new();
    for (rule, ok) in program.rules.iter().zip(ok) {
        if interpreted[component[index[&rule.rel]]] {
            feeds.insert((rule.rel.0, rule.head.len()));
            feeds.extend(rule.body.iter().map(|goal| (goal.rel.0, goal.args.len())));
            rules.rules.push(rule.clone());
        } else {
            if ok {
                *reasons.entry("read by a compiled rule".into()).or_default() += 1;
            }
            compiled.rules.push(rule.clone());
        }
    }
    if rules.rules.is_empty() {
        return Err(reasons
            .keys()
            .next()
            .cloned()
            .unwrap_or_else(|| "no rule".into()));
    }
    Ok(Split {
        compiled,
        interpreted: rules,
        feeds,
        reasons,
    })
}

/// One rule as rows: its stratum, goal rows and head items.
struct RulePlan {
    key: String,
    stratum: u32,
    rel: TermId,
    arity: usize,
    /// Head items, last position first.
    head: Vec<[Cell; 3]>,
    goals: Vec<GoalPlan>,
}

struct GoalPlan {
    rel: TermId,
    arity: usize,
    polarity: Cell,
    fact: Vec<[Cell; 3]>,
    env: Vec<[Cell; 3]>,
}

/// The rules the interpreter evaluates, as rows.
pub struct Plan {
    rules: Vec<RulePlan>,
    levels: u32,
    /// `(relation, arity)` of every seed, head and goal: the relations a frontier may change.
    pub keys: BTreeSet<(u32, usize)>,
}

/// The program as rule rows, or why the compiled program evaluates it.
pub fn plan(u: &mut Universe, program: &Program) -> Result<Plan, String> {
    if let Some(reason) = program
        .rules
        .iter()
        .find_map(|rule| rule_reason(u, program, rule))
    {
        return Err(reason);
    }
    let (strata, diagnostics) = stratify(u, program);
    if !diagnostics.is_empty() {
        return Err("stratification diagnostic".into());
    }
    let mut rules = Vec::with_capacity(program.rules.len());
    'rules: for rule in &program.rules {
        // Variable to (fact goal, position) of its first positive occurrence.
        let mut bound: HashMap<u32, (Cell, Cell)> = HashMap::new();
        let mut goals = Vec::with_capacity(rule.body.len());
        for (index, goal) in rule.body.iter().enumerate() {
            let mut fact = Vec::new();
            let mut env = Vec::new();
            let mut here: HashMap<u32, Cell> = HashMap::new();
            for (position, arg) in goal.args.iter().enumerate() {
                let position = position as Cell;
                match arg {
                    Arg::Ground(term) => fact.push([1, position, term.0 as Cell]),
                    Arg::Var(v) => {
                        if let Some(&(g, p)) = bound.get(&v.0) {
                            fact.push([0, position, 0]);
                            env.push([0, g, p]);
                        } else if goal.polarity == Polarity::Negative {
                            // An unbound variable in a negated goal: the rule derives nothing.
                            continue 'rules;
                        } else if let Some(&first) = here.get(&v.0) {
                            fact.push([2, first, position]);
                        } else {
                            here.insert(v.0, position);
                        }
                    }
                    _ => unreachable!("checked above"),
                }
            }
            for (v, position) in here {
                bound.insert(v, (index as Cell, position));
            }
            goals.push(GoalPlan {
                rel: goal.rel,
                arity: goal.args.len(),
                polarity: match goal.polarity {
                    Polarity::Positive => POSITIVE,
                    Polarity::Negative => NEGATIVE,
                },
                fact,
                env,
            });
        }
        let mut head = Vec::with_capacity(rule.head.len());
        for arg in rule.head.iter().rev() {
            match arg {
                Arg::Ground(term) => head.push([1, 0, term.0 as Cell]),
                Arg::Var(v) => match bound.get(&v.0) {
                    Some(&(g, p)) => head.push([0, g, p]),
                    None => return Err("head variable no positive goal binds".into()),
                },
                _ => unreachable!("checked above"),
            }
        }
        rules.push(RulePlan {
            key: format!("{rule:?}"),
            stratum: strata.level(rule.rel),
            rel: rule.rel,
            arity: rule.head.len(),
            head,
            goals,
        });
    }
    let mut keys: BTreeSet<(u32, usize)> = program
        .seeds
        .iter()
        .map(|row| (row.rel.0, row.args.len()))
        .collect();
    for rule in &program.rules {
        keys.insert((rule.rel.0, rule.head.len()));
        keys.extend(rule.body.iter().map(|goal| (goal.rel.0, goal.args.len())));
    }
    Ok(Plan {
        keys,
        rules,
        levels: strata.max_level + 1,
    })
}

/// The smallest power of two at least `levels`.
pub fn bucket(levels: u32) -> u32 {
    levels.max(1).next_power_of_two()
}

/// Relation ids of the interpreter program the session writes and reads.
#[derive(Clone)]
pub struct Shape {
    pub ir: ir::Program,
    pub bucket: u32,
    seed: ir::RelId,
    rule: ir::RelId,
    goal: ir::RelId,
    cons: ir::RelId,
    item: ir::RelId,
    facts: ir::RelId,
}

struct B {
    p: ir::Program,
}

fn col(c: usize) -> ir::Expr {
    ir::Expr::Col(c as ir::ColId)
}

fn lit(v: Cell) -> ir::Expr {
    ir::Expr::Lit(v)
}

fn eq(a: ir::Expr, b: ir::Expr) -> ir::Expr {
    ir::Expr::Call(ir::Func::Eq, vec![a, b])
}

fn ne(a: ir::Expr, b: ir::Expr) -> ir::Expr {
    ir::Expr::Call(ir::Func::Ne, vec![a, b])
}

fn cols(cs: &[usize]) -> Vec<ir::ColId> {
    cs.iter().map(|c| *c as ir::ColId).collect()
}

impl B {
    fn rel(&mut self, name: String, width: usize, kind: ir::RelKind) -> ir::RelId {
        let id = self.p.rels.len() as ir::RelId;
        let mut types = vec![ir::Ty::Int; width];
        if kind == ir::RelKind::Constructor {
            types[0] = ir::Ty::Id;
        }
        self.p.rels.push(ir::Relation {
            id,
            name,
            cols: types,
            kind,
        });
        id
    }
    fn node(&mut self, op: ir::Op) -> ir::NodeId {
        self.p.nodes.push(op);
        (self.p.nodes.len() - 1) as ir::NodeId
    }
    fn get(&mut self, rel: ir::RelId) -> ir::NodeId {
        self.node(ir::Op::Get(rel))
    }
    fn mfp(
        &mut self,
        input: ir::NodeId,
        filter: Vec<ir::Expr>,
        map: Vec<ir::Expr>,
        project: &[usize],
    ) -> ir::NodeId {
        self.node(ir::Op::Mfp {
            input,
            filter,
            map,
            project: cols(project),
        })
    }
    fn filter(&mut self, input: ir::NodeId, filter: Vec<ir::Expr>) -> ir::NodeId {
        self.mfp(input, filter, vec![], &[])
    }
    /// Two-input join; `on` pairs a left column with a right column.
    fn join(&mut self, l: ir::NodeId, r: ir::NodeId, on: &[(usize, usize)]) -> ir::NodeId {
        self.node(ir::Op::Join {
            inputs: vec![l, r],
            equivalences: on
                .iter()
                .map(|(a, b)| vec![(0, *a as ir::ColId), (1, *b as ir::ColId)])
                .collect(),
            project: vec![],
        })
    }
    /// The constructor row whose id is column `at`, appended: `id, args...`.
    fn decode(&mut self, input: ir::NodeId, at: usize, ctor: ir::RelId) -> ir::NodeId {
        let get = self.get(ctor);
        self.join(input, get, &[(at, 0)])
    }
    fn mint(&mut self, input: ir::NodeId, ctor: ir::RelId, args: &[usize]) -> ir::NodeId {
        self.node(ir::Op::Mint {
            input,
            functor: ctor,
            args: cols(args),
        })
    }
    fn union(&mut self, parts: Vec<ir::NodeId>) -> ir::NodeId {
        self.node(ir::Op::Union(parts))
    }
    fn threshold(&mut self, input: ir::NodeId) -> ir::NodeId {
        self.node(ir::Op::Threshold(input))
    }

    /// One step of a fact pattern walk `(R, i, L, cur, acc)` against `cells` `(L, p, x)`:
    /// `(R, i, L, rest, acc')`.
    fn fact_step(
        &mut self,
        state: ir::NodeId,
        cells: ir::NodeId,
        cons: ir::RelId,
        item: ir::RelId,
    ) -> ir::NodeId {
        // 5 id, 6 item, 7 rest; 8 id, 9 kind, 10 a, 11 b
        let d1 = self.decode(state, 3, cons);
        let d2 = self.decode(d1, 6, item);
        let k0 = self.filter(d2, vec![eq(col(9), lit(0))]);
        let j0 = self.join(k0, cells, &[(2, 0), (10, 1)]);
        let m0 = self.mint(j0, cons, &[14, 4]);
        let o0 = self.mfp(m0, vec![], vec![], &[0, 1, 2, 7, 15]);
        let k1 = self.filter(d2, vec![eq(col(9), lit(1))]);
        let j1 = self.join(k1, cells, &[(2, 0), (10, 1)]);
        let o1 = self.mfp(j1, vec![eq(col(14), col(11))], vec![], &[0, 1, 2, 7, 4]);
        let k2 = self.filter(d2, vec![eq(col(9), lit(2))]);
        let j2 = self.join(k2, cells, &[(2, 0), (10, 1)]);
        let j2b = self.join(j2, cells, &[(2, 0), (11, 1)]);
        let o2 = self.mfp(j2b, vec![eq(col(14), col(17))], vec![], &[0, 1, 2, 7, 4]);
        self.union(vec![o0, o1, o2])
    }

    /// Conses the item's value (kind 0: fact `a` position `b` of the env at `env_at`; kind 1: the
    /// constant `b`) onto the list at `acc_at`; keeps `keep` and the new list.
    #[allow(clippy::too_many_arguments)]
    fn push_value(
        &mut self,
        rows: ir::NodeId,
        width: usize,
        kind_at: usize,
        env_at: usize,
        acc_at: usize,
        efact: ir::NodeId,
        cells: ir::NodeId,
        cons: ir::RelId,
        keep: &[usize],
    ) -> ir::NodeId {
        let (a, b) = (kind_at + 1, kind_at + 2);
        let k0 = self.filter(rows, vec![eq(col(kind_at), lit(0))]);
        // + E, g, L at width.., then L, p, x
        let j1 = self.join(k0, efact, &[(env_at, 0), (a, 1)]);
        let j2 = self.join(j1, cells, &[(width + 2, 0), (b, 1)]);
        let m0 = self.mint(j2, cons, &[width + 5, acc_at]);
        let mut p0 = keep.to_vec();
        p0.push(width + 6);
        let o0 = self.mfp(m0, vec![], vec![], &p0);
        let k1 = self.filter(rows, vec![eq(col(kind_at), lit(1))]);
        let m1 = self.mint(k1, cons, &[b, acc_at]);
        let mut p1 = keep.to_vec();
        p1.push(width);
        let o1 = self.mfp(m1, vec![], vec![], &p1);
        self.union(vec![o0, o1])
    }

    /// Decodes the walk's `cur` column and its item: id, item, rest, item id, kind, a, b.
    fn walk_head(
        &mut self,
        state: ir::NodeId,
        width: usize,
        cur: usize,
        cons: ir::RelId,
        item: ir::RelId,
    ) -> ir::NodeId {
        let d1 = self.decode(state, cur, cons);
        self.decode(d1, width + 1, item)
    }
}

/// `(L, p, x)` of every list `L` in column `at` of `lists` (`width` columns), as a LetRec over
/// `(L, cur, p)`.
fn cell_walk(
    b: &mut B,
    lists: ir::NodeId,
    width: usize,
    at: usize,
    cons: ir::RelId,
    name: &str,
) -> (ir::Stratum, ir::RelId) {
    let walk = b.rel(format!("{name}_cw"), 3, ir::RelKind::Derived);
    let cells = b.rel(format!("{name}_cells"), 3, ir::RelKind::Derived);
    let start = b.mfp(
        lists,
        vec![ne(col(at), lit(NIL))],
        vec![lit(0)],
        &[at, at, width],
    );
    let current = b.get(walk);
    let d = b.decode(current, 1, cons); // 3 id, 4 head, 5 tail
    let next = b.mfp(
        d,
        vec![ne(col(5), lit(NIL))],
        vec![ir::Expr::Call(ir::Func::Add, vec![col(2), lit(1)])],
        &[0, 5, 6],
    );
    let walk_body = b.union(vec![start, next]);
    let cells_body = b.mfp(d, vec![], vec![], &[0, 2, 4]);
    (
        ir::Stratum::LetRec(ir::LetRec {
            ids: vec![walk, cells],
            bodies: vec![walk_body, cells_body],
            limit: None,
            nested: vec![],
        }),
        cells,
    )
}

/// The interpreter program with `bucket` stratum copies.
pub fn shape(bucket: u32) -> Shape {
    let mut b = B {
        p: ir::Program {
            texts: vec![],
            terms: vec![],
            rels: vec![],
            nodes: vec![],
            strata: vec![],
            outputs: vec![],
        },
    };
    let cons = b.rel("icons".into(), 3, ir::RelKind::Constructor);
    let item = b.rel("iitem".into(), 4, ir::RelKind::Constructor);
    let seed = b.rel("iseed".into(), 3, ir::RelKind::Source);
    let rule = b.rel("irule".into(), 6, ir::RelKind::Source);
    let goal = b.rel("igoal".into(), 8, ir::RelKind::Source);

    // Cells only of facts some goal reads.
    let goal_rows = b.get(goal);
    let read_keys = b.mfp(goal_rows, vec![], vec![], &[3, 4]);
    let read_keys = b.threshold(read_keys);
    let seeds = b.get(seed);
    let read = b.join(seeds, read_keys, &[(0, 0), (1, 1)]);
    let read = b.mfp(read, vec![], vec![], &[0, 1, 2]);
    let (stratum, seed_cells) = cell_walk(&mut b, read, 3, 2, cons, "iseed");
    b.p.strata.push(stratum);
    let mut facts_lo = seed;
    let mut cells_lo = seed_cells;
    for s in 0..bucket {
        let s_cell = s as Cell;
        let name = |x: &str| format!("i{s}_{x}");
        // Goals (R0, i1, s2, rel3, n4, pol5, FP6, EP7) and rules (R0, s1, ng2, rel3, n4, HP5) of s.
        let goals_all = b.get(goal);
        let goals = b.filter(goals_all, vec![eq(col(2), lit(s_cell))]);
        let rules_all = b.get(rule);
        let rules = b.filter(rules_all, vec![eq(col(1), lit(s_cell))]);

        // Keys of the facts below this copy, every goal of it: (R, i, L, K).
        let fw_lo = b.rel(name("fw_lo"), 5, ir::RelKind::Derived);
        let lo = b.get(facts_lo);
        // (rel0, n1, L2) x goals 3.. -> R3, i4, FP9
        let j = b.join(lo, goals, &[(0, 3), (1, 4)]);
        let start_lo = b.mfp(j, vec![], vec![lit(NIL)], &[3, 4, 2, 9, 11]);
        let cells = b.get(cells_lo);
        let current = b.get(fw_lo);
        let step = b.fact_step(current, cells, cons, item);
        let all = b.union(vec![start_lo, step]);
        let body = b.filter(all, vec![ne(col(3), lit(NIL))]);
        b.p.strata.push(ir::Stratum::LetRec(ir::LetRec {
            ids: vec![fw_lo],
            bodies: vec![body],
            limit: None,
            nested: vec![],
        }));
        let key_lo = b.rel(name("key_lo"), 4, ir::RelKind::Derived);
        let lo = b.get(facts_lo);
        let j = b.join(lo, goals, &[(0, 3), (1, 4)]);
        let start_lo = b.mfp(j, vec![], vec![lit(NIL)], &[3, 4, 2, 9, 11]);
        let cells = b.get(cells_lo);
        let current = b.get(fw_lo);
        let step = b.fact_step(current, cells, cons, item);
        let all = b.union(vec![start_lo, step]);
        let done = b.mfp(all, vec![eq(col(3), lit(NIL))], vec![], &[0, 1, 2, 4]);
        b.p.strata.push(ir::Stratum::Let {
            id: key_lo,
            body: done,
        });

        // The fixpoint of this copy.
        let env = b.rel(name("env"), 3, ir::RelKind::Derived); // R, i, E
        let efact = b.rel(name("efact"), 3, ir::RelKind::Derived); // E, g, L
        let ew = b.rel(name("ew"), 6, ir::RelKind::Derived); // R, i, pol, E, cur, acc
        let fw = b.rel(name("fw"), 5, ir::RelKind::Derived); // R, i, L, cur, acc
        let hw = b.rel(name("hw"), 6, ir::RelKind::Derived); // R, rel, n, E, cur, acc
        let new = b.rel(name("new"), 3, ir::RelKind::Derived); // rel, n, L
        let cw = b.rel(name("cw"), 3, ir::RelKind::Derived); // L, cur, p
        let cn = b.rel(name("cells_new"), 3, ir::RelKind::Derived); // L, p, x

        let cells_lo_get = b.get(cells_lo);
        let cn_get = b.get(cn);
        let cells_all = b.union(vec![cells_lo_get, cn_get]);
        let efact_get = b.get(efact);

        // Env key walk: env (R0, i1, E2) x goals -> (R, i, pol, E, EP, NIL).
        let env_get = b.get(env);
        let j = b.join(env_get, goals, &[(0, 0), (1, 1)]); // goals at 3..: pol 8, EP 10
        let ew_start = b.mfp(j, vec![], vec![lit(NIL)], &[0, 1, 8, 2, 10, 11]);
        let ew_get = b.get(ew);
        let d = b.walk_head(ew_get, 6, 4, cons, item); // 6 id, 7 item, 8 rest, 9 id, 10 kind, 11 a, 12 b
        let ew_step = b.push_value(
            d,
            13,
            10,
            3,
            5,
            efact_get,
            cells_all,
            cons,
            &[0, 1, 2, 3, 8],
        );
        let ew_all = b.union(vec![ew_start, ew_step]);
        let ew_body = b.filter(ew_all, vec![ne(col(4), lit(NIL))]);
        let key_env = b.mfp(ew_all, vec![eq(col(4), lit(NIL))], vec![], &[0, 1, 2, 3, 5]); // R, i, pol, E, K

        // Fact key walk over the facts this copy derives.
        let new_get = b.get(new);
        let j = b.join(new_get, goals, &[(0, 3), (1, 4)]);
        let fw_start = b.mfp(j, vec![], vec![lit(NIL)], &[3, 4, 2, 9, 11]);
        let fw_get = b.get(fw);
        let fw_step = b.fact_step(fw_get, cells_all, cons, item);
        let fw_all = b.union(vec![fw_start, fw_step]);
        let fw_body = b.filter(fw_all, vec![ne(col(3), lit(NIL))]);
        let key_new = b.mfp(fw_all, vec![eq(col(3), lit(NIL))], vec![], &[0, 1, 2, 4]);
        let key_lo_get = b.get(key_lo);
        let key_fact = b.union(vec![key_lo_get, key_new]); // R, i, L, K

        // Positive goals: (R0, i1, pol2, E3, K4) x (R5, i6, L7, K8) -> E' at 9.
        let pos = b.filter(key_env, vec![eq(col(2), lit(POSITIVE))]);
        let matched = b.join(pos, key_fact, &[(0, 0), (1, 1), (4, 3)]);
        let extended = b.mint(matched, cons, &[7, 3]);
        let env_pos = b.mfp(
            extended,
            vec![],
            vec![ir::Expr::Call(ir::Func::Add, vec![col(1), lit(1)])],
            &[0, 10, 9],
        );
        let efact_new = b.mfp(extended, vec![], vec![], &[9, 1, 7]);
        let link = b.mfp(extended, vec![], vec![], &[9, 3]); // E', E
        let inherited = b.join(link, efact_get, &[(1, 0)]); // E'0 E1 E2 g3 L4
        let efact_inherited = b.mfp(inherited, vec![], vec![], &[0, 3, 4]);
        let efact_body = b.union(vec![efact_new, efact_inherited]);

        // Negative goals: the env survives when no lower fact has its key.
        let neg = b.filter(key_env, vec![eq(col(2), lit(NEGATIVE))]);
        let key_lo_get = b.get(key_lo);
        let kept = b.node(ir::Op::Antijoin {
            l: neg,
            r: key_lo_get,
            lk: cols(&[0, 1, 4]),
            rk: cols(&[0, 1, 3]),
        });
        let env_neg = b.mfp(
            kept,
            vec![],
            vec![ir::Expr::Call(ir::Func::Add, vec![col(1), lit(1)])],
            &[0, 5, 3],
        );
        let env_start = b.mfp(rules, vec![], vec![lit(0), lit(NIL)], &[0, 6, 7]);
        let env_body = b.union(vec![env_start, env_pos, env_neg]);

        // Head walk: env at i = goals -> (R, rel, n, E, HP, NIL).
        let env_get = b.get(env);
        let j = b.join(env_get, rules, &[(0, 0), (1, 2)]); // rules at 3..: rel 6, n 7, HP 8
        let hw_start = b.mfp(j, vec![], vec![lit(NIL)], &[0, 6, 7, 2, 8, 9]);
        let hw_get = b.get(hw);
        let d = b.walk_head(hw_get, 6, 4, cons, item);
        let hw_step = b.push_value(
            d,
            13,
            10,
            3,
            5,
            efact_get,
            cells_all,
            cons,
            &[0, 1, 2, 3, 8],
        );
        let hw_all = b.union(vec![hw_start, hw_step]);
        let hw_body = b.filter(hw_all, vec![ne(col(4), lit(NIL))]);
        let new_body = b.mfp(hw_all, vec![eq(col(4), lit(NIL))], vec![], &[1, 2, 5]);

        // Cells of the facts this copy derives.
        let new_get = b.get(new);
        let new_read = b.join(new_get, read_keys, &[(0, 0), (1, 1)]);
        let cw_start = b.mfp(
            new_read,
            vec![ne(col(2), lit(NIL))],
            vec![lit(0)],
            &[2, 2, 5],
        );
        let cw_get = b.get(cw);
        let d = b.decode(cw_get, 1, cons); // 3 id, 4 head, 5 tail
        let cw_next = b.mfp(
            d,
            vec![ne(col(5), lit(NIL))],
            vec![ir::Expr::Call(ir::Func::Add, vec![col(2), lit(1)])],
            &[0, 5, 6],
        );
        let cw_body = b.union(vec![cw_start, cw_next]);
        let cn_body = b.mfp(d, vec![], vec![], &[0, 2, 4]);

        b.p.strata.push(ir::Stratum::LetRec(ir::LetRec {
            ids: vec![env, efact, ew, fw, hw, new, cw, cn],
            bodies: vec![
                env_body, efact_body, ew_body, fw_body, hw_body, new_body, cw_body, cn_body,
            ],
            limit: None,
            nested: vec![],
        }));

        let facts = b.rel(name("facts"), 3, ir::RelKind::Derived);
        let lo = b.get(facts_lo);
        let new_get = b.get(new);
        let union = b.union(vec![lo, new_get]);
        let body = b.threshold(union);
        b.p.strata.push(ir::Stratum::Let { id: facts, body });
        let all_cells = b.rel(name("cells"), 3, ir::RelKind::Derived);
        let lo = b.get(cells_lo);
        let cn_get = b.get(cn);
        let union = b.union(vec![lo, cn_get]);
        let body = b.threshold(union);
        b.p.strata.push(ir::Stratum::Let {
            id: all_cells,
            body,
        });
        facts_lo = facts;
        cells_lo = all_cells;
    }
    let out = b.rel("ifact".into(), 3, ir::RelKind::Derived);
    let last = b.get(facts_lo);
    let body = b.threshold(last);
    b.p.strata.push(ir::Stratum::Let { id: out, body });
    b.p.outputs.push(out);
    Shape {
        ir: b.p,
        bucket,
        seed,
        rule,
        goal,
        cons,
        item,
        facts: out,
    }
}

/// Source rows the plan needs, with every list and item interned through `intern`.
pub struct Interner {
    conses: HashMap<(Cell, Cell), Cell>,
    items: HashMap<[Cell; 3], Cell>,
    rule_ids: HashMap<String, Cell>,
}

impl Interner {
    /// Every key and value cell the interner holds.
    pub fn for_each_cell(&self, f: &mut dyn FnMut(Cell)) {
        for ((head, tail), cell) in &self.conses {
            [*head, *tail, *cell].into_iter().for_each(&mut *f);
        }
        for (item, cell) in &self.items {
            item.iter().copied().chain([*cell]).for_each(&mut *f);
        }
        self.rule_ids.values().copied().for_each(&mut *f);
    }
}

impl Default for Interner {
    fn default() -> Self {
        Self {
            conses: HashMap::new(),
            items: HashMap::new(),
            rule_ids: HashMap::new(),
        }
    }
}

impl Interner {
    fn items<E: Engine>(
        &mut self,
        engine: &mut E,
        shape: &Shape,
        wanted: &BTreeSet<[Cell; 3]>,
    ) -> Result<(), ivm_engine::EngineError> {
        let fresh: Vec<[Cell; 3]> = wanted
            .iter()
            .filter(|item| !self.items.contains_key(*item))
            .copied()
            .collect();
        if fresh.is_empty() {
            return Ok(());
        }
        let terms: Vec<(ir::RelId, ir::Row)> = fresh
            .iter()
            .map(|item| (shape.item, item.to_vec()))
            .collect();
        let ids = engine.intern_terms(&terms)?;
        for (item, id) in fresh.into_iter().zip(ids) {
            self.items.insert(item, id);
        }
        Ok(())
    }

    /// Every list of `lists` interned, innermost cons first, one engine call per depth.
    fn lists<E: Engine>(
        &mut self,
        engine: &mut E,
        shape: &Shape,
        lists: &[Vec<Cell>],
    ) -> Result<Vec<Cell>, ivm_engine::EngineError> {
        let mut tails: Vec<Cell> = vec![NIL; lists.len()];
        let deepest = lists.iter().map(Vec::len).max().unwrap_or(0);
        for depth in 1..=deepest {
            let mut fresh: Vec<(Cell, Cell)> = Vec::new();
            for (index, list) in lists.iter().enumerate() {
                if list.len() < depth {
                    continue;
                }
                let key = (list[list.len() - depth], tails[index]);
                if !self.conses.contains_key(&key) && !fresh.contains(&key) {
                    fresh.push(key);
                }
            }
            if !fresh.is_empty() {
                let terms: Vec<(ir::RelId, ir::Row)> = fresh
                    .iter()
                    .map(|(h, t)| (shape.cons, vec![*h, *t]))
                    .collect();
                let ids = engine.intern_terms(&terms)?;
                for (key, id) in fresh.into_iter().zip(ids) {
                    self.conses.insert(key, id);
                }
            }
            for (index, list) in lists.iter().enumerate() {
                if list.len() >= depth {
                    tails[index] = self.conses[&(list[list.len() - depth], tails[index])];
                }
            }
        }
        Ok(tails)
    }

    /// The `iseed` row of one fact.
    pub fn seed_row<E: Engine>(
        &mut self,
        engine: &mut E,
        shape: &Shape,
        row: &Row,
    ) -> Result<(ir::RelId, Vec<Cell>), ivm_engine::EngineError> {
        let list = self.lists(
            engine,
            shape,
            &[row.args.iter().map(|a| a.0 as Cell).collect()],
        )?[0];
        Ok((
            shape.seed,
            vec![row.rel.0 as Cell, row.args.len() as Cell, list],
        ))
    }

    /// The source rows of `plan` and `seeds`, sorted.
    pub fn rows<E: Engine>(
        &mut self,
        engine: &mut E,
        shape: &Shape,
        plan: &Plan,
        seeds: &[Row],
    ) -> Result<BTreeSet<(ir::RelId, Vec<Cell>)>, ivm_engine::EngineError> {
        let mut wanted = BTreeSet::new();
        for rule in &plan.rules {
            wanted.extend(rule.head.iter().copied());
            for goal in &rule.goals {
                wanted.extend(goal.fact.iter().copied());
                wanted.extend(goal.env.iter().copied());
            }
        }
        self.items(engine, shape, &wanted)?;
        let mut lists: Vec<Vec<Cell>> = Vec::new();
        for row in seeds {
            lists.push(row.args.iter().map(|a| a.0 as Cell).collect());
        }
        for rule in &plan.rules {
            lists.push(rule.head.iter().map(|item| self.items[item]).collect());
            for goal in &rule.goals {
                lists.push(goal.fact.iter().map(|item| self.items[item]).collect());
                lists.push(goal.env.iter().map(|item| self.items[item]).collect());
            }
        }
        let ids = self.lists(engine, shape, &lists)?;
        let mut ids = ids.into_iter();
        let mut rows = BTreeSet::new();
        for row in seeds {
            rows.insert((
                shape.seed,
                vec![
                    row.rel.0 as Cell,
                    row.args.len() as Cell,
                    ids.next().unwrap(),
                ],
            ));
        }
        for rule in &plan.rules {
            let next = self.rule_ids.len() as Cell;
            let r = *self.rule_ids.entry(rule.key.clone()).or_insert(next);
            let head = ids.next().unwrap();
            let s = rule.stratum as Cell;
            rows.insert((
                shape.rule,
                vec![
                    r,
                    s,
                    rule.goals.len() as Cell,
                    rule.rel.0 as Cell,
                    rule.arity as Cell,
                    head,
                ],
            ));
            for (i, goal) in rule.goals.iter().enumerate() {
                let fact = ids.next().unwrap();
                let env = ids.next().unwrap();
                rows.insert((
                    shape.goal,
                    vec![
                        r,
                        i as Cell,
                        s,
                        goal.rel.0 as Cell,
                        goal.arity as Cell,
                        goal.polarity,
                        fact,
                        env,
                    ],
                ));
            }
        }
        Ok(rows)
    }
}

/// The facts of `ifact` as rows, decoded through the `icons` dictionary, sorted as
/// `snapshot_closure` sorts.
pub fn decode_facts<E: Engine>(
    u: &Universe,
    engine: &E,
    shape: &Shape,
) -> Result<Vec<Row>, ivm_engine::EngineError> {
    let conses: HashMap<Cell, (Cell, Cell)> = engine
        .intern_snapshot(shape.cons)?
        .into_iter()
        .filter(|(_, w)| *w > 0)
        .map(|(row, _)| (row[0], (row[1], row[2])))
        .collect();
    let mut rows = Vec::new();
    for (cells, w) in engine.snapshot(shape.facts)? {
        if w <= 0 {
            continue;
        }
        let mut args = Vec::with_capacity(cells[1] as usize);
        let mut at = cells[2];
        for _ in 0..cells[1] {
            let Some(&(head, tail)) = conses.get(&at) else {
                return Err(ivm_engine::EngineError::new(
                    ivm_engine::Stage::Snapshot,
                    Some(shape.facts),
                    ivm_engine::ErrorKind::Unsupported("undecodable fact list"),
                ));
            };
            args.push(TermId(head as u32));
            at = tail;
        }
        rows.push(Row {
            rel: TermId(cells[0] as u32),
            args,
        });
    }
    rows.sort_by(|a, b| {
        u.cmp(a.rel, b.rel)
            .then_with(|| u.cmp_rows(&a.args, &b.args))
    });
    rows.dedup();
    Ok(rows)
}

/// Signed source changes taking `installed` to `wanted`.
pub fn diff(
    installed: &BTreeSet<(ir::RelId, Vec<Cell>)>,
    wanted: &BTreeSet<(ir::RelId, Vec<Cell>)>,
) -> Vec<ir::SourceChange> {
    let mut changes = Vec::new();
    for (rel, row) in installed.difference(wanted) {
        changes.push(ir::SourceChange {
            rel: *rel,
            row: row.clone(),
            w: -1,
        });
    }
    for (rel, row) in wanted.difference(installed) {
        changes.push(ir::SourceChange {
            rel: *rel,
            row: row.clone(),
            w: 1,
        });
    }
    changes.sort_by(|a, b| (a.rel, &a.row).cmp(&(b.rel, &b.row)));
    changes
}

/// Rule count by stratum, for the install log.
pub fn census(plan: &Plan) -> BTreeMap<u32, usize> {
    let mut out = BTreeMap::new();
    for rule in &plan.rules {
        *out.entry(rule.stratum).or_default() += 1;
    }
    out
}

impl Plan {
    pub fn levels(&self) -> u32 {
        self.levels
    }
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }
}
