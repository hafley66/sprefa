//! S3: the per-family output rows. `Node<F>` / `Edge<F>` / `FamilyBundle<F>` are
//! generic over the family marker; the kind vocabularies come from
//! `F::NodeKind` / `F::EdgeKind`. No `NodeKind`/`EdgeKind` sum; the family lives
//! in the type, not in a discriminant field.

use std::marker::PhantomData;

use crate::family::Family;
use crate::shape::{NameId, NodeRef, Span};

/// One located, kinded thing in a file. Identity = `(family, span, kind)`; the
/// engine seam interns a node by that triple under a store-assigned key. `name`
/// is the optional bare identifier for resolution joins, NOT the identity.
#[derive(Clone, Debug)]
pub struct Node<F: Family> {
    pub span: Span,
    pub kind: F::NodeKind,
    pub name: Option<NameId>,
    _f: PhantomData<fn() -> F>,
}

impl<F: Family> Node<F> {
    pub fn new(span: Span, kind: F::NodeKind) -> Self {
        Self { span, kind, name: None, _f: PhantomData }
    }

    pub fn with_name(mut self, name: NameId) -> Self {
        self.name = Some(name);
        self
    }
}

/// One resolved relationship between two nodes in the same file. `src`/`dst` are
/// local `NodeRef`s into the producing file's node vec; the wire flattens both
/// to spans so `NodeRef` never crosses the seam.
#[derive(Clone, Copy, Debug)]
pub struct Edge<F: Family> {
    pub src: NodeRef,
    pub dst: NodeRef,
    pub kind: F::EdgeKind,
    _f: PhantomData<fn() -> F>,
}

impl<F: Family> Edge<F> {
    pub fn new(src: NodeRef, dst: NodeRef, kind: F::EdgeKind) -> Self {
        Self { src, dst, kind, _f: PhantomData }
    }
}

/// One family's output for one file. The dispatch produces a bundle per masked
/// family; the wire flattens (bundle, strings) -> flat facts. `aux` carries the
/// family's side-channel payload (TypeF arrow-type sigs; later df param_pos/
/// args/...): per-node attributes that are not span-pair edges.
#[derive(Clone, Debug)]
pub struct FamilyBundle<F: Family> {
    pub nodes: Vec<Node<F>>,
    pub edges: Vec<Edge<F>>,
    pub aux: F::Aux,
}

impl<F: Family> Default for FamilyBundle<F> {
    fn default() -> Self {
        Self { nodes: Vec::new(), edges: Vec::new(), aux: F::Aux::default() }
    }
}

impl<F: Family> FamilyBundle<F> {
    pub fn node(&self, r: NodeRef) -> &Node<F> {
        &self.nodes[r.0 as usize]
    }
}
