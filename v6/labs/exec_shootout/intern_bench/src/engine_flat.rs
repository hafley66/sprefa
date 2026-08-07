// engine2's IR walk with the two changes a 4-column TEXT rel forces: a flat
// membership set (no single column to shard on) and a join-prefix index.

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

pub type NodeId = u32;
pub type Tuple = SmallVec<[u32; 4]>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Term {
    Variable(usize),
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

fn pack_prefix(tuple: &[NodeId], key_width: usize) -> u64 {
    match key_width {
        1 => tuple[0] as u64,
        2 => ((tuple[0] as u64) << 32) | tuple[1] as u64,
        other => panic!("key_width must be 1 or 2, got {other}"),
    }
}

pub struct RelationData {
    pub key_width: usize,
    pub rows: Vec<Tuple>,
    pub members: FxHashSet<Tuple>,
    pub index: Option<FxHashMap<u64, Vec<usize>>>,
}

impl RelationData {
    pub fn new(key_width: usize, indexed: bool) -> Self {
        RelationData {
            key_width,
            rows: Vec::new(),
            members: FxHashSet::default(),
            index: if indexed {
                Some(FxHashMap::default())
            } else {
                None
            },
        }
    }

    pub fn insert(&mut self, tuple: Tuple) -> bool {
        if !self.members.insert(tuple.clone()) {
            return false;
        }
        let row = self.rows.len();
        if let Some(index) = self.index.as_mut() {
            index
                .entry(pack_prefix(&tuple, self.key_width))
                .or_default()
                .push(row);
        }
        self.rows.push(tuple);
        true
    }
}

pub struct Program {
    pub relations: Vec<RelationData>,
    pub rules: Vec<Rule>,
}

enum RowSource<'rows> {
    All,
    Delta(&'rows [usize]),
}

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
            .map(|Term::Variable(variable)| bindings[*variable].expect("head variable is bound"))
            .collect();
        outputs.push(head);
        return;
    }

    let atom = &body[position];
    let relation = &program.relations[atom.relation];

    let mut constraints: SmallVec<[(usize, NodeId); 4]> = SmallVec::new();
    let mut prefix: SmallVec<[NodeId; 2]> = SmallVec::new();
    let mut prefix_complete = true;
    for (column, Term::Variable(variable)) in atom.args.iter().enumerate() {
        match bindings[*variable] {
            Some(value) => {
                constraints.push((column, value));
                if column < relation.key_width {
                    prefix.push(value);
                }
            }
            None => {
                if column < relation.key_width {
                    prefix_complete = false;
                }
            }
        }
    }

    // A full join prefix probes the index; anything less scans, which is the
    // plan a 4-column key would get without a composite index.
    let mut candidates: Vec<usize> = Vec::new();
    match body_sources[position] {
        RowSource::All => match (&relation.index, prefix_complete) {
            (Some(index), true) => {
                if let Some(rows) = index.get(&pack_prefix(&prefix, relation.key_width)) {
                    candidates.extend_from_slice(rows);
                }
            }
            _ => candidates.extend(0..relation.rows.len()),
        },
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
        for (column, Term::Variable(variable)) in atom.args.iter().enumerate() {
            if bindings[*variable].is_none() {
                bindings[*variable] = Some(tuple[column]);
                bound_here.push(*variable);
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
        .map(|Term::Variable(variable)| *variable)
        .max()
        .unwrap_or(0)
}

fn evaluate_rule(program: &Program, rule: &Rule, sources: &[RowSource]) -> Vec<Tuple> {
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

fn variables(ids: &[usize]) -> Vec<Term> {
    ids.iter().map(|id| Term::Variable(*id)).collect()
}

// key_width columns identify one endpoint, so an edge row is 2 * key_width wide
// and the recursive rule joins the middle endpoint.
pub fn build_program(key_width: usize) -> Program {
    let width = key_width;
    let left: Vec<usize> = (0..width).collect();
    let middle: Vec<usize> = (width..2 * width).collect();
    let right: Vec<usize> = (2 * width..3 * width).collect();
    let left_middle: Vec<usize> = left.iter().chain(middle.iter()).copied().collect();
    let middle_right: Vec<usize> = middle.iter().chain(right.iter()).copied().collect();
    let left_right: Vec<usize> = left.iter().chain(right.iter()).copied().collect();

    let rules = vec![
        Rule {
            head_relation: 1,
            head_args: variables(&left_middle),
            body: vec![Atom {
                relation: 0,
                args: variables(&left_middle),
            }],
        },
        Rule {
            head_relation: 1,
            head_args: variables(&left_right),
            body: vec![
                Atom {
                    relation: 1,
                    args: variables(&left_middle),
                },
                Atom {
                    relation: 0,
                    args: variables(&middle_right),
                },
            ],
        },
    ];

    let relations = vec![
        RelationData::new(width, true),
        RelationData::new(width, false),
    ];
    Program { relations, rules }
}

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
