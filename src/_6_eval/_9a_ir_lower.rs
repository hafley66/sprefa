//! Checked DL8 goals lowered to the shared IVM operator algebra.
use super::kernel::{kernel_ref, Kernel};
use super::program::{Arg, Diagnostic, Polarity, Program, Row, Rule};
use super::stratify::stratify;
use super::term::{Term, TermId, Universe};
use ivm_ir as ir;
use std::collections::{BTreeMap, BTreeSet, HashMap};

const LOWER_SITE: &str = concat!(file!(), ":", line!());
const COMPONENT_BUDGET: usize = 4096;

#[derive(Clone)]
pub struct TermSpec {
    pub original: TermId,
    pub functor: ir::RelId,
    pub args: Vec<TermId>,
    pub raw: Option<ir::Cell>,
    pub source: ir::RelId,
    pub output: ir::RelId,
}

pub struct Lowered {
    pub ir: ir::Program,
    pub terms: Vec<TermSpec>,
    pub seeds: Vec<(ir::RelId, Row)>,
    pub term_map: ir::RelId,
    pub unit: ir::RelId,
    pub visible: Vec<(ir::RelId, TermId, usize)>,
}

struct Builder {
    ir: ir::Program,
    next_rel: ir::RelId,
}

impl Builder {
    fn new() -> Self {
        Self {
            ir: ir::Program {
                rels: Vec::new(),
                nodes: Vec::new(),
                strata: Vec::new(),
                outputs: Vec::new(),
            },
            next_rel: 0,
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
        let id = self.ir.nodes.len() as ir::NodeId;
        self.ir.nodes.push(op);
        id
    }
    fn get(&mut self, rel: ir::RelId) -> ir::NodeId {
        self.node(ir::Op::Get(rel))
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

fn unsupported(u: &mut Universe, name: &str) -> Diagnostic {
    let construct = u.atom(name);
    let site = u.string(LOWER_SITE);
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

fn mapped_ground(b: &mut Builder, map_rel: ir::RelId, original: TermId) -> ir::NodeId {
    let source = b.get(map_rel);
    b.mfp(
        source,
        vec![ir::Expr::Call(
            ir::Func::Eq,
            vec![ir::Expr::Col(0), ir::Expr::Lit(original.0 as i64)],
        )],
        vec![],
        vec![],
    )
}

fn compile_rule(
    b: &mut Builder,
    u: &Universe,
    rule: &Rule,
    relations: &BTreeMap<(u32, usize), ir::RelId>,
    term_map: ir::RelId,
    unit: ir::RelId,
) -> Result<ir::NodeId, &'static str> {
    if rule.is_aggregate() {
        return Err("aggregate");
    }
    let mut current = b.get(unit);
    let mut width = 1usize;
    let mut bound: HashMap<u32, ir::ColId> = HashMap::new();
    for goal in &rule.body {
        if let Some(kernel) = Kernel::of(u, goal.rel).filter(|kernel| *kernel != Kernel::Nil) {
            return Err(match kernel {
                Kernel::StrCons => "str.cons",
                Kernel::StrNil => "str.nil",
                Kernel::IntAdd => "int.add",
                Kernel::Int(_) => "int.comparison",
                Kernel::TermLt => "any.lt",
                Kernel::Nil => "nil",
                Kernel::Cons => "cons",
                Kernel::Intern => "intern",
                Kernel::EdgeRef => "edge_ref",
                Kernel::CountStep => "count_step",
                Kernel::MinStep => "min_step",
                Kernel::MaxStep => "max_step",
            });
        }
        let rel = *relations
            .get(&(goal.rel.0, goal.args.len()))
            .ok_or("unresolved relation")?;
        let right = b.get(rel);
        let mut equivalences = Vec::new();
        let mut first_here: HashMap<u32, ir::ColId> = HashMap::new();
        let mut grounds = Vec::new();
        for (position, arg) in goal.args.iter().enumerate() {
            let position = assert_width(position);
            if let Some(v) = var(arg) {
                if let Some(&left) = bound.get(&v) {
                    equivalences.push(vec![(0, left), (1, position)]);
                } else if let Some(&prior) = first_here.get(&v) {
                    equivalences.push(vec![(1, prior), (1, position)]);
                } else {
                    first_here.insert(v, position);
                }
            } else if let Some(id) = ground(arg) {
                grounds.push((position, id));
            } else {
                return Err("goal aggregate or fold");
            }
        }
        let mut inputs = vec![current, right];
        for (index, (position, original)) in grounds.iter().enumerate() {
            inputs.push(mapped_ground(b, term_map, *original));
            equivalences.push(vec![(1, *position), ((index + 2) as u8, 1)]);
        }
        if goal.polarity == Polarity::Negative {
            // Negative arguments must already be bound. A ground term has a
            // one-row map source and joins the right before the antijoin.
            if first_here.keys().any(|v| !bound.contains_key(v)) {
                return Ok(b.mfp(current, vec![ir::Expr::Lit(0)], vec![], vec![]));
            }
            let right_node = if grounds.is_empty() {
                right
            } else {
                let mut negative_inputs = vec![right];
                let mut negative_eq = Vec::new();
                for (index, (position, original)) in grounds.iter().enumerate() {
                    negative_inputs.push(mapped_ground(b, term_map, *original));
                    negative_eq.push(vec![(0, *position), ((index + 1) as u8, 1)]);
                }
                b.node(ir::Op::Join {
                    inputs: negative_inputs,
                    equivalences: negative_eq,
                })
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
            continue;
        }
        current = b.node(ir::Op::Join {
            inputs,
            equivalences,
        });
        let total = width + goal.args.len().max(1);
        if !grounds.is_empty() {
            current = b.mfp(
                current,
                vec![],
                vec![],
                (0..total).map(assert_width).collect(),
            );
        }
        for (v, position) in first_here {
            bound.insert(v, assert_width(width + position as usize));
        }
        width = total;
    }
    let mut head_cols = Vec::new();
    for arg in &rule.head {
        if let Some(v) = var(arg) {
            head_cols.push(*bound.get(&v).ok_or("unbound head variable")?);
        } else if let Some(id) = ground(arg) {
            let mapped = mapped_ground(b, term_map, id);
            current = b.node(ir::Op::Join {
                inputs: vec![current, mapped],
                equivalences: vec![],
            });
            head_cols.push(assert_width(width + 1));
            width += 2;
        } else {
            return Err("head aggregate or fold");
        }
    }
    if head_cols.is_empty() {
        head_cols.push(0);
    }
    Ok(b.mfp(current, vec![], vec![], head_cols))
}

pub fn lower(u: &mut Universe, program: &Program) -> Result<Lowered, Diagnostic> {
    let mut program = program.clone();
    let nil_rel = kernel_ref(u, "nil");
    let empty = u.empty_list();
    let nil = u.compound("const", vec![empty]);
    program.seeds.push(Row {
        rel: nil_rel,
        args: vec![nil],
    });
    let (strata, diagnostics) = stratify(u, &program);
    if let Some(diag) = diagnostics.into_iter().next() {
        return Err(diag);
    }
    let (program, hidden) = super::sqlite_eval::with_demand(u, &program);
    let mut b = Builder::new();
    let unit = b.rel("__ir_unit".into(), vec![ir::Ty::Id], ir::RelKind::Source);
    let term_map = b.rel(
        "__ir_term_map".into(),
        vec![ir::Ty::Int, ir::Ty::Id],
        ir::RelKind::Source,
    );
    let mut constructor_keys = BTreeSet::new();
    let mut shapes = Vec::new();
    let atoms: BTreeMap<String, i64> = {
        let mut names: Vec<_> = u.syms.iter().cloned().collect();
        names.sort();
        names
            .into_iter()
            .enumerate()
            .map(|(i, name)| (name, i as i64 + 1))
            .collect()
    };
    let existing_terms: Vec<_> = u.terms.iter().cloned().collect();
    for (index, term) in existing_terms.iter().enumerate() {
        let (name, args, raw) = match term {
            Term::Int(n) => ("0:int".to_string(), vec![], Some(*n)),
            Term::Bool(value) => ("1:bool".to_string(), vec![], Some(i64::from(*value))),
            Term::Str(sym) => ("2:str".to_string(), vec![], Some(atoms[u.sym_str(*sym)])),
            Term::Atom(sym) => (
                "3:atom".to_string(),
                vec![],
                Some(if *sym == u.nil {
                    0
                } else {
                    atoms[u.sym_str(*sym)]
                }),
            ),
            Term::Compound(sym, children) => {
                (format!("9:{}", u.sym_str(*sym)), children.clone(), None)
            }
            Term::Float(_) => return Err(unsupported(u, "float")),
        };
        constructor_keys.insert((name.clone(), args.len(), raw.is_some()));
        shapes.push((TermId(index as u32), name, args, raw));
    }
    for (name, args, raw) in [
        ("0:int", 0, true),
        ("3:atom", 0, true),
        ("9:const", 1, false),
        ("9:ref", 1, false),
        ("9:[|]", 2, false),
        ("9:application", 2, false),
        ("9:edge", 2, false),
    ] {
        constructor_keys.insert((name.to_string(), args, raw));
    }
    let mut constructors = BTreeMap::new();
    let mut prewarm = BTreeMap::new();
    for (name, width, raw) in constructor_keys {
        let mut ctor_cols = vec![ir::Ty::Id];
        ctor_cols.extend((0..width.max(usize::from(raw))).map(|_| {
            if raw {
                ir::Ty::Int
            } else {
                ir::Ty::Id
            }
        }));
        let ctor = b.rel(name.clone(), ctor_cols, ir::RelKind::Constructor);
        let mut source_cols = vec![ir::Ty::Int];
        source_cols.extend((0..width.max(usize::from(raw))).map(|_| {
            if raw {
                ir::Ty::Int
            } else {
                ir::Ty::Id
            }
        }));
        let source = b.rel(
            format!("__ir_prewarm_{}", ctor),
            source_cols.clone(),
            ir::RelKind::Source,
        );
        let output = b.rel(
            format!("__ir_prewarm_out_{}", ctor),
            {
                let mut cols = source_cols;
                cols.push(ir::Ty::Id);
                cols
            },
            ir::RelKind::Derived,
        );
        let get = b.get(source);
        let mint = b.node(ir::Op::Mint {
            input: get,
            functor: ctor,
            args: (1..=width.max(usize::from(raw)))
                .map(assert_width)
                .collect(),
        });
        b.ir.strata.push(ir::Stratum::Let {
            id: output,
            body: mint,
        });
        b.ir.outputs.push(output);
        constructors.insert((name, width, raw), ctor);
        prewarm.insert(ctor, (source, output));
    }
    let terms = shapes
        .into_iter()
        .map(|(original, name, args, raw)| {
            let ctor = constructors[&(name, args.len(), raw.is_some())];
            let (source, output) = prewarm[&ctor];
            TermSpec {
                original,
                functor: ctor,
                args,
                raw,
                source,
                output,
            }
        })
        .collect();
    let mut keys = BTreeSet::new();
    for seed in &program.seeds {
        keys.insert((seed.rel.0, seed.args.len()));
    }
    for rule in &program.rules {
        keys.insert((rule.rel.0, rule.head.len()));
        for goal in &rule.body {
            if Kernel::of(u, goal.rel).is_none_or(|kernel| kernel == Kernel::Nil) {
                keys.insert((goal.rel.0, goal.args.len()));
            }
        }
    }
    let mut relations = BTreeMap::new();
    let mut sources = BTreeMap::new();
    let mut visible = Vec::new();
    for (rel, width) in keys.iter().copied() {
        let cols = vec![ir::Ty::Id; width.max(1)];
        let source = b.rel(
            format!("__ir_seed_n{rel}_a{width}"),
            cols.clone(),
            ir::RelKind::Source,
        );
        let output = b.rel(format!("__ir_n{rel}_a{width}"), cols, ir::RelKind::Derived);
        sources.insert((rel, width), source);
        relations.insert((rel, width), output);
        if !hidden.contains(&TermId(rel)) {
            visible.push((output, TermId(rel), width));
            b.ir.outputs.push(output);
        }
    }
    let mut bodies: BTreeMap<(u32, usize), Vec<ir::NodeId>> = BTreeMap::new();
    for rule in &program.rules {
        let body = compile_rule(&mut b, u, rule, &relations, term_map, unit)
            .map_err(|name| unsupported(u, name))?;
        bodies
            .entry((rule.rel.0, rule.head.len()))
            .or_default()
            .push(body);
    }
    let mut pending: BTreeSet<_> = keys.clone();
    for _ in 0..COMPONENT_BUDGET {
        if pending.is_empty() {
            break;
        }
        let ready: Vec<_> = pending
            .iter()
            .copied()
            .filter(|key| {
                program
                    .rules
                    .iter()
                    .filter(|rule| (rule.rel.0, rule.head.len()) == *key)
                    .all(|rule| {
                        rule.body
                            .iter()
                            .all(|goal| !pending.contains(&(goal.rel.0, goal.args.len())))
                    })
            })
            .collect();
        if ready.is_empty() {
            let ids: Vec<_> = pending.iter().map(|key| relations[key]).collect();
            let mut rec_bodies = Vec::new();
            for key in &pending {
                let mut parts = vec![b.get(sources[key])];
                parts.extend(bodies.remove(key).unwrap_or_default());
                let union = b.node(ir::Op::Union(parts));
                rec_bodies.push(b.node(ir::Op::Threshold(union)));
            }
            b.ir.strata.push(ir::Stratum::LetRec(ir::LetRec {
                ids,
                bodies: rec_bodies,
                limit: Some(64),
            }));
            pending.clear();
            break;
        }
        for key in ready {
            let mut parts = vec![b.get(sources[&key])];
            parts.extend(bodies.remove(&key).unwrap_or_default());
            let union = b.node(ir::Op::Union(parts));
            let body = b.node(ir::Op::Threshold(union));
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
    // The SQLite host adapter resolves source columns through its first
    // output plan. Include every source in that plan.
    let sources_in_ir: Vec<_> =
        b.ir.rels
            .iter()
            .filter(|rel| rel.kind == ir::RelKind::Source)
            .map(|rel| (rel.id, rel.cols.len()))
            .collect();
    let mut touch = Vec::new();
    for (source, width) in sources_in_ir {
        let get = b.get(source);
        touch.push(b.mfp(
            get,
            vec![],
            vec![ir::Expr::Lit(1)],
            vec![assert_width(width)],
        ));
    }
    let sentinel = b.rel(
        "__ir_all_sources".into(),
        vec![ir::Ty::Int],
        ir::RelKind::Derived,
    );
    let body = b.node(ir::Op::Union(touch));
    b.ir.strata.push(ir::Stratum::Let { id: sentinel, body });
    b.ir.outputs.insert(0, sentinel);
    let seeds = program
        .seeds
        .into_iter()
        .map(|row| (sources[&(row.rel.0, row.args.len())], row))
        .collect();
    let _ = strata;
    Ok(Lowered {
        ir: b.ir,
        terms,
        seeds,
        term_map,
        unit,
        visible,
    })
}
