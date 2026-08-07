// Copied verbatim from interp/src/main.rs so the TEXT-keyed rate is compared
// against the int-keyed one with the loader as the only variable.

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

pub type NodeId = u32;
// Inline for arity <= 4, which is every relation this IR carries. A heap
// Vec per derived row put 39.5% of a profiled run inside malloc/free.
pub type Tuple = SmallVec<[u32; 4]>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Term {
    Variable(usize),
    // Constants are part of the generic IR; this workload binds no literals.
    #[allow(dead_code)]
    Constant(NodeId),
}

#[derive(Clone, Debug)]
pub struct Atom {
    pub relation: usize,
    pub args: Vec<Term>,
}

#[derive(Clone, Debug)]
pub struct Rule {
    pub head_relation: usize,
    pub head_args: Vec<Term>,
    pub body: Vec<Atom>,
}

pub struct RelationData {
    pub arity: usize,
    pub rows: Vec<Tuple>,
    // Sharded on column 0, the same layout mono and rxgraph use, so the
    // standings price rule-reading rather than the dedup structure.
    pub members: Vec<FxHashSet<Tuple>>,
    pub index: Vec<FxHashMap<NodeId, Vec<usize>>>,
}

impl RelationData {
    pub fn new(arity: usize, indexed: bool) -> Self {
        RelationData {
            arity,
            rows: Vec::new(),
            members: Vec::new(),
            index: if indexed {
                (0..arity).map(|_| FxHashMap::default()).collect()
            } else {
                Vec::new()
            },
        }
    }

    pub fn insert(&mut self, tuple: Tuple) -> bool {
        let shard = tuple[0] as usize;
        if self.members.len() <= shard {
            self.members.resize(shard + 1, FxHashSet::default());
        }
        if !self.members[shard].insert(tuple.clone()) {
            return false;
        }
        let row = self.rows.len();
        if self.index.len() == self.arity {
            for (column, table) in self.index.iter_mut().enumerate() {
                table.entry(tuple[column]).or_default().push(row);
            }
        }
        self.rows.push(tuple);
        true
    }
}

pub struct Program {
    pub relations: Vec<RelationData>,
    pub rules: Vec<Rule>,
}

// Each body atom reads its relation either in full or only the delta rows.
enum RowSource<'rows> {
    All,
    Delta(&'rows [usize]),
}

// Recursively match body atoms from `position` onward against the binding,
// appending a head tuple to `outputs` once every atom has matched.
fn match_body(
    program: &Program,
    body: &[Atom],
    body_sources: &[RowSource],
    bindings: &mut [Option<NodeId>],
    position: usize,
    head_args: &[Term],
    outputs: &mut Vec<Tuple>,
) {
    if position == body.len() {
        let head: Tuple = head_args
            .iter()
            .map(|term| match term {
                Term::Variable(variable) => bindings[*variable].expect("head variable is bound"),
                Term::Constant(value) => *value,
            })
            .collect();
        outputs.push(head);
        return;
    }

    let atom = &body[position];
    let relation = &program.relations[atom.relation];

    // Column constraints come from constants and already-bound variables;
    // free variables are bound by whichever tuple matches here.
    let mut constraints: SmallVec<[(usize, NodeId); 4]> = SmallVec::new();
    for (column, term) in atom.args.iter().enumerate() {
        match term {
            Term::Variable(variable) => {
                if let Some(value) = bindings[*variable] {
                    constraints.push((column, value));
                }
            }
            Term::Constant(value) => constraints.push((column, *value)),
        }
    }

    // The delta atom scans its listed rows; any constrained column probes the
    // relation index; an all-free atom scans every row once.
    let mut candidates: Vec<usize> = Vec::new();
    match body_sources[position] {
        RowSource::All => {
            if let Some((column, value)) = constraints.first() {
                if let Some(rows) = relation.index[*column].get(value) {
                    candidates.extend_from_slice(rows);
                }
            } else {
                candidates.extend(0..relation.rows.len());
            }
        }
        RowSource::Delta(rows) => {
            candidates.extend_from_slice(rows);
        }
    }

    for row in candidates {
        let tuple = &relation.rows[row];
        let mut compatible = true;
        for (column, value) in &constraints {
            if tuple[*column] != *value {
                compatible = false;
                break;
            }
        }
        if !compatible {
            continue;
        }

        let mut bound_here: SmallVec<[usize; 4]> = SmallVec::new();
        for (column, term) in atom.args.iter().enumerate() {
            if let Term::Variable(variable) = term {
                if bindings[*variable].is_none() {
                    bindings[*variable] = Some(tuple[column]);
                    bound_here.push(*variable);
                }
            }
        }

        match_body(
            program,
            body,
            body_sources,
            bindings,
            position + 1,
            head_args,
            outputs,
        );

        for variable in &bound_here {
            bindings[*variable] = None;
        }
    }
}

fn max_variable_id(rule: &Rule) -> usize {
    let body_terms = rule.body.iter().flat_map(|atom| atom.args.iter());
    rule.head_args
        .iter()
        .chain(body_terms)
        .filter_map(|term| match term {
            Term::Variable(variable) => Some(*variable),
            Term::Constant(_) => None,
        })
        .max()
        .unwrap_or(0)
}

// Compute all head tuples a rule can derive against per-atom row sources.
fn evaluate_rule(program: &Program, rule: &Rule, sources: &[RowSource]) -> Vec<Tuple> {
    // Variable ids are dense, so bindings index directly instead of hashing.
    let mut bindings = vec![None; max_variable_id(rule) + 1];
    let mut outputs = Vec::new();
    match_body(
        program,
        &rule.body,
        sources,
        &mut bindings,
        0,
        &rule.head_args,
        &mut outputs,
    );
    outputs
}

pub fn build_program() -> Program {
    let mut rules = Vec::new();

    // rule 0: reachable(x, y) <- edge(x, y)
    rules.push(Rule {
        head_relation: 1,
        head_args: vec![Term::Variable(0), Term::Variable(1)],
        body: vec![Atom {
            relation: 0,
            args: vec![Term::Variable(0), Term::Variable(1)],
        }],
    });

    // rule 1: reachable(x, z) <- reachable(x, y), edge(y, z)
    rules.push(Rule {
        head_relation: 1,
        head_args: vec![Term::Variable(0), Term::Variable(2)],
        body: vec![
            Atom {
                relation: 1,
                args: vec![Term::Variable(0), Term::Variable(1)],
            },
            Atom {
                relation: 0,
                args: vec![Term::Variable(1), Term::Variable(2)],
            },
        ],
    });

    // A relation read in full needs an index to probe; one read only through the
    // delta is scanned, so indexing it costs a Vec push per row for no probe.
    let probed = probed_relations(&rules);
    let relations = (0..2)
        .map(|relation| RelationData::new(2, probed.contains(&relation)))
        .collect();

    Program { relations, rules }
}

fn probed_relations(rules: &[Rule]) -> FxHashSet<usize> {
    let mut probed = FxHashSet::default();
    for rule in rules {
        for atom in &rule.body {
            if atom.relation != rule.head_relation {
                probed.insert(atom.relation);
            }
        }
    }
    probed
}

// Run semi-naive evaluation: seed from the non-recursive rule, then fold the
// recursive rule until a batch contributes nothing (returns derived count).
pub fn semi_naive(program: &mut Program) -> usize {
    let idb_relations: FxHashSet<usize> = program
        .rules
        .iter()
        .map(|rule| rule.head_relation)
        .collect();

    let is_recursive = |rule: &Rule| {
        rule.body
            .iter()
            .any(|atom| idb_relations.contains(&atom.relation))
    };

    // Seed: each non-recursive rule inserts its head once.
    let mut delta: Vec<Vec<usize>> = vec![Vec::new(); program.relations.len()];
    for rule in &program.rules {
        if is_recursive(rule) {
            continue;
        }
        let sources: Vec<RowSource> = rule.body.iter().map(|_| RowSource::All).collect();
        for head in evaluate_rule(program, rule, &sources) {
            let relation = &mut program.relations[rule.head_relation];
            if relation.insert(head) {
                let row = relation.rows.len() - 1;
                delta[rule.head_relation].push(row);
            }
        }
    }

    // Fold: each recursive rule runs with the idb atom read from the delta.
    loop {
        let mut next_delta: Vec<Vec<usize>> = vec![Vec::new(); program.relations.len()];
        let mut any_new = false;
        for rule in &program.rules {
            if !is_recursive(rule) {
                continue;
            }
            let sources: Vec<RowSource> = rule
                .body
                .iter()
                .map(|atom| {
                    if atom.relation == rule.head_relation {
                        RowSource::Delta(&delta[rule.head_relation])
                    } else {
                        RowSource::All
                    }
                })
                .collect();
            for head in evaluate_rule(program, rule, &sources) {
                let relation = &mut program.relations[rule.head_relation];
                if relation.insert(head) {
                    let row = relation.rows.len() - 1;
                    next_delta[rule.head_relation].push(row);
                    any_new = true;
                }
            }
        }
        if !any_new {
            break;
        }
        delta = next_delta;
    }

    let derived_idb = *idb_relations.iter().next().unwrap();
    program.relations[derived_idb].rows.len()
}
