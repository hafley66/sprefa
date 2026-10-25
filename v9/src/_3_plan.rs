//! The checked program: ids in place of names, rules lowered to binding slots, relations grouped
//! into strata by SCC over the rule graph. Pure; the only bridge between front and back.

use crate::_0_id::{id_of, Dict};
use crate::_2_ir::{Builtin, Col, Goal, Program, Rel, RelKind, Rule, Term};
use crate::_2a_diag::Diag;
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::{HashMap, HashSet};

pub type RelId = u32;
/// Text: the Dict id of its bytes. Int: the i64's bits.
pub type Value = u64;
pub type Row = Vec<Value>;

/// `Var` is a slot in the rule's binding row; slots are numbered in binding order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PTerm {
    Var(u32),
    Const(Value),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PRel {
    pub name: String,
    pub cols: Vec<Col>,
    pub kind: RelKind,
    /// For an Answer rel: the Request rel of its service, whose cols prefix this rel's cols.
    pub request: Option<RelId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PAtom {
    pub rel: RelId,
    pub args: Vec<PTerm>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PGoal {
    Pos(PAtom),
    Neg(PAtom),
    Call(Builtin, Vec<PTerm>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PRule {
    pub head: PAtom,
    pub body: Vec<PGoal>,
    /// Slot to var name; its length is the binding row's width.
    pub vars: Vec<String>,
}

/// One SCC holding a Derived, Request or Answer rel, in dependency order: every rel a stratum
/// reads is a Base rel or lives in this or an earlier stratum. An Answer rel sits after its
/// Request rel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stratum {
    pub rels: Vec<RelId>,
    pub recursive: bool,
    /// Indexes into `Plan::rules` of every rule whose head is in `rels`.
    pub rules: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub program: Program,
    pub rels: Vec<PRel>,
    pub rules: Vec<PRule>,
    pub strata: Vec<Stratum>,
    /// Id of the program's Debug text; the store keeps it to refuse a reopen under another plan.
    pub hash: u64,
}

pub fn check(program: Program, dict: &mut Dict) -> Result<Plan, Vec<Diag>> {
    let mut diags = Vec::new();
    let rels = lower_rels(&program.rels, &mut diags);
    let ids: HashMap<&str, RelId> = program
        .rels
        .iter()
        .enumerate()
        .rev()
        .map(|(index, rel)| (rel.name.as_str(), index as RelId))
        .collect();
    let mut texts = Vec::new();
    let rules: Vec<PRule> = program
        .rules
        .iter()
        .enumerate()
        .map(|(index, rule)| {
            Lower {
                rule: index,
                program: &program,
                ids: &ids,
                diags: &mut diags,
                texts: &mut texts,
            }
            .rule(rule)
        })
        .collect();
    if !diags.is_empty() {
        return Err(diags);
    }
    let strata = stratify(&rels, &rules)?;
    for text in texts {
        dict.intern(text.as_bytes());
    }
    let hash = id_of(format!("{program:?}").as_bytes());
    Ok(Plan {
        program,
        rels,
        rules,
        strata,
        hash,
    })
}

fn lower_rels(decls: &[Rel], diags: &mut Vec<Diag>) -> Vec<PRel> {
    let mut names = HashSet::new();
    let mut requests: HashMap<&str, RelId> = HashMap::new();
    let mut answers = HashSet::new();
    for (index, rel) in decls.iter().enumerate() {
        if !names.insert(rel.name.as_str()) {
            diags.push(Diag::Duplicate {
                what: "rel",
                name: rel.name.clone(),
            });
        }
        let mut cols = HashSet::new();
        for (col, _) in &rel.cols {
            if !cols.insert(col) {
                diags.push(Diag::Duplicate {
                    what: "column",
                    name: format!("{}.{col}", rel.name),
                });
            }
        }
        match &rel.kind {
            RelKind::Request { service } if requests.insert(service, index as RelId).is_some() => {
                diags.push(Diag::Duplicate {
                    what: "request service",
                    name: service.clone(),
                })
            }
            RelKind::Answer { service } if !answers.insert(service) => {
                diags.push(Diag::Duplicate {
                    what: "answer service",
                    name: service.clone(),
                })
            }
            _ => {}
        }
    }
    decls
        .iter()
        .map(|rel| {
            let request = match &rel.kind {
                RelKind::Answer { service } => match requests.get(service.as_str()) {
                    None => {
                        diags.push(Diag::Unknown {
                            what: "service",
                            name: service.clone(),
                            rule: None,
                        });
                        None
                    }
                    Some(&request) => {
                        let prefix = &decls[request as usize].cols;
                        if !rel.cols.starts_with(prefix) {
                            diags.push(Diag::AnswerShape {
                                answer: rel.name.clone(),
                                request: decls[request as usize].name.clone(),
                            });
                        }
                        Some(request)
                    }
                },
                _ => None,
            };
            PRel {
                name: rel.name.clone(),
                cols: rel.cols.iter().map(|(_, col)| *col).collect(),
                kind: rel.kind.clone(),
                request,
            }
        })
        .collect()
}

/// One rule's lowering. Goals are read in written order; a var is bound by the first Pos atom or
/// `add` output that names it.
struct Lower<'p, 'd> {
    rule: usize,
    program: &'p Program,
    ids: &'d HashMap<&'p str, RelId>,
    diags: &'d mut Vec<Diag>,
    texts: &'d mut Vec<&'p str>,
}

#[derive(Default)]
struct Scope<'p> {
    slots: HashMap<&'p str, u32>,
    types: HashMap<&'p str, Col>,
    vars: Vec<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum Use {
    Binds,
    NeedsBound(Option<usize>),
}

impl<'p> Lower<'p, '_> {
    fn rule(&mut self, rule: &'p Rule) -> PRule {
        let mut scope = Scope::default();
        let body = rule
            .body
            .iter()
            .enumerate()
            .map(|(goal, written)| match written {
                Goal::Pos(atom) => {
                    PGoal::Pos(self.atom(&mut scope, &atom.rel, &atom.args, Use::Binds))
                }
                Goal::Neg(atom) => PGoal::Neg(self.atom(
                    &mut scope,
                    &atom.rel,
                    &atom.args,
                    Use::NeedsBound(Some(goal)),
                )),
                Goal::Call(builtin, args) => {
                    PGoal::Call(*builtin, self.builtin(&mut scope, goal, *builtin, args))
                }
            })
            .collect();
        if let Some(&id) = self.ids.get(rule.head.rel.as_str()) {
            let kind = &self.program.rels[id as usize].kind;
            if matches!(kind, RelKind::Base | RelKind::Answer { .. }) {
                self.diags.push(Diag::HeadKind {
                    rule: self.rule,
                    rel: rule.head.rel.clone(),
                    kind: kind.clone(),
                });
            }
        }
        let head = self.atom(
            &mut scope,
            &rule.head.rel,
            &rule.head.args,
            Use::NeedsBound(None),
        );
        PRule {
            head,
            body,
            vars: scope.vars,
        }
    }

    fn atom(&mut self, scope: &mut Scope<'p>, rel: &'p str, args: &'p [Term], how: Use) -> PAtom {
        let id = self.ids.get(rel).copied();
        let cols: Option<Vec<Col>> = match id {
            None => {
                self.diags.push(Diag::Unknown {
                    what: "rel",
                    name: rel.to_string(),
                    rule: Some(self.rule),
                });
                None
            }
            Some(id) => {
                let cols = &self.program.rels[id as usize].cols;
                if cols.len() == args.len() {
                    Some(cols.iter().map(|(_, col)| *col).collect())
                } else {
                    self.diags.push(Diag::Arity {
                        rule: self.rule,
                        name: rel.to_string(),
                        expected: cols.len(),
                        found: args.len(),
                    });
                    None
                }
            }
        };
        let args = args
            .iter()
            .enumerate()
            .map(|(index, arg)| {
                if let Some(cols) = &cols {
                    self.expect(scope, arg, cols[index]);
                }
                self.term(scope, arg, how)
            })
            .collect();
        PAtom {
            rel: id.unwrap_or(RelId::MAX),
            args,
        }
    }

    fn builtin(
        &mut self,
        scope: &mut Scope<'p>,
        goal: usize,
        builtin: Builtin,
        args: &'p [Term],
    ) -> Vec<PTerm> {
        let (name, arity) = match builtin {
            Builtin::Add => ("add", 3),
            Builtin::Lt => ("lt", 2),
            Builtin::Neq => ("neq", 2),
        };
        if args.len() != arity {
            self.diags.push(Diag::Arity {
                rule: self.rule,
                name: name.to_string(),
                expected: arity,
                found: args.len(),
            });
            return args
                .iter()
                .map(|arg| self.term(scope, arg, Use::Binds))
                .collect();
        }
        match builtin {
            Builtin::Add | Builtin::Lt => {
                for arg in args {
                    self.expect(scope, arg, Col::Int);
                }
            }
            Builtin::Neq => {
                if let Some(expected) = type_of(scope, &args[0]) {
                    self.expect(scope, &args[1], expected);
                }
            }
        }
        args.iter()
            .enumerate()
            .map(|(index, arg)| {
                let how = if builtin == Builtin::Add && index == 2 {
                    Use::Binds
                } else {
                    Use::NeedsBound(Some(goal))
                };
                self.term(scope, arg, how)
            })
            .collect()
    }

    /// Records a var's first type; a later different one, or a constant of the other type, is a
    /// `Type` diag.
    fn expect(&mut self, scope: &mut Scope<'p>, term: &'p Term, expected: Col) {
        let found = match term {
            Term::Var(name) => *scope.types.entry(name).or_insert(expected),
            Term::Text(_) => Col::Text,
            Term::Int(_) => Col::Int,
        };
        if found != expected {
            self.diags.push(Diag::Type {
                rule: self.rule,
                term: written(term),
                expected,
                found,
            });
        }
    }

    fn term(&mut self, scope: &mut Scope<'p>, term: &'p Term, how: Use) -> PTerm {
        match term {
            Term::Var(name) => {
                if let Some(&slot) = scope.slots.get(name.as_str()) {
                    return PTerm::Var(slot);
                }
                if let Use::NeedsBound(goal) = how {
                    self.diags.push(Diag::Unbound {
                        rule: self.rule,
                        goal,
                        var: name.clone(),
                    });
                }
                let slot = scope.vars.len() as u32;
                scope.slots.insert(name, slot);
                scope.vars.push(name.clone());
                PTerm::Var(slot)
            }
            Term::Text(value) => {
                self.texts.push(value);
                PTerm::Const(id_of(value.as_bytes()))
            }
            Term::Int(value) => PTerm::Const(*value as Value),
        }
    }
}

fn type_of(scope: &Scope<'_>, term: &Term) -> Option<Col> {
    match term {
        Term::Var(name) => scope.types.get(name.as_str()).copied(),
        Term::Text(_) => Some(Col::Text),
        Term::Int(_) => Some(Col::Int),
    }
}

fn written(term: &Term) -> String {
    match term {
        Term::Var(name) => name.clone(),
        Term::Text(value) => format!("{value:?}"),
        Term::Int(value) => value.to_string(),
    }
}

/// Rule graph: an edge from each body rel to the head rel, weighted `true` when negated.
/// `tarjan_scc` yields SCCs in reverse topological order.
fn stratify(rels: &[PRel], rules: &[PRule]) -> Result<Vec<Stratum>, Vec<Diag>> {
    let mut graph = DiGraph::<(), bool>::with_capacity(rels.len(), rules.len());
    let nodes: Vec<NodeIndex> = rels.iter().map(|_| graph.add_node(())).collect();
    // An Answer rel is its input semijoined with its Request rel, so it reads the Request rel.
    for (index, rel) in rels.iter().enumerate() {
        if let Some(request) = rel.request {
            graph.add_edge(nodes[request as usize], nodes[index], false);
        }
    }
    for rule in rules {
        for goal in &rule.body {
            match goal {
                PGoal::Pos(atom) => graph.add_edge(
                    nodes[atom.rel as usize],
                    nodes[rule.head.rel as usize],
                    false,
                ),
                PGoal::Neg(atom) => graph.add_edge(
                    nodes[atom.rel as usize],
                    nodes[rule.head.rel as usize],
                    true,
                ),
                PGoal::Call(..) => continue,
            };
        }
    }
    let sccs = petgraph::algo::tarjan_scc(&graph);
    let mut component = vec![0; rels.len()];
    for (index, scc) in sccs.iter().enumerate() {
        for node in scc {
            component[node.index()] = index;
        }
    }
    let mut diags = Vec::new();
    for (index, rule) in rules.iter().enumerate() {
        for goal in &rule.body {
            if let PGoal::Neg(atom) = goal {
                if component[atom.rel as usize] == component[rule.head.rel as usize] {
                    diags.push(Diag::NegationCycle {
                        rule: index,
                        head: rels[rule.head.rel as usize].name.clone(),
                        negated: rels[atom.rel as usize].name.clone(),
                    });
                }
            }
        }
    }
    if !diags.is_empty() {
        return Err(diags);
    }
    Ok(sccs
        .iter()
        .rev()
        .filter(|scc| {
            scc.iter().any(|node| {
                matches!(
                    rels[node.index()].kind,
                    RelKind::Derived | RelKind::Request { .. } | RelKind::Answer { .. }
                )
            })
        })
        .map(|scc| {
            let mut members: Vec<RelId> = scc.iter().map(|node| node.index() as RelId).collect();
            members.sort_unstable();
            Stratum {
                recursive: scc.len() > 1 || graph.contains_edge(scc[0], scc[0]),
                rules: (0..rules.len())
                    .filter(|&index| members.contains(&rules[index].head.rel))
                    .collect(),
                rels: members,
            }
        })
        .collect())
}
