//! Checked DL8 goals lowered to the shared IVM operator algebra.
use super::evaluate::{needs_bound_head, sum_readback_constructors};
use super::kernel::{kernel_ref, linear, IntCmp, Kernel};
use super::program::{
    next_inner, AggregateKind, Arg, Diagnostic, Fold, Folding, Goal, Polarity, Program, Row, Rule,
};
use super::stratify::stratify;
use super::term::{Term, TermId, Universe};
use ivm_ir as ir;
use ivm_ir::StrKind;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

const COMPONENT_BUDGET: usize = 4096;

type RelationKey = (u32, usize);

/// Body evaluations a wave LetRec may change a relation in: wave 0 (the seeds) and the waves
/// after it, one per evaluation. The macrotime wave loop's bound.
const WAVE_LIMIT: u32 = crate::_1_macrotime::WAVE_LIMIT as u32;

/// The relation a rule's head derives, and whether the head is `(next ...)`.
pub(crate) fn rule_key(u: &Universe, rule: &Rule) -> (RelationKey, bool) {
    match next_inner(u, rule.rel) {
        Some(inner) => ((inner.0, rule.head.len()), true),
        None => ((rule.rel.0, rule.head.len()), false),
    }
}

/// `wave`: relations a `next` head derives (wave 0 = seeds). `cone`: relations derived from them,
/// as (component, recursive) in dependency order. `reads`: every other relation those rules read.
struct Waves {
    wave: BTreeSet<RelationKey>,
    cone: Vec<(Vec<RelationKey>, bool)>,
    reads: BTreeSet<RelationKey>,
}

/// `Err` names the construct the wave lowering does not build.
fn waves(u: &Universe, rules: &[Rule], keys: &BTreeSet<RelationKey>) -> Result<Option<Waves>, &'static str> {
    let wave: BTreeSet<RelationKey> = rules
        .iter()
        .map(|rule| rule_key(u, rule))
        .filter_map(|(key, next)| next.then_some(key))
        .collect();
    if wave.is_empty() {
        return Ok(None);
    }
    let goal_keys = |rule: &Rule| -> Vec<RelationKey> {
        rule.body.iter().map(|goal| (goal.rel.0, goal.args.len())).filter(|key| keys.contains(key)).collect()
    };
    let ordinary: Vec<&Rule> = rules.iter().filter(|rule| !rule_key(u, rule).1).collect();
    if ordinary.iter().any(|rule| wave.contains(&rule_key(u, rule).0)) {
        return Err("next relation with an ordinary rule");
    }
    let mut cone: BTreeSet<RelationKey> = BTreeSet::new();
    // Each pass adds at least one relation or stops.
    for _ in 0..=ordinary.len() {
        let before = cone.len();
        for rule in &ordinary {
            if goal_keys(rule).iter().any(|key| wave.contains(key) || cone.contains(key)) {
                cone.insert(rule_key(u, rule).0);
            }
        }
        if cone.len() == before {
            break;
        }
    }
    let mut needs: BTreeMap<RelationKey, BTreeSet<RelationKey>> = cone.iter().map(|key| (*key, BTreeSet::new())).collect();
    for rule in &ordinary {
        let head = rule_key(u, rule).0;
        if let Some(set) = needs.get_mut(&head) {
            set.extend(goal_keys(rule).into_iter().filter(|key| cone.contains(key)));
        }
    }
    // Components: relations that reach each other through `needs`. A component is recursive
    // when its relation reaches itself.
    let reaches: BTreeMap<RelationKey, BTreeSet<RelationKey>> = cone
        .iter()
        .map(|start| (*start, reachable(*start, &cone, &needs).map(|mut set| { set.remove(start); set })))
        .map(|(start, set)| set.map(|set| (start, set)))
        .collect::<Result<_, _>>()?;
    let self_reaching = |key: &RelationKey| needs[key].iter().any(|dep| dep == key || reaches[dep].contains(key));
    let mut components: Vec<(Vec<RelationKey>, bool)> = Vec::new();
    let mut component_of: BTreeMap<RelationKey, usize> = BTreeMap::new();
    for key in &cone {
        if component_of.contains_key(key) {
            continue;
        }
        let members: Vec<RelationKey> = cone
            .iter()
            .filter(|other| *other == key || (reaches[key].contains(*other) && reaches[*other].contains(key)))
            .copied()
            .collect();
        for member in &members {
            component_of.insert(*member, components.len());
        }
        components.push((members, self_reaching(key)));
    }
    // Dependency order over the components.
    let mut waiting: BTreeMap<usize, BTreeSet<usize>> = (0..components.len()).map(|at| (at, BTreeSet::new())).collect();
    for (key, set) in &needs {
        let at = component_of[key];
        waiting.get_mut(&at).unwrap().extend(set.iter().map(|dep| component_of[dep]).filter(|dep| *dep != at));
    }
    let mut order = Vec::with_capacity(components.len());
    for _ in 0..=components.len() {
        let ready: Vec<usize> = waiting.iter().filter(|(_, set)| set.is_empty()).map(|(at, _)| *at).collect();
        if ready.is_empty() {
            break;
        }
        for at in ready {
            waiting.remove(&at);
            for set in waiting.values_mut() {
                set.remove(&at);
            }
            order.push(components[at].clone());
        }
    }
    if !waiting.is_empty() {
        return Err("component ordering inside one wave");
    }
    let reads = rules
        .iter()
        .filter(|rule| {
            let (key, next) = rule_key(u, rule);
            next || cone.contains(&key)
        })
        .flat_map(goal_keys)
        .filter(|key| !wave.contains(key) && !cone.contains(key))
        .collect();
    Ok(Some(Waves { wave, cone: order, reads }))
}

/// The inputs of one IR node, in operator order.
fn op_inputs(op: &ir::Op) -> Vec<ir::NodeId> {
    match op {
        ir::Op::Get(_) => Vec::new(),
        ir::Op::Mint { input, .. } | ir::Op::StrCons { input, .. } | ir::Op::Str { input, .. }
        | ir::Op::Mfp { input, .. } | ir::Op::Negate(input) | ir::Op::Threshold(input)
        | ir::Op::Delay(input) | ir::Op::Reduce { input, .. } | ir::Op::TopK { input, .. }
        | ir::Op::Window { input, .. } => vec![*input],
        ir::Op::Union(inputs) | ir::Op::Join { inputs, .. } => inputs.clone(),
        ir::Op::Antijoin { l, r, .. } => vec![*l, *r],
    }
}

/// `op` with each input `n` replaced by `map(n)`.
fn op_with_inputs(op: &ir::Op, map: impl Fn(ir::NodeId) -> ir::NodeId) -> ir::Op {
    let mut op = op.clone();
    match &mut op {
        ir::Op::Get(_) => {}
        ir::Op::Mint { input, .. } | ir::Op::StrCons { input, .. } | ir::Op::Str { input, .. }
        | ir::Op::Mfp { input, .. } | ir::Op::Negate(input) | ir::Op::Threshold(input)
        | ir::Op::Delay(input) | ir::Op::Reduce { input, .. } | ir::Op::TopK { input, .. }
        | ir::Op::Window { input, .. } => *input = map(*input),
        ir::Op::Union(inputs) | ir::Op::Join { inputs, .. } => inputs.iter_mut().for_each(|n| *n = map(*n)),
        ir::Op::Antijoin { l, r, .. } => {
            *l = map(*l);
            *r = map(*r);
        }
    }
    op
}

/// `root` with every `Get` of a relation in `values` replaced by its value node: the nodes
/// that reach such a `Get` are copied, the rest are shared. Post-order with an explicit
/// stack; `memo` carries copies across calls. `Err` when `root` reads a cone relation in
/// `cone` that has no value yet.
fn substitute(
    b: &mut Builder,
    root: ir::NodeId,
    values: &HashMap<ir::RelId, ir::NodeId>,
    cone: &HashSet<ir::RelId>,
    memo: &mut HashMap<ir::NodeId, ir::NodeId>,
) -> Result<ir::NodeId, &'static str> {
    let mut stack = vec![(root, false)];
    while let Some((node, expanded)) = stack.pop() {
        if memo.contains_key(&node) {
            continue;
        }
        let op = b.ir.nodes[node as usize].clone();
        if let ir::Op::Get(rel) = op {
            let value = match values.get(&rel) {
                Some(value) => *value,
                None if cone.contains(&rel) => return Err("cone relation read before its value"),
                None => node,
            };
            memo.insert(node, value);
            continue;
        }
        let inputs = op_inputs(&op);
        if !expanded {
            stack.push((node, true));
            stack.extend(inputs.iter().filter(|n| !memo.contains_key(n)).map(|n| (*n, false)));
            continue;
        }
        let copy = if inputs.iter().all(|n| memo[n] == *n) {
            node
        } else {
            let op = op_with_inputs(&op, |n| memo[&n]);
            b.node(op)
        };
        memo.insert(node, copy);
    }
    Ok(memo[&root])
}

/// `expr` with every `Col(c)` replaced by `col(c)`: post-order over an explicit stack.
fn with_cols(expr: &ir::Expr, col: &dyn Fn(ir::ColId) -> ir::Expr) -> ir::Expr {
    enum Step<'e> {
        Enter(&'e ir::Expr),
        Build(ir::Func, usize),
    }
    let mut steps = vec![Step::Enter(expr)];
    let mut done: Vec<ir::Expr> = Vec::new();
    while let Some(step) = steps.pop() {
        match step {
            Step::Enter(ir::Expr::Col(c)) => done.push(col(*c)),
            Step::Enter(ir::Expr::Call(func, args)) => {
                steps.push(Step::Build(*func, args.len()));
                steps.extend(args.iter().rev().map(Step::Enter));
            }
            Step::Enter(other) => done.push(other.clone()),
            Step::Build(func, arity) => {
                let args = done.split_off(done.len() - arity);
                done.push(ir::Expr::Call(func, args));
            }
        }
    }
    done.pop().expect("one expression")
}

/// Readers of each node: every input reference and every stratum body (a LetRec's nested bodies
/// included).
fn node_readers(p: &ir::Program) -> Vec<usize> {
    let mut readers = vec![0usize; p.nodes.len()];
    for op in &p.nodes {
        for input in op_inputs(op) {
            readers[input as usize] += 1;
        }
    }
    for stratum in &p.strata {
        match stratum {
            ir::Stratum::Let { body, .. } => readers[*body as usize] += 1,
            ir::Stratum::LetRec(rec) => {
                for body in rec.bodies.iter().chain(rec.nested.iter().flat_map(|inner| &inner.bodies)) {
                    readers[*body as usize] += 1;
                }
            }
        }
    }
    readers
}

/// `outer` over `inner` as one Mfp over `inner`'s input, whose width is `width`. The inner maps
/// are inlined where the outer filter reads them: a filter runs before every map.
fn compose_mfps(
    width: usize,
    inner: (&[ir::Expr], &[ir::Expr], &[ir::ColId]),
    outer: (&[ir::Expr], &[ir::Expr], &[ir::ColId]),
) -> (Vec<ir::Expr>, Vec<ir::Expr>, Vec<ir::ColId>) {
    let (inner_filter, inner_map, inner_project) = inner;
    let (outer_filter, outer_map, outer_project) = outer;
    let mut inlined: Vec<ir::Expr> = Vec::with_capacity(inner_map.len());
    for expr in inner_map {
        let expr = with_cols(expr, &|c| {
            if (c as usize) < width { ir::Expr::Col(c) } else { inlined[c as usize - width].clone() }
        });
        inlined.push(expr);
    }
    let inner_out: Vec<ir::ColId> = if inner_project.is_empty() {
        (0..width + inner_map.len()).map(assert_width).collect()
    } else {
        inner_project.to_vec()
    };
    let at_inner = |c: ir::ColId| -> ir::Expr {
        if (c as usize) < width { ir::Expr::Col(c) } else { inlined[c as usize - width].clone() }
    };
    let mut filter = inner_filter.to_vec();
    filter.extend(outer_filter.iter().map(|expr| with_cols(expr, &|i| at_inner(inner_out[i as usize]))));
    let base = width + inner_map.len();
    let fused_col = |i: ir::ColId| -> ir::ColId {
        match inner_out.get(i as usize) {
            Some(col) => *col,
            None => assert_width(base + i as usize - inner_out.len()),
        }
    };
    let mut map = inner_map.to_vec();
    map.extend(outer_map.iter().map(|expr| with_cols(expr, &|i| ir::Expr::Col(fused_col(i)))));
    let mut project: Vec<ir::ColId> = if outer_project.is_empty() {
        (0..inner_out.len() + outer_map.len()).map(|i| fused_col(assert_width(i))).collect()
    } else {
        outer_project.iter().map(|i| fused_col(*i)).collect()
    };
    if project.iter().copied().eq((0..base + outer_map.len()).map(assert_width)) {
        project.clear();
    }
    (filter, map, project)
}

/// An Mfp whose input no other node or stratum reads absorbs that input: a projection with no
/// filter or map over a Join becomes the Join's `project`; an Mfp over an Mfp becomes one Mfp
/// over the inner Mfp's input. The absorbed node leaves the program and node ids are renumbered.
/// Inputs precede their readers, so one ascending pass folds a chain into its last node.
fn fuse_projections(p: &mut ir::Program) {
    let mut readers = node_readers(p);
    let mut types: Vec<Option<Option<Vec<ir::Ty>>>> = Vec::new();
    let mut absorbed = vec![false; p.nodes.len()];
    for id in 0..p.nodes.len() {
        let ir::Op::Mfp { input, filter, map, project } = &p.nodes[id] else { continue };
        let inner_id = *input as usize;
        if readers[inner_id] != 1 {
            continue;
        }
        if let ir::Op::Join { inputs, equivalences, project: inner_project } = &p.nodes[inner_id] {
            if !filter.is_empty() || !map.is_empty() || project.is_empty() {
                continue;
            }
            let project = project
                .iter()
                .map(|col| if inner_project.is_empty() { *col } else { inner_project[*col as usize] })
                .collect();
            p.nodes[id] = ir::Op::Join { inputs: inputs.clone(), equivalences: equivalences.clone(), project };
            absorbed[inner_id] = true;
            readers[inner_id] = 0;
            continue;
        }
        let ir::Op::Mfp { input: source, filter: inner_filter, map: inner_map, project: inner_project } = &p.nodes[inner_id] else {
            continue;
        };
        let Some(width) = p.node_types_memo(*source, &mut types).map(|cols| cols.len()) else { continue };
        let source = *source;
        let (filter, map, project) = compose_mfps(
            width,
            (inner_filter, inner_map, inner_project),
            (filter, map, project),
        );
        p.nodes[id] = ir::Op::Mfp { input: source, filter, map, project };
        absorbed[inner_id] = true;
        readers[inner_id] = 0;
    }
    if !absorbed.contains(&true) {
        return;
    }
    let mut renumber = vec![0 as ir::NodeId; p.nodes.len()];
    let mut next = 0;
    for (id, gone) in absorbed.iter().enumerate() {
        if !gone {
            renumber[id] = next;
            next += 1;
        }
    }
    let nodes = std::mem::take(&mut p.nodes);
    p.nodes = nodes
        .iter()
        .enumerate()
        .filter(|(id, _)| !absorbed[*id])
        .map(|(_, op)| op_with_inputs(op, |n| renumber[n as usize]))
        .collect();
    for stratum in &mut p.strata {
        match stratum {
            ir::Stratum::Let { body, .. } => *body = renumber[*body as usize],
            ir::Stratum::LetRec(rec) => {
                let nested = rec.nested.iter_mut().flat_map(|inner| inner.bodies.iter_mut());
                rec.bodies.iter_mut().chain(nested).for_each(|body| *body = renumber[*body as usize]);
            }
        }
    }
}

fn reachable(
    start: RelationKey,
    pending: &BTreeSet<RelationKey>,
    edges: &BTreeMap<RelationKey, BTreeSet<RelationKey>>,
) -> Result<BTreeSet<RelationKey>, &'static str> {
    let mut seen = BTreeSet::new();
    let mut todo = vec![start];
    for _ in 0..COMPONENT_BUDGET {
        let Some(key) = todo.pop() else {
            return Ok(seen);
        };
        if !seen.insert(key) {
            continue;
        }
        todo.extend(
            edges
                .get(&key)
                .into_iter()
                .flatten()
                .filter(|key| pending.contains(key))
                .copied(),
        );
    }
    Err("component reachability budget")
}

#[derive(Clone)]
pub struct TermSpec {
    pub original: TermId,
    pub functor: ir::RelId,
    pub args: Vec<TermId>,
    pub raw: Option<ir::Cell>,
    /// Index into `Lowered::texts`: the one argument of a `2:str` term is that text's cell.
    pub text: Option<usize>,
}

#[derive(Clone)]
pub struct Lowered {
    pub ir: ir::Program,
    pub terms: Vec<TermSpec>,
    pub texts: Vec<(TermId, String)>,
    pub seeds: Vec<(ir::RelId, Row)>,
    pub term_map: ir::RelId,
    pub unit: ir::RelId,
    pub visible: Vec<(ir::RelId, TermId, usize)>,
    pub internal_seeds: Vec<(ir::RelId, Vec<TermId>)>,
    /// `__ir_plain_list_seeds` and `__ir_bad_lists`: where a `const` term
    /// installed after lowering sends its `internal_seeds` row.
    pub plain_source: ir::RelId,
    pub bad_lists: ir::RelId,
    pub served: Vec<ServedSpec>,
    pub sources: BTreeMap<(u32, usize), ir::RelId>,
    /// Constructor relation per term shape `(name, arity, raw)`.
    pub constructors: BTreeMap<(String, usize, bool), ir::RelId>,
    pub atom_raws: HashMap<TermId, i64>,
    pub dynamic_atom_base: i64,
    pub fold_program: Option<Program>,
    pub declared_folds: Vec<DeclaredFoldSpec>,
    /// The term of each `ir.terms` entry.
    pub literals: Vec<TermId>,
}

#[derive(Clone)]
pub struct DeclaredFoldSpec {
    pub ranked: ir::RelId,
    pub key_count: usize,
    pub fold: Fold,
}

#[derive(Clone)]
pub struct ServedSpec {
    pub relation: TermId,
    pub request: ir::RelId,
    pub answer: ir::RelId,
    pub arity: usize,
}

#[derive(Clone)]
struct Builder {
    ir: ir::Program,
    next_rel: ir::RelId,
    constructor_gets: HashMap<ir::RelId, ir::NodeId>,
    /// A relation read through one column projection: every goal that keeps the same columns
    /// reads one node, so the engines share its arrangement.
    narrowed_gets: HashMap<(ir::RelId, Vec<ir::ColId>), ir::NodeId>,
    /// `(relation, join key positions)` to the union of the positions its positive readers
    /// keep. A recording pass over every rule fills it; the lowering pass narrows each reader
    /// to its group's union, so readers that join a relation on one key read one node and the
    /// engines build one arrangement. The join's projection drops the columns a reader does
    /// not keep.
    narrow_unions: HashMap<(ir::RelId, Vec<ir::ColId>), BTreeSet<ir::ColId>>,
    /// The recording pass: `compile_rule` adds each reader's kept positions to
    /// `narrow_unions` and narrows to them alone. It builds no node: `node` hands out fresh ids
    /// past `ir.nodes` (counted in `recorded`) and keeps no op, since the pass reads only
    /// `narrow_unions` back.
    narrow_record: bool,
    recorded: usize,
    intern_nodes: Vec<ir::NodeId>,
    intern_relation: Option<ir::RelId>,
    intern_readback: bool,
    intern_readback_count: usize,
    effect_nodes: BTreeMap<u32, Vec<ir::NodeId>>,
    snapshot_nodes: Vec<ir::NodeId>,
    declared_folds: Vec<DeclaredFoldSpec>,
    /// `__ir_semantic` and the `9:1:ref` and `9:1:const` constructors it unions.
    semantic: Option<(ir::RelId, [ir::RelId; 2])>,
    /// Tuples the current rule body constructed: the value column to its
    /// tagged element columns. An `intern` over one builds the plain list from
    /// the elements and never waits on `__ir_plain_lists` for a fresh tuple.
    built_tuples: HashMap<ir::ColId, Vec<ir::ColId>>,
    /// One node per structurally equal op: the op (a `Union` with its inputs sorted) to its node.
    /// Every op denotes a function of its operator and its inputs' collections (a `Mint` of a
    /// functor and columns, a `Get` of a relation with one binder), so equal ops are one collection.
    consed: HashMap<String, ir::NodeId>,
    /// Every term the program names, by its `TermId`: the source of `ir.terms` entries.
    term_specs: HashMap<TermId, TermSpec>,
    /// `ir.terms` index per term already in the table.
    literals: HashMap<TermId, u32>,
    /// The term of each `ir.terms` entry.
    literal_terms: Vec<TermId>,
    /// `Lowered::texts`, by `TermSpec::text`; `ir.texts` holds only the texts the term table names,
    /// so the IR of a program does not change with the strings its seeds carry.
    spec_texts: Vec<String>,
}

struct FoldAux {
    ranked: ir::RelId,
    ranked_body: ir::NodeId,
    accumulator: ir::RelId,
    accumulator_body: ir::NodeId,
}

struct CompiledRule {
    body: ir::NodeId,
    fold_aux: Option<FoldAux>,
}

impl Builder {
    fn new() -> Self {
        Self {
            ir: ir::Program {
                texts: Vec::new(),
                terms: Vec::new(),
                rels: Vec::new(),
                nodes: Vec::new(),
                strata: Vec::new(),
                outputs: Vec::new(),
            },
            next_rel: 0,
            constructor_gets: HashMap::new(),
            narrowed_gets: HashMap::new(),
            narrow_unions: HashMap::new(),
            narrow_record: false,
            recorded: 0,
            intern_nodes: Vec::new(),
            intern_relation: None,
            intern_readback: false,
            intern_readback_count: 0,
            effect_nodes: BTreeMap::new(),
            snapshot_nodes: Vec::new(),
            built_tuples: HashMap::new(),
            declared_folds: Vec::new(),
            semantic: None,
            consed: HashMap::new(),
            term_specs: HashMap::new(),
            literals: HashMap::new(),
            literal_terms: Vec::new(),
            spec_texts: Vec::new(),
        }
    }
    fn rel(&mut self, name: String, cols: Vec<ir::Ty>, kind: ir::RelKind) -> ir::RelId {
        let id = self.next_rel;
        self.next_rel += 1;
        self.ir.rels.push(ir::Relation {
            id,
            name,
            cols,
            kind,
        });
        id
    }
    fn node(&mut self, op: ir::Op) -> ir::NodeId {
        if self.narrow_record {
            let id = (self.ir.nodes.len() + self.recorded) as ir::NodeId;
            self.recorded += 1;
            return id;
        }
        let key = match &op {
            ir::Op::Union(inputs) => {
                let mut sorted = inputs.clone();
                sorted.sort_unstable();
                format!("{:?}", ir::Op::Union(sorted))
            }
            op => format!("{op:?}"),
        };
        if let Some(&id) = self.consed.get(&key) {
            return id;
        }
        let id = self.ir.nodes.len() as ir::NodeId;
        self.ir.nodes.push(op);
        self.consed.insert(key, id);
        id
    }
    fn get(&mut self, rel: ir::RelId) -> ir::NodeId {
        self.node(ir::Op::Get(rel))
    }
    fn get_constructor(&mut self, rel: ir::RelId) -> ir::NodeId {
        if let Some(&id) = self.constructor_gets.get(&rel) {
            return id;
        }
        let id = self.get(rel);
        self.constructor_gets.insert(rel, id);
        id
    }
    /// The `ir.terms` index of `term`, its argument terms entered first. Post-order over an
    /// explicit stack. `Err`: a term outside the lowered term specs.
    fn literal(&mut self, term: TermId) -> Result<u32, &'static str> {
        let mut stack = vec![(term, false)];
        while let Some((at, expanded)) = stack.pop() {
            if self.literals.contains_key(&at) {
                continue;
            }
            let spec = self.term_specs.get(&at).ok_or("ground term outside the term table")?;
            if !expanded {
                stack.push((at, true));
                stack.extend(spec.args.iter().filter(|arg| !self.literals.contains_key(arg)).map(|arg| (*arg, false)));
                continue;
            }
            let args = if let Some(raw) = spec.raw {
                vec![ir::TermArg::Raw(raw)]
            } else if let Some(text) = spec.text {
                let value = self.spec_texts.get(text).ok_or("term text outside the text table")?;
                let index = match self.ir.texts.iter().position(|known| known == value) {
                    Some(index) => index,
                    None => {
                        self.ir.texts.push(value.clone());
                        self.ir.texts.len() - 1
                    }
                };
                vec![ir::TermArg::Text(index as u32)]
            } else {
                spec.args
                    .iter()
                    .map(|arg| self.literals.get(arg).map(|index| ir::TermArg::Term(*index)))
                    .collect::<Option<Vec<_>>>()
                    .ok_or("term argument outside the term table")?
            };
            let index = self.ir.terms.len() as u32;
            self.ir.terms.push(ir::TermLit { functor: spec.functor, args });
            self.literals.insert(at, index);
            self.literal_terms.push(at);
        }
        Ok(self.literals[&term])
    }
    fn mfp(
        &mut self,
        input: ir::NodeId,
        filter: Vec<ir::Expr>,
        map: Vec<ir::Expr>,
        project: Vec<ir::ColId>,
    ) -> ir::NodeId {
        self.node(ir::Op::Mfp {
            input,
            filter,
            map,
            project,
        })
    }
}

#[track_caller]
fn unsupported(u: &mut Universe, name: &str) -> Diagnostic {
    let construct = u.atom(name);
    let location = std::panic::Location::caller();
    let site = u.string(&format!("{}:{}", location.file(), location.line()));
    Diagnostic {
        phase: "ir_lower",
        payload: u.compound("unsupported_construct", vec![construct, site]),
    }
}

fn var(arg: &Arg) -> Option<u32> {
    match arg {
        Arg::Var(v) => Some(v.0),
        _ => None,
    }
}

fn ground(arg: &Arg) -> Option<TermId> {
    match arg {
        Arg::Ground(id) => Some(*id),
        _ => None,
    }
}

fn assert_width(width: usize) -> ir::ColId {
    ir::ColId::try_from(width).expect("IR column width exceeds u16")
}

/// `input` extended by a ground term's two columns, its `TermId` and its cell (an IR literal,
/// interned at install). `key`: the input column that must hold the term; none appends it to every
/// row.
fn join_ground(
    b: &mut Builder,
    input: ir::NodeId,
    key: Option<ir::ColId>,
    original: TermId,
) -> Result<ir::NodeId, &'static str> {
    let cell = ir::Expr::Term(b.literal(original)?);
    let filter = key
        .map(|col| vec![ir::Expr::Call(ir::Func::Eq, vec![ir::Expr::Col(col), cell.clone()])])
        .unwrap_or_default();
    Ok(b.mfp(input, filter, vec![ir::Expr::Lit(original.0 as i64), cell], vec![]))
}

/// One row holding the cell of each of `terms`, read off the unit relation.
fn ground_row(b: &mut Builder, unit: ir::RelId, terms: &[TermId]) -> Result<ir::NodeId, &'static str> {
    let unit_rows = b.get(unit);
    let mut map = Vec::with_capacity(terms.len());
    for term in terms {
        map.push(ir::Expr::Term(b.literal(*term)?));
    }
    let project = (1..=terms.len()).map(assert_width).collect();
    Ok(b.mfp(unit_rows, vec![], map, project))
}

fn constructor(
    constructors: &BTreeMap<(String, usize, bool), ir::RelId>,
    name: &str,
    arity: usize,
    raw: bool,
) -> Result<ir::RelId, &'static str> {
    constructors
        .get(&(name.to_owned(), arity, raw))
        .copied()
        .ok_or("constructor relation")
}

fn join_constructor(
    b: &mut Builder,
    input: ir::NodeId,
    _width: usize,
    relation: ir::RelId,
    id: ir::ColId,
) -> ir::NodeId {
    if let Some((semantic, _)) = b.semantic {
        if relation == semantic {
            // One join on `__ir_semantic`, the Union of the `ref` and `const` constructors: their
            // ids are disjoint, so each id finds its one wrapper row.
            let wrapped = b.get(semantic);
            return b.node(ir::Op::Join {
                inputs: vec![input, wrapped],
                equivalences: vec![vec![(0, id), (1, 0)]],
                project: vec![],
            });
        }
    }
    let ctor = b.get_constructor(relation);
    b.node(ir::Op::Join {
        inputs: vec![input, ctor],
        equivalences: vec![vec![(0, id), (1, 0)]],
        project: vec![],
    })
}

/// `const(2:str(Text))` at `id` to the text cell column.
fn const_text(
    b: &mut Builder,
    constructors: &BTreeMap<(String, usize, bool), ir::RelId>,
    current: &mut ir::NodeId,
    width: &mut usize,
    id: ir::ColId,
) -> Result<ir::ColId, &'static str> {
    let wrap = constructor(constructors, "9:1:const", 1, false)?;
    let text = constructor(constructors, "2:str", 1, false)?;
    *current = join_constructor(b, *current, *width, wrap, id);
    let inner = assert_width(*width + 1);
    *width += 2;
    *current = join_constructor(b, *current, *width, text, inner);
    let cell = assert_width(*width + 1);
    *width += 2;
    Ok(cell)
}

/// The text cell at `cell` minted as `const(2:str(Text))`.
fn text_const(
    b: &mut Builder,
    constructors: &BTreeMap<(String, usize, bool), ir::RelId>,
    current: &mut ir::NodeId,
    width: &mut usize,
    cell: ir::ColId,
) -> Result<ir::ColId, &'static str> {
    let wrap = constructor(constructors, "9:1:const", 1, false)?;
    let text = constructor(constructors, "2:str", 1, false)?;
    *current = b.node(ir::Op::Mint {
        input: *current,
        functor: text,
        args: vec![cell],
    });
    let inner = assert_width(*width);
    *width += 1;
    *current = b.node(ir::Op::Mint {
        input: *current,
        functor: wrap,
        args: vec![inner],
    });
    let id = assert_width(*width);
    *width += 1;
    Ok(id)
}

fn equal_columns(left: ir::ColId, right: ir::ColId) -> ir::Expr {
    ir::Expr::Call(
        ir::Func::Eq,
        vec![ir::Expr::Col(left), ir::Expr::Col(right)],
    )
}

fn served_effect(
    b: &mut Builder,
    goal: &super::program::Goal,
    current: ir::NodeId,
    width: usize,
    bound: &HashMap<u32, ir::ColId>,
    constructors: &BTreeMap<(String, usize, bool), ir::RelId>,
    semantic: ir::RelId,
    none: TermId,
    empty: TermId,
    tuple: TermId,
) -> Result<(), &'static str> {
    let mut node = current;
    let mut size = width;
    let mut values = Vec::new();
    for arg in &goal.args {
        let term = match arg {
            Arg::Var(v) => bound.get(&v.0).copied(),
            Arg::Ground(id) => {
                node = join_ground(b, node, None, *id)?;
                let col = assert_width(size + 1);
                size += 2;
                Some(col)
            }
            _ => return Err("served aggregate or fold"),
        };
        values.push(term);
    }
    node = join_ground(b, node, None, none)?;
    let missing_col = assert_width(size + 1);
    size += 2;
    node = join_ground(b, node, None, empty)?;
    let mut list = assert_width(size + 1);
    let mut plain_list = list;
    size += 2;
    let cons = constructor(constructors, "9:2:[|]", 2, false)?;
    for value in values.into_iter().rev() {
        let tagged = value.unwrap_or(missing_col);
        node = join_constructor(b, node, size, semantic, tagged);
        let plain = assert_width(size + 1);
        size += 2;
        node = b.node(ir::Op::Mint {
            input: node,
            functor: cons,
            args: vec![tagged, list],
        });
        list = assert_width(size);
        size += 1;
        node = b.node(ir::Op::Mint {
            input: node,
            functor: cons,
            args: vec![plain, plain_list],
        });
        plain_list = assert_width(size);
        size += 1;
    }
    let wrap = constructor(constructors, "9:1:ref", 1, false)?;
    let application = constructor(constructors, "9:2:application", 2, false)?;
    node = join_ground(b, node, None, tuple)?;
    let tuple_col = assert_width(size + 1);
    size += 2;
    node = b.node(ir::Op::Mint {
        input: node,
        functor: application,
        args: vec![tuple_col, list],
    });
    let tuple_application = assert_width(size);
    size += 1;
    node = b.node(ir::Op::Mint {
        input: node,
        functor: wrap,
        args: vec![tuple_application],
    });
    let arguments = assert_width(size);
    size += 1;
    node = join_ground(b, node, None, goal.rel)?;
    let rel = assert_width(size + 1);
    size += 2;
    let ref_ctor = constructor(constructors, "9:1:ref", 1, false)?;
    node = join_constructor(b, node, size, ref_ctor, rel);
    let rel_inner = assert_width(size + 1);
    size += 2;
    let app_ctor = constructor(constructors, "9:2:application", 2, false)?;
    node = b.node(ir::Op::Mint {
        input: node,
        functor: app_ctor,
        args: vec![rel_inner, plain_list],
    });
    let app = assert_width(size);
    size += 1;
    node = b.node(ir::Op::Mint {
        input: node,
        functor: ref_ctor,
        args: vec![app],
    });
    let application = assert_width(size);
    let request = b.mfp(node, vec![], vec![], vec![rel, arguments, application]);
    b.intern_nodes.push(request);
    b.snapshot_nodes.push(request);
    let effect = b.mfp(node, vec![], vec![], vec![rel, application]);
    b.effect_nodes.entry(goal.rel.0).or_default().push(effect);
    Ok(())
}

fn compile_kernel(
    b: &mut Builder,
    goal: &super::program::Goal,
    kernel: Kernel,
    constructors: &BTreeMap<(String, usize, bool), ir::RelId>,
    semantic: ir::RelId,
    plain_lists: ir::RelId,
    current: &mut ir::NodeId,
    width: &mut usize,
    bound: &mut HashMap<u32, ir::ColId>,
    empty: TermId,
    tuple: TermId,
) -> Result<(), &'static str> {
    let mut inputs = Vec::new();
    for arg in &goal.args {
        match arg {
            Arg::Var(v) => inputs.push(bound.get(&v.0).copied()),
            Arg::Ground(id) => {
                *current = join_ground(b, *current, None, *id)?;
                inputs.push(Some(assert_width(*width + 1)));
                *width += 2;
            }
            _ => return Err("kernel aggregate or fold"),
        }
    }
    if kernel == Kernel::Intern
        && (inputs.first().is_some_and(Option::is_none)
            || inputs.get(1).is_some_and(Option::is_none))
    {
        if goal.polarity == Polarity::Negative {
            return Err("negative intern readback");
        }
        let relation = b.intern_relation.ok_or("intern request relation")?;
        let right = b.get(relation);
        let equivalences = inputs
            .iter()
            .enumerate()
            .filter_map(|(index, input)| input.map(|col| vec![(0, col), (1, assert_width(index))]))
            .collect();
        *current = b.node(ir::Op::Join {
            inputs: vec![*current, right],
            equivalences,
    project: vec![],
        });
        for (index, arg) in goal.args.iter().enumerate() {
            if let (Arg::Var(v), None) = (arg, inputs[index]) {
                bound.insert(v.0, assert_width(*width + index));
            }
        }
        *width += 3;
        b.intern_readback = true;
        b.intern_readback_count += 1;
        return Ok(());
    }
    if goal.polarity == Polarity::Negative && inputs.iter().any(Option::is_none) {
        *current = b.mfp(*current, vec![ir::Expr::Lit(0)], vec![], vec![]);
        return Ok(());
    }
    let base = *current;
    let base_width = *width;
    let mut output = inputs.clone();
    match kernel {
        Kernel::Int(_) | Kernel::TermLt => {
            if inputs.len() != 2 {
                return Err("comparison arity");
            }
            let (Some(left), Some(right)) = (inputs[0], inputs[1]) else {
                *current = b.mfp(*current, vec![ir::Expr::Lit(0)], vec![], vec![]);
                return Ok(());
            };
            let (left_value, right_value, order) = if matches!(kernel, Kernel::Int(_)) {
                let wrap = constructor(constructors, "9:1:const", 1, false)?;
                let int = constructor(constructors, "0:int", 0, true)?;
                *current = join_constructor(b, *current, *width, wrap, left);
                let left_inner = assert_width(*width + 1);
                *width += 2;
                *current = join_constructor(b, *current, *width, wrap, right);
                let right_inner = assert_width(*width + 1);
                *width += 2;
                *current = join_constructor(b, *current, *width, int, left_inner);
                let left_raw = assert_width(*width + 1);
                *width += 2;
                *current = join_constructor(b, *current, *width, int, right_inner);
                let right_raw = assert_width(*width + 1);
                *width += 2;
                (left_raw, right_raw, ir::Func::Lt)
            } else {
                (left, right, ir::Func::TermLt)
            };
            let a = ir::Expr::Col(left_value);
            let z = ir::Expr::Col(right_value);
            let lt = || ir::Expr::Call(order, vec![a.clone(), z.clone()]);
            let gt = || ir::Expr::Call(order, vec![z.clone(), a.clone()]);
            let eq = || ir::Expr::Call(ir::Func::Eq, vec![a.clone(), z.clone()]);
            let predicate = match kernel {
                Kernel::TermLt | Kernel::Int(IntCmp::Lt) => lt(),
                Kernel::Int(IntCmp::Gt) => gt(),
                Kernel::Int(IntCmp::Eq) => eq(),
                Kernel::Int(IntCmp::Ne) => ir::Expr::Call(ir::Func::Ne, vec![a, z]),
                Kernel::Int(IntCmp::Le) => ir::Expr::Call(ir::Func::Or, vec![lt(), eq()]),
                Kernel::Int(IntCmp::Ge) => ir::Expr::Call(ir::Func::Or, vec![gt(), eq()]),
                _ => unreachable!(),
            };
            *current = b.mfp(*current, vec![predicate], vec![], vec![]);
        }
        Kernel::StrCons => {
            if inputs.len() != 3 {
                return Err("str.cons arity");
            }
            if let (Some(head), Some(rest)) = (inputs[0], inputs[1]) {
                let head_text = const_text(b, constructors, current, width, head)?;
                let rest_text = const_text(b, constructors, current, width, rest)?;
                *current = b.node(ir::Op::StrCons {
                    input: *current,
                    mode: ir::StrMode::Construct {
                        head: head_text,
                        rest: rest_text,
                    },
                });
                let text = assert_width(*width);
                *width += 1;
                output[2] = Some(text_const(b, constructors, current, width, text)?);
            } else if let Some(whole) = inputs[2] {
                let whole_text = const_text(b, constructors, current, width, whole)?;
                *current = b.node(ir::Op::StrCons {
                    input: *current,
                    mode: ir::StrMode::Decompose { whole: whole_text },
                });
                let head_text = assert_width(*width);
                let rest_text = assert_width(*width + 1);
                *width += 2;
                output[0] = Some(text_const(b, constructors, current, width, head_text)?);
                output[1] = Some(text_const(b, constructors, current, width, rest_text)?);
            } else {
                *current = b.mfp(*current, vec![ir::Expr::Lit(0)], vec![], vec![]);
                return Ok(());
            }
        }
        Kernel::StrNil => {
            if inputs.len() != 1 {
                return Err("str.nil arity");
            }
            *current = b.mfp(
                *current,
                vec![],
                vec![ir::Expr::Call(ir::Func::StrNil, vec![])],
                vec![],
            );
            let text = assert_width(*width);
            *width += 1;
            output[0] = Some(text_const(b, constructors, current, width, text)?);
        }
        Kernel::Str(op) => {
            let kinds = op.args();
            if inputs.len() != kinds.len() + usize::from(op.out().is_some()) {
                return Err("str op arity");
            }
            let wrap = constructor(constructors, "9:1:const", 1, false)?;
            let int = constructor(constructors, "0:int", 0, true)?;
            let mut columns = Vec::with_capacity(kinds.len());
            for (input, kind) in inputs.iter().zip(kinds) {
                let Some(input) = *input else {
                    *current = b.mfp(*current, vec![ir::Expr::Lit(0)], vec![], vec![]);
                    return Ok(());
                };
                let column = if *kind == StrKind::Int {
                    *current = join_constructor(b, *current, *width, wrap, input);
                    let inner = assert_width(*width + 1);
                    *width += 2;
                    *current = join_constructor(b, *current, *width, int, inner);
                    let column = assert_width(*width + 1);
                    *width += 2;
                    column
                } else {
                    const_text(b, constructors, current, width, input)?
                };
                columns.push(column);
            }
            *current = b.node(ir::Op::Str {
                input: *current,
                op,
                args: columns,
            });
            if op.out() == Some(StrKind::Int) {
                let result = assert_width(*width);
                *width += 1;
                *current = b.node(ir::Op::Mint {
                    input: *current,
                    functor: int,
                    args: vec![result],
                });
                let result = assert_width(*width);
                *width += 1;
                *current = b.node(ir::Op::Mint {
                    input: *current,
                    functor: wrap,
                    args: vec![result],
                });
                output[kinds.len()] = Some(assert_width(*width));
                *width += 1;
            } else if op.out().is_some() {
                let result = assert_width(*width);
                *width += 1;
                output[kinds.len()] = Some(text_const(b, constructors, current, width, result)?);
            }
        }
        Kernel::IntAdd => {
            if inputs.len() != 3 {
                return Err("int.add arity");
            }
            let (Some(left), Some(right)) = (inputs[0], inputs[1]) else {
                *current = b.mfp(*current, vec![ir::Expr::Lit(0)], vec![], vec![]);
                return Ok(());
            };
            let wrap = constructor(constructors, "9:1:const", 1, false)?;
            let int = constructor(constructors, "0:int", 0, true)?;
            *current = join_constructor(b, *current, *width, wrap, left);
            let left_inner = assert_width(*width + 1);
            *width += 2;
            *current = join_constructor(b, *current, *width, wrap, right);
            let right_inner = assert_width(*width + 1);
            *width += 2;
            *current = join_constructor(b, *current, *width, int, left_inner);
            let left_number = assert_width(*width + 1);
            *width += 2;
            *current = join_constructor(b, *current, *width, int, right_inner);
            let right_number = assert_width(*width + 1);
            *width += 2;
            let a = ir::Expr::Col(left_number);
            let z = ir::Expr::Col(right_number);
            *current = b.mfp(
                *current,
                vec![],
                vec![ir::Expr::Call(ir::Func::Add, vec![a.clone(), z.clone()])],
                vec![],
            );
            let sum = assert_width(*width);
            *width += 1;
            let overflow_positive = ir::Expr::Call(
                ir::Func::And,
                vec![
                    ir::Expr::Call(ir::Func::Gt, vec![z.clone(), ir::Expr::Lit(0)]),
                    ir::Expr::Call(ir::Func::Lt, vec![ir::Expr::Col(sum), a.clone()]),
                ],
            );
            let overflow_negative = ir::Expr::Call(
                ir::Func::And,
                vec![
                    ir::Expr::Call(ir::Func::Lt, vec![z, ir::Expr::Lit(0)]),
                    ir::Expr::Call(ir::Func::Gt, vec![ir::Expr::Col(sum), a]),
                ],
            );
            let safe = ir::Expr::Call(
                ir::Func::Not,
                vec![ir::Expr::Call(
                    ir::Func::Or,
                    vec![overflow_positive, overflow_negative],
                )],
            );
            *current = b.mfp(*current, vec![safe], vec![], vec![]);
            *current = b.node(ir::Op::Mint {
                input: *current,
                functor: int,
                args: vec![sum],
            });
            let int_id = assert_width(*width);
            *width += 1;
            *current = b.node(ir::Op::Mint {
                input: *current,
                functor: wrap,
                args: vec![int_id],
            });
            output[2] = Some(assert_width(*width));
            *width += 1;
        }
        Kernel::Tuple => {
            if inputs.is_empty() {
                return Err("tuple arity");
            }
            let cons = constructor(constructors, "9:2:[|]", 2, false)?;
            let ref_ctor = constructor(constructors, "9:1:ref", 1, false)?;
            let app_ctor = constructor(constructors, "9:2:application", 2, false)?;
            let value_at = inputs.len() - 1;
            if let Some(whole) = inputs[value_at] {
                *current = join_constructor(b, *current, *width, ref_ctor, whole);
                let application = assert_width(*width + 1);
                *width += 2;
                *current = join_constructor(b, *current, *width, app_ctor, application);
                let constructor_col = assert_width(*width + 1);
                let mut list = assert_width(*width + 2);
                *width += 3;
                *current = join_ground(b, *current, None, tuple)?;
                let tuple_col = assert_width(*width + 1);
                *width += 2;
                *current = b.mfp(
                    *current,
                    vec![equal_columns(constructor_col, tuple_col)],
                    vec![],
                    vec![],
                );
                for position in 0..value_at {
                    *current = join_constructor(b, *current, *width, cons, list);
                    let head = assert_width(*width + 1);
                    list = assert_width(*width + 2);
                    *width += 3;
                    output[position] = Some(head);
                    if let (Some(want), Arg::Var(v)) = (inputs[position], &goal.args[position]) {
                        *current = b.mfp(*current, vec![equal_columns(want, head)], vec![], vec![]);
                        let _ = v;
                    }
                }
                // Exactly `value_at` elements: the list ends after the last one.
                *current = join_ground(b, *current, None, empty)?;
                let end = assert_width(*width + 1);
                *width += 2;
                *current = b.mfp(*current, vec![equal_columns(list, end)], vec![], vec![]);
            } else {
                let mut plain: Vec<ir::ColId> = Vec::with_capacity(value_at);
                for position in 0..value_at {
                    let Some(element) = inputs[position] else {
                        *current = b.mfp(*current, vec![ir::Expr::Lit(0)], vec![], vec![]);
                        return Ok(());
                    };
                    plain.push(element);
                }
                let elements = plain.clone();
                *current = join_ground(b, *current, None, empty)?;
                let mut list = assert_width(*width + 1);
                *width += 2;
                for element in plain.into_iter().rev() {
                    *current = b.node(ir::Op::Mint {
                        input: *current,
                        functor: cons,
                        args: vec![element, list],
                    });
                    list = assert_width(*width);
                    *width += 1;
                }
                *current = join_ground(b, *current, None, tuple)?;
                let tuple_col = assert_width(*width + 1);
                *width += 2;
                *current = b.node(ir::Op::Mint {
                    input: *current,
                    functor: app_ctor,
                    args: vec![tuple_col, list],
                });
                let application = assert_width(*width);
                *width += 1;
                *current = b.node(ir::Op::Mint {
                    input: *current,
                    functor: ref_ctor,
                    args: vec![application],
                });
                let whole = assert_width(*width);
                output[value_at] = Some(whole);
                *width += 1;
                b.built_tuples.insert(whole, elements);
            }
        }
        Kernel::Intern => {
            if inputs.len() != 3 {
                return Err("intern arity");
            }
            let (Some(ctor), Some(arguments)) = (inputs[0], inputs[1]) else {
                *current = b.mfp(*current, vec![ir::Expr::Lit(0)], vec![], vec![]);
                return Ok(());
            };
            let ref_ctor = constructor(constructors, "9:1:ref", 1, false)?;
            let app_ctor = constructor(constructors, "9:2:application", 2, false)?;
            *current = join_constructor(b, *current, *width, ref_ctor, ctor);
            let ctor_inner = assert_width(*width + 1);
            *width += 2;
            let plain = if let Some(elements) = b.built_tuples.get(&arguments).cloned() {
                let cons = constructor(constructors, "9:2:[|]", 2, false)?;
                let mut plain = Vec::with_capacity(elements.len());
                for element in elements {
                    *current = join_constructor(b, *current, *width, semantic, element);
                    plain.push(assert_width(*width + 1));
                    *width += 2;
                }
                *current = join_ground(b, *current, None, empty)?;
                let mut list = assert_width(*width + 1);
                *width += 2;
                for element in plain.into_iter().rev() {
                    *current = b.node(ir::Op::Mint {
                        input: *current,
                        functor: cons,
                        args: vec![element, list],
                    });
                    list = assert_width(*width);
                    *width += 1;
                }
                list
            } else {
                let kept: Vec<ir::ColId> = (0..*width).map(assert_width).collect();
                let listed = join_constructor(b, *current, *width, plain_lists, arguments);
                let mut listed_columns = kept.clone();
                listed_columns.push(assert_width(*width + 1));
                let listed = b.mfp(listed, vec![], vec![], listed_columns);
                let mut vector = join_constructor(b, *current, *width, ref_ctor, arguments);
                let application = assert_width(*width + 1);
                vector = join_constructor(b, vector, *width + 2, app_ctor, application);
                let vector_constructor = assert_width(*width + 3);
                let vector_list = assert_width(*width + 4);
                vector = join_ground(b, vector, None, tuple)?;
                let tuple_col = assert_width(*width + 6);
                vector = b.mfp(
                    vector,
                    vec![equal_columns(vector_constructor, tuple_col)],
                    vec![],
                    vec![],
                );
                let const_ctor = constructor(constructors, "9:1:const", 1, false)?;
                vector = b.node(ir::Op::Mint {
                    input: vector,
                    functor: const_ctor,
                    args: vec![vector_list],
                });
                let wrapped = assert_width(*width + 7);
                vector = join_constructor(b, vector, *width + 8, plain_lists, wrapped);
                let mut vector_columns = kept;
                vector_columns.push(assert_width(*width + 9));
                let vector = b.mfp(vector, vec![], vec![], vector_columns);
                *current = b.node(ir::Op::Union(vec![listed, vector]));
                let plain = assert_width(*width);
                *width += 1;
                plain
            };
            *current = b.node(ir::Op::Mint {
                input: *current,
                functor: app_ctor,
                args: vec![ctor_inner, plain],
            });
            let app = assert_width(*width);
            *width += 1;
            *current = b.node(ir::Op::Mint {
                input: *current,
                functor: ref_ctor,
                args: vec![app],
            });
            output[2] = Some(assert_width(*width));
            *width += 1;
            let request = b.mfp(
                *current,
                vec![],
                vec![],
                vec![ctor, arguments, output[2].unwrap()],
            );
            b.intern_nodes.push(request);
        }
        Kernel::EdgeRef => {
            if inputs.len() != 3 {
                return Err("edge_ref arity");
            }
            let (Some(owner), Some(label)) = (inputs[0], inputs[1]) else {
                *current = b.mfp(*current, vec![ir::Expr::Lit(0)], vec![], vec![]);
                return Ok(());
            };
            let ref_ctor = constructor(constructors, "9:1:ref", 1, false)?;
            let edge_ctor = constructor(constructors, "9:2:edge", 2, false)?;
            *current = join_constructor(b, *current, *width, ref_ctor, owner);
            let owner_inner = assert_width(*width + 1);
            *width += 2;
            *current = join_constructor(b, *current, *width, semantic, label);
            let label_inner = assert_width(*width + 1);
            *width += 2;
            *current = b.node(ir::Op::Mint {
                input: *current,
                functor: edge_ctor,
                args: vec![owner_inner, label_inner],
            });
            let edge = assert_width(*width);
            *width += 1;
            *current = b.node(ir::Op::Mint {
                input: *current,
                functor: ref_ctor,
                args: vec![edge],
            });
            output[2] = Some(assert_width(*width));
            *width += 1;
        }
        Kernel::CountStep => {
            if inputs.len() != 3 {
                return Err("count_step arity");
            }
            let (Some(accumulator), Some(_value)) = (inputs[0], inputs[1]) else {
                *current = b.mfp(*current, vec![ir::Expr::Lit(0)], vec![], vec![]);
                return Ok(());
            };
            let wrap = constructor(constructors, "9:1:const", 1, false)?;
            let int = constructor(constructors, "0:int", 0, true)?;
            *current = join_constructor(b, *current, *width, wrap, accumulator);
            let inner = assert_width(*width + 1);
            *width += 2;
            *current = join_constructor(b, *current, *width, int, inner);
            let raw = assert_width(*width + 1);
            *width += 2;
            *current = b.mfp(
                *current,
                vec![ir::Expr::Call(
                    ir::Func::Ne,
                    vec![ir::Expr::Col(raw), ir::Expr::Lit(i64::MAX)],
                )],
                vec![ir::Expr::Call(
                    ir::Func::Add,
                    vec![ir::Expr::Col(raw), ir::Expr::Lit(1)],
                )],
                vec![],
            );
            let next = assert_width(*width);
            *width += 1;
            *current = b.node(ir::Op::Mint {
                input: *current,
                functor: int,
                args: vec![next],
            });
            let int_id = assert_width(*width);
            *width += 1;
            *current = b.node(ir::Op::Mint {
                input: *current,
                functor: wrap,
                args: vec![int_id],
            });
            output[2] = Some(assert_width(*width));
            *width += 1;
        }
        Kernel::MinStep | Kernel::MaxStep => {
            if inputs.len() != 3 {
                return Err("extremum step arity");
            }
            let (Some(accumulator), Some(value)) = (inputs[0], inputs[1]) else {
                *current = b.mfp(*current, vec![ir::Expr::Lit(0)], vec![], vec![]);
                return Ok(());
            };
            let wins = if kernel == Kernel::MinStep {
                (value, accumulator)
            } else {
                (accumulator, value)
            };
            let less = ir::Expr::Call(
                ir::Func::TermLt,
                vec![ir::Expr::Col(wins.0), ir::Expr::Col(wins.1)],
            );
            let chosen = b.mfp(
                *current,
                vec![less.clone()],
                vec![ir::Expr::Col(value)],
                vec![],
            );
            let kept = b.mfp(
                *current,
                vec![ir::Expr::Call(ir::Func::Not, vec![less])],
                vec![ir::Expr::Col(accumulator)],
                vec![],
            );
            *current = b.node(ir::Op::Union(vec![chosen, kept]));
            output[2] = Some(assert_width(*width));
            *width += 1;
        }
    }
    let mut filters = Vec::new();
    for (arg, (before, after)) in goal.args.iter().zip(inputs.iter().zip(&output)) {
        let Some(after) = after else {
            return Err("kernel unbound output");
        };
        if let Some(before) = before {
            if before != after {
                filters.push(equal_columns(*before, *after));
            }
        } else if let Arg::Var(v) = arg {
            bound.insert(v.0, *after);
        }
    }
    if !filters.is_empty() {
        *current = b.mfp(*current, filters, vec![], vec![]);
    }
    if goal.polarity == Polarity::Negative {
        let positive = b.mfp(
            *current,
            vec![],
            vec![],
            (0..base_width).map(assert_width).collect(),
        );
        let columns: Vec<_> = (0..base_width).map(assert_width).collect();
        *current = b.node(ir::Op::Antijoin {
            l: base,
            r: positive,
            lk: columns.clone(),
            rk: columns,
        });
        *width = base_width;
        b.built_tuples.retain(|whole, elements| {
            usize::from(*whole) < base_width
                && elements
                    .iter()
                    .all(|element| usize::from(*element) < base_width)
        });
    }
    Ok(())
}

/// `(not (Relation Args..))` in a fold step: the step rows whose bound
/// arguments have no row in the relation. The right side reads the stored
/// relation, never the step's own recursive rows.
fn fold_step_antijoin(
    b: &mut Builder,
    goal: &Goal,
    current: ir::NodeId,
    bound: &HashMap<u32, ir::ColId>,
    relations: &BTreeMap<(u32, usize), ir::RelId>,
) -> Result<ir::NodeId, &'static str> {
    let mut right = b.get(
        *relations
            .get(&(goal.rel.0, goal.args.len()))
            .ok_or("unresolved fold step relation")?,
    );
    let mut lk = Vec::new();
    let mut rk = Vec::new();
    for (position, arg) in goal.args.iter().enumerate() {
        let position = assert_width(position);
        match arg {
            Arg::Var(v) => {
                lk.push(*bound.get(&v.0).ok_or("unbound negative fold step goal")?);
                rk.push(position);
            }
            Arg::Ground(id) => {
                right = join_ground(b, right, Some(position), *id)?;
            }
            _ => return Err("fold step aggregate"),
        }
    }
    let projected = b.mfp(right, vec![], vec![], rk);
    let columns = (0..lk.len()).map(assert_width).collect();
    Ok(b.node(ir::Op::Antijoin {
        l: current,
        r: projected,
        lk,
        rk: columns,
    }))
}

#[allow(clippy::too_many_arguments)]
fn compile_fold_step_rule(
    b: &mut Builder,
    u: &Universe,
    rule: &Rule,
    mut current: ir::NodeId,
    mut width: usize,
    mut bound: HashMap<u32, ir::ColId>,
    relations: &BTreeMap<(u32, usize), ir::RelId>,
    constructors: &BTreeMap<(String, usize, bool), ir::RelId>,
    semantic: ir::RelId,
    plain_lists: ir::RelId,
    empty: TermId,
    tuple: TermId,
) -> Result<(ir::NodeId, ir::ColId), &'static str> {
    b.built_tuples.clear();
    for goal in &rule.body {
        if let Some(kernel) = Kernel::of(u, goal.rel) {
            compile_kernel(
                b,
                goal,
                kernel,
                constructors,
                semantic,
                plain_lists,
                &mut current,
                &mut width,
                &mut bound,
                empty,
                tuple,
            )?;
            continue;
        }
        if goal.polarity == Polarity::Negative {
            current = fold_step_antijoin(b, goal, current, &bound, relations)?;
            continue;
        }
        let right = b.get(
            *relations
                .get(&(goal.rel.0, goal.args.len()))
                .ok_or("unresolved fold step relation")?,
        );
        let mut equivalences = Vec::new();
        let mut first_here = HashMap::new();
        for (position, arg) in goal.args.iter().enumerate() {
            let position = assert_width(position);
            match arg {
                Arg::Var(v) => {
                    if let Some(&left) = bound.get(&v.0) {
                        equivalences.push(vec![(0, left), (1, position)]);
                    } else if let Some(&prior) = first_here.get(&v.0) {
                        equivalences.push(vec![(1, prior), (1, position)]);
                    } else {
                        first_here.insert(v.0, position);
                    }
                }
                Arg::Ground(id) => {
                    current = join_ground(b, current, None, *id)?;
                    let ground_col = assert_width(width + 1);
                    width += 2;
                    equivalences.push(vec![(0, ground_col), (1, position)]);
                }
                _ => return Err("fold step aggregate"),
            }
        }
        current = b.node(ir::Op::Join {
            inputs: vec![current, right],
            equivalences,
    project: vec![],
        });
        for (variable, position) in first_here {
            bound.insert(variable, assert_width(width + position as usize));
        }
        width += goal.args.len().max(1);
    }
    let next = match rule.head.get(2) {
        Some(Arg::Var(v)) => *bound.get(&v.0).ok_or("unbound fold step output")?,
        Some(Arg::Ground(id)) => {
            current = join_ground(b, current, None, *id)?;
            assert_width(width + 1)
        }
        _ => return Err("fold step output"),
    };
    Ok((current, next))
}

/// `later[i]`: the variables of the goals after goal `i` and of the head. A column holding any
/// other variable is dead once goal `i` has run.
fn later_vars(rule: &Rule) -> Vec<HashSet<u32>> {
    let mut later = vec![HashSet::new(); rule.body.len()];
    let mut live: HashSet<u32> = rule.head.iter().filter_map(|arg| var(arg.innermost())).collect();
    for (index, goal) in rule.body.iter().enumerate().rev() {
        later[index] = live.clone();
        live.extend(goal.args.iter().filter_map(|arg| var(arg.innermost())));
    }
    later
}

/// Projects `current` onto the columns of the `live` variables, plus the element columns of a
/// tuple a kept column holds (`Builder::built_tuples`), and renumbers `bound` and
/// `built_tuples`. One column stays when none is live: an empty `project` keeps every column.
fn keep_live(
    b: &mut Builder,
    current: &mut ir::NodeId,
    width: &mut usize,
    bound: &mut HashMap<u32, ir::ColId>,
    live: &HashSet<u32>,
) {
    let mut keep: BTreeSet<ir::ColId> = bound
        .iter()
        .filter(|(v, _)| live.contains(v))
        .map(|(_, col)| *col)
        .collect();
    // A tuple's element may itself be a built tuple: repeat until no column joins `keep`.
    // Bounded by the tuple count: each pass that changes `keep` adds at least one tuple.
    for _ in 0..=b.built_tuples.len() {
        let before = keep.len();
        for (whole, elements) in &b.built_tuples {
            if keep.contains(whole) {
                keep.extend(elements.iter().copied());
            }
        }
        if keep.len() == before {
            break;
        }
    }
    if keep.is_empty() {
        keep.insert(0);
    }
    if keep.len() == *width {
        return;
    }
    let project: Vec<ir::ColId> = keep.into_iter().collect();
    let at = |col: ir::ColId| project.binary_search(&col).ok().map(assert_width);
    bound.retain(|_, col| match at(*col) {
        Some(moved) => {
            *col = moved;
            true
        }
        None => false,
    });
    b.built_tuples = std::mem::take(&mut b.built_tuples)
        .into_iter()
        .filter_map(|(whole, elements)| {
            let elements = elements.into_iter().map(at).collect::<Option<Vec<_>>>()?;
            Some((at(whole)?, elements))
        })
        .collect();
    *width = project.len();
    *current = b.mfp(*current, vec![], vec![], project);
}

fn compile_rule(
    b: &mut Builder,
    u: &Universe,
    rule: &Rule,
    relations: &BTreeMap<(u32, usize), ir::RelId>,
    constructors: &BTreeMap<(String, usize, bool), ir::RelId>,
    semantic: ir::RelId,
    plain_lists: ir::RelId,
    unit: ir::RelId,
    served: &std::collections::HashSet<TermId>,
    none: TermId,
    empty: TermId,
    tuple: TermId,
    all_rules: &[Rule],
) -> Result<CompiledRule, &'static str> {
    b.built_tuples.clear();
    // A positive relation read first supplies the first row itself (`current = right` below,
    // before anything reads `current`); every other first goal starts from the unit row.
    let reads_first = rule.body.first().is_some_and(|goal| {
        goal.polarity == Polarity::Positive
            && !served.contains(&goal.rel)
            && (Kernel::of(u, goal.rel).is_none() || rule.is_aggregate())
    });
    let mut current = if reads_first { ir::NodeId::MAX } else { b.get(unit) };
    let mut width = 1usize;
    let mut bound: HashMap<u32, ir::ColId> = HashMap::new();
    let later = later_vars(rule);
    for (goal_index, goal) in rule.body.iter().enumerate() {
        let live = &later[goal_index];
        if served.contains(&goal.rel) {
            served_effect(
                b,
                goal,
                current,
                width,
                &bound,
                constructors,
                semantic,
                none,
                empty,
                tuple,
            )?;
        }
        if let Some(kernel) = Kernel::of(u, goal.rel)
            .filter(|_| !rule.is_aggregate() || goal.polarity == Polarity::Negative)
        {
            compile_kernel(
                b,
                goal,
                kernel,
                constructors,
                semantic,
                plain_lists,
                &mut current,
                &mut width,
                &mut bound,
                empty,
                tuple,
            )?;
            keep_live(b, &mut current, &mut width, &mut bound, live);
            continue;
        }
        let rel = *relations
            .get(&(goal.rel.0, goal.args.len()))
            .ok_or("unresolved relation")?;
        let mut equivalences = Vec::new();
        let mut repeated = Vec::new();
        let mut first_here: HashMap<u32, ir::ColId> = HashMap::new();
        let mut grounds = Vec::new();
        for (position, arg) in goal.args.iter().enumerate() {
            let position = assert_width(position);
            if let Some(v) = var(arg) {
                if let Some(&left) = bound.get(&v) {
                    equivalences.push(vec![(0, left), (1, position)]);
                } else if let Some(&prior) = first_here.get(&v) {
                    repeated.push(equal_columns(prior, position));
                } else {
                    first_here.insert(v, position);
                }
            } else if let Some(id) = ground(arg) {
                grounds.push((position, id));
            } else {
                return Err("goal aggregate or fold");
            }
        }
        if goal.polarity == Polarity::Negative {
            let mut right = b.get(rel);
            if !repeated.is_empty() {
                right = b.mfp(right, std::mem::take(&mut repeated), vec![], vec![]);
            }
            // Negative arguments must already be bound. A ground term has a
            // one-row map source and joins the right before the antijoin.
            // Unbound here: no rows, in the head's arity.
            if first_here.keys().any(|v| !bound.contains_key(v)) {
                let out_width = rule.head.len().max(1);
                return Ok(CompiledRule {
                    body: b.mfp(
                        current,
                        vec![ir::Expr::Lit(0)],
                        (0..out_width).map(|_| ir::Expr::Col(0)).collect(),
                        (width..width + out_width).map(assert_width).collect(),
                    ),
                    fold_aux: None,
                });
            }
            let right_node = if grounds.is_empty() {
                right
            } else {
                let mut filtered = right;
                let mut size = goal.args.len().max(1);
                for (position, original) in &grounds {
                    filtered = join_ground(b, filtered, Some(*position), *original)?;
                    filtered = b.mfp(
                        filtered,
                        vec![],
                        vec![],
                        (0..size).map(assert_width).collect(),
                    );
                }
                filtered
            };
            let mut lk = Vec::new();
            let mut rk = Vec::new();
            for (position, arg) in goal.args.iter().enumerate() {
                if let Some(v) = var(arg) {
                    if let Some(&col) = bound.get(&v) {
                        lk.push(col);
                        rk.push(assert_width(position));
                    }
                }
            }
            current = b.node(ir::Op::Antijoin {
                l: current,
                r: right_node,
                lk,
                rk,
            });
            keep_live(b, &mut current, &mut width, &mut bound, live);
            continue;
        }
        // The right input keeps its join keys, its ground positions, and the first position of
        // each new variable still live after this goal; a repeated position is filtered first.
        let mut kept: Vec<ir::ColId> = Vec::new();
        for (position, arg) in goal.args.iter().enumerate() {
            let position = assert_width(position);
            let keep = match var(arg) {
                Some(v) => {
                    bound.contains_key(&v)
                        || (first_here.get(&v) == Some(&position) && live.contains(&v))
                }
                None => true,
            };
            if keep {
                kept.push(position);
            }
        }
        if kept.is_empty() && !goal.args.is_empty() {
            kept.push(0);
        }
        // One shared narrowing per (relation, join key positions): the union of what every
        // reader in the group keeps (`Builder::narrow_unions`). A later goal's key is its bound
        // positions. The first goal is the left input of its first ground term's join, or else
        // of the second goal's join: keyed on that ground position, on the positions whose
        // variable a positive relation second goal reads, or on none (a kernel's cross join with
        // its ground rows). It joins that group so the left arrangement is the shared one.
        let joins_next = |next: &Goal| {
            next.polarity == Polarity::Positive
                && (Kernel::of(u, next.rel).is_none() || rule.is_aggregate())
        };
        let group_keys: Option<Vec<ir::ColId>> = if !repeated.is_empty() {
            None
        } else if goal_index > 0 {
            Some(
                goal.args
                    .iter()
                    .enumerate()
                    .filter(|(_, arg)| var(arg).is_some_and(|v| bound.contains_key(&v)))
                    .map(|(position, _)| assert_width(position))
                    .collect(),
            )
        } else if let Some((position, _)) = grounds.first() {
            // The first ground term's cross join keys the read on that position.
            Some(vec![*position])
        } else {
            let read: HashSet<u32> = rule
                .body
                .get(1)
                .filter(|next| joins_next(next))
                .map(|next| next.args.iter().filter_map(var).collect())
                .unwrap_or_default();
            let mut keys: Vec<ir::ColId> = first_here
                .iter()
                .filter(|(v, _)| read.contains(v))
                .map(|(_, position)| *position)
                .collect();
            keys.sort_unstable();
            Some(keys)
        };
        let shared_first = goal_index == 0 && grounds.is_empty() && group_keys.is_some();
        if let Some(keys) = group_keys {
            let group = (rel, keys);
            if b.narrow_record {
                b.narrow_unions
                    .entry(group)
                    .or_default()
                    .extend(kept.iter().copied());
            } else {
                let union = b
                    .narrow_unions
                    .get(&group)
                    .ok_or("reader missing from the narrowing pass")?;
                if !kept.iter().all(|position| union.contains(position)) {
                    return Err("narrowing union misses a kept column");
                }
                kept = union.iter().copied().collect();
            }
        }
        let narrowed = kept.len() < goal.args.len();
        let right = if !repeated.is_empty() {
            let right = b.get(rel);
            b.mfp(
                right,
                repeated,
                vec![],
                if narrowed { kept.clone() } else { vec![] },
            )
        } else if narrowed {
            match b.narrowed_gets.get(&(rel, kept.clone())) {
                Some(&node) => node,
                None => {
                    let right = b.get(rel);
                    let node = b.mfp(right, vec![], vec![], kept.clone());
                    b.narrowed_gets.insert((rel, kept.clone()), node);
                    node
                }
            }
        } else {
            b.get(rel)
        };
        let at = |position: ir::ColId| -> Option<ir::ColId> {
            if narrowed {
                kept.iter().position(|kept| *kept == position).map(assert_width)
            } else {
                Some(position)
            }
        };
        for class in &mut equivalences {
            for (side, col) in class.iter_mut() {
                if *side == 1 {
                    *col = at(*col).ok_or("join key projected away")?;
                }
            }
        }
        if goal_index == 0 {
            // The first positive atom supplies the row. No bound column can
            // refer to the unit relation's single placeholder column.
            current = right;
            width = 0;
            b.built_tuples.clear();
        } else {
            current = b.node(ir::Op::Join {
                inputs: vec![current, right],
                equivalences,
    project: vec![],
            });
        }
        let total = width + if narrowed { kept.len() } else { goal.args.len().max(1) };
        let mut grown = total;
        for (position, original) in &grounds {
            let position = at(*position).ok_or("ground position projected away")?;
            current = join_ground(b, current, Some(assert_width(width + position as usize)), *original)?;
            grown += 2;
        }
        for (v, position) in first_here {
            if let Some(position) = at(position) {
                bound.insert(v, assert_width(width + position as usize));
            }
        }
        width = grown;
        // A shared first read stays whole into the second goal's join, whose projection drops
        // its dead columns.
        if !shared_first {
            keep_live(b, &mut current, &mut width, &mut bound, live);
        }
    }
    if rule.is_aggregate() {
        if rule.aggregate_args() != 1 {
            return Err("aggregate head arity");
        }
        let (position, folding, subject) = rule.folding_head().ok_or("aggregate head")?;
        let subject = match subject {
            Arg::Var(v) => *bound.get(&v.0).ok_or("unbound aggregate subject")?,
            Arg::Ground(id) => {
                current = join_ground(b, current, None, *id)?;
                let col = assert_width(width + 1);
                width += 2;
                col
            }
            _ => return Err("nested aggregate subject"),
        };
        let mut keys = Vec::new();
        for (index, arg) in rule.head.iter().enumerate() {
            if index == position {
                continue;
            }
            match arg {
                Arg::Var(v) => keys.push(*bound.get(&v.0).ok_or("unbound aggregate key")?),
                Arg::Ground(id) => {
                    current = join_ground(b, current, None, *id)?;
                    keys.push(assert_width(width + 1));
                    width += 2;
                }
                _ => return Err("multiple aggregate heads"),
            }
        }
        if let Folding::Declared(fold) = folding {
            let key_count = keys.len();
            let linear_step = match Kernel::of(u, fold.step) {
                Some(Kernel::IntAdd) => linear((Some("int"), "add")),
                Some(Kernel::CountStep) => linear((None, "count_step")),
                _ => false,
            };
            if linear_step {
                let kernel = Kernel::of(u, fold.step).unwrap();
                let mut window_input;
                let func;
                if kernel == Kernel::IntAdd {
                    let wrap = constructor(constructors, "9:1:const", 1, false)?;
                    let int = constructor(constructors, "0:int", 0, true)?;
                    current = join_constructor(b, current, width, wrap, subject);
                    let inner = assert_width(width + 1);
                    width += 2;
                    current = join_constructor(b, current, width, int, inner);
                    let raw = assert_width(width + 1);
                    window_input = b.mfp(
                        current,
                        vec![],
                        vec![],
                        keys.iter().copied().chain([subject, raw]).collect(),
                    );
                    func = ir::WinFn::Sum(assert_width(key_count + 1));
                } else {
                    window_input = b.mfp(
                        current,
                        vec![],
                        vec![],
                        keys.iter().copied().chain([subject]).collect(),
                    );
                    func = ir::WinFn::Count;
                }
                window_input = b.node(ir::Op::Window {
                    input: window_input,
                    partition: (0..key_count).map(assert_width).collect(),
                    order: vec![ir::Order {
                        col: assert_width(key_count),
                        desc: false,
                    }],
                    func,
                });
                current = b.node(ir::Op::TopK {
                    input: window_input,
                    key: (0..key_count).map(assert_width).collect(),
                    order: vec![ir::Order {
                        col: assert_width(key_count),
                        desc: true,
                    }],
                    limit: 1,
                });
                let mut folded_raw = assert_width(if kernel == Kernel::IntAdd {
                    key_count + 2
                } else {
                    key_count + 1
                });
                let seed = match fold.seed {
                    super::program::Seed::Zero => 0,
                    super::program::Seed::Term(seed) => u
                        .unary(seed, "const")
                        .and_then(|raw| u.as_int(raw))
                        .ok_or("linear fold seed is not an integer")?,
                    super::program::Seed::FirstValue => return Err("linear fold first-value seed"),
                };
                if seed != 0 {
                    current = b.mfp(
                        current,
                        vec![],
                        vec![ir::Expr::Call(
                            ir::Func::Add,
                            vec![ir::Expr::Col(folded_raw), ir::Expr::Lit(seed)],
                        )],
                        vec![],
                    );
                    folded_raw = assert_width(if kernel == Kernel::IntAdd {
                        key_count + 3
                    } else {
                        key_count + 2
                    });
                }
                let int = constructor(constructors, "0:int", 0, true)?;
                let wrap = constructor(constructors, "9:1:const", 1, false)?;
                current = b.node(ir::Op::Mint {
                    input: current,
                    functor: int,
                    args: vec![folded_raw],
                });
                let inner = folded_raw + 1;
                current = b.node(ir::Op::Mint {
                    input: current,
                    functor: wrap,
                    args: vec![inner],
                });
                let result = inner + 1;
                let mut projected = Vec::new();
                let mut key_index = 0;
                for index in 0..rule.head.len() {
                    if index == position {
                        projected.push(result);
                    } else {
                        projected.push(assert_width(key_index));
                        key_index += 1;
                    }
                }
                return Ok(CompiledRule {
                    body: b.mfp(current, vec![], vec![], projected),
                    fold_aux: None,
                });
            }
            if matches!(
                Kernel::of(u, fold.step),
                Some(Kernel::MinStep | Kernel::MaxStep)
            ) {
                current = b.mfp(
                    current,
                    vec![],
                    vec![],
                    keys.iter().copied().chain([subject]).collect(),
                );
                if let super::program::Seed::Term(seed) = fold.seed {
                    let groups = b.node(ir::Op::Reduce {
                        input: current,
                        key: (0..key_count).map(assert_width).collect(),
                        aggs: vec![ir::Agg::Count],
                    });
                    let seeded = join_ground(b, groups, None, seed)?;
                    let seeded = b.mfp(
                        seeded,
                        vec![],
                        vec![],
                        (0..key_count)
                            .map(assert_width)
                            .chain([assert_width(key_count + 2)])
                            .collect(),
                    );
                    current = b.node(ir::Op::Union(vec![current, seeded]));
                } else if fold.seed != super::program::Seed::FirstValue {
                    return Err("extremum fold seed");
                }
                let aggregate = if Kernel::of(u, fold.step) == Some(Kernel::MinStep) {
                    ir::Agg::Min(assert_width(key_count))
                } else {
                    ir::Agg::Max(assert_width(key_count))
                };
                current = b.node(ir::Op::Reduce {
                    input: current,
                    key: (0..key_count).map(assert_width).collect(),
                    aggs: vec![aggregate],
                });
                let result = assert_width(key_count);
                let mut projected = Vec::new();
                let mut key_index = 0;
                for index in 0..rule.head.len() {
                    if index == position {
                        projected.push(result);
                    } else {
                        projected.push(assert_width(key_index));
                        key_index += 1;
                    }
                }
                return Ok(CompiledRule {
                    body: b.mfp(current, vec![], vec![], projected),
                    fold_aux: None,
                });
            }
            let ranked = b.rel(
                format!("__ir_fold_ranked_{}", b.next_rel),
                keys.iter()
                    .map(|_| ir::Ty::Id)
                    .chain([ir::Ty::Id, ir::Ty::Int])
                    .collect(),
                ir::RelKind::Derived,
            );
            let ranked_input = b.mfp(
                current,
                vec![],
                vec![],
                keys.iter().copied().chain([subject]).collect(),
            );
            let ranked_body = b.node(ir::Op::Window {
                input: ranked_input,
                partition: (0..key_count).map(assert_width).collect(),
                order: vec![ir::Order {
                    col: assert_width(key_count),
                    desc: false,
                }],
                func: ir::WinFn::RowNumber,
            });
            b.declared_folds.push(DeclaredFoldSpec {
                ranked,
                key_count,
                fold,
            });
            b.ir.outputs.push(ranked);
            let accumulator = b.rel(
                format!("__ir_fold_acc_{}", b.next_rel),
                keys.iter()
                    .map(|_| ir::Ty::Id)
                    .chain([ir::Ty::Int, ir::Ty::Id])
                    .collect(),
                ir::RelKind::Derived,
            );
            let seed = match fold.seed {
                super::program::Seed::Term(seed) => seed,
                _ => return Err("declared fold seed"),
            };
            let groups = b.node(ir::Op::Reduce {
                input: ranked_input,
                key: (0..key_count).map(assert_width).collect(),
                aggs: vec![ir::Agg::Count],
            });
            let seeded = join_ground(b, groups, None, seed)?;
            let seeded = b.mfp(
                seeded,
                vec![],
                vec![ir::Expr::Lit(0)],
                (0..key_count)
                    .map(assert_width)
                    .chain([assert_width(key_count + 3), assert_width(key_count + 2)])
                    .collect(),
            );
            let mut recursive_parts = Vec::new();
            for step_rule in all_rules
                .iter()
                .filter(|candidate| candidate.rel == fold.step && candidate.head.len() == 3)
            {
                let acc = b.get(accumulator);
                let acc_with_next_rank = b.mfp(
                    acc,
                    vec![],
                    vec![ir::Expr::Call(
                        ir::Func::Add,
                        vec![ir::Expr::Col(assert_width(key_count)), ir::Expr::Lit(1)],
                    )],
                    vec![],
                );
                let ranked_rows = b.get(ranked);
                let mut equivalences: Vec<Vec<(u8, ir::ColId)>> = (0..key_count)
                    .map(|column| vec![(0, assert_width(column)), (1, assert_width(column))])
                    .collect();
                equivalences.push(vec![
                    (0, assert_width(key_count + 2)),
                    (1, assert_width(key_count + 1)),
                ]);
                let step_input = b.node(ir::Op::Join {
                    inputs: vec![acc_with_next_rank, ranked_rows],
                    equivalences,
    project: vec![],
                });
                let mut step_width = key_count * 2 + 5;
                let accumulator_col = assert_width(key_count + 1);
                let subject_col = assert_width(key_count * 2 + 3);
                let mut step_bound = HashMap::new();
                let mut prepared = step_input;
                for (argument, column) in step_rule
                    .head
                    .iter()
                    .take(2)
                    .zip([accumulator_col, subject_col])
                {
                    match argument {
                        Arg::Var(variable) => {
                            if let Some(&prior) = step_bound.get(&variable.0) {
                                prepared = b.mfp(
                                    prepared,
                                    vec![equal_columns(prior, column)],
                                    vec![],
                                    vec![],
                                );
                            } else {
                                step_bound.insert(variable.0, column);
                            }
                        }
                        Arg::Ground(term) => {
                            prepared = join_ground(b, prepared, Some(column), *term)?;
                            step_width += 2;
                        }
                        _ => return Err("fold step head aggregate"),
                    }
                }
                let (step_body, next) = compile_fold_step_rule(
                    b,
                    u,
                    step_rule,
                    prepared,
                    step_width,
                    step_bound,
                    relations,
                    constructors,
                    semantic,
                    plain_lists,
                    empty,
                    tuple,
                )?;
                recursive_parts.push(
                    b.mfp(
                        step_body,
                        vec![],
                        vec![],
                        (0..key_count)
                            .map(assert_width)
                            .chain([assert_width(key_count + 2), next])
                            .collect(),
                    ),
                );
            }
            if recursive_parts.is_empty() {
                let step_relation = *relations
                    .get(&(fold.step.0, 3))
                    .ok_or("declared fold step relation")?;
                let acc = b.get(accumulator);
                let acc_with_next_rank = b.mfp(
                    acc,
                    vec![],
                    vec![ir::Expr::Call(
                        ir::Func::Add,
                        vec![ir::Expr::Col(assert_width(key_count)), ir::Expr::Lit(1)],
                    )],
                    vec![],
                );
                let ranked_rows = b.get(ranked);
                let mut rank_equivalences: Vec<Vec<(u8, ir::ColId)>> = (0..key_count)
                    .map(|column| vec![(0, assert_width(column)), (1, assert_width(column))])
                    .collect();
                rank_equivalences.push(vec![
                    (0, assert_width(key_count + 2)),
                    (1, assert_width(key_count + 1)),
                ]);
                let step_input = b.node(ir::Op::Join {
                    inputs: vec![acc_with_next_rank, ranked_rows],
                    equivalences: rank_equivalences,
                    project: vec![],
                });
                let step_rows = b.get(step_relation);
                let accumulator_col = assert_width(key_count + 1);
                let subject_col = assert_width(key_count * 2 + 3);
                let stepped = b.node(ir::Op::Join {
                    inputs: vec![step_input, step_rows],
                    equivalences: vec![
                        vec![(0, accumulator_col), (1, 0)],
                        vec![(0, subject_col), (1, 1)],
                    ],
                    project: vec![],
                });
                recursive_parts.push(
                    b.mfp(
                        stepped,
                        vec![],
                        vec![],
                        (0..key_count)
                            .map(assert_width)
                            .chain([assert_width(key_count + 2), assert_width(key_count * 2 + 7)])
                            .collect(),
                    ),
                );
            }
            let mut acc_parts = vec![seeded];
            acc_parts.extend(recursive_parts);
            let accumulator_body = b.node(ir::Op::Union(acc_parts));
            let ranked_rows = b.get(ranked);
            let max_rank = b.node(ir::Op::Reduce {
                input: ranked_rows,
                key: (0..key_count).map(assert_width).collect(),
                aggs: vec![ir::Agg::Max(assert_width(key_count + 1))],
            });
            let final_acc = b.get(accumulator);
            let mut equivalences: Vec<Vec<(u8, ir::ColId)>> = (0..key_count)
                .map(|column| vec![(0, assert_width(column)), (1, assert_width(column))])
                .collect();
            equivalences.push(vec![
                (0, assert_width(key_count)),
                (1, assert_width(key_count)),
            ]);
            let final_rows = b.node(ir::Op::Join {
                inputs: vec![max_rank, final_acc],
                equivalences,
    project: vec![],
            });
            let result = assert_width(key_count * 2 + 2);
            let mut projected = Vec::new();
            let mut key_index = 0;
            for index in 0..rule.head.len() {
                if index == position {
                    projected.push(result);
                } else {
                    projected.push(assert_width(key_count + 1 + key_index));
                    key_index += 1;
                }
            }
            return Ok(CompiledRule {
                body: b.mfp(final_rows, vec![], vec![], projected),
                fold_aux: Some(FoldAux {
                    ranked,
                    ranked_body,
                    accumulator,
                    accumulator_body,
                }),
            });
        }
        let Folding::Builtin(kind) = folding else {
            unreachable!()
        };
        let aggregate = match kind {
            AggregateKind::Count => ir::Agg::Count,
            AggregateKind::Min => ir::Agg::Min(subject),
            AggregateKind::Max => ir::Agg::Max(subject),
            AggregateKind::Sum => {
                let wrap = constructor(constructors, "9:1:const", 1, false)?;
                let int = constructor(constructors, "0:int", 0, true)?;
                current = join_constructor(b, current, width, wrap, subject);
                let inner = assert_width(width + 1);
                width += 2;
                current = join_constructor(b, current, width, int, inner);
                let raw = assert_width(width + 1);
                ir::Agg::Sum(raw)
            }
        };
        current = b.node(ir::Op::Reduce {
            input: current,
            key: keys.clone(),
            aggs: vec![aggregate],
        });
        let mut result = assert_width(keys.len());
        if matches!(kind, AggregateKind::Count | AggregateKind::Sum) {
            let int = constructor(constructors, "0:int", 0, true)?;
            let wrap = constructor(constructors, "9:1:const", 1, false)?;
            current = b.node(ir::Op::Mint {
                input: current,
                functor: int,
                args: vec![result],
            });
            result = assert_width(keys.len() + 1);
            current = b.node(ir::Op::Mint {
                input: current,
                functor: wrap,
                args: vec![result],
            });
            result = assert_width(keys.len() + 2);
        }
        let mut projected = Vec::new();
        let mut key_index = 0;
        for index in 0..rule.head.len() {
            if index == position {
                projected.push(result);
            } else {
                projected.push(assert_width(key_index));
                key_index += 1;
            }
        }
        return Ok(CompiledRule {
            body: b.mfp(current, vec![], vec![], projected),
            fold_aux: None,
        });
    }
    let mut head_cols = Vec::new();
    for arg in &rule.head {
        if let Some(v) = var(arg) {
            let Some(&col) = bound.get(&v) else {
                let out_width = rule.head.len().max(1);
                let mapped = b.mfp(
                    current,
                    vec![ir::Expr::Lit(0)],
                    (0..out_width).map(|_| ir::Expr::Col(0)).collect(),
                    (width..width + out_width).map(assert_width).collect(),
                );
                return Ok(CompiledRule {
                    body: mapped,
                    fold_aux: None,
                });
            };
            head_cols.push(col);
        } else if let Some(id) = ground(arg) {
            current = join_ground(b, current, None, id)?;
            head_cols.push(assert_width(width + 1));
            width += 2;
        } else {
            return Err("head aggregate or fold");
        }
    }
    if head_cols.is_empty() {
        head_cols.push(0);
    }
    Ok(CompiledRule {
        body: b.mfp(current, vec![], vec![], head_cols),
        fold_aux: None,
    })
}

/// Magic sets, one demand copy per caller level: a copy shared across strata
/// can pull a lower negative read into its caller's recursive component.
fn with_demand_in_strata(
    u: &mut Universe,
    program: &Program,
    strata: &super::stratify::Strata,
) -> (Program, HashSet<TermId>) {
    let snapshots = {
        let name = u.atom("intern_snapshot");
        let kernel = u.compound("kernel", vec![name]);
        u.compound("ref", vec![kernel])
    };
    let mut by_rel: HashMap<TermId, Vec<Rule>> = HashMap::new();
    for rule in &program.rules {
        by_rel.entry(rule.rel).or_default().push(rule.clone());
    }
    let aggregates: HashSet<TermId> = program
        .rules
        .iter()
        .filter(|rule| rule.is_aggregate())
        .map(|rule| rule.rel)
        .collect();
    let demanded: HashSet<TermId> = by_rel
        .iter()
        .filter(|(relation, rules)| {
            !aggregates.contains(*relation)
                && rules
                    .iter()
                    .any(|rule| needs_bound_head(u, rule, snapshots))
        })
        .map(|(relation, _)| *relation)
        .collect();
    // Pass-through variable names exist from the first run on, so a reload
    // with more seeded products mints nothing new.
    let widest = program
        .rules
        .iter()
        .map(|rule| rule.head.len())
        .max()
        .unwrap_or(0);
    let pass_vars: Vec<TermId> = (0..widest).map(|i| u.atom(&format!("__s{i}"))).collect();
    let mut out = program.clone();
    if demanded.is_empty() {
        return (out, HashSet::new());
    }
    let mut hidden: HashSet<TermId> = HashSet::new();
    let mut adorned: HashMap<(TermId, Vec<usize>, u32), (TermId, TermId)> = HashMap::new();
    let mut queue: Vec<(Rule, u32)> = std::mem::take(&mut out.rules)
        .into_iter()
        .map(|rule| {
            let level = strata.level(rule.rel);
            (rule, level)
        })
        .collect();
    let mut done: Vec<Rule> = Vec::new();
    // Every adorned copy re-enters the queue, so the walk is bounded by the
    // number of (product, bound positions) pairs times the rule count.
    let budget = program.rules.len() * (1 + program.rules.len() * 8);
    for _ in 0..budget {
        let Some((mut rule, level)) = queue.pop() else {
            break;
        };
        // Aggregate reads already force a strict dependency. Adorning their
        // bodies can route that dependency back through a demand relation.
        if rule.is_aggregate() {
            done.push(rule);
            continue;
        }
        let mut bound: HashSet<u32> = HashSet::new();
        for at in 0..rule.body.len() {
            let goal = rule.body[at].clone();
            let positive = goal.polarity == Polarity::Positive;
            if positive && demanded.contains(&goal.rel) {
                let positions: Vec<usize> = goal
                    .args
                    .iter()
                    .enumerate()
                    .filter(|(_, arg)| match arg {
                        Arg::Var(v) => bound.contains(&v.0),
                        Arg::Ground(_) => true,
                        _ => false,
                    })
                    .map(|(position, _)| position)
                    .collect();
                let key = (goal.rel, positions.clone(), level);
                let (demand_rel, adorned_rel) = match adorned.get(&key) {
                    Some(pair) => *pair,
                    None => {
                        let base = u
                            .unary(goal.rel, "ref")
                            .and_then(|inner| {
                                u.functor_or_atom(inner).map(|(name, _)| name.to_string())
                            })
                            .unwrap_or_else(|| format!("n{}", goal.rel.0));
                        let mask: String = positions.iter().map(|p| p.to_string()).collect();
                        // The suffix is the rel term itself, so the name survives a
                        // db reload that renumbers the arena.
                        let spelled = u.display(goal.rel).to_string();
                        let demand_name =
                            u.atom(&format!("dmd__{base}__{mask}__{spelled}__l{level}"));
                        let demand_rel = u.compound("ref", vec![demand_name]);
                        let adorned_name =
                            u.atom(&format!("adn__{base}__{mask}__{spelled}__l{level}"));
                        let adorned_rel = u.compound("ref", vec![adorned_name]);
                        adorned.insert(key, (demand_rel, adorned_rel));
                        hidden.insert(demand_rel);
                        hidden.insert(adorned_rel);
                        for original in by_rel.get(&goal.rel).into_iter().flatten() {
                            let mut body = vec![Goal {
                                polarity: Polarity::Positive,
                                rel: demand_rel,
                                args: positions
                                    .iter()
                                    .map(|p| original.head[*p].clone())
                                    .collect(),
                            }];
                            body.extend(original.body.iter().cloned());
                            queue.push((
                                Rule {
                                    rel: adorned_rel,
                                    head: original.head.clone(),
                                    body,
                                    vars: original.vars.clone(),
                                },
                                level,
                            ));
                        }
                        // Answers and frontiers write every product's source,
                        // so the adorned copy reads the product too.
                        let arity = goal.args.len();
                        let vars: Vec<TermId> = pass_vars[..arity.min(pass_vars.len())].to_vec();
                        let args: Vec<Arg> = (0..arity)
                            .map(|i| Arg::Var(super::program::VarId(i as u32)))
                            .collect();
                        done.push(Rule {
                            rel: adorned_rel,
                            head: args.clone(),
                            body: vec![Goal {
                                polarity: Polarity::Positive,
                                rel: goal.rel,
                                args,
                            }],
                            vars,
                        });
                        (demand_rel, adorned_rel)
                    }
                };
                done.push(Rule {
                    rel: demand_rel,
                    head: positions.iter().map(|p| goal.args[*p].clone()).collect(),
                    body: rule.body[..at].to_vec(),
                    vars: rule.vars.clone(),
                });
                rule.body[at].rel = adorned_rel;
            }
            if positive {
                for arg in &goal.args {
                    if let Arg::Var(v) = arg {
                        bound.insert(v.0);
                    }
                }
            }
        }
        done.push(rule);
    }
    if !queue.is_empty() {
        tracing::error!(target: "dl8::eval", left = queue.len(), "demand rewrite budget exhausted");
    }
    out.rules = done;
    (out, hidden)
}

fn collect_arg(arg: &Arg, todo: &mut Vec<TermId>) {
    for layer in arg.layers() {
        match layer {
            Arg::Ground(term) => todo.push(*term),
            Arg::Fold(fold, _) => {
                todo.push(fold.step);
                if let super::program::Seed::Term(seed) = fold.seed {
                    todo.push(seed);
                }
            }
            Arg::Aggregate(..) | Arg::Var(_) => {}
        }
    }
}

/// Relations and term shapes the IR carries beyond what the program's rules and seeds name:
/// rows and terms the caller sends later through `IrSession::apply`, never through lowering.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct Declared {
    /// `(relation, arity)` of every relation the caller seeds.
    pub sources: Vec<(TermId, usize)>,
    /// Constructor shapes `(name, arity, raw)`, keyed as `Lowered::constructors`.
    pub shapes: BTreeSet<(String, usize, bool)>,
    /// Rows reach the program only as its own seeds and its served relations' answers (a comptime
    /// round: the next round's frozen rows are that round's seeds). A sealed program without a
    /// served relation gives no seed source to a relation a rule heads and no seed names. Unsealed,
    /// every relation keeps one: runtime frontiers and `IrSession::apply` may send rows to any.
    pub sealed: bool,
}

/// One LetRec for every wave relation, one round per wave, `limit` the wave limit. Its
/// variables are `__ir_wave_started`, empty in round 1 and one row after, and each wave
/// relation `R`, whose body is `R`'s seed rows while nothing has started and the union of
/// its `next` rule bodies after. A cone relation a `next` body reads is inlined as its own
/// value over the same round's variables; a recursive cone component is a LetRec nested in
/// the wave LetRec, over fresh relations, so it runs to its fixed point inside every wave.
#[allow(clippy::too_many_arguments)]
fn lower_waves(
    b: &mut Builder,
    waves: &Waves,
    relations: &BTreeMap<RelationKey, ir::RelId>,
    sources: &BTreeMap<RelationKey, ir::RelId>,
    bodies: &BTreeMap<RelationKey, Vec<ir::NodeId>>,
    next_bodies: &mut BTreeMap<RelationKey, Vec<ir::NodeId>>,
    unit: ir::RelId,
    none: TermId,
) -> Result<(), &'static str> {
    let started = b.rel("__ir_wave_started".into(), vec![ir::Ty::Id], ir::RelKind::Derived);
    let started_body = ground_row(b, unit, &[none])?;
    let cone: HashSet<ir::RelId> = waves.cone.iter().flat_map(|(members, _)| members).map(|key| relations[key]).collect();
    let mut values: HashMap<ir::RelId, ir::NodeId> = HashMap::new();
    let mut memo: HashMap<ir::NodeId, ir::NodeId> = HashMap::new();
    let mut nested = Vec::new();
    for (members, recursive) in &waves.cone {
        if !recursive {
            let key = &members[0];
            let mut parts = vec![b.get(sources[key])];
            for body in bodies.get(key).into_iter().flatten() {
                parts.push(substitute(b, *body, &values, &cone, &mut memo)?);
            }
            let union = b.node(ir::Op::Union(parts));
            let value = b.node(ir::Op::Threshold(union));
            values.insert(relations[key], value);
            continue;
        }
        let mut ids = Vec::with_capacity(members.len());
        for key in members {
            let relation = relations[key];
            let cols = b.ir.rels.iter().find(|rel| rel.id == relation).map(|rel| rel.cols.clone()).unwrap_or_default();
            let id = b.rel(format!("__ir_wave_rec_{relation}"), cols, ir::RelKind::Derived);
            let get = b.get(id);
            values.insert(relation, get);
            ids.push(id);
        }
        let mut rec_bodies = Vec::with_capacity(members.len());
        for key in members {
            let mut parts = vec![b.get(sources[key])];
            for body in bodies.get(key).into_iter().flatten() {
                parts.push(substitute(b, *body, &values, &cone, &mut memo)?);
            }
            rec_bodies.push(b.node(ir::Op::Union(parts)));
        }
        nested.push(ir::LetRec { ids, bodies: rec_bodies, limit: None, nested: Vec::new() });
    }
    let started_rows = b.get(started);
    let mut ids = vec![started];
    let mut rec_bodies = vec![started_body];
    for key in &waves.wave {
        let width = key.1.max(1);
        let seeds = b.get(sources[key]);
        let first = b.node(ir::Op::Antijoin { l: seeds, r: started_rows, lk: vec![], rk: vec![] });
        let mut parts = Vec::new();
        for body in next_bodies.remove(key).unwrap_or_default() {
            parts.push(substitute(b, body, &values, &cone, &mut memo)?);
        }
        let next = match parts[..] {
            [one] => one,
            [] => return Err("wave relation without a next rule"),
            _ => b.node(ir::Op::Union(parts)),
        };
        let joined = b.node(ir::Op::Join { inputs: vec![next, started_rows], equivalences: vec![], project: vec![] });
        let later = b.mfp(joined, vec![], vec![], (0..width).map(assert_width).collect());
        ids.push(relations[key]);
        rec_bodies.push(b.node(ir::Op::Union(vec![first, later])));
    }
    b.ir.strata.push(ir::Stratum::LetRec(ir::LetRec {
        ids,
        bodies: rec_bodies,
        limit: Some(WAVE_LIMIT),
        nested,
    }));
    Ok(())
}

pub fn lower(u: &mut Universe, program: &Program) -> Result<Lowered, Diagnostic> {
    lower_declared(u, program, &Declared::default())
}

/// `lower` with a source per declared relation and a constructor per declared shape.
pub fn lower_declared(
    u: &mut Universe,
    program: &Program,
    declared: &Declared,
) -> Result<Lowered, Diagnostic> {
    let mut program = program.clone();
    let intern_rel = kernel_ref(u, "intern");
    let effect_rel = kernel_ref(u, "effect");
    let snapshot_rel = kernel_ref(u, "intern_snapshot");
    let none_atom = u.atom("none");
    let none = u.compound("const", vec![none_atom]);
    let empty = u.empty_list();
    let tuple = u.atom("tuple");
    let (strata, diagnostics) = stratify(u, &program);
    if let Some(diag) = diagnostics.into_iter().next() {
        return Err(diag);
    }
    let (program, hidden) = with_demand_in_strata(u, &program, &strata);
    let sum_readbacks = sum_readback_constructors(u, &program, snapshot_rel);
    let (strata, diagnostics) = stratify(u, &program);
    if let Some(diag) = diagnostics.into_iter().next() {
        return Err(diag);
    }
    // Term indices reachable from the program, marked in `seen`; sorted before each read.
    let mut needed: Vec<u32> = Vec::new();
    let mut seen = vec![false; u.terms.len()];
    let mut todo = vec![none, empty, tuple, intern_rel, effect_rel, snapshot_rel];
    todo.extend(program.served.iter().copied());
    for seed in &program.seeds {
        todo.push(seed.rel);
        todo.extend(seed.args.iter().copied());
    }
    todo.extend(declared.sources.iter().map(|(rel, _)| *rel));
    for rule in &program.rules {
        todo.push(rule.rel);
        for arg in &rule.head {
            collect_arg(arg, &mut todo);
        }
        for goal in &rule.body {
            todo.push(goal.rel);
            for arg in &goal.args {
                collect_arg(arg, &mut todo);
            }
        }
    }
    while let Some(term) = todo.pop() {
        let index = term.0 as usize;
        if index >= seen.len() {
            seen.resize(index + 1, false);
        }
        if !seen[index] {
            seen[index] = true;
            needed.push(term.0);
            if let Term::Compound(_, args) = u.get(term) {
                todo.extend(args.iter().copied());
            }
        }
    }
    needed.sort_unstable();
    let dynamic_lists = program.rules.iter().any(|rule| {
        rule.body.iter().any(|goal| {
            Kernel::of(u, goal.rel) == Some(Kernel::Intern)
                && matches!(goal.args.get(1), Some(Arg::Var(_)))
        })
    });
    let possible_lists: Vec<TermId> = needed.iter().copied().map(TermId).collect();
    let mut plain_list_pairs = Vec::new();
    for tagged in possible_lists {
        let Some(list) = u.unary(tagged, "const").and_then(|id| u.as_list(id)) else {
            continue;
        };
        let Some(plain) = list
            .iter()
            .map(|id| super::kernel::semantic(u, *id))
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        let plain = u.list(&plain);
        plain_list_pairs.push((tagged, plain));
    }
    todo.extend(plain_list_pairs.iter().map(|(_, plain)| *plain));
    while let Some(term) = todo.pop() {
        let index = term.0 as usize;
        if index >= seen.len() {
            seen.resize(index + 1, false);
        }
        if !seen[index] {
            seen[index] = true;
            needed.push(term.0);
            if let Term::Compound(_, args) = u.get(term) {
                todo.extend(args.iter().copied());
            }
        }
    }
    needed.sort_unstable();
    let bad_list_terms: Vec<TermId> = needed
        .iter()
        .copied()
        .map(TermId)
        .filter(|&id| {
            u.unary(id, "const")
                .is_some_and(|payload| u.as_list(payload).is_none())
        })
        .collect();
    let mut b = Builder::new();
    let unit = b.rel("__ir_unit".into(), vec![ir::Ty::Id], ir::RelKind::Source);
    let term_map = b.rel(
        "__ir_term_map".into(),
        vec![ir::Ty::Int, ir::Ty::Id],
        ir::RelKind::Source,
    );
    let plain_source = b.rel(
        "__ir_plain_list_seeds".into(),
        vec![ir::Ty::Id, ir::Ty::Id],
        ir::RelKind::Source,
    );
    let bad_lists = b.rel(
        "__ir_bad_lists".into(),
        vec![ir::Ty::Id],
        ir::RelKind::Source,
    );
    let plain_lists = b.rel(
        "__ir_plain_lists".into(),
        vec![ir::Ty::Id, ir::Ty::Id],
        ir::RelKind::Derived,
    );
    let mut constructor_keys = BTreeSet::new();
    let mut shapes = Vec::new();
    let mut texts = Vec::new();
    let mut atom_raws = HashMap::new();
    // Universe term IDs are stable for its lifetime. A sorted symbol rank
    // shifts when an answer introduces a name between two existing names.
    let dynamic_atom_base = 1;
    let existing_terms: Vec<_> = needed
        .iter()
        .map(|index| (*index as usize, u.terms[*index as usize].clone()))
        .collect();
    for (index, term) in &existing_terms {
        let (name, args, raw) = match term {
            Term::Int(n) => ("0:int".to_string(), vec![], Some(*n)),
            Term::Float(f) => ("0:float".to_string(), vec![], Some(f.0.to_bits() as i64)),
            Term::Bool(value) => ("1:bool".to_string(), vec![], Some(i64::from(*value))),
            Term::Str(sym) => {
                // `ivm_ir::compare` puts bare text before every constructor;
                // `2:str` sorts strings after bools and before atoms, as Rust does.
                texts.push((TermId(*index as u32), u.sym_str(*sym).to_owned()));
                shapes.push((
                    TermId(*index as u32),
                    "2:str".to_string(),
                    vec![],
                    None,
                    Some(texts.len() - 1),
                ));
                continue;
            }
            Term::Atom(sym) => (
                "3:atom".to_string(),
                vec![],
                Some(if *sym == u.nil { 0 } else { *index as i64 + 1 }),
            ),
            Term::Compound(sym, children) => (
                format!("9:{}:{}", children.len(), u.sym_str(*sym)),
                children.clone(),
                None,
            ),
        };
        constructor_keys.insert((name.clone(), args.len(), raw.is_some()));
        if matches!(term, Term::Atom(_)) {
            atom_raws.insert(TermId(*index as u32), raw.unwrap());
        }
        shapes.push((TermId(*index as u32), name, args, raw, None));
    }
    for (name, args, raw) in [
        ("0:int", 0, true),
        ("0:float", 0, true),
        ("1:bool", 0, true),
        ("2:str", 1, false),
        ("3:atom", 0, true),
        ("9:1:const", 1, false),
        ("9:1:primitive", 1, false),
        ("9:1:ref", 1, false),
        ("9:2:[|]", 2, false),
        ("9:2:application", 2, false),
        ("9:2:edge", 2, false),
    ] {
        constructor_keys.insert((name.to_string(), args, raw));
    }
    constructor_keys.extend(declared.shapes.iter().cloned());
    // Terms are interned through `Engine::intern_terms` (`bootstrap_changes`), never by an
    // operator: a constructor is a relation no node needs to mint into at install.
    let mut constructors = BTreeMap::new();
    for (name, width, raw) in constructor_keys {
        let column = if name == "0:float" {
            ir::Ty::Real
        } else if raw {
            ir::Ty::Int
        } else {
            ir::Ty::Id
        };
        let mut ctor_cols = vec![ir::Ty::Id];
        ctor_cols.extend((0..width.max(usize::from(raw))).map(|_| column));
        let ctor = b.rel(name.clone(), ctor_cols, ir::RelKind::Constructor);
        constructors.insert((name, width, raw), ctor);
    }
    let terms: Vec<TermSpec> = shapes
        .into_iter()
        .map(|(original, name, args, raw, text)| {
            let width = if text.is_some() { 1 } else { args.len() };
            let ctor = constructors[&(name, width, raw.is_some())];
            TermSpec {
                original,
                functor: ctor,
                args,
                raw,
                text,
            }
        })
        .collect();
    b.term_specs = terms.iter().map(|spec: &TermSpec| (spec.original, spec.clone())).collect();
    b.spec_texts = texts.iter().map(|(_, text)| text.clone()).collect();
    let semantic = b.rel(
        "__ir_semantic".into(),
        vec![ir::Ty::Id, ir::Ty::Id],
        ir::RelKind::Derived,
    );
    let semantic_stratum = b.ir.strata.len();
    b.semantic = Some((
        semantic,
        [
            constructors[&("9:1:ref".to_string(), 1, false)],
            constructors[&("9:1:const".to_string(), 1, false)],
        ],
    ));
    if !dynamic_lists && program.rules.iter().any(|rule| {
        rule.body
            .iter()
            .any(|goal| Kernel::of(u, goal.rel) == Some(Kernel::Intern))
    }) {
        let seed_lists = b.get(plain_source);
        b.ir.strata.push(ir::Stratum::Let {
            id: plain_lists,
            body: seed_lists,
        });
    }
    let mut keys = BTreeSet::new();
    for seed in &program.seeds {
        keys.insert((seed.rel.0, seed.args.len()));
    }
    for (rel, width) in &declared.sources {
        keys.insert((rel.0, *width));
    }
    for rule in &program.rules {
        keys.insert(rule_key(u, rule).0);
        for goal in &rule.body {
            if Kernel::of(u, goal.rel).is_none()
                || (rule.is_aggregate() && goal.polarity == Polarity::Positive)
            {
                keys.insert((goal.rel.0, goal.args.len()));
            }
        }
    }
    let waves = waves(u, &program.rules, &keys).map_err(|name| unsupported(u, name))?;
    // In a sealed program without a served relation (`Declared::sealed`), a relation gets a seed
    // source unless an ordinary rule heads it and no row can reach it from outside: none of the
    // program's seeds, the caller's declared sources, the snapshot rows a later comptime round
    // freezes (`edge_snapshot`, `intern_snapshot`), or a wave or wave-cone relation's seeds.
    // Executor answers may write any relation, so a served relation keeps every source.
    let headed: BTreeSet<RelationKey> = program
        .rules
        .iter()
        .map(|rule| rule_key(u, rule))
        .filter(|(_, next)| !next)
        .map(|(key, _)| key)
        .collect();
    let mut external: BTreeSet<RelationKey> = program
        .seeds
        .iter()
        .map(|seed| (seed.rel.0, seed.args.len()))
        .chain(declared.sources.iter().map(|(rel, width)| (rel.0, *width)))
        .collect();
    if let Some(waves) = &waves {
        external.extend(waves.wave.iter().copied());
        external.extend(waves.cone.iter().flat_map(|(members, _)| members.iter().copied()));
    }
    let frozen = [kernel_ref(u, "edge_snapshot"), snapshot_rel];
    let open = !declared.sealed || !program.served.is_empty();
    let seeded = |key: &RelationKey| {
        open
            || !headed.contains(key)
            || external.contains(key)
            || program.served.contains(&TermId(key.0))
            || frozen.contains(&TermId(key.0))
    };
    // Relations whose one rule derives each row once: no aggregate, a head of variables only,
    // every variable of its positive relation goals in the head (no projection drops one), and no
    // `intern` goal (its listed and vector branches are a Union). Inputs are sets and the other
    // kernels are functions of their inputs, so the rule body holds no row twice.
    let mut rule_counts: BTreeMap<RelationKey, usize> = BTreeMap::new();
    for rule in &program.rules {
        *rule_counts.entry(rule_key(u, rule).0).or_default() += 1;
    }
    let distinct: BTreeSet<RelationKey> = program
        .rules
        .iter()
        .filter(|rule| {
            let (key, next) = rule_key(u, rule);
            if next || rule_counts[&key] != 1 || rule.is_aggregate() || rule.head.is_empty() {
                return false;
            }
            let Some(head) = rule.head.iter().map(var).collect::<Option<HashSet<u32>>>() else {
                return false;
            };
            rule.body.iter().all(|goal| match Kernel::of(u, goal.rel) {
                Some(kernel) => kernel != Kernel::Intern,
                None => {
                    goal.polarity == Polarity::Negative
                        || goal.args.iter().all(|arg| var(arg).is_none_or(|v| head.contains(&v)))
                }
            })
        })
        .map(|rule| rule_key(u, rule).0)
        .collect();
    let mut relations = BTreeMap::new();
    let mut sources = BTreeMap::new();
    let mut visible = Vec::new();
    for (rel, width) in keys.iter().copied() {
        let cols = vec![ir::Ty::Id; width.max(1)];
        if seeded(&(rel, width)) {
            let source = b.rel(
                if program.served.contains(&TermId(rel)) {
                    format!("{}.answer", u.display(TermId(rel)))
                } else {
                    format!("__ir_seed_n{rel}_a{width}")
                },
                cols.clone(),
                ir::RelKind::Source,
            );
            sources.insert((rel, width), source);
        }
        let output = b.rel(format!("__ir_n{rel}_a{width}"), cols, ir::RelKind::Derived);
        relations.insert((rel, width), output);
        if !hidden.contains(&TermId(rel)) {
            visible.push((output, TermId(rel), width));
            b.ir.outputs.push(output);
        }
    }
    // A readback in a lower stratum must not wait for requests produced by
    // higher strata. Each request relation carries prior requests forward.
    let request_rels: Vec<_> = (0..=strata.max_level)
        .map(|level| {
            b.rel(
                format!("__ir_intern_requests_{level}"),
                vec![ir::Ty::Id; 3],
                ir::RelKind::Derived,
            )
        })
        .collect();
    let mut bodies: BTreeMap<(u32, usize), Vec<ir::NodeId>> = BTreeMap::new();
    let mut requests_by_level: BTreeMap<u32, Vec<ir::NodeId>> = BTreeMap::new();
    let mut request_dependencies: BTreeMap<u32, BTreeSet<RelationKey>> = BTreeMap::new();
    let mut readback_heads: BTreeMap<u32, BTreeSet<RelationKey>> = BTreeMap::new();
    let mut effect_dependencies = BTreeSet::new();
    let mut snapshot_dependencies = BTreeSet::new();
    let mut fold_auxiliaries: BTreeMap<RelationKey, Vec<FoldAux>> = BTreeMap::new();
    let mut next_bodies: BTreeMap<RelationKey, Vec<ir::NodeId>> = BTreeMap::new();
    let mut wave_constructs = false;
    // Recording pass: the same rules into a scratch builder, collecting each relation's
    // narrowing union per join key (`Builder::narrow_unions`). Errors surface in the real pass.
    {
        let mut scratch = b.clone();
        scratch.narrow_record = true;
        for rule in &program.rules {
            scratch.intern_relation = Some(request_rels[strata.level(rule.rel) as usize]);
            let _ = compile_rule(
                &mut scratch,
                u,
                rule,
                &relations,
                &constructors,
                semantic,
                plain_lists,
                unit,
                &program.served,
                none,
                empty,
                tuple,
                &program.rules,
            );
        }
        b.narrow_unions = scratch.narrow_unions;
    }
    for rule in &program.rules {
        let level = strata.level(rule.rel);
        b.intern_relation = Some(request_rels[level as usize]);
        if rule.is_aggregate() && rule.aggregate_args() != 1 {
            let count = u.int(rule.aggregate_args() as i64);
            return Err(Diagnostic {
                phase: "evaluate",
                payload: u.compound("malformed_aggregate_head", vec![count]),
            });
        }
        let requests_before = b.intern_nodes.len();
        let readbacks_before = b.intern_readback_count;
        let body = compile_rule(
            &mut b,
            u,
            rule,
            &relations,
            &constructors,
            semantic,
            plain_lists,
            unit,
            &program.served,
            none,
            empty,
            tuple,
            &program.rules,
        )
        .map_err(|name| unsupported(u, name))?;
        let constructs = b.intern_nodes.len() > requests_before;
        let reads_back = b.intern_readback_count > readbacks_before;
        let constructs_readback = constructs
            && rule.body.iter().any(|goal| {
                Kernel::of(u, goal.rel) == Some(Kernel::Intern)
                    && matches!(goal.args.first(), Some(Arg::Ground(ctor)) if sum_readbacks.contains(ctor))
            });
        if constructs {
            if constructs_readback {
                for request in b.intern_nodes[requests_before..].to_vec() {
                    for constructor in &sum_readbacks {
                        let joined = join_ground(&mut b, request, Some(0), *constructor)
                            .map_err(|name| unsupported(u, name))?;
                        let snapshot = b.mfp(joined, vec![], vec![], vec![0, 1, 2]);
                        b.snapshot_nodes.push(snapshot);
                        for goal in &rule.body {
                            let key = (goal.rel.0, goal.args.len());
                            if keys.contains(&key) {
                                snapshot_dependencies.insert(key);
                            }
                        }
                    }
                }
            }
            requests_by_level
                .entry(level)
                .or_default()
                .extend(b.intern_nodes.drain(requests_before..));
            for goal in &rule.body {
                let key = (goal.rel.0, goal.args.len());
                if keys.contains(&key) {
                    request_dependencies.entry(level).or_default().insert(key);
                }
            }
        }
        if b.intern_readback_count > readbacks_before {
            readback_heads
                .entry(level)
                .or_default()
                .insert((rule.rel.0, rule.head.len()));
        }
        for (index, goal) in rule.body.iter().enumerate() {
            if !program.served.contains(&goal.rel) {
                continue;
            }
            for prior in &rule.body[..index] {
                let key = (prior.rel.0, prior.args.len());
                if keys.contains(&key) {
                    effect_dependencies.insert(key);
                }
            }
        }
        let (head_key, next) = rule_key(u, rule);
        if let Some(waves) = &waves {
            if next || waves.cone.iter().any(|(members, _)| members.contains(&head_key)) {
                if reads_back {
                    return Err(unsupported(u, "intern readback in a wave rule"));
                }
                if constructs_readback {
                    return Err(unsupported(u, "intern snapshot in a wave rule"));
                }
                wave_constructs |= constructs;
                if body.fold_aux.is_some() {
                    return Err(unsupported(u, "declared fold in a wave rule"));
                }
                if rule.body.iter().any(|goal| program.served.contains(&goal.rel)) {
                    return Err(unsupported(u, "served relation in a wave rule"));
                }
            }
        }
        if next {
            next_bodies.entry(head_key).or_default().push(body.body);
            continue;
        }
        if let Some(aux) = body.fold_aux {
            fold_auxiliaries.entry(head_key).or_default().push(aux);
        }
        bodies.entry(head_key).or_default().push(body.body);
    }
    // `plain_inner` maps an untagged list cell to its plain twin. The
    // recursion walks cons cells, so a list built at run time needs no
    // `const` wrapper on any of its tails. It reads `9:2:[|]` by tail and
    // `9:1:const` by argument: a constructor row enters only after the epoch
    // or pass that minted it, so each settle that mints a list or a plain
    // twin runs one more epoch. Built only when a rule reads `plain_lists`.
    if dynamic_lists
        && b.ir
            .nodes
            .iter()
            .any(|node| matches!(node, ir::Op::Get(id) if *id == plain_lists))
    {
        let plain_inner = b.rel(
            "__ir_plain_inner".into(),
            vec![ir::Ty::Id, ir::Ty::Id],
            ir::RelKind::Derived,
        );
        let wrap = constructors[&("9:1:const".to_string(), 1, false)];
        let cons = constructors[&("9:2:[|]".to_string(), 2, false)];
        let seed_lists = b.get(plain_source);
        let seeded = join_constructor(&mut b, seed_lists, 2, wrap, 0);
        let seeded = b.mfp(seeded, vec![], vec![], vec![3, 1]);
        let empty_cell = ground_row(&mut b, unit, &[empty]).map_err(|name| unsupported(u, name))?;
        let empty_pair = b.mfp(empty_cell, vec![], vec![], vec![0, 0]);
        let mut pairs = b.get(plain_inner);
        let cons_rows = b.get_constructor(cons);
        pairs = b.node(ir::Op::Join {
            inputs: vec![pairs, cons_rows],
            equivalences: vec![vec![(0, 0), (1, 2)]],
            project: vec![],
        });
        let cell = 2;
        let head = 3;
        pairs = join_constructor(&mut b, pairs, 5, semantic, head);
        let plain_head = 6;
        pairs = b.node(ir::Op::Mint {
            input: pairs,
            functor: cons,
            args: vec![plain_head, 1],
        });
        pairs = b.mfp(pairs, vec![], vec![], vec![cell, 7]);
        let inner_body = b.node(ir::Op::Union(vec![seeded, empty_pair, pairs]));
        let seed_lists = b.get(plain_source);
        let mut wrapped = b.get(plain_inner);
        let wraps = b.get_constructor(wrap);
        wrapped = b.node(ir::Op::Join {
            inputs: vec![wrapped, wraps],
            equivalences: vec![vec![(0, 0), (1, 1)]],
            project: vec![],
        });
        wrapped = b.mfp(wrapped, vec![], vec![], vec![2, 1]);
        let plain_body = b.node(ir::Op::Union(vec![seed_lists, wrapped]));
        b.ir.strata.insert(
            semantic_stratum,
            ir::Stratum::LetRec(ir::LetRec {
                ids: vec![plain_inner, plain_lists],
                bodies: vec![inner_body, plain_body],
                limit: None,
                nested: Vec::new(),
            }),
        );
    }
    if b.ir
        .nodes
        .iter()
        .any(|node| matches!(node, ir::Op::Get(id) if *id == semantic))
    {
        let refs = b.get_constructor(constructors[&("9:1:ref".to_string(), 1, false)]);
        let consts = b.get_constructor(constructors[&("9:1:const".to_string(), 1, false)]);
        let body = b.node(ir::Op::Union(vec![refs, consts]));
        b.ir.strata
            .insert(semantic_stratum, ir::Stratum::Let { id: semantic, body });
    }
    if b.intern_readback {
        for (level, request_rel) in request_rels.iter().enumerate() {
            let request_key = (u32::MAX - level as u32, 3);
            keys.insert(request_key);
            let source = b.rel(
                format!("__ir_intern_request_seed_{level}"),
                vec![ir::Ty::Id; 3],
                ir::RelKind::Source,
            );
            sources.insert(request_key, source);
            relations.insert(request_key, *request_rel);
            bodies.insert(
                request_key,
                requests_by_level
                    .remove(&(level as u32))
                    .unwrap_or_default(),
            );
        }
        let last = *request_rels.last().unwrap();
        b.ir.outputs.push(last);
        visible.push((last, intern_rel, 3));
    }
    // A readback anywhere waits on per-level request relations a wave body cannot feed.
    if wave_constructs && b.intern_readback {
        return Err(unsupported(u, "intern construction in a wave rule beside an intern readback"));
    }
    let effect_key = (effect_rel.0, 2);
    let internal_effect = relations.get(&effect_key).copied();
    if internal_effect.is_some() {
        bodies
            .entry(effect_key)
            .or_default()
            .extend(b.effect_nodes.values().flatten().copied());
    }
    let snapshot_key = (snapshot_rel.0, 3);
    let internal_snapshot = relations.get(&snapshot_key).copied();
    if internal_snapshot.is_some() {
        bodies
            .entry(snapshot_key)
            .or_default()
            .extend(b.snapshot_nodes.iter().copied());
    }
    let mut pending: BTreeSet<_> = keys.clone();
    let mut edges: BTreeMap<RelationKey, BTreeSet<RelationKey>> = BTreeMap::new();
    let mut reverse: BTreeMap<RelationKey, BTreeSet<RelationKey>> = BTreeMap::new();
    for rule in &program.rules {
        let head = (rule.rel.0, rule.head.len());
        for goal in &rule.body {
            let dependency = (goal.rel.0, goal.args.len());
            if keys.contains(&dependency) {
                edges.entry(head).or_default().insert(dependency);
                reverse.entry(dependency).or_default().insert(head);
            }
        }
        if let Some((_, Folding::Declared(fold), _)) = rule.folding_head() {
            let step = (fold.step.0, 3);
            if keys.contains(&step) {
                edges.entry(head).or_default().insert(step);
                reverse.entry(step).or_default().insert(head);
            }
            for step_rule in program
                .rules
                .iter()
                .filter(|candidate| candidate.rel == fold.step && candidate.head.len() == 3)
            {
                for goal in &step_rule.body {
                    let dependency = (goal.rel.0, goal.args.len());
                    if keys.contains(&dependency) {
                        edges.entry(head).or_default().insert(dependency);
                        reverse.entry(dependency).or_default().insert(head);
                    }
                }
            }
        }
    }
    if internal_effect.is_some() || internal_snapshot.is_some() {
        for dependency in effect_dependencies {
            if internal_effect.is_some() {
                edges.entry(effect_key).or_default().insert(dependency);
                reverse.entry(dependency).or_default().insert(effect_key);
            }
            if internal_snapshot.is_some() {
                edges.entry(snapshot_key).or_default().insert(dependency);
                reverse.entry(dependency).or_default().insert(snapshot_key);
            }
        }
        if internal_snapshot.is_some() {
            for dependency in snapshot_dependencies {
                edges.entry(snapshot_key).or_default().insert(dependency);
                reverse.entry(dependency).or_default().insert(snapshot_key);
            }
        }
    }
    if b.intern_readback {
        for level in 0..=strata.max_level {
            let request_key = (u32::MAX - level, 3);
            for dependency in request_dependencies.remove(&level).unwrap_or_default() {
                edges.entry(request_key).or_default().insert(dependency);
                reverse.entry(dependency).or_default().insert(request_key);
            }
            for head in readback_heads.remove(&level).unwrap_or_default() {
                edges.entry(head).or_default().insert(request_key);
                reverse.entry(request_key).or_default().insert(head);
            }
            if level > 0 {
                let prior = (u32::MAX - (level - 1), 3);
                let prior_rows = b.get(relations[&prior]);
                bodies.entry(request_key).or_default().push(prior_rows);
                edges.entry(request_key).or_default().insert(prior);
                reverse.entry(prior).or_default().insert(request_key);
            }
        }
    }
    // The wave relations lower once every relation their rules read outside the wave is
    // lowered; until then no wave relation is ready or a component.
    let mut wave_pending: BTreeSet<RelationKey> = waves.as_ref().map(|w| w.wave.clone()).unwrap_or_default();
    for _ in 0..COMPONENT_BUDGET {
        if pending.is_empty() {
            break;
        }
        if let Some(w) = waves.as_ref().filter(|w| !wave_pending.is_empty() && w.reads.iter().all(|key| !pending.contains(key))) {
            lower_waves(&mut b, w, &relations, &sources, &bodies, &mut next_bodies, unit, none)
                .map_err(|name| unsupported(u, name))?;
            for key in std::mem::take(&mut wave_pending) {
                pending.remove(&key);
            }
            continue;
        }
        let ready: Vec<_> = pending
            .iter()
            .copied()
            .filter(|key| !wave_pending.contains(key))
            .filter(|key| {
                edges
                    .get(key)
                    .into_iter()
                    .flatten()
                    .all(|dependency| !pending.contains(dependency))
            })
            .collect();
        if ready.is_empty() {
            let mut component = None;
            for &candidate in pending.iter().filter(|key| !wave_pending.contains(key)) {
                let forward =
                    reachable(candidate, &pending, &edges).map_err(|name| unsupported(u, name))?;
                let backward = reachable(candidate, &pending, &reverse)
                    .map_err(|name| unsupported(u, name))?;
                let group: BTreeSet<_> = forward.intersection(&backward).copied().collect();
                let external = group.iter().any(|key| {
                    edges
                        .get(key)
                        .into_iter()
                        .flatten()
                        .any(|dep| pending.contains(dep) && !group.contains(dep))
                });
                if !external {
                    component = Some(group);
                    break;
                }
            }
            let component = component.ok_or_else(|| unsupported(u, "component ordering"))?;
            let ids: Vec<_> = component.iter().map(|key| relations[key]).collect();
            let mut rec_bodies = Vec::new();
            for key in &component {
                let mut parts: Vec<ir::NodeId> = sources.get(key).map(|source| b.get(*source)).into_iter().collect();
                parts.extend(bodies.remove(key).unwrap_or_default());
                let body = match parts[..] {
                    [] => return Err(unsupported(u, "relation without a seed source or a rule body")),
                    [one] => one,
                    _ => b.node(ir::Op::Union(parts)),
                };
                rec_bodies.push(body);
            }
            b.ir.strata.push(ir::Stratum::LetRec(ir::LetRec {
                ids,
                bodies: rec_bodies,
                limit: None,
                nested: Vec::new(),
            }));
            for key in component {
                pending.remove(&key);
            }
            continue;
        }
        for key in ready {
            if let Some(auxiliaries) = fold_auxiliaries.remove(&key) {
                for aux in auxiliaries {
                    let ranked_body = b.node(ir::Op::Threshold(aux.ranked_body));
                    b.ir.strata.push(ir::Stratum::Let {
                        id: aux.ranked,
                        body: ranked_body,
                    });
                    b.ir.strata.push(ir::Stratum::LetRec(ir::LetRec {
                        ids: vec![aux.accumulator],
                        bodies: vec![aux.accumulator_body],
                        limit: None,
                        nested: Vec::new(),
                    }));
                }
            }
            let source = sources.get(&key).map(|source| b.get(*source));
            let mut parts: Vec<ir::NodeId> = source.into_iter().collect();
            parts.extend(bodies.remove(&key).unwrap_or_default());
            // A source alone is distinct: both hosts reject duplicate inserts
            // and emit one row per live source key. No rule adds another row.
            let body = match parts[..] {
                [] => return Err(unsupported(u, "relation without a seed source or a rule body")),
                [one] if source.is_some() || distinct.contains(&key) => one,
                [one] => b.node(ir::Op::Threshold(one)),
                _ => {
                    let union = b.node(ir::Op::Union(parts));
                    b.node(ir::Op::Threshold(union))
                }
            };
            b.ir.strata.push(ir::Stratum::Let {
                id: relations[&key],
                body,
            });
            pending.remove(&key);
        }
    }
    if !pending.is_empty() {
        return Err(unsupported(u, "component budget"));
    }
    if !b.intern_readback {
        let requests: Vec<_> = requests_by_level.into_values().flatten().collect();
        let has_requests = !requests.is_empty();
        if has_requests {
            let body = {
                let union = b.node(ir::Op::Union(requests));
                b.node(ir::Op::Threshold(union))
            };
            b.ir.strata.push(ir::Stratum::Let {
                id: request_rels[0],
                body,
            });
            b.ir.outputs.push(request_rels[0]);
            visible.push((request_rels[0], intern_rel, 3));
        }
    }
    let mut served = Vec::new();
    for (relation, nodes) in std::mem::take(&mut b.effect_nodes) {
        let relation = TermId(relation);
        let request = b.rel(
            format!("{}.request", u.display(relation)),
            vec![ir::Ty::Id; 2],
            ir::RelKind::Derived,
        );
        let body = if let Some(effect) = internal_effect {
            let effects = b.get(effect);
            let joined = join_ground(&mut b, effects, Some(0), relation).map_err(|name| unsupported(u, name))?;
            b.mfp(joined, vec![], vec![], vec![0, 1])
        } else {
            b.node(ir::Op::Union(nodes))
        };
        let body = b.node(ir::Op::Threshold(body));
        b.ir.strata.push(ir::Stratum::Let { id: request, body });
        b.ir.outputs.push(request);
        visible.push((request, effect_rel, 2));
        let arity = keys
            .iter()
            .find_map(|(rel, arity)| (*rel == relation.0).then_some(*arity))
            .ok_or_else(|| unsupported(u, "served relation arity"))?;
        served.push(ServedSpec {
            relation,
            request,
            answer: sources[&(relation.0, arity)],
            arity,
        });
    }
    if !b.snapshot_nodes.is_empty() && internal_snapshot.is_none() {
        let snapshot_output = b.rel(
            "__ir_intern_snapshots".into(),
            vec![ir::Ty::Id; 3],
            ir::RelKind::Derived,
        );
        let nodes = std::mem::take(&mut b.snapshot_nodes);
        let body = b.node(ir::Op::Union(nodes));
        b.ir.strata.push(ir::Stratum::Let {
            id: snapshot_output,
            body,
        });
        b.ir.outputs.push(snapshot_output);
        visible.push((snapshot_output, snapshot_rel, 3));
    }
    fuse_projections(&mut b.ir);
    let fold_program = (!b.declared_folds.is_empty()).then(|| program.clone());
    let seeds = program
        .seeds
        .into_iter()
        .map(|row| (sources[&(row.rel.0, row.args.len())], row))
        .collect();
    Ok(Lowered {
        ir: b.ir,
        terms,
        texts,
        seeds,
        term_map,
        unit,
        visible,
        internal_seeds: plain_list_pairs
            .into_iter()
            .map(|(tagged, plain)| (plain_source, vec![tagged, plain]))
            .chain(
                bad_list_terms
                    .into_iter()
                    .map(|term| (bad_lists, vec![term])),
            )
            .collect(),
        plain_source,
        bad_lists,
        served,
        sources,
        constructors,
        atom_raws,
        dynamic_atom_base,
        fold_program,
        declared_folds: b.declared_folds,
        literals: b.literal_terms,
    })
}
