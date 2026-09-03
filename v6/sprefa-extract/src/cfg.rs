//! The CFG plane: ONE generic builder over the CstF parse, plus a hand-authored
//! `kind_role` table per language.

//! The tables are the census in `plans/2026-08-16-cpg-spec-research.REPORT.md`
//! section 4, with two deliberate departures.

//! The census's branch-TARGET rows (`match_arm`, `*_case`, `when_entry`,
//! `else_clause`) are absent: role-free is transparent, and an arm body is a

//! SEQUENCE. Marking them `Branch` makes each statement of a case body an arm.

//! Kotlin's `jump_expression` folds return/throw/break/continue into one kind
//! (tree-sitter-kotlin-sg 0.4.1 grammar.js:1119-1126), so its row reads a token.

use std::collections::HashSet;

use crate::lang::source_for;
use crate::rows::{Edge, FamilyBundle, Node};
use crate::shape::{NodeRef, Span, Strings};
use crate::source::{ExtractOutput, FamilyMask};
use crate::types::{CfgEdgeKind, CfgF, CfgNodeKind, CstEdgeKind, CstF};
use crate::wire::{flatten_cfg, FlatFact};

/// The control role one CST kind plays. `Callable` and the two loop shapes name
/// WHICH child is the body, which is the only structural fact the walk needs.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CfgRole {
    /// Its own Entry/Exit pair; the body is the last child ending where it ends.
    Callable,
    /// Its own Entry/Exit pair, head at child 0 and produced LAST: the later
    /// children run in sequence, then the head's aggregates, then the head.
    Clause,
    /// Child 0 is the condition; every later child is an arm.
    Branch,
    /// Child 0 is the condition; the LAST child's children are the arms (the
    /// grammar wraps the arms in one block).
    Cases,
    /// Every child is an arm, none a condition (`;`); a Branch first arm makes
    /// it an if-then-else whose failed condition enters the next arm.
    Fork,
    /// The last child is the one arm (`\+ Goal`, `not(Goal)`).
    Negation,
    /// Child 0 is the guarded body, walked in sequence; every later child is a
    /// handler arm.
    Try,
    /// The children link in sequence even when none holds a role (`,`).
    Seq,
    /// The body is the last child (`for`, `while`, `loop`).
    Loop,
    /// The body is the FIRST child (`do { } while`).
    DoLoop,
    /// break / continue / goto: control leaves the enclosing loop.
    Jump,
    /// return / throw / yield: control leaves the callable.
    Exit,
    /// A goal naming the enclosing clause's own head: one statement plus a jump
    /// edge back to the Entry. Assigned only where the names agree.
    SelfCall,
    /// A fold over the body's rows (`count(child)` in a dl6 head): one Loop
    /// node with no arm.
    Aggregate,
}

/// How a kind's role is decided. `LeadingKeyword` is for grammars that fold
/// several jumps into ONE kind, where the kind name cannot split jump from exit.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RoleRule {
    Fixed(CfgRole),
    LeadingKeyword,
    /// The operator text before or between the named children picks the role
    /// (prolog's operators are anonymous tokens); unlisted stays role-free.
    Operator(&'static [(&'static str, CfgRole)]),
    /// The leading name of a call-shaped node picks the role (`not(...)`,
    /// `count(...)`); an unlisted name takes the fallback.
    Named(&'static [(&'static str, CfgRole)], Option<CfgRole>),
}

use CfgRole::{
    Aggregate, Branch, Callable, Cases, Clause, DoLoop, Exit, Fork, Jump, Loop, Negation, SelfCall,
    Seq, Try,
};
use RoleRule::{Fixed, LeadingKeyword, Named, Operator};

/// tree-sitter-rust 0.24.2 node kinds.
pub const RUST_ROLES: &[(&str, RoleRule)] = &[
    ("function_item", Fixed(Callable)),
    ("closure_expression", Fixed(Callable)),
    ("if_expression", Fixed(Branch)),
    ("match_expression", Fixed(Branch)),
    ("loop_expression", Fixed(Loop)),
    ("while_expression", Fixed(Loop)),
    ("for_expression", Fixed(Loop)),
    ("break_expression", Fixed(Jump)),
    ("continue_expression", Fixed(Jump)),
    ("return_expression", Fixed(Exit)),
    ("yield_expression", Fixed(Exit)),
];

/// tree-sitter-go 0.23.4 node kinds.
pub const GO_ROLES: &[(&str, RoleRule)] = &[
    ("function_declaration", Fixed(Callable)),
    ("method_declaration", Fixed(Callable)),
    ("func_literal", Fixed(Callable)),
    ("if_statement", Fixed(Branch)),
    ("expression_switch_statement", Fixed(Branch)),
    ("type_switch_statement", Fixed(Branch)),
    ("select_statement", Fixed(Branch)),
    ("for_statement", Fixed(Loop)),
    ("break_statement", Fixed(Jump)),
    ("continue_statement", Fixed(Jump)),
    ("goto_statement", Fixed(Jump)),
    ("fallthrough_statement", Fixed(Jump)),
    ("return_statement", Fixed(Exit)),
];

/// tree-sitter-typescript 0.23.2 node kinds.
pub const TS_ROLES: &[(&str, RoleRule)] = &[
    ("function_declaration", Fixed(Callable)),
    ("generator_function_declaration", Fixed(Callable)),
    ("function_expression", Fixed(Callable)),
    ("arrow_function", Fixed(Callable)),
    ("method_definition", Fixed(Callable)),
    ("if_statement", Fixed(Branch)),
    ("switch_statement", Fixed(Branch)),
    ("ternary_expression", Fixed(Branch)),
    ("for_statement", Fixed(Loop)),
    ("for_in_statement", Fixed(Loop)),
    ("while_statement", Fixed(Loop)),
    ("do_statement", Fixed(DoLoop)),
    ("break_statement", Fixed(Jump)),
    ("continue_statement", Fixed(Jump)),
    ("return_statement", Fixed(Exit)),
    ("throw_statement", Fixed(Exit)),
    ("yield_expression", Fixed(Exit)),
];

/// tree-sitter-kotlin-sg 0.4.1 node kinds.
pub const KOTLIN_ROLES: &[(&str, RoleRule)] = &[
    ("function_declaration", Fixed(Callable)),
    ("anonymous_function", Fixed(Callable)),
    ("lambda_literal", Fixed(Callable)),
    ("if_expression", Fixed(Branch)),
    ("when_expression", Fixed(Branch)),
    ("for_statement", Fixed(Loop)),
    ("while_statement", Fixed(Loop)),
    ("do_while_statement", Fixed(DoLoop)),
    ("jump_expression", LeadingKeyword),
];

/// tree-sitter-python 0.23 node kinds (ast-grep's grammar). `elif_clause` is a
/// Branch of its own: it carries its own condition, unlike `else_clause`.
pub const PYTHON_ROLES: &[(&str, RoleRule)] = &[
    ("function_definition", Fixed(Callable)),
    ("lambda", Fixed(Callable)),
    ("if_statement", Fixed(Branch)),
    ("elif_clause", Fixed(Branch)),
    ("match_statement", Fixed(Cases)),
    ("try_statement", Fixed(Try)),
    ("for_statement", Fixed(Loop)),
    ("while_statement", Fixed(Loop)),
    ("break_statement", Fixed(Jump)),
    ("continue_statement", Fixed(Jump)),
    ("return_statement", Fixed(Exit)),
    ("raise_statement", Fixed(Exit)),
    ("yield", Fixed(Exit)),
];

/// tree-sitter-prolog 0.1.0 node kinds. The `:-` operation is the callable:
/// the clause's own `.` lies outside it, so its body ends where it ends.
pub const PROLOG_ROLES: &[(&str, RoleRule)] = &[
    (
        "binary_operation",
        Operator(&[
            (":-", Callable),
            ("-->", Callable),
            (",", Seq),
            (";", Fork),
            ("|", Fork),
            ("->", Branch),
            ("*->", Branch),
        ]),
    ),
    ("unary_operation", Operator(&[("\\+", Negation)])),
    ("compound_term", Named(&[("not", Negation)], Some(SelfCall))),
    ("atom", Named(&[], Some(SelfCall))),
];

/// tree-sitter-dl6 node kinds. Aggregates live in the head only
/// (`compile/registry.pl` `surface(count/1, aggregate, ...)`).
pub const DL6_ROLES: &[(&str, RoleRule)] = &[
    ("rule", Fixed(Clause)),
    ("fact", Fixed(Clause)),
    ("query", Fixed(Clause)),
    ("match_statement", Fixed(Branch)),
    ("goal_list", Fixed(Seq)),
    (
        "atom",
        Named(
            &[
                ("not", Negation),
                ("count", Aggregate),
                ("sum", Aggregate),
                ("min", Aggregate),
                ("max", Aggregate),
                ("avg", Aggregate),
                ("group_concat", Aggregate),
                ("json_group_array", Aggregate),
                ("json_object", Aggregate),
            ],
            Some(SelfCall),
        ),
    ),
];

/// The kind_role table for one `Source::name()`, None for a language with no
/// hand-authored rows (its CFG is empty rather than wrong).
pub fn roles_for(lang: &str) -> Option<&'static [(&'static str, RoleRule)]> {
    match lang {
        "rust" => Some(RUST_ROLES),
        "go" => Some(GO_ROLES),
        "ts" => Some(TS_ROLES),
        "kotlin" => Some(KOTLIN_ROLES),
        "python" => Some(PYTHON_ROLES),
        "prolog" => Some(PROLOG_ROLES),
        "dl6" => Some(DL6_ROLES),
        _ => None,
    }
}

/// The keyword read behind `LeadingKeyword` and behind break-vs-continue: the
/// leading word of the node's own source text.
fn keyword_role(word: &str) -> Option<CfgRole> {
    match word {
        "return" | "throw" | "yield" => Some(Exit),
        "break" | "continue" => Some(Jump),
        _ => None,
    }
}

/// One file's CFG from its CstF bundle. `content` is the file's bytes: the
/// leading-keyword read needs the source text, the CST kind name is not enough.
pub fn build_cfg(
    roles: &[(&str, RoleRule)],
    cst: &FamilyBundle<CstF>,
    strings: &Strings,
    content: &[u8],
) -> FamilyBundle<CfgF> {
    let mut build = CfgBuild::new(roles, cst, strings, content);
    let callables: Vec<NodeRef> = (0..cst.nodes.len())
        .map(|ix| NodeRef(ix as u32))
        .filter(|node| matches!(build.roles_by_node[node.0 as usize], Some(Callable | Clause)))
        .collect();
    for callable in callables {
        match build.roles_by_node[callable.0 as usize] {
            Some(Clause) => build.clause(callable),
            _ => build.callable(callable),
        }
    }
    build.bundle
}

/// The CFG of one already-extracted file, keyed on the path's language. None
/// when no `Source` matches the path or the language has no kind_role rows.
pub fn cfg_bundle(
    path: &str,
    output: &ExtractOutput,
    content: &[u8],
) -> Option<FamilyBundle<CfgF>> {
    let lang = source_for(path)?.name();
    let roles = roles_for(lang)?;
    let cst = output.cst.as_ref()?;
    Some(build_cfg(roles, cst, &output.strings, content))
}

/// The standalone door: parse `content` for its CST and flatten its CFG. Empty
/// for a path no `Source` matches or a language with no kind_role rows.
pub fn cfg_facts(path: &str, content: &[u8]) -> Vec<FlatFact> {
    let Some(source) = source_for(path) else {
        return Vec::new();
    };
    let mut mask = FamilyMask::NONE;
    mask.cst = true;
    let output = source.extract(path, content, mask);
    match cfg_bundle(path, &output, content) {
        Some(bundle) => flatten_cfg(&bundle),
        None => Vec::new(),
    }
}

/// Where control can go when a subtree is done. `exits` is the set of nodes that
/// fall THROUGH; a return or a break leaves it empty.
#[derive(Default)]
struct Flow {
    entry: Option<NodeRef>,
    exits: Vec<NodeRef>,
}

/// One enclosing loop: its header, and the break nodes that leave it. A break's
/// successor is not known until the loop's own successor is, so it is collected.
struct LoopFrame {
    header: NodeRef,
    breaks: Vec<NodeRef>,
}

struct CfgBuild<'a> {
    cst: &'a FamilyBundle<CstF>,
    content: &'a [u8],
    children: Vec<Vec<NodeRef>>,
    roles_by_node: Vec<Option<CfgRole>>,
    roled_subtree: Vec<bool>,
    bundle: FamilyBundle<CfgF>,
    seen: HashSet<(u32, u32, u8)>,
    loops: Vec<LoopFrame>,
    entry: NodeRef,
    exit: NodeRef,
}

impl<'a> CfgBuild<'a> {
    fn new(
        roles: &[(&str, RoleRule)],
        cst: &'a FamilyBundle<CstF>,
        strings: &'a Strings,
        content: &'a [u8],
    ) -> Self {
        let count = cst.nodes.len();
        let mut children: Vec<Vec<NodeRef>> = vec![Vec::new(); count];
        let mut parent: Vec<Option<NodeRef>> = vec![None; count];
        for edge in &cst.edges {
            let CstEdgeKind::Child = edge.kind;
            children[edge.src.0 as usize].push(edge.dst);
            parent[edge.dst.0 as usize] = Some(edge.src);
        }
        let rules: Vec<Option<RoleRule>> = cst
            .nodes
            .iter()
            .map(|node| {
                let kind = strings.lookup(node.kind);
                roles
                    .iter()
                    .find(|(name, _)| *name == kind)
                    .map(|(_, rule)| *rule)
            })
            .collect();
        let mut roles_by_node = Vec::with_capacity(count);
        for (ix, node) in cst.nodes.iter().enumerate() {
            roles_by_node.push(match rules[ix] {
                Some(Fixed(role)) => Some(role),
                Some(LeadingKeyword) => keyword_role(leading_word(content, node.span)),
                Some(Operator(table)) => operator_text(content, node.span, &children[ix], cst)
                    .and_then(|op| table.iter().find(|(text, _)| *text == op))
                    .map(|(_, role)| *role),
                Some(Named(table, fallback)) => table
                    .iter()
                    .find(|(name, _)| *name == leading_name(content, node.span))
                    .map(|(_, role)| *role)
                    .or(fallback),
                None => None,
            });
        }
        // Pre-order layout: a parent's role is settled before its children's.
        for ix in 0..count {
            if roles_by_node[ix] != Some(SelfCall) {
                continue;
            }
            roles_by_node[ix] = None;
            let Some(above) = parent[ix] else { continue };
            let above_ix = above.0 as usize;
            let argument_of_call = matches!(rules[above_ix], Some(Named(..)))
                && matches!(roles_by_node[above_ix], None | Some(Aggregate | SelfCall));
            let own_head = matches!(roles_by_node[above_ix], Some(Callable | Clause))
                && children[above_ix].first() == Some(&NodeRef(ix as u32));
            if argument_of_call || own_head {
                continue;
            }
            let goal = leading_name(content, cst.nodes[ix].span);
            if goal.is_empty() {
                continue;
            }
            let mut walk = Some(above);
            while let Some(node) = walk {
                let node_ix = node.0 as usize;
                if matches!(roles_by_node[node_ix], Some(Callable | Clause)) {
                    if leading_name(content, cst.nodes[node_ix].span) == goal {
                        roles_by_node[ix] = Some(SelfCall);
                    }
                    break;
                }
                walk = parent[node_ix];
            }
        }
        let mut roled_subtree = vec![false; count];
        for ix in 0..count {
            if roles_by_node[ix].is_none() {
                continue;
            }
            let mut walk = Some(NodeRef(ix as u32));
            while let Some(node) = walk {
                if roled_subtree[node.0 as usize] {
                    break;
                }
                roled_subtree[node.0 as usize] = true;
                walk = parent[node.0 as usize];
            }
        }
        Self {
            cst,
            content,
            children,
            roles_by_node,
            roled_subtree,
            bundle: FamilyBundle::default(),
            seen: HashSet::new(),
            loops: Vec::new(),
            entry: NodeRef(0),
            exit: NodeRef(0),
        }
    }

    fn span(&self, node: NodeRef) -> Span {
        self.cst.nodes[node.0 as usize].span
    }

    fn kids(&self, node: NodeRef) -> Vec<NodeRef> {
        self.children[node.0 as usize].clone()
    }

    fn node(&mut self, span: Span, kind: CfgNodeKind) -> NodeRef {
        let placed = NodeRef(self.bundle.nodes.len() as u32);
        self.bundle.nodes.push(Node::new(span, kind));
        placed
    }

    fn edge(&mut self, src: NodeRef, dst: NodeRef, kind: CfgEdgeKind) {
        if self.seen.insert((src.0, dst.0, kind as u8)) {
            self.bundle.edges.push(Edge::new(src, dst, kind));
        }
    }

    /// An edge into the Exit node is `Exit`, out of a break/continue is `Jump`,
    /// and otherwise plain succession.
    fn connect(&mut self, srcs: &[NodeRef], dst: NodeRef) {
        for &src in srcs {
            let kind = if dst == self.exit {
                CfgEdgeKind::Exit
            } else if self.bundle.nodes[src.0 as usize].kind == CfgNodeKind::Jump {
                CfgEdgeKind::Jump
            } else {
                CfgEdgeKind::Next
            };
            self.edge(src, dst, kind);
        }
    }

    /// A callable's body is its last child, and a body ends where the callable
    /// ends: that test is what tells a declaration-only signature from a body.
    fn callable(&mut self, callable: NodeRef) {
        let span = self.span(callable);
        let (entry, exit) = self.open(span);
        let body = self
            .kids(callable)
            .last()
            .copied()
            .filter(|child| self.span(*child).end() == span.end());
        let flow = match body {
            Some(child) => self.link(child),
            None => Flow::default(),
        };
        self.close(entry, exit, flow);
    }

    /// A clause runs its body, folds the head's aggregates over the rows, and
    /// produces the head last; a fact or a `?` query is head only.
    fn clause(&mut self, clause: NodeRef) {
        let span = self.span(clause);
        let (entry, exit) = self.open(span);
        let kids = self.kids(clause);
        let Some((&head, body)) = kids.split_first() else {
            self.close(entry, exit, Flow::default());
            return;
        };
        let mut flow = self.link_seq(body);
        for fold in self.descendants(head) {
            if self.roles_by_node[fold.0 as usize] == Some(Aggregate) {
                let step = self.fold(self.span(fold));
                self.chain(&mut flow, step);
            }
        }
        let produced = self.leaf(self.span(head));
        self.chain(&mut flow, produced);
        self.close(entry, exit, flow);
    }

    fn open(&mut self, span: Span) -> (NodeRef, NodeRef) {
        let entry = self.node(span, CfgNodeKind::Entry);
        let exit = self.node(span, CfgNodeKind::Exit);
        self.entry = entry;
        self.exit = exit;
        self.loops.clear();
        (entry, exit)
    }

    fn close(&mut self, entry: NodeRef, exit: NodeRef, flow: Flow) {
        match flow.entry {
            Some(first) => {
                self.edge(entry, first, CfgEdgeKind::Next);
                self.connect(&flow.exits, exit);
            }
            None => self.edge(entry, exit, CfgEdgeKind::Exit),
        }
    }

    fn descendants(&self, node: NodeRef) -> Vec<NodeRef> {
        let mut found = Vec::new();
        let mut stack = self.kids(node);
        stack.reverse();
        while let Some(next) = stack.pop() {
            found.push(next);
            let mut kids = self.kids(next);
            kids.reverse();
            stack.extend(kids);
        }
        found
    }

    /// A role-free node is TRANSPARENT when its subtree holds a role (its
    /// children link as a sequence) and one plain node when it does not.
    fn link(&mut self, node: NodeRef) -> Flow {
        let span = self.span(node);
        match self.roles_by_node[node.0 as usize] {
            Some(Callable | Clause) => self.leaf(span),
            Some(Aggregate) => self.fold(span),
            Some(SelfCall) => {
                let call = self.node(span, CfgNodeKind::Stmt);
                let entry = self.entry;
                self.edge(call, entry, CfgEdgeKind::Jump);
                Flow {
                    entry: Some(call),
                    exits: vec![call],
                }
            }
            Some(Seq) => {
                let kids = self.kids(node);
                self.link_seq(&kids)
            }
            Some(Fork) => self.link_fork(node, span),
            Some(Negation) => self.link_negation(node, span),
            Some(Try) => self.link_try(node, span),
            Some(Exit) => {
                let ret = self.node(span, CfgNodeKind::Ret);
                let exit = self.exit;
                self.edge(ret, exit, CfgEdgeKind::Exit);
                Flow {
                    entry: Some(ret),
                    exits: Vec::new(),
                }
            }
            Some(Jump) => self.link_jump(span),
            Some(Loop) => self.link_loop(node, span, false),
            Some(DoLoop) => self.link_loop(node, span, true),
            Some(Branch) => self.link_branch(node, span, &[]),
            Some(Cases) => {
                let arms = self.kids(node).last().map(|block| self.kids(*block));
                self.link_arms(span, &arms.unwrap_or_default())
            }
            None => {
                if self.roled_subtree[node.0 as usize] {
                    let kids = self.kids(node);
                    self.link_seq(&kids)
                } else {
                    self.leaf(span)
                }
            }
        }
    }

    fn leaf(&mut self, span: Span) -> Flow {
        let node = self.node(span, CfgNodeKind::Stmt);
        Flow {
            entry: Some(node),
            exits: vec![node],
        }
    }

    fn fold(&mut self, span: Span) -> Flow {
        let node = self.node(span, CfgNodeKind::Loop);
        Flow {
            entry: Some(node),
            exits: vec![node],
        }
    }

    /// Appends `step` after `flow`; a step with no entry leaves `flow` as is.
    fn chain(&mut self, flow: &mut Flow, step: Flow) {
        let Some(entry) = step.entry else { return };
        if flow.entry.is_none() {
            flow.entry = Some(entry);
        } else {
            let pending = std::mem::take(&mut flow.exits);
            self.connect(&pending, entry);
        }
        flow.exits = step.exits;
    }

    fn link_seq(&mut self, kids: &[NodeRef]) -> Flow {
        let mut flow = Flow::default();
        for &kid in kids {
            let step = self.link(kid);
            self.chain(&mut flow, step);
        }
        flow
    }

    /// `continue` re-enters the loop header; `break` waits for the loop's own
    /// successor; a `goto` with no enclosing loop leaves control unmodelled.
    fn link_jump(&mut self, span: Span) -> Flow {
        let jump = self.node(span, CfgNodeKind::Jump);
        let word = leading_word(self.content, span);
        let header = self.loops.last().map(|frame| frame.header);
        match (header, word) {
            (Some(header), "continue") => self.edge(jump, header, CfgEdgeKind::Jump),
            (Some(_), _) => self
                .loops
                .last_mut()
                .expect("the header read proves a frame")
                .breaks
                .push(jump),
            (None, _) => {}
        }
        Flow {
            entry: Some(jump),
            exits: Vec::new(),
        }
    }

    fn link_loop(&mut self, node: NodeRef, span: Span, body_first: bool) -> Flow {
        let header = self.node(span, CfgNodeKind::Loop);
        let kids = self.kids(node);
        let body = if body_first {
            kids.first().copied()
        } else {
            kids.last().copied()
        };
        self.loops.push(LoopFrame {
            header,
            breaks: Vec::new(),
        });
        let inner = match body {
            Some(child) => self.link(child),
            None => Flow::default(),
        };
        let frame = self.loops.pop().expect("the frame pushed just above");
        if let Some(entry) = inner.entry {
            self.edge(header, entry, CfgEdgeKind::Arm);
            self.connect(&inner.exits, header);
        }
        let mut exits = vec![header];
        exits.extend(frame.breaks);
        Flow {
            entry: Some(header),
            exits,
        }
    }

    /// Child 0 is the condition and rides the branch node; later children are
    /// arms. The branch stays in the exit set unless `else_arms` (the arms a
    /// failed condition enters) is non-empty: the CST cannot say arms exhaust.
    fn link_branch(&mut self, node: NodeRef, span: Span, else_arms: &[NodeRef]) -> Flow {
        let kids = self.kids(node);
        let arms: Vec<NodeRef> = kids.iter().skip(1).chain(else_arms).copied().collect();
        let mut flow = self.link_arms(span, &arms);
        if else_arms.is_empty() {
            flow.exits.insert(0, flow.entry.expect("link_arms mints the branch node"));
        }
        flow
    }

    /// One Branch node with `arms` off it; the exits are the arms' own.
    fn link_arms(&mut self, span: Span, arms: &[NodeRef]) -> Flow {
        let branch = self.node(span, CfgNodeKind::Branch);
        let mut exits = Vec::new();
        for &arm in arms {
            let flow = self.link(arm);
            if let Some(entry) = flow.entry {
                self.edge(branch, entry, CfgEdgeKind::Arm);
                exits.extend(flow.exits);
            }
        }
        Flow {
            entry: Some(branch),
            exits,
        }
    }

    /// `(C -> T ; E)` parses as `;`(`->`(C, T), E): the `->` is the branch
    /// and E is where its failed condition goes, so the fork mints no node.
    fn link_fork(&mut self, node: NodeRef, span: Span) -> Flow {
        let kids = self.kids(node);
        if let Some((&first, rest)) = kids.split_first() {
            if self.roles_by_node[first.0 as usize] == Some(Branch) {
                return self.link_branch(first, self.span(first), rest);
            }
        }
        let mut flow = self.link_arms(span, &kids);
        if flow.exits.is_empty() {
            flow.exits.push(flow.entry.expect("link_arms mints the fork node"));
        }
        flow
    }

    /// The negated goal is the one arm; the negation falls through whether
    /// the goal succeeds or fails, so it stays in the exit set.
    fn link_negation(&mut self, node: NodeRef, span: Span) -> Flow {
        let negation = self.node(span, CfgNodeKind::Branch);
        let mut exits = vec![negation];
        if let Some(&goal) = self.kids(node).last() {
            let flow = self.link(goal);
            if let Some(entry) = flow.entry {
                self.edge(negation, entry, CfgEdgeKind::Arm);
                exits.extend(flow.exits);
            }
        }
        Flow {
            entry: Some(negation),
            exits,
        }
    }

    /// The guarded body follows the try node; every handler (except, else,
    /// finally) is an arm off it. Which statement raises is not modelled.
    fn link_try(&mut self, node: NodeRef, span: Span) -> Flow {
        let guard = self.node(span, CfgNodeKind::Branch);
        let kids = self.kids(node);
        let mut exits = Vec::new();
        let Some((&body, handlers)) = kids.split_first() else {
            return Flow {
                entry: Some(guard),
                exits: vec![guard],
            };
        };
        let inner = self.link(body);
        match inner.entry {
            Some(entry) => {
                self.edge(guard, entry, CfgEdgeKind::Next);
                exits.extend(inner.exits);
            }
            None => exits.push(guard),
        }
        for &handler in handlers {
            let flow = self.link(handler);
            if let Some(entry) = flow.entry {
                self.edge(guard, entry, CfgEdgeKind::Arm);
                exits.extend(flow.exits);
            }
        }
        Flow {
            entry: Some(guard),
            exits,
        }
    }
}

/// The leading ASCII word of a node's own source text.
fn leading_word(content: &[u8], span: Span) -> &str {
    leading_run(content, span, |byte| byte.is_ascii_alphabetic())
}

/// The leading identifier of a node's own source text: the head or goal name
/// a self-call compares (`walk_2` is one name, not `walk`).
fn leading_name(content: &[u8], span: Span) -> &str {
    leading_run(content, span, |byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn leading_run(content: &[u8], span: Span, keep: impl Fn(u8) -> bool) -> &str {
    let start = (span.start as usize).min(content.len());
    let end = (span.end() as usize).min(content.len());
    let text = &content[start..end];
    let cut = text.iter().position(|byte| !keep(*byte)).unwrap_or(text.len());
    std::str::from_utf8(&text[..cut]).unwrap_or("")
}

/// The operator token of an operation node: the trimmed source text between
/// its first two named children, or before its only child for a prefix form.
fn operator_text<'c>(
    content: &'c [u8],
    span: Span,
    kids: &[NodeRef],
    cst: &FamilyBundle<CstF>,
) -> Option<&'c str> {
    let child_span = |kid: NodeRef| cst.nodes[kid.0 as usize].span;
    let (start, end) = match kids {
        [first, second, ..] => (child_span(*first).end(), child_span(*second).start),
        [only] => (span.start, child_span(*only).start),
        [] => return None,
    };
    let start = (start as usize).min(content.len());
    let end = (end as usize).min(content.len()).max(start);
    std::str::from_utf8(&content[start..end])
        .ok()
        .map(str::trim)
        .filter(|text| !text.is_empty())
}
