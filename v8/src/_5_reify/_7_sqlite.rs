//! `dl8 emit sqlite`: each derived relation as one `sqlite_ivm` virtual table.
//! sqlite_ivm reads ordinary tables only (`sqlite_ivm/src/0b_relational.rs:655`), so upstream relations inline as CTEs.

use super::api::Stop;
use crate::_6_eval::kernel::{linear, IntCmp, Kernel};
use crate::_6_eval::program::{
    AggregateKind, Arg, Fold as FoldSpec, Folding, Goal, Order, Polarity, Program, Rule, Seed,
};
use crate::_6_eval::stratify::stratify;
use crate::_6_eval::{Diagnostic, Term, TermId, Universe};
use crate::_9_runtime::store::{kernel_owned, nameable};
use crate::_9_runtime::CellKind;
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

pub struct SqliteArtifact {
    pub views: Vec<SqliteView>,
    pub diagnostics: Vec<Diagnostic>,
}

pub struct SqliteView {
    pub relation: String,
    pub ddl: String,
    pub stratum: usize,
}

/// `term.kind` as `_9_runtime/_1_sqlite.rs:15-20` writes it.
const KIND_INT: i64 = 0;
const KIND_FLOAT: i64 = 1;
const KIND_BOOL: i64 = 2;
const KIND_ATOM: i64 = 3;
const KIND_STR: i64 = 4;
const KIND_COMPOUND: i64 = 5;

/// A derived relation as the store keys it: one table per relation and arity.
type Key = (TermId, usize);

/// Why one rule has no SQL.
#[derive(Clone, Debug)]
enum Unsupported {
    Kernel(String),
    Relation(String),
    Fold(String),
    Unbound(String),
    Constant(TermId),
    MalformedAggregate,
    NonlinearRecursion,
    RecursionWithoutAnchor,
    MixedColumn(usize),
    SeededRuleHead,
    UnnamedRelation,
    UnstoredRelation(String),
    DependsOn(String),
    NoSource,
}

/// One view per derived relation, in dependency order. `prefix` is the table
/// prefix `dl8 eval --db` derives from the program's JSON file name.
pub fn emit_sqlite(
    u: &mut Universe,
    program: &Program,
    names: &HashMap<String, TermId>,
    prefix: &str,
) -> Result<SqliteArtifact, Stop> {
    let (strata, stratify_diagnostics) = stratify(u, program);
    if !stratify_diagnostics.is_empty() {
        return Ok(SqliteArtifact {
            views: vec![],
            diagnostics: stratify_diagnostics,
        });
    }
    let catalog = Catalog::new(u, program, names, prefix);
    let mut failures: Vec<(Key, usize, Unsupported)> = Vec::new();
    let mut failed: HashSet<Key> = HashSet::new();

    for key in &catalog.order {
        let rules = &catalog.derived[key];
        let reason = match catalog.names.get(&key.0) {
            None => Some(Unsupported::UnnamedRelation),
            Some(_) if catalog.seeded.contains(key) => Some(Unsupported::SeededRuleHead),
            Some(_) => None,
        };
        if let Some(reason) = reason {
            for ordinal in 0..rules.len() {
                failures.push((*key, ordinal, reason.clone()));
            }
            failed.insert(*key);
        }
    }

    let components = catalog.components();
    let mut kinds: HashMap<Key, Vec<CellKind>> = HashMap::new();
    let mut sql: HashMap<usize, Vec<String>> = HashMap::new();
    for (index, component) in components.iter().enumerate() {
        let members: Vec<Key> = component
            .iter()
            .copied()
            .filter(|key| !failed.contains(key))
            .collect();
        if members.len() != component.len() {
            mark_component(&catalog, component, &mut failed, &mut failures);
            continue;
        }
        let mut errors = Vec::new();
        for key in component {
            for (ordinal, rule) in catalog.derived[key].iter().enumerate() {
                if let Some(upstream) = catalog
                    .reads(rule)
                    .into_iter()
                    .find(|read| failed.contains(read) && !component.contains(read))
                {
                    let name = catalog.name(upstream.0);
                    errors.push((*key, ordinal, Unsupported::DependsOn(name)));
                }
            }
        }
        if errors.is_empty() {
            match lower_component(&catalog, component, &mut kinds) {
                Ok(pieces) => {
                    sql.insert(index, pieces);
                    continue;
                }
                Err(found) => errors = found,
            }
        }
        failures.extend(errors);
        mark_component(&catalog, component, &mut failed, &mut failures);
    }

    let mut views = Vec::new();
    for (index, component) in components.iter().enumerate() {
        if !sql.contains_key(&index) {
            continue;
        }
        for key in component {
            let needed = catalog.upstream(&components, index);
            let mut ctes = Vec::new();
            for other in needed {
                ctes.extend(sql[&other].iter().cloned());
            }
            let recursive = ctes.iter().any(|cte| cte.contains(RECURSIVE_MARK));
            let ctes: Vec<String> = ctes
                .into_iter()
                .map(|cte| cte.replace(RECURSIVE_MARK, ""))
                .collect();
            let columns = column_names(&kinds[key]).join(", ");
            let query = format!(
                "WITH {}{} SELECT {columns} FROM {}",
                if recursive { "RECURSIVE " } else { "" },
                ctes.join(", "),
                catalog.cte_name(*key),
            );
            let view = quote_identifier(&format!("{prefix}.{}_v{}", catalog.name(key.0), key.1));
            views.push(SqliteView {
                relation: catalog.name(key.0),
                ddl: format!(
                    "CREATE VIRTUAL TABLE {view} USING sqlite_ivm({})",
                    quote_text(&query)
                ),
                stratum: strata.level(key.0) as usize,
            });
        }
    }

    let mut named: Vec<(String, usize, Unsupported)> = failures
        .into_iter()
        .map(|(key, ordinal, reason)| (catalog.name(key.0), ordinal, reason))
        .collect();
    named.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
    let mut diagnostics: Vec<Diagnostic> = named
        .iter()
        .map(|(name, ordinal, reason)| Diagnostic {
            phase: "emit",
            payload: unsupported_payload(u, name, *ordinal, reason),
        })
        .collect();
    diagnostics.dedup();
    Ok(SqliteArtifact { views, diagnostics })
}

/// Every member of a component that has no diagnostic of its own reads the
/// failing member through the shared CTE.
fn mark_component(
    catalog: &Catalog,
    component: &[Key],
    failed: &mut HashSet<Key>,
    failures: &mut Vec<(Key, usize, Unsupported)>,
) {
    let culprit = component
        .iter()
        .find(|key| failed.contains(*key) || failures.iter().any(|f| f.0 == **key))
        .copied();
    for key in component {
        let has_own = failures.iter().any(|f| f.0 == *key);
        if !has_own {
            if let Some(culprit) = culprit {
                let name = catalog.name(culprit.0);
                for ordinal in 0..catalog.derived[key].len() {
                    failures.push((*key, ordinal, Unsupported::DependsOn(name.clone())));
                }
            }
        }
        failed.insert(*key);
    }
}

/// Marks a CTE that recurses; the view's `WITH` takes `RECURSIVE` when any does.
const RECURSIVE_MARK: &str = "\u{0}recursive\u{0}";

fn unsupported_payload(u: &mut Universe, name: &str, ordinal: usize, reason: &Unsupported) -> TermId {
    let relation = u.atom(name);
    let ordinal = u.int(ordinal as i64);
    let rule = u.compound("rule", vec![relation, ordinal]);
    let reason = match reason {
        Unsupported::Kernel(kernel) => u.atom(kernel),
        Unsupported::Relation(name) => {
            let name = u.atom(name);
            u.compound("relation", vec![name])
        }
        Unsupported::Fold(step) => {
            let step = u.atom(step);
            u.compound("fold", vec![step])
        }
        Unsupported::Unbound(variable) => {
            let variable = u.atom(variable);
            u.compound("unbound", vec![variable])
        }
        Unsupported::Constant(term) => u.compound("constant", vec![*term]),
        Unsupported::MalformedAggregate => u.atom("malformed_aggregate"),
        Unsupported::NonlinearRecursion => u.atom("nonlinear_recursion"),
        Unsupported::RecursionWithoutAnchor => u.atom("recursion_without_anchor"),
        Unsupported::MixedColumn(position) => {
            let position = u.int(*position as i64);
            u.compound("mixed_column", vec![position])
        }
        Unsupported::SeededRuleHead => u.atom("seeded_rule_head"),
        Unsupported::UnnamedRelation => u.atom("unnamed_relation"),
        Unsupported::UnstoredRelation(name) => {
            let name = u.atom(name);
            u.compound("unstored_relation", vec![name])
        }
        Unsupported::DependsOn(name) => {
            let name = u.atom(name);
            u.compound("depends_on", vec![name])
        }
        Unsupported::NoSource => u.atom("no_source"),
    };
    u.compound("emit_sqlite_unsupported", vec![rule, reason])
}

struct Catalog<'a> {
    u: &'a Universe,
    prefix: String,
    /// The name `SqliteRowStore::name_relations` picks: the first sorted
    /// nameable name of a relation.
    names: HashMap<TermId, String>,
    seeded: HashSet<Key>,
    derived: HashMap<Key, Vec<&'a Rule>>,
    order: Vec<Key>,
}

impl<'a> Catalog<'a> {
    fn new(
        u: &'a Universe,
        program: &'a Program,
        declared: &HashMap<String, TermId>,
        prefix: &str,
    ) -> Catalog<'a> {
        let sorted: BTreeMap<&String, &TermId> = declared.iter().collect();
        let mut names = HashMap::new();
        for (name, rel) in sorted {
            if nameable(name) {
                names.entry(*rel).or_insert_with(|| name.clone());
            }
        }
        let seeded = program
            .seeds
            .iter()
            .map(|seed| (seed.rel, seed.args.len()))
            .collect();
        let mut derived: HashMap<Key, Vec<&Rule>> = HashMap::new();
        for rule in &program.rules {
            if !kernel_owned(u, rule.rel) {
                derived
                    .entry((rule.rel, rule.head.len()))
                    .or_default()
                    .push(rule);
            }
        }
        let mut order: Vec<Key> = derived.keys().copied().collect();
        order.sort_by(|a, b| {
            let name = |key: &Key| names.get(&key.0).cloned().unwrap_or_default();
            (name(a), a.1, a.0 .0).cmp(&(name(b), b.1, b.0 .0))
        });
        Catalog {
            u,
            prefix: prefix.to_string(),
            names,
            seeded,
            derived,
            order,
        }
    }

    fn name(&self, rel: TermId) -> String {
        self.names
            .get(&rel)
            .cloned()
            .unwrap_or_else(|| self.u.display(rel).to_string())
    }

    fn cte_name(&self, key: Key) -> String {
        quote_identifier(&format!("{}_v{}", self.name(key.0), key.1))
    }

    fn table_name(&self, key: Key) -> String {
        quote_identifier(&format!("{}.{}_a{}", self.prefix, self.name(key.0), key.1))
    }

    fn store_object(&self, object: &str) -> String {
        quote_identifier(&format!("{}.{object}", self.prefix))
    }

    /// Derived relations a rule's body reads, negated or not.
    fn reads(&self, rule: &Rule) -> Vec<Key> {
        rule.body
            .iter()
            .map(|goal| (goal.rel, goal.args.len()))
            .filter(|key| self.derived.contains_key(key))
            .collect()
    }

    /// Strongly connected components, each after every component it reads.
    fn components(&self) -> Vec<Vec<Key>> {
        let mut reach: HashMap<Key, HashSet<Key>> = HashMap::new();
        for key in &self.order {
            let mut seen: HashSet<Key> = HashSet::new();
            let mut stack: Vec<Key> = self.derived[key]
                .iter()
                .flat_map(|rule| self.reads(rule))
                .collect();
            while let Some(next) = stack.pop() {
                if seen.insert(next) {
                    stack.extend(self.derived[&next].iter().flat_map(|rule| self.reads(rule)));
                }
            }
            reach.insert(*key, seen);
        }
        let mut assigned: HashSet<Key> = HashSet::new();
        let mut components: Vec<Vec<Key>> = Vec::new();
        for key in &self.order {
            if assigned.contains(key) {
                continue;
            }
            let component: Vec<Key> = self
                .order
                .iter()
                .copied()
                .filter(|other| {
                    other == key || (reach[key].contains(other) && reach[other].contains(key))
                })
                .collect();
            assigned.extend(component.iter().copied());
            components.push(component);
        }
        let mut placed: Vec<Vec<Key>> = Vec::new();
        let mut done: HashSet<Key> = HashSet::new();
        while !components.is_empty() {
            let ready = components
                .iter()
                .position(|component| {
                    component.iter().all(|key| {
                        reach[key]
                            .iter()
                            .all(|read| done.contains(read) || component.contains(read))
                    })
                })
                .unwrap_or(0);
            let component = components.remove(ready);
            done.extend(component.iter().copied());
            placed.push(component);
        }
        placed
    }

    /// Indices of the components `index` reads, itself last.
    fn upstream(&self, components: &[Vec<Key>], index: usize) -> Vec<usize> {
        let mut needed: BTreeSet<usize> = BTreeSet::new();
        let mut stack = vec![index];
        while let Some(at) = stack.pop() {
            if !needed.insert(at) {
                continue;
            }
            for key in &components[at] {
                for rule in &self.derived[key] {
                    for read in self.reads(rule) {
                        if let Some(found) = components.iter().position(|c| c.contains(&read)) {
                            stack.push(found);
                        }
                    }
                }
            }
        }
        needed.into_iter().collect()
    }

    fn recursive(&self, component: &[Key]) -> bool {
        component.len() > 1
            || component.iter().any(|key| {
                self.derived[key]
                    .iter()
                    .any(|rule| self.reads(rule).contains(key))
            })
    }
}

/// One component as CTE definitions. Column kinds settle by fixpoint: a
/// member column read before any rule fixes it lowers as a term id.
fn lower_component(
    catalog: &Catalog,
    component: &[Key],
    kinds: &mut HashMap<Key, Vec<CellKind>>,
) -> Result<Vec<String>, Vec<(Key, usize, Unsupported)>> {
    let recursive = catalog.recursive(component);
    let width = component.iter().map(|key| key.1).max().unwrap_or(0);
    let shared = recursive.then(|| {
        let first = component[0];
        quote_identifier(&format!("{}_r{}", catalog.name(first.0), first.1))
    });
    let layout = |key: &Key| {
        shared.as_ref().map(|name| Member {
            cte: name.clone(),
            member: component.iter().position(|k| k == key).unwrap(),
            width,
        })
    };

    let mut slots: HashMap<Key, Vec<Slot>> = component
        .iter()
        .map(|key| (*key, vec![Slot::Open; key.1]))
        .collect();
    for _ in 0..=(width * component.len() + 1) {
        for key in component {
            kinds.insert(*key, slots[key].iter().map(|slot| slot.kind()).collect());
        }
        let mut next: HashMap<Key, Vec<Slot>> = component
            .iter()
            .map(|key| (*key, vec![Slot::Open; key.1]))
            .collect();
        for key in component {
            for rule in &catalog.derived[key] {
                let lowering = Lowering::new(catalog, kinds, component, shared.as_deref());
                let Ok((_, head)) = lowering.body(rule) else {
                    continue;
                };
                for (position, value) in head.iter().enumerate() {
                    let slot = &mut next.get_mut(key).unwrap()[position];
                    *slot = slot.merge(value.kind());
                }
            }
        }
        if next == slots {
            break;
        }
        slots = next;
    }

    let mut errors = Vec::new();
    for key in component {
        kinds.insert(*key, slots[key].iter().map(|slot| slot.kind()).collect());
        if let Some(position) = slots[key].iter().position(|slot| *slot == Slot::Mixed) {
            errors.push((*key, 0, Unsupported::MixedColumn(position)));
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    let mut anchors = Vec::new();
    let mut steps = Vec::new();
    for key in component {
        for (ordinal, rule) in catalog.derived[key].iter().enumerate() {
            let lowering = Lowering::new(catalog, kinds, component, shared.as_deref());
            let own = rule
                .body
                .iter()
                .filter(|goal| {
                    goal.polarity == Polarity::Positive
                        && component.contains(&(goal.rel, goal.args.len()))
                })
                .count();
            if recursive && own > 1 {
                errors.push((*key, ordinal, Unsupported::NonlinearRecursion));
                continue;
            }
            let rendered = lowering.body(rule).and_then(|(scope, head)| {
                lowering.render(scope, &head, &kinds[key], layout(key), !recursive)
            });
            match rendered {
                Ok(select) if own == 0 => anchors.push(select),
                Ok(select) => steps.push(select),
                Err(reason) => errors.push((*key, ordinal, reason)),
            }
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    let Some(shared) = shared else {
        let key = component[0];
        let columns = column_names(&kinds[&key]);
        return Ok(vec![format!(
            "{}({}) AS ({})",
            catalog.cte_name(key),
            columns.join(", "),
            anchors.join(" UNION ")
        )]);
    };
    if anchors.is_empty() {
        let key = component[0];
        return Err((0..catalog.derived[&key].len())
            .map(|ordinal| (key, ordinal, Unsupported::RecursionWithoutAnchor))
            .collect());
    }
    let mut columns = vec![quote_identifier("member")];
    columns.extend((0..width).map(|position| quote_identifier(&format!("c{position}"))));
    let mut pieces = vec![format!(
        "{RECURSIVE_MARK}{shared}({}) AS ({})",
        columns.join(", "),
        anchors
            .iter()
            .chain(steps.iter())
            .cloned()
            .collect::<Vec<_>>()
            .join(" UNION ")
    )];
    for (member, key) in component.iter().enumerate() {
        let names = column_names(&kinds[key]);
        let values: Vec<String> = if key.1 == 0 {
            vec!["1".to_string()]
        } else {
            (0..key.1)
                .map(|position| quote_identifier(&format!("c{position}")))
                .collect()
        };
        pieces.push(format!(
            "{}({}) AS (SELECT {} FROM {shared} WHERE {} = {member})",
            catalog.cte_name(*key),
            names.join(", "),
            values.join(", "),
            quote_identifier("member"),
        ));
    }
    Ok(pieces)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Slot {
    Open,
    Fixed(CellKind),
    Mixed,
}

impl Slot {
    fn merge(self, kind: Option<CellKind>) -> Slot {
        match (self, kind) {
            (slot, None) => slot,
            (Slot::Open, Some(kind)) => Slot::Fixed(kind),
            (Slot::Fixed(held), Some(kind)) if held == kind => self,
            _ => Slot::Mixed,
        }
    }

    fn kind(self) -> CellKind {
        match self {
            Slot::Fixed(kind) => kind,
            _ => CellKind::Term,
        }
    }
}

/// The recursive component's one CTE: `member` says which relation a row
/// belongs to, columns past a member's arity hold 0.
struct Member {
    cte: String,
    member: usize,
    width: usize,
}

#[derive(Clone)]
struct Value {
    sql: String,
    kind: CellKind,
}

enum Head {
    Value(Value),
    Ground(TermId),
    Fold(Reduce, Value),
}

#[derive(Clone, Copy)]
enum Reduce {
    Count,
    Sum,
    Min,
    Max,
    Add(i64),
}

impl Head {
    /// `count`, `sum` and an `int_add` fold mint an integer no arena holds, so
    /// their column carries the value under the store's `_int` suffix.
    fn kind(&self) -> Option<CellKind> {
        match self {
            Head::Value(value) => Some(value.kind),
            Head::Ground(_) => None,
            Head::Fold(Reduce::Min | Reduce::Max, subject) => Some(subject.kind),
            Head::Fold(_, _) => Some(CellKind::Int),
        }
    }
}

struct Lowering<'a> {
    catalog: &'a Catalog<'a>,
    kinds: &'a HashMap<Key, Vec<CellKind>>,
    component: &'a [Key],
    shared: Option<&'a str>,
    next: Cell<usize>,
}

/// The FROM list, the WHERE conjuncts and the payload joins of one SELECT.
struct Scope {
    from: Vec<String>,
    conditions: Vec<String>,
    payloads: HashMap<String, Payload>,
    const_sym: Option<String>,
}

#[derive(Clone)]
struct Payload {
    wrapper: String,
    payload: String,
    text: Option<String>,
}

enum Source {
    Table(String),
    Member(String, usize),
    Kernel(Kernel),
}

impl<'a> Lowering<'a> {
    fn new(
        catalog: &'a Catalog<'a>,
        kinds: &'a HashMap<Key, Vec<CellKind>>,
        component: &'a [Key],
        shared: Option<&'a str>,
    ) -> Lowering<'a> {
        Lowering {
            catalog,
            kinds,
            component,
            shared,
            next: Cell::new(0),
        }
    }

    fn alias(&self) -> String {
        let n = self.next.get();
        self.next.set(n + 1);
        format!("t{n}")
    }

    fn variable_name(&self, rule: &Rule, variable: usize) -> String {
        let identity = rule.vars[variable];
        let u = self.catalog.u;
        match u.functor(identity).and_then(|(_, args)| args.last().copied()) {
            Some(last) => match u.get(last) {
                Term::Atom(s) => u.sym_str(*s).to_string(),
                _ => u.display(identity).to_string(),
            },
            None => u.display(identity).to_string(),
        }
    }

    fn source(&self, goal: &Goal) -> Result<Source, Unsupported> {
        let u = self.catalog.u;
        let key = (goal.rel, goal.args.len());
        if kernel_owned(u, goal.rel) {
            if let Some(kernel @ (Kernel::Int(_) | Kernel::IntAdd | Kernel::TermLt)) =
                Kernel::of(u, goal.rel)
            {
                return Ok(Source::Kernel(kernel));
            }
            let kernel = u
                .unary(goal.rel, "ref")
                .and_then(|inner| u.unary(inner, "kernel"))
                .and_then(|name| u.functor_or_atom(name).map(|(name, _)| name.to_string()));
            return Err(match kernel {
                Some(name) => Unsupported::Kernel(name),
                None => Unsupported::Relation(self.catalog.name(goal.rel)),
            });
        }
        if let (Some(shared), Some(member)) =
            (self.shared, self.component.iter().position(|k| *k == key))
        {
            return Ok(Source::Member(shared.to_string(), member));
        }
        if self.catalog.derived.contains_key(&key) {
            return Ok(Source::Table(self.catalog.cte_name(key)));
        }
        if !self.catalog.names.contains_key(&goal.rel) {
            return Err(Unsupported::UnnamedRelation);
        }
        if !self.catalog.seeded.contains(&key) {
            return Err(Unsupported::UnstoredRelation(self.catalog.name(goal.rel)));
        }
        Ok(Source::Table(self.catalog.table_name(key)))
    }

    /// The column of `position` in a goal's source, as the source stores it.
    fn column(&self, goal: &Goal, source: &Source, alias: &str, position: usize) -> Value {
        let key = (goal.rel, goal.args.len());
        let kind = self
            .kinds
            .get(&key)
            .map(|kinds| kinds[position])
            .unwrap_or(CellKind::Term);
        let name = match source {
            Source::Member(..) => format!("c{position}"),
            _ => column_name(position, kind),
        };
        Value {
            sql: format!("{alias}.{}", quote_identifier(&name)),
            kind,
        }
    }

    fn body(&self, rule: &Rule) -> Result<(Scope, Vec<Head>), Unsupported> {
        let mut scope = Scope::new();
        let mut vars: HashMap<usize, Value> = HashMap::new();
        for goal in &rule.body {
            let source = self.source(goal)?;
            match (goal.polarity, &source) {
                (Polarity::Positive, Source::Kernel(kernel)) => {
                    let condition = self.kernel(&mut scope, rule, &mut vars, goal, *kernel, true)?;
                    scope.conditions.push(condition);
                }
                (Polarity::Negative, Source::Kernel(kernel)) => {
                    let condition = self.kernel(&mut scope, rule, &mut vars, goal, *kernel, false)?;
                    scope.conditions.push(format!("NOT ({condition})"));
                }
                (Polarity::Positive, _) => {
                    let alias = self.alias();
                    self.relation(&mut scope, rule, &mut vars, goal, &source, &alias, true)?;
                }
                (Polarity::Negative, _) => {
                    let alias = self.alias();
                    let mut inner = Scope::new();
                    self.relation(&mut inner, rule, &mut vars, goal, &source, &alias, false)?;
                    scope.conditions.push(format!(
                        "NOT EXISTS (SELECT 1 FROM {} WHERE {})",
                        inner.from.join(", "),
                        inner.conjunction()
                    ));
                }
            }
        }
        let mut head = Vec::with_capacity(rule.head.len());
        let aggregates = rule.aggregate_args();
        for argument in &rule.head {
            head.push(match argument {
                Arg::Var(v) => match vars.get(&(v.0 as usize)) {
                    Some(value) => Head::Value(value.clone()),
                    None => return Err(Unsupported::Unbound(self.variable_name(rule, v.0 as usize))),
                },
                Arg::Ground(term) => match self.ground_fold(rule, &vars, *term)? {
                    Some(head) => head,
                    None => Head::Ground(*term),
                },
                Arg::Aggregate(..) | Arg::Fold(..) if aggregates != 1 => {
                    return Err(Unsupported::MalformedAggregate)
                }
                Arg::Aggregate(..) | Arg::Fold(..) => {
                    let (_, folding, subject) = rule.folding_head().unwrap();
                    let fold = self.fold(folding)?;
                    let Arg::Var(v) = subject else {
                        return Err(Unsupported::MalformedAggregate);
                    };
                    let Some(value) = vars.get(&(v.0 as usize)) else {
                        return Err(Unsupported::Unbound(self.variable_name(rule, v.0 as usize)));
                    };
                    Head::Fold(fold, value.clone())
                }
            });
        }
        Ok((scope, head))
    }

    /// `fold(Step, Seed, var(Identity))` as the eval JSON transport carries a
    /// fold head, with no `fold` key (`_6_eval/_6_json.rs` `arg_from_json`).
    fn ground_fold(
        &self,
        rule: &Rule,
        vars: &HashMap<usize, Value>,
        term: TermId,
    ) -> Result<Option<Head>, Unsupported> {
        let u = self.catalog.u;
        let Some([step, seed, subject]) = u.args::<3>(term, "fold") else {
            return Ok(None);
        };
        let Some(identity) = u.unary(subject, "var") else {
            return Ok(None);
        };
        let folding = Folding::Declared(FoldSpec {
            step,
            seed: Seed::Term(seed),
            order: Order::TermLt,
        });
        let reduce = self.fold(folding)?;
        match rule.vars.iter().position(|v| *v == identity) {
            Some(at) if vars.contains_key(&at) => Ok(Some(Head::Fold(reduce, vars[&at].clone()))),
            _ => Err(Unsupported::Unbound(
                u.args::<2>(identity, "variable")
                    .map(|[_, name]| u.display(name).to_string())
                    .unwrap_or_else(|| u.display(identity).to_string()),
            )),
        }
    }

    fn fold(&self, folding: Folding) -> Result<Reduce, Unsupported> {
        let u = self.catalog.u;
        let fold = match folding {
            Folding::Builtin(AggregateKind::Count) => return Ok(Reduce::Count),
            Folding::Builtin(AggregateKind::Sum) => return Ok(Reduce::Sum),
            Folding::Builtin(AggregateKind::Min) => return Ok(Reduce::Min),
            Folding::Builtin(AggregateKind::Max) => return Ok(Reduce::Max),
            Folding::Declared(fold) => fold,
        };
        let seed = match fold.seed {
            Seed::Term(term) => u.unary(term, "const").and_then(|payload| u.as_int(payload)),
            _ => None,
        };
        match (Kernel::of(u, fold.step), seed) {
            (Some(Kernel::IntAdd), Some(seed)) if linear("int_add") => Ok(Reduce::Add(seed)),
            _ => {
                let step = u
                    .unary(fold.step, "ref")
                    .and_then(|inner| u.unary(inner, "kernel"))
                    .map(|name| u.display(name).to_string())
                    .unwrap_or_else(|| self.catalog.name(fold.step));
                Err(Unsupported::Fold(step))
            }
        }
    }

    /// A stored or derived relation goal. Positive goals bind their unbound
    /// variables; a negative goal reads only bound ones, as `solve` does.
    #[allow(clippy::too_many_arguments)]
    fn relation(
        &self,
        scope: &mut Scope,
        rule: &Rule,
        vars: &mut HashMap<usize, Value>,
        goal: &Goal,
        source: &Source,
        alias: &str,
        positive: bool,
    ) -> Result<(), Unsupported> {
        match source {
            Source::Member(cte, member) => {
                scope.from.push(format!("{cte} AS {alias}"));
                scope.conditions.push(format!(
                    "{alias}.{} = {member}",
                    quote_identifier("member")
                ));
            }
            Source::Table(table) => scope.from.push(format!("{table} AS {alias}")),
            Source::Kernel(_) => unreachable!("kernel goals lower in `kernel`"),
        }
        for (position, argument) in goal.args.iter().enumerate() {
            let column = self.column(goal, source, alias, position);
            match argument {
                Arg::Var(v) => match vars.get(&(v.0 as usize)) {
                    Some(bound) => {
                        let condition = self.equal(scope, bound, &column);
                        scope.conditions.push(condition);
                    }
                    None if positive => {
                        vars.insert(v.0 as usize, column);
                    }
                    None => return Err(Unsupported::Unbound(self.variable_name(rule, v.0 as usize))),
                },
                Arg::Ground(term) => {
                    let condition = self.matches(scope, &column, *term)?;
                    scope.conditions.push(condition);
                }
                Arg::Aggregate(..) | Arg::Fold(..) => return Err(Unsupported::MalformedAggregate),
            }
        }
        Ok(())
    }

    /// One condition for a kernel goal; a negative goal wraps it in `NOT`.
    fn kernel(
        &self,
        scope: &mut Scope,
        rule: &Rule,
        vars: &mut HashMap<usize, Value>,
        goal: &Goal,
        kernel: Kernel,
        positive: bool,
    ) -> Result<String, Unsupported> {
        let mut guards = Vec::new();
        let condition = match kernel {
            Kernel::Int(comparison) => {
                let left = self.int_argument(scope, rule, vars, &goal.args[0], &mut guards)?;
                let right = self.int_argument(scope, rule, vars, &goal.args[1], &mut guards)?;
                let operator = match comparison {
                    IntCmp::Lt => "<",
                    IntCmp::Le => "<=",
                    IntCmp::Eq => "=",
                    IntCmp::Ne => "<>",
                    IntCmp::Ge => ">=",
                    IntCmp::Gt => ">",
                };
                format!("{left} {operator} {right}")
            }
            Kernel::TermLt => {
                let left = self.order_key(scope, rule, vars, &goal.args[0], &mut guards)?;
                let right = self.order_key(scope, rule, vars, &goal.args[1], &mut guards)?;
                lexicographic_less(&left, &right)
            }
            Kernel::IntAdd => {
                let left = self.int_argument(scope, rule, vars, &goal.args[0], &mut guards)?;
                let right = self.int_argument(scope, rule, vars, &goal.args[1], &mut guards)?;
                let sum = format!("({left} + {right})");
                // SQLite turns an overflowing integer sum into a REAL; `int_add` has no row.
                guards.push(format!("typeof({sum}) = 'integer'"));
                match &goal.args[2] {
                    Arg::Var(v) if !vars.contains_key(&(v.0 as usize)) => {
                        if !positive {
                            return Err(Unsupported::Unbound(self.variable_name(rule, v.0 as usize)));
                        }
                        vars.insert(
                            v.0 as usize,
                            Value {
                                sql: sum,
                                kind: CellKind::Int,
                            },
                        );
                        "1".to_string()
                    }
                    other => {
                        let result = self.int_argument(scope, rule, vars, other, &mut guards)?;
                        format!("{sum} = {result}")
                    }
                }
            }
            _ => unreachable!("`source` admits comparisons and int_add only"),
        };
        guards.push(condition);
        Ok(guards.join(" AND "))
    }

    fn bound(
        &self,
        rule: &Rule,
        vars: &HashMap<usize, Value>,
        argument: &Arg,
    ) -> Result<Option<Value>, Unsupported> {
        match argument {
            Arg::Var(v) => match vars.get(&(v.0 as usize)) {
                Some(value) => Ok(Some(value.clone())),
                None => Err(Unsupported::Unbound(self.variable_name(rule, v.0 as usize))),
            },
            Arg::Ground(_) => Ok(None),
            Arg::Aggregate(..) | Arg::Fold(..) => Err(Unsupported::MalformedAggregate),
        }
    }

    /// The integer an argument holds. A cell that is not `const(Int)` has no
    /// kernel row, so its guard is part of the goal.
    fn int_argument(
        &self,
        scope: &mut Scope,
        rule: &Rule,
        vars: &HashMap<usize, Value>,
        argument: &Arg,
        guards: &mut Vec<String>,
    ) -> Result<String, Unsupported> {
        let u = self.catalog.u;
        if let Some(value) = self.bound(rule, vars, argument)? {
            return Ok(self.int_of(scope, &value, guards));
        }
        let Arg::Ground(term) = argument else {
            unreachable!("`bound` returns None for ground arguments only");
        };
        match u.unary(*term, "const").and_then(|payload| u.as_int(payload)) {
            Some(n) => Ok(n.to_string()),
            None => {
                guards.push("0".to_string());
                Ok("0".to_string())
            }
        }
    }

    fn int_of(&self, scope: &mut Scope, value: &Value, guards: &mut Vec<String>) -> String {
        match value.kind {
            CellKind::Int => value.sql.clone(),
            _ => {
                let payload = self.payload(scope, &value.sql);
                guards.push(self.is_const(scope, &payload));
                guards.push(format!("{}.\"kind\" = {KIND_INT}", payload.payload));
                format!("{}.\"ival\"", payload.payload)
            }
        }
    }

    /// `Universe::cmp` over `const` scalar payloads; a compound payload has no
    /// key here and ties with every other compound.
    fn order_key(
        &self,
        scope: &mut Scope,
        rule: &Rule,
        vars: &HashMap<usize, Value>,
        argument: &Arg,
        guards: &mut Vec<String>,
    ) -> Result<[String; 4], Unsupported> {
        let u = self.catalog.u;
        if let Some(value) = self.bound(rule, vars, argument)? {
            if value.kind == CellKind::Int {
                return Ok(["0".into(), value.sql, KIND_INT.to_string(), "''".into()]);
            }
            let payload = self.payload(scope, &value.sql);
            guards.push(self.is_const(scope, &payload));
            let text = self.text(scope, &value.sql);
            return Ok(payload_key(&payload.payload, &text));
        }
        let Arg::Ground(term) = argument else {
            unreachable!("`bound` returns None for ground arguments only");
        };
        let Some(payload) = u.unary(*term, "const") else {
            guards.push("0".to_string());
            return Ok(["0".into(), "0".into(), "0".into(), "''".into()]);
        };
        Ok(match u.get(payload) {
            Term::Int(n) => ["0".into(), n.to_string(), KIND_INT.to_string(), "''".into()],
            Term::Float(x) if x.0.is_finite() => [
                "0".into(),
                format!("{:?}", x.0),
                KIND_FLOAT.to_string(),
                "''".into(),
            ],
            Term::Bool(b) => ["1".into(), (*b as i64).to_string(), KIND_BOOL.to_string(), "''".into()],
            Term::Str(s) => ["2".into(), "0".into(), KIND_STR.to_string(), quote_text(u.sym_str(*s))],
            Term::Atom(s) if *s == u.nil => ["3".into(), "0".into(), KIND_ATOM.to_string(), quote_text("[]")],
            Term::Atom(s) => ["4".into(), "0".into(), KIND_ATOM.to_string(), quote_text(u.sym_str(*s))],
            _ => return Err(Unsupported::Constant(*term)),
        })
    }

    fn equal(&self, scope: &mut Scope, left: &Value, right: &Value) -> String {
        if left.kind == right.kind {
            return format!("{} = {}", left.sql, right.sql);
        }
        let (cell, value) = match left.kind {
            CellKind::Int => (right, left),
            _ => (left, right),
        };
        let mut guards = Vec::new();
        let payload = self.int_of(scope, cell, &mut guards);
        guards.push(format!("{payload} = {}", value.sql));
        guards.join(" AND ")
    }

    /// A column holding the constant cell `term`.
    fn matches(&self, scope: &mut Scope, column: &Value, term: TermId) -> Result<String, Unsupported> {
        let u = self.catalog.u;
        let Some(payload) = u.unary(term, "const") else {
            return Err(Unsupported::Constant(term));
        };
        if column.kind == CellKind::Int {
            return Ok(match u.as_int(payload) {
                Some(n) => format!("{} = {n}", column.sql),
                None => "0".to_string(),
            });
        }
        let decoded = self.payload(scope, &column.sql);
        let wrapper = self.is_const(scope, &decoded);
        let p = &decoded.payload;
        let test = match u.get(payload) {
            Term::Int(n) => format!("{p}.\"kind\" = {KIND_INT} AND {p}.\"ival\" = {n}"),
            Term::Float(x) if x.0.is_finite() => {
                format!("{p}.\"kind\" = {KIND_FLOAT} AND {p}.\"rval\" = {:?}", x.0)
            }
            Term::Bool(b) => format!("{p}.\"kind\" = {KIND_BOOL} AND {p}.\"ival\" = {}", *b as i64),
            Term::Atom(s) | Term::Str(s) => {
                let kind = match u.get(payload) {
                    Term::Atom(_) => KIND_ATOM,
                    _ => KIND_STR,
                };
                let text = self.text(scope, &column.sql);
                format!(
                    "{p}.\"kind\" = {kind} AND {text} = {}",
                    quote_text(u.sym_str(*s))
                )
            }
            _ => return Err(Unsupported::Constant(term)),
        };
        Ok(format!("{wrapper} AND {test}"))
    }

    /// A cell is the arena id of `const(Payload)`; these joins reach the
    /// payload, once per scope.
    fn payload(&self, scope: &mut Scope, cell: &str) -> Payload {
        if let Some(found) = scope.payloads.get(cell) {
            return found.clone();
        }
        let term = self.catalog.store_object("term");
        let argument = self.catalog.store_object("term_arg");
        let (wrapper, link, payload) = (self.alias(), self.alias(), self.alias());
        scope.from.push(format!("{term} AS {wrapper}"));
        scope.from.push(format!("{argument} AS {link}"));
        scope.from.push(format!("{term} AS {payload}"));
        scope.conditions.push(format!("{wrapper}.\"id\" = {cell}"));
        scope
            .conditions
            .push(format!("{link}.\"term\" = {cell} AND {link}.\"position\" = 0"));
        scope
            .conditions
            .push(format!("{payload}.\"id\" = {link}.\"child\""));
        let found = Payload {
            wrapper,
            payload,
            text: None,
        };
        scope.payloads.insert(cell.to_string(), found.clone());
        found
    }

    fn text(&self, scope: &mut Scope, cell: &str) -> String {
        let payload = self.payload(scope, cell);
        if let Some(text) = payload.text {
            return format!("{text}.\"text\"");
        }
        let sym = self.alias();
        scope
            .from
            .push(format!("{} AS {sym}", self.catalog.store_object("sym")));
        scope
            .conditions
            .push(format!("{sym}.\"id\" = {}.\"sym\"", payload.payload));
        scope.payloads.get_mut(cell).unwrap().text = Some(sym.clone());
        format!("{sym}.\"text\"")
    }

    fn is_const(&self, scope: &mut Scope, payload: &Payload) -> String {
        let sym = match &scope.const_sym {
            Some(sym) => sym.clone(),
            None => {
                let sym = self.alias();
                scope
                    .from
                    .push(format!("{} AS {sym}", self.catalog.store_object("sym")));
                scope
                    .conditions
                    .push(format!("{sym}.\"text\" = {}", quote_text("const")));
                scope.const_sym = Some(sym.clone());
                sym
            }
        };
        format!(
            "{w}.\"kind\" = {KIND_COMPOUND} AND {w}.\"sym\" = {sym}.\"id\"",
            w = payload.wrapper
        )
    }

    /// The arena id of a constant cell. A constant the store never committed
    /// has no `term` row, and the rule derives nothing.
    fn constant_cell(&self, scope: &mut Scope, term: TermId) -> Result<String, Unsupported> {
        let alias = self.alias();
        scope
            .from
            .push(format!("{} AS {alias}", self.catalog.store_object("term")));
        let cell = Value {
            sql: format!("{alias}.\"id\""),
            kind: CellKind::Term,
        };
        let condition = self.matches(scope, &cell, term)?;
        scope.conditions.push(condition);
        Ok(cell.sql)
    }

    /// One SELECT for a lowered rule, columns in head order.
    fn render(
        &self,
        mut scope: Scope,
        head: &[Head],
        kinds: &[CellKind],
        member: Option<Member>,
        distinct: bool,
    ) -> Result<String, Unsupported> {
        let u = self.catalog.u;
        let mut columns = Vec::with_capacity(head.len());
        let mut reduce = None;
        for (position, value) in head.iter().enumerate() {
            columns.push(match value {
                Head::Value(value) if value.kind == kinds[position] => value.sql.clone(),
                Head::Value(_) => return Err(Unsupported::MixedColumn(position)),
                Head::Ground(term) if kinds[position] == CellKind::Int => {
                    match u.unary(*term, "const").and_then(|payload| u.as_int(payload)) {
                        Some(n) => n.to_string(),
                        None => return Err(Unsupported::MixedColumn(position)),
                    }
                }
                Head::Ground(term) => self.constant_cell(&mut scope, *term)?,
                Head::Fold(kind, subject) => {
                    if head[position].kind() != Some(kinds[position]) {
                        return Err(Unsupported::MixedColumn(position));
                    }
                    reduce = Some((position, *kind, subject.clone()));
                    String::new()
                }
            });
        }
        if scope.from.is_empty() {
            return Err(Unsupported::NoSource);
        }
        if head.is_empty() {
            columns.push("1".to_string());
        }
        if let Some(member) = &member {
            columns.insert(0, member.member.to_string());
            columns.resize(member.width.max(head.len()) + 1, "0".to_string());
            debug_assert!(!member.cte.is_empty());
        }

        let Some((position, kind, subject)) = reduce else {
            return Ok(format!(
                "SELECT {}{} FROM {} WHERE {}",
                if distinct { "DISTINCT " } else { "" },
                columns.join(", "),
                scope.from.join(", "),
                scope.conjunction()
            ));
        };
        let groups: Vec<String> = columns
            .iter()
            .enumerate()
            .filter(|(at, _)| *at != position)
            .map(|(_, sql)| sql.clone())
            .collect();
        let mut guards = Vec::new();
        let aggregate = match (kind, subject.kind) {
            (Reduce::Count, _) => "COUNT(*)".to_string(),
            (Reduce::Sum, _) => format!("SUM({})", self.int_of(&mut scope, &subject, &mut guards)),
            (Reduce::Add(seed), _) => format!(
                "({seed} + SUM({}))",
                self.int_of(&mut scope, &subject, &mut guards)
            ),
            (Reduce::Min, CellKind::Int) => format!("MIN({})", subject.sql),
            (Reduce::Max, CellKind::Int) => format!("MAX({})", subject.sql),
            (Reduce::Min | Reduce::Max, _) => {
                return Ok(self.extremum(scope, &columns, position, kind, &subject));
            }
        };
        scope.conditions.extend(guards);
        columns[position] = aggregate;
        let tail = if groups.is_empty() {
            "HAVING COUNT(*) > 0".to_string()
        } else {
            format!("GROUP BY {}", groups.join(", "))
        };
        Ok(format!(
            "SELECT {} FROM {} WHERE {} {tail}",
            columns.join(", "),
            scope.from.join(", "),
            scope.conjunction()
        ))
    }

    /// `min` and `max` return the winning cell unchanged: the first row of
    /// each group in term order.
    fn extremum(
        &self,
        mut scope: Scope,
        columns: &[String],
        position: usize,
        kind: Reduce,
        subject: &Value,
    ) -> String {
        let payload = self.payload(&mut scope, &subject.sql);
        let text = self.text(&mut scope, &subject.sql);
        let direction = match kind {
            Reduce::Max => " DESC",
            _ => "",
        };
        let order: Vec<String> = payload_key(&payload.payload, &text)
            .iter()
            .map(|field| format!("{field}{direction}"))
            .collect();
        let mut inner = Vec::new();
        let mut outer = Vec::new();
        let mut partition = Vec::new();
        for (at, sql) in columns.iter().enumerate() {
            let name = quote_identifier(&format!("g{at}"));
            if at == position {
                inner.push(format!("{} AS {name}", subject.sql));
            } else {
                inner.push(format!("{sql} AS {name}"));
                partition.push(sql.clone());
            }
            outer.push(name);
        }
        let partition = if partition.is_empty() {
            String::new()
        } else {
            format!("PARTITION BY {} ", partition.join(", "))
        };
        let rank = quote_identifier("order");
        format!(
            "SELECT DISTINCT {} FROM (SELECT {}, ROW_NUMBER() OVER ({partition}ORDER BY {}) AS {rank} FROM {} WHERE {}) WHERE {rank} = 1",
            outer.join(", "),
            inner.join(", "),
            order.join(", "),
            scope.from.join(", "),
            scope.conjunction()
        )
    }
}

impl Scope {
    fn new() -> Scope {
        Scope {
            from: Vec::new(),
            conditions: Vec::new(),
            payloads: HashMap::new(),
            const_sym: None,
        }
    }

    fn conjunction(&self) -> String {
        if self.conditions.is_empty() {
            "1".to_string()
        } else {
            self.conditions.join(" AND ")
        }
    }
}

/// `(rank, number, kind, text)`: numbers rank 0, bools 1, strings 2, `[]` 3,
/// atoms 4, compounds 5, as `Universe::cmp` orders them.
fn payload_key(payload: &str, text: &str) -> [String; 4] {
    [
        format!(
            "CASE {payload}.\"kind\" WHEN {KIND_INT} THEN 0 WHEN {KIND_FLOAT} THEN 0 WHEN {KIND_BOOL} THEN 1 WHEN {KIND_STR} THEN 2 WHEN {KIND_ATOM} THEN (CASE WHEN {text} = '[]' THEN 3 ELSE 4 END) ELSE 5 END"
        ),
        format!(
            "CASE {payload}.\"kind\" WHEN {KIND_INT} THEN {payload}.\"ival\" WHEN {KIND_FLOAT} THEN {payload}.\"rval\" WHEN {KIND_BOOL} THEN {payload}.\"ival\" ELSE 0 END"
        ),
        format!("{payload}.\"kind\""),
        format!("CASE WHEN {payload}.\"kind\" IN ({KIND_ATOM}, {KIND_STR}) THEN {text} ELSE '' END"),
    ]
}

fn lexicographic_less(left: &[String; 4], right: &[String; 4]) -> String {
    let mut out = format!("{} < {}", left[3], right[3]);
    for at in (0..3).rev() {
        out = format!(
            "({l} < {r} OR ({l} = {r} AND {out}))",
            l = left[at],
            r = right[at]
        );
    }
    out
}

fn column_name(position: usize, kind: CellKind) -> String {
    format!("c{position}_{}", kind.suffix())
}

fn column_names(kinds: &[CellKind]) -> Vec<String> {
    if kinds.is_empty() {
        return vec![quote_identifier("__id")];
    }
    kinds
        .iter()
        .enumerate()
        .map(|(position, kind)| quote_identifier(&column_name(position, *kind)))
        .collect()
}

fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn quote_text(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}
