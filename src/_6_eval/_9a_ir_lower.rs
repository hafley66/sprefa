//! Checked DL8 goals lowered to the shared IVM operator algebra.
use super::kernel::{kernel_ref, IntCmp, Kernel};
use super::program::{AggregateKind, Arg, Diagnostic, Folding, Polarity, Program, Row, Rule};
use super::stratify::stratify;
use super::term::{Term, TermId, Universe};
use ivm_ir as ir;
use std::collections::{BTreeMap, BTreeSet, HashMap};

const COMPONENT_BUDGET: usize = 4096;

type RelationKey = (u32, usize);

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
    pub source: ir::RelId,
    pub output: ir::RelId,
}

pub struct Lowered {
    pub ir: ir::Program,
    pub terms: Vec<TermSpec>,
    pub texts: Vec<(TermId, String)>,
    pub seeds: Vec<(ir::RelId, Row)>,
    pub term_map: ir::RelId,
    pub unit: ir::RelId,
    pub visible: Vec<(ir::RelId, TermId, usize)>,
    pub internal_seeds: Vec<(ir::RelId, Vec<TermId>)>,
}

struct Builder {
    ir: ir::Program,
    next_rel: ir::RelId,
    constructor_gets: HashMap<ir::RelId, ir::NodeId>,
    intern_nodes: Vec<ir::NodeId>,
    intern_relation: Option<ir::RelId>,
    intern_readback: bool,
    intern_readback_count: usize,
    effect_nodes: Vec<ir::NodeId>,
    snapshot_nodes: Vec<ir::NodeId>,
}

impl Builder {
    fn new() -> Self {
        Self {
            ir: ir::Program {
                texts: Vec::new(),
                rels: Vec::new(),
                nodes: Vec::new(),
                strata: Vec::new(),
                outputs: Vec::new(),
            },
            next_rel: 0,
            constructor_gets: HashMap::new(),
            intern_nodes: Vec::new(),
            intern_relation: None,
            intern_readback: false,
            intern_readback_count: 0,
            effect_nodes: Vec::new(),
            snapshot_nodes: Vec::new(),
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
    fn get_constructor(&mut self, rel: ir::RelId) -> ir::NodeId {
        if let Some(&id) = self.constructor_gets.get(&rel) {
            return id;
        }
        let id = self.get(rel);
        self.constructor_gets.insert(rel, id);
        id
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
    let ctor = b.get_constructor(relation);
    b.node(ir::Op::Join {
        inputs: vec![input, ctor],
        equivalences: vec![vec![(0, id), (1, 0)]],
    })
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
    term_map: ir::RelId,
    none: TermId,
    empty: TermId,
) -> Result<(), &'static str> {
    let mut node = current;
    let mut size = width;
    let mut values = Vec::new();
    for arg in &goal.args {
        let term = match arg {
            Arg::Var(v) => bound.get(&v.0).copied(),
            Arg::Ground(id) => {
                let mapped = mapped_ground(b, term_map, *id);
                node = b.node(ir::Op::Join {
                    inputs: vec![node, mapped],
                    equivalences: vec![],
                });
                let col = assert_width(size + 1);
                size += 2;
                Some(col)
            }
            _ => return Err("served aggregate or fold"),
        };
        values.push(term);
    }
    let missing = mapped_ground(b, term_map, none);
    node = b.node(ir::Op::Join {
        inputs: vec![node, missing],
        equivalences: vec![],
    });
    let missing_col = assert_width(size + 1);
    size += 2;
    let empty_row = mapped_ground(b, term_map, empty);
    node = b.node(ir::Op::Join {
        inputs: vec![node, empty_row],
        equivalences: vec![],
    });
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
    let wrap = constructor(constructors, "9:1:const", 1, false)?;
    node = b.node(ir::Op::Mint {
        input: node,
        functor: wrap,
        args: vec![list],
    });
    let arguments = assert_width(size);
    size += 1;
    let relation = mapped_ground(b, term_map, goal.rel);
    node = b.node(ir::Op::Join {
        inputs: vec![node, relation],
        equivalences: vec![],
    });
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
    b.effect_nodes.push(effect);
    Ok(())
}

fn compile_kernel(
    b: &mut Builder,
    goal: &super::program::Goal,
    kernel: Kernel,
    constructors: &BTreeMap<(String, usize, bool), ir::RelId>,
    semantic: ir::RelId,
    bad_lists: ir::RelId,
    plain_lists: ir::RelId,
    term_map: ir::RelId,
    current: &mut ir::NodeId,
    width: &mut usize,
    bound: &mut HashMap<u32, ir::ColId>,
) -> Result<(), &'static str> {
    let mut inputs = Vec::new();
    for arg in &goal.args {
        match arg {
            Arg::Var(v) => inputs.push(bound.get(&v.0).copied()),
            Arg::Ground(id) => {
                let mapped = mapped_ground(b, term_map, *id);
                *current = b.node(ir::Op::Join {
                    inputs: vec![*current, mapped],
                    equivalences: vec![],
                });
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
        Kernel::Cons => {
            if inputs.len() != 3 {
                return Err("cons arity");
            }
            let wrap = constructor(constructors, "9:1:const", 1, false)?;
            let list = constructor(constructors, "9:2:[|]", 2, false)?;
            if let Some(whole) = inputs[2] {
                let bad = b.get(bad_lists);
                *current = b.node(ir::Op::Antijoin {
                    l: *current,
                    r: bad,
                    lk: vec![whole],
                    rk: vec![0],
                });
                *current = join_constructor(b, *current, *width, wrap, whole);
                let inner = assert_width(*width + 1);
                *width += 2;
                *current = join_constructor(b, *current, *width, list, inner);
                let head = assert_width(*width + 1);
                let tail = assert_width(*width + 2);
                *width += 3;
                *current = b.node(ir::Op::Mint {
                    input: *current,
                    functor: wrap,
                    args: vec![tail],
                });
                output[0] = Some(head);
                output[1] = Some(assert_width(*width));
                *width += 1;
            } else {
                let (Some(head), Some(tail)) = (inputs[0], inputs[1]) else {
                    *current = b.mfp(*current, vec![ir::Expr::Lit(0)], vec![], vec![]);
                    return Ok(());
                };
                let bad = b.get(bad_lists);
                *current = b.node(ir::Op::Antijoin {
                    l: *current,
                    r: bad,
                    lk: vec![tail],
                    rk: vec![0],
                });
                *current = join_constructor(b, *current, *width, wrap, tail);
                let tail_inner = assert_width(*width + 1);
                *width += 2;
                *current = b.node(ir::Op::Mint {
                    input: *current,
                    functor: list,
                    args: vec![head, tail_inner],
                });
                let inner = assert_width(*width);
                *width += 1;
                *current = b.node(ir::Op::Mint {
                    input: *current,
                    functor: wrap,
                    args: vec![inner],
                });
                output[2] = Some(assert_width(*width));
                *width += 1;
            }
        }
        Kernel::Nil => return Err("nil relation routing"),
        Kernel::StrCons => {
            if inputs.len() != 3 {
                return Err("str.cons arity");
            }
            let wrap = constructor(constructors, "9:1:const", 1, false)?;
            if let (Some(head), Some(rest)) = (inputs[0], inputs[1]) {
                *current = join_constructor(b, *current, *width, wrap, head);
                let head_text = assert_width(*width + 1);
                *width += 2;
                *current = join_constructor(b, *current, *width, wrap, rest);
                let rest_text = assert_width(*width + 1);
                *width += 2;
                *current = b.node(ir::Op::StrCons {
                    input: *current,
                    mode: ir::StrMode::Construct {
                        head: head_text,
                        rest: rest_text,
                    },
                });
                let text = assert_width(*width);
                *width += 1;
                *current = b.node(ir::Op::Mint {
                    input: *current,
                    functor: wrap,
                    args: vec![text],
                });
                output[2] = Some(assert_width(*width));
                *width += 1;
            } else if let Some(whole) = inputs[2] {
                *current = join_constructor(b, *current, *width, wrap, whole);
                let whole_text = assert_width(*width + 1);
                *width += 2;
                *current = b.node(ir::Op::StrCons {
                    input: *current,
                    mode: ir::StrMode::Decompose { whole: whole_text },
                });
                let head_text = assert_width(*width);
                let rest_text = assert_width(*width + 1);
                *width += 2;
                *current = b.node(ir::Op::Mint {
                    input: *current,
                    functor: wrap,
                    args: vec![head_text],
                });
                output[0] = Some(assert_width(*width));
                *width += 1;
                *current = b.node(ir::Op::Mint {
                    input: *current,
                    functor: wrap,
                    args: vec![rest_text],
                });
                output[1] = Some(assert_width(*width));
                *width += 1;
            } else {
                *current = b.mfp(*current, vec![ir::Expr::Lit(0)], vec![], vec![]);
                return Ok(());
            }
        }
        Kernel::StrNil => {
            if inputs.len() != 1 {
                return Err("str.nil arity");
            }
            let wrap = constructor(constructors, "9:1:const", 1, false)?;
            *current = b.mfp(
                *current,
                vec![],
                vec![ir::Expr::Call(ir::Func::StrNil, vec![])],
                vec![],
            );
            let text = assert_width(*width);
            *width += 1;
            *current = b.node(ir::Op::Mint {
                input: *current,
                functor: wrap,
                args: vec![text],
            });
            output[0] = Some(assert_width(*width));
            *width += 1;
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
            *current = join_constructor(b, *current, *width, plain_lists, arguments);
            let plain = assert_width(*width + 1);
            *width += 2;
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
    }
    Ok(())
}

fn compile_rule(
    b: &mut Builder,
    u: &Universe,
    rule: &Rule,
    relations: &BTreeMap<(u32, usize), ir::RelId>,
    constructors: &BTreeMap<(String, usize, bool), ir::RelId>,
    semantic: ir::RelId,
    bad_lists: ir::RelId,
    plain_lists: ir::RelId,
    term_map: ir::RelId,
    unit: ir::RelId,
    served: &std::collections::HashSet<TermId>,
    none: TermId,
    empty: TermId,
) -> Result<ir::NodeId, &'static str> {
    let mut current = b.get(unit);
    let mut width = 1usize;
    let mut bound: HashMap<u32, ir::ColId> = HashMap::new();
    for goal in &rule.body {
        if served.contains(&goal.rel) {
            served_effect(
                b,
                goal,
                current,
                width,
                &bound,
                constructors,
                semantic,
                term_map,
                none,
                empty,
            )?;
        }
        if let Some(kernel) = Kernel::of(u, goal.rel).filter(|kernel| {
            *kernel != Kernel::Nil && (!rule.is_aggregate() || goal.polarity == Polarity::Negative)
        }) {
            compile_kernel(
                b,
                goal,
                kernel,
                constructors,
                semantic,
                bad_lists,
                plain_lists,
                term_map,
                &mut current,
                &mut width,
                &mut bound,
            )?;
            continue;
        }
        let rel = *relations
            .get(&(goal.rel.0, goal.args.len()))
            .ok_or("unresolved relation")?;
        let mut right = b.get(rel);
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
        if !repeated.is_empty() {
            right = b.mfp(right, repeated, vec![], vec![]);
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
                let mut filtered = right;
                let mut size = goal.args.len().max(1);
                for (position, original) in &grounds {
                    let mapped = mapped_ground(b, term_map, *original);
                    filtered = b.node(ir::Op::Join {
                        inputs: vec![filtered, mapped],
                        equivalences: vec![vec![(0, *position), (1, 1)]],
                    });
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
            continue;
        }
        current = b.node(ir::Op::Join {
            inputs: vec![current, right],
            equivalences,
        });
        let total = width + goal.args.len().max(1);
        for (position, original) in &grounds {
            let mapped = mapped_ground(b, term_map, *original);
            current = b.node(ir::Op::Join {
                inputs: vec![current, mapped],
                equivalences: vec![vec![(0, assert_width(width + *position as usize)), (1, 1)]],
            });
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
    if rule.is_aggregate() {
        if rule.aggregate_args() != 1 {
            return Err("aggregate head arity");
        }
        let (position, folding, subject) = rule.folding_head().ok_or("aggregate head")?;
        let Folding::Builtin(kind) = folding else {
            return Err("declared fold");
        };
        let subject = match subject {
            Arg::Var(v) => *bound.get(&v.0).ok_or("unbound aggregate subject")?,
            Arg::Ground(id) => {
                let mapped = mapped_ground(b, term_map, *id);
                current = b.node(ir::Op::Join {
                    inputs: vec![current, mapped],
                    equivalences: vec![],
                });
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
                    let mapped = mapped_ground(b, term_map, *id);
                    current = b.node(ir::Op::Join {
                        inputs: vec![current, mapped],
                        equivalences: vec![],
                    });
                    keys.push(assert_width(width + 1));
                    width += 2;
                }
                _ => return Err("multiple aggregate heads"),
            }
        }
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
        return Ok(b.mfp(current, vec![], vec![], projected));
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
                return Ok(mapped);
            };
            head_cols.push(col);
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
    let intern_rel = kernel_ref(u, "intern");
    let effect_rel = kernel_ref(u, "effect");
    let snapshot_rel = kernel_ref(u, "intern_snapshot");
    let none_atom = u.atom("none");
    let none = u.compound("const", vec![none_atom]);
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
    let dynamic_lists = program.rules.iter().any(|rule| {
        rule.body.iter().any(|goal| {
            Kernel::of(u, goal.rel) == Some(Kernel::Intern)
                && matches!(goal.args.get(1), Some(Arg::Var(_)))
        })
    });
    let possible_lists: Vec<TermId> = (0..u.terms.len()).map(|i| TermId(i as u32)).collect();
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
    let bad_list_terms: Vec<TermId> = (0..u.terms.len())
        .map(|i| TermId(i as u32))
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
            Term::Str(sym) => {
                texts.push((TermId(index as u32), u.sym_str(*sym).to_owned()));
                continue;
            }
            Term::Atom(sym) => (
                "3:atom".to_string(),
                vec![],
                Some(if *sym == u.nil {
                    0
                } else {
                    atoms[u.sym_str(*sym)]
                }),
            ),
            Term::Compound(sym, children) => (
                format!("9:{}:{}", children.len(), u.sym_str(*sym)),
                children.clone(),
                None,
            ),
            Term::Float(_) => return Err(unsupported(u, "float")),
        };
        constructor_keys.insert((name.clone(), args.len(), raw.is_some()));
        shapes.push((TermId(index as u32), name, args, raw));
    }
    for (name, args, raw) in [
        ("0:int", 0, true),
        ("3:atom", 0, true),
        ("9:1:const", 1, false),
        ("9:1:ref", 1, false),
        ("9:2:[|]", 2, false),
        ("9:2:application", 2, false),
        ("9:2:edge", 2, false),
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
    let semantic = b.rel(
        "__ir_semantic".into(),
        vec![ir::Ty::Id, ir::Ty::Id],
        ir::RelKind::Derived,
    );
    let refs = b.get_constructor(constructors[&("9:1:ref".to_string(), 1, false)]);
    let consts = b.get_constructor(constructors[&("9:1:const".to_string(), 1, false)]);
    let body = b.node(ir::Op::Union(vec![refs, consts]));
    b.ir.strata.push(ir::Stratum::Let { id: semantic, body });
    let seed_lists = b.get(plain_source);
    if dynamic_lists {
        let wrap = constructors[&("9:1:const".to_string(), 1, false)];
        let cons = constructors[&("9:2:[|]".to_string(), 2, false)];
        let mut pairs = b.get(plain_lists);
        pairs = join_constructor(&mut b, pairs, 2, wrap, 0);
        let tail_inner = 3;
        let cons_rows = b.get_constructor(cons);
        pairs = b.node(ir::Op::Join {
            inputs: vec![pairs, cons_rows],
            equivalences: vec![vec![(0, tail_inner), (1, 2)]],
        });
        let tagged_head = 5;
        let list_inner = 4;
        let wraps = b.get_constructor(wrap);
        pairs = b.node(ir::Op::Join {
            inputs: vec![pairs, wraps],
            equivalences: vec![vec![(0, list_inner), (1, 1)]],
        });
        let whole_tagged = 7;
        pairs = join_constructor(&mut b, pairs, 9, semantic, tagged_head);
        let plain_head = 10;
        pairs = b.node(ir::Op::Mint {
            input: pairs,
            functor: cons,
            args: vec![plain_head, 1],
        });
        pairs = b.mfp(pairs, vec![], vec![], vec![whole_tagged, 11]);
        let body = b.node(ir::Op::Union(vec![seed_lists, pairs]));
        b.ir.strata.push(ir::Stratum::LetRec(ir::LetRec {
            ids: vec![plain_lists],
            bodies: vec![body],
            limit: None,
        }));
    } else {
        b.ir.strata.push(ir::Stratum::Let {
            id: plain_lists,
            body: seed_lists,
        });
    }
    let mut keys = BTreeSet::new();
    for seed in &program.seeds {
        keys.insert((seed.rel.0, seed.args.len()));
    }
    for rule in &program.rules {
        keys.insert((rule.rel.0, rule.head.len()));
        for goal in &rule.body {
            if Kernel::of(u, goal.rel).is_none_or(|kernel| kernel == Kernel::Nil)
                || (rule.is_aggregate() && goal.polarity == Polarity::Positive)
            {
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
    let request_rel = b.rel(
        "__ir_intern_requests".into(),
        vec![ir::Ty::Id; 3],
        ir::RelKind::Derived,
    );
    b.intern_relation = Some(request_rel);
    let mut bodies: BTreeMap<(u32, usize), Vec<ir::NodeId>> = BTreeMap::new();
    let mut request_dependencies = BTreeSet::new();
    let mut readback_heads = BTreeSet::new();
    for rule in &program.rules {
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
            bad_lists,
            plain_lists,
            term_map,
            unit,
            &program.served,
            none,
            empty,
        )
        .map_err(|name| unsupported(u, name))?;
        if b.intern_nodes.len() > requests_before {
            for goal in &rule.body {
                let key = (goal.rel.0, goal.args.len());
                if keys.contains(&key) {
                    request_dependencies.insert(key);
                }
            }
        }
        if b.intern_readback_count > readbacks_before {
            readback_heads.insert((rule.rel.0, rule.head.len()));
        }
        bodies
            .entry((rule.rel.0, rule.head.len()))
            .or_default()
            .push(body);
    }
    if b.intern_readback {
        let request_key = (u32::MAX, 3);
        keys.insert(request_key);
        let source = b.rel(
            "__ir_intern_request_seed".into(),
            vec![ir::Ty::Id; 3],
            ir::RelKind::Source,
        );
        sources.insert(request_key, source);
        relations.insert(request_key, request_rel);
        let requests = std::mem::take(&mut b.intern_nodes);
        bodies.insert(request_key, requests);
        b.ir.outputs.push(request_rel);
        visible.push((request_rel, intern_rel, 3));
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
    }
    if b.intern_readback {
        let request_key = (u32::MAX, 3);
        for dependency in request_dependencies {
            edges.entry(request_key).or_default().insert(dependency);
            reverse.entry(dependency).or_default().insert(request_key);
        }
        for head in readback_heads {
            edges.entry(head).or_default().insert(request_key);
            reverse.entry(request_key).or_default().insert(head);
        }
    }
    for _ in 0..COMPONENT_BUDGET {
        if pending.is_empty() {
            break;
        }
        let ready: Vec<_> = pending
            .iter()
            .copied()
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
            for &candidate in &pending {
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
                let mut parts = vec![b.get(sources[key])];
                parts.extend(bodies.remove(key).unwrap_or_default());
                let union = b.node(ir::Op::Union(parts));
                rec_bodies.push(union);
            }
            b.ir.strata.push(ir::Stratum::LetRec(ir::LetRec {
                ids,
                bodies: rec_bodies,
                limit: None,
            }));
            for key in component {
                pending.remove(&key);
            }
            continue;
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
    if !b.intern_readback {
        let requests = std::mem::take(&mut b.intern_nodes);
        let has_requests = !requests.is_empty();
        let body = if has_requests {
            let union = b.node(ir::Op::Union(requests));
            b.node(ir::Op::Threshold(union))
        } else {
            let unit_row = b.get(unit);
            b.mfp(
                unit_row,
                vec![ir::Expr::Lit(0)],
                vec![ir::Expr::Col(0), ir::Expr::Col(0)],
                vec![0, 1, 2],
            )
        };
        b.ir.strata.push(ir::Stratum::Let {
            id: request_rel,
            body,
        });
        if has_requests {
            b.ir.outputs.push(request_rel);
            visible.push((request_rel, intern_rel, 3));
        }
    }
    if !b.effect_nodes.is_empty() {
        let effect_output = b.rel(
            "__ir_effects".into(),
            vec![ir::Ty::Id; 2],
            ir::RelKind::Derived,
        );
        let nodes = std::mem::take(&mut b.effect_nodes);
        let body = b.node(ir::Op::Union(nodes));
        b.ir.strata.push(ir::Stratum::Let {
            id: effect_output,
            body,
        });
        b.ir.outputs.push(effect_output);
        visible.push((effect_output, effect_rel, 2));
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
    })
}
