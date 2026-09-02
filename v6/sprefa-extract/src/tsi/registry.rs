//! The relation table, DATA rather than code: `--schema` prints these rows and
//! `--ingest` validates against them. A relation absent here is a named stop.

use super::types::Arg;

/// The five argument shapes the wire spells: `{"id"}`, `{"span"}`, `{"text"}`,
/// `{"int"}`, `{"atom"}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgKind {
    Id,
    Span,
    Text,
    Int,
    Atom,
}

impl ArgKind {
    /// The wire's own spelling, which is the word a mismatch names.
    pub const fn word(self) -> &'static str {
        match self {
            ArgKind::Id => "id",
            ArgKind::Span => "span",
            ArgKind::Text => "text",
            ArgKind::Int => "int",
            ArgKind::Atom => "atom",
        }
    }

    /// The kind an argument carries.
    pub const fn of(arg: &Arg) -> Self {
        match arg {
            Arg::Id(_) => ArgKind::Id,
            Arg::Span(_, _, _) => ArgKind::Span,
            Arg::Text(_) => ArgKind::Text,
            Arg::Int(_) => ArgKind::Int,
            Arg::Atom(_) => ArgKind::Atom,
        }
    }
}

/// One registry row. `args` is the arity and the kinds at once.
pub struct Relation {
    pub name: &'static str,
    pub args: &'static [ArgKind],
}

use ArgKind::{Atom, Id, Int, Span, Text};

/// A `const` slice: no allocation, no map built per lookup. Trailing comments
/// name each argument's role, which the kinds alone cannot show.
pub const REGISTRY: &[Relation] = &[
    Relation {
        name: "tsi.type",
        args: &[Id],
    },
    /// A declaration symbol's own id: the declaring row for the symbol
    /// positions of `tsi.denotes` and `rust.impl`, as `tsi.type` is for types.
    Relation {
        name: "tsi.symbol",
        args: &[Id],
    },
    Relation {
        name: "tsi.denotes",
        args: &[Id, Id], // symbol, type
    },
    /// Optional bridge from a run-local symbol id to the SCIP symbol text,
    /// present only when a SCIP index also ran; identity stays run-local.
    Relation {
        name: "tsi.scip_symbol",
        args: &[Id, Text], // symbol, scip symbol text
    },
    /// A value entity with its type; the declaring row for value ids.
    /// `tsi.argument` stays type-only; a value in argument position goes
    /// through `tsi.value_argument`.
    Relation {
        name: "tsi.value",
        args: &[Id, Id], // value, type
    },
    Relation {
        name: "tsi.value_argument",
        args: &[Id, Int, Id], // argument list, position, value
    },
    Relation {
        name: "tsi.has_type",
        args: &[Span, Id], // occurrence range, type
    },
    Relation {
        name: "tsi.origin",
        args: &[Id, Atom, Span], // type, language, declaration range
    },
    Relation {
        name: "tsi.product",
        args: &[Id],
    },
    Relation {
        name: "tsi.sum",
        args: &[Id],
    },
    Relation {
        name: "tsi.callable",
        args: &[Id],
    },
    Relation {
        name: "tsi.primitive",
        args: &[Id, Atom], // type, class
    },
    Relation {
        name: "tsi.edge",
        args: &[Id, Id, Text, Id, Int], // edge, owner, label, target, position
    },
    Relation {
        name: "tsi.parameter",
        args: &[Id, Id, Int, Atom], // param, callee, position, variance
    },
    Relation {
        name: "tsi.called",
        args: &[Id, Id, Id], // result, callee, argument list
    },
    Relation {
        name: "tsi.argument",
        args: &[Id, Int, Id], // list, position, type
    },
    Relation {
        name: "tsi.input",
        args: &[Id, Int, Id], // callable, position, type
    },
    Relation {
        name: "tsi.output",
        args: &[Id, Int, Id], // callable, position, type
    },
    Relation {
        name: "tsi.subtype",
        args: &[Id, Id, Atom],
    },
    Relation {
        name: "tsi.assignable",
        args: &[Id, Id, Atom],
    },
    Relation {
        name: "tsi.conforms",
        args: &[Id, Id, Atom],
    },
    Relation {
        name: "tsi.equivalent",
        args: &[Id, Id, Atom],
    },
    Relation {
        name: "ts.interface",
        args: &[Id],
    },
    Relation {
        name: "ts.conditional",
        args: &[Id, Id, Id, Id, Id], // result, check, extends, true, false
    },
    Relation {
        name: "ts.mapped",
        args: &[Id, Id, Id, Id], // result, key param, constraint, template
    },
    Relation {
        name: "ts.readonly",
        args: &[Id], // edge
    },
    Relation {
        name: "ts.optional",
        args: &[Id], // edge
    },
    Relation {
        name: "rust.trait",
        args: &[Id],
    },
    Relation {
        name: "rust.impl",
        args: &[Id, Id, Id], // impl symbol, type, trait
    },
    Relation {
        name: "rust.lifetime",
        args: &[Id, Atom], // param, name
    },
    Relation {
        name: "rust.ownership",
        args: &[Id, Atom], // edge, shared|exclusive|owned
    },
    Relation {
        name: "rust.assoc",
        args: &[Id, Text, Id], // owner, name, target
    },
    Relation {
        name: "go.interface",
        args: &[Id],
    },
    Relation {
        name: "go.type_set",
        args: &[Id, Id],
    },
    Relation {
        name: "go.embedding",
        args: &[Id, Id],
    },
];

/// Linear scan over the table. The row count is small enough that a hash map
/// would cost more to build than the scan costs to run.
pub fn relation(name: &str) -> Option<&'static Relation> {
    REGISTRY.iter().find(|row| row.name == name)
}

/// The check both doors run: the sink's `debug_assert!` and the reverse door's
/// per-line validation. `Err` carries the detail a caller reports verbatim.
pub fn check(name: &str, args: &[Arg]) -> Result<(), String> {
    let Some(row) = relation(name) else {
        return Err("not in registry".to_string());
    };
    if args.len() != row.args.len() {
        return Err(format!(
            "arity {}, the registry says {}",
            args.len(),
            row.args.len()
        ));
    }
    for (position, (given, want)) in args.iter().zip(row.args).enumerate() {
        let got = ArgKind::of(given);
        if got != *want {
            return Err(format!(
                "argument at position {position} is {}, the registry says {}",
                got.word(),
                want.word()
            ));
        }
    }
    Ok(())
}
