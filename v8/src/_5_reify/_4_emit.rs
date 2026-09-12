//! `emit_compiled/4` and `compiler_view/2`. Port of
//! `0_logical_program_reifier.pl` clients in `1_artifact_emitter.pl:22-248`.
//!
//! The built-in Datalog arm returns the checked monomorphic program. A DL7
//! emitter is a type identity connected to one or more output relations by
//! `emits(Emitter, ArtifactName, OutputRelation)` rows (`:30-33`).

use super::api::{CompiledUnit, CompilerView, Emitted, Stop};
use super::calls::{colon_arguments, emit_diagnostic, logical_program_rows_calls};
use super::graph::logical_program_graph_calls;
use super::rows::{checked_parts, logical_program_rows_term};
use super::validate::validate_functional_rows;
use crate::_3_check::api::prolog_sort;
use crate::_3_check::strata::eval_program;
use crate::_6_eval::program::{Program, Row};
use crate::_6_eval::term::{Term, TermId, Universe};
use crate::_6_eval::{evaluate, Trace};

/// `:22-26`.
pub fn compiler_view(u: &mut Universe, unit: &CompiledUnit) -> Result<CompilerView, Stop> {
    let logical_program_rows = logical_program_rows_term(u, unit.runtime_program)?;
    Ok(CompilerView {
        type_graph_facts: unit.type_graph_facts.clone(),
        compiler_facts: unit.compiler_facts.clone(),
        logical_program_rows,
        runtime_program: unit.runtime_program,
    })
}

/// `:34-69`, one variant per clause head.
pub enum Emitter {
    MonomorphicDatalog,
    RelationalProgram,
    /// `:49`. A host-Prolog callable; see the PLAN's Out of scope table.
    Prolog(TermId),
    Dl7(TermId),
    Unknown(TermId),
}

impl Emitter {
    pub fn of(u: &Universe, term: TermId) -> Self {
        if let Term::Atom(s) = u.get(term) {
            return match u.sym_str(*s) {
                "monomorphic_datalog" => Emitter::MonomorphicDatalog,
                "relational_program" => Emitter::RelationalProgram,
                _ => Emitter::Unknown(term),
            };
        }
        if let Some(callable) = u.unary(term, "prolog") {
            return Emitter::Prolog(callable);
        }
        match u.unary(term, "dl7") {
            Some(emitter) => Emitter::Dl7(emitter),
            None => Emitter::Unknown(term),
        }
    }
}

/// `:34-69`.
pub fn emit_compiled(
    u: &mut Universe,
    emitter: &Emitter,
    unit: &CompiledUnit,
) -> Result<Emitted, Stop> {
    match emitter {
        Emitter::MonomorphicDatalog => {
            let name = u.atom("monomorphic_datalog");
            Ok(Emitted {
                artifact: u.compound("artifact", vec![name, unit.runtime_program]),
                diagnostics: vec![],
            })
        }
        Emitter::RelationalProgram => {
            let view = compiler_view(u, unit)?;
            let name = u.atom("relational_program");
            let rows = u.list(&view.logical_program_rows);
            Ok(Emitted {
                artifact: u.compound("artifact", vec![name, rows]),
                diagnostics: vec![],
            })
        }
        Emitter::Prolog(_) => Err(Stop::Fail("prolog emitter: not built yet")),
        Emitter::Dl7(identity) => {
            let view = compiler_view(u, unit)?;
            dl7_emitter_artifact(u, *identity, &view)
        }
        Emitter::Unknown(term) => {
            let reason = u.compound("unknown_emitter", vec![*term]);
            let diagnostic = emit_diagnostic(u, reason);
            Ok(Emitted {
                artifact: u.empty_list(),
                diagnostics: vec![diagnostic],
            })
        }
    }
}

/// `:196-209`, the pattern
/// `call(ref(kernel(':')), [ref(_), const(Name), ref(Relation), const(_)])`.
fn named_relation_id(
    u: &mut Universe,
    compiler_facts: &[TermId],
    name: &str,
) -> Result<TermId, TermId> {
    let wanted = u.atom(name);
    let mut relations = Vec::new();
    for fact in compiler_facts {
        let Some(arguments) = colon_arguments(u, *fact) else {
            continue;
        };
        let (Some(_), Some(found), Some(relation), Some(_)) = (
            u.unary(arguments[0], "ref"),
            u.unary(arguments[1], "const"),
            u.unary(arguments[2], "ref"),
            u.unary(arguments[3], "const"),
        ) else {
            continue;
        };
        if found == wanted {
            relations.push(relation);
        }
    }
    relations.sort_by(|a, b| u.cmp(*a, *b));
    relations.dedup();
    match relations.len() {
        1 => Ok(relations[0]),
        0 => Err(u.compound("emitter_protocol_missing", vec![wanted])),
        _ => {
            let list = u.list(&relations);
            Err(u.compound("emitter_protocol_ambiguous", vec![wanted, list]))
        }
    }
}

/// `artifact_ref(Name, Output)` at `:93`.
struct ArtifactRef {
    name: TermId,
    output: TermId,
}

/// `:80-117`.
fn dl7_emitter_artifact(
    u: &mut Universe,
    emitter: TermId,
    view: &CompilerView,
) -> Result<Emitted, Stop> {
    let emits = match named_relation_id(u, &view.compiler_facts, "emits") {
        Ok(emits) => emits,
        Err(reason) => return Ok(one_diagnostic(u, reason)),
    };
    let refs = artifact_refs(u, emits, emitter, &view.compiler_facts);
    let reasons = artifact_ref_diagnostics(u, emitter, &refs);
    if !reasons.is_empty() {
        let diagnostics = reasons
            .into_iter()
            .map(|reason| emit_diagnostic(u, reason))
            .collect();
        return Ok(Emitted {
            artifact: u.empty_list(),
            diagnostics,
        });
    }

    let mut outputs: Vec<TermId> = refs.iter().map(|r| r.output).collect();
    outputs = prolog_sort(u, outputs);
    let (rows, diagnostics) = dl7_emitter_rows(u, &outputs, view)?;

    // :106-115. The artifact functor is `artifacts/1` even when the diagnostics
    // list is non-empty; the clause head fixes it before they are known.
    let artifacts = if diagnostics.is_empty() {
        refs.iter()
            .map(|r| materialize_artifact_ref(u, r, &rows))
            .collect()
    } else {
        Vec::new()
    };
    let artifacts = u.list(&artifacts);
    Ok(Emitted {
        artifact: u.compound("artifacts", vec![artifacts]),
        diagnostics,
    })
}

fn one_diagnostic(u: &mut Universe, reason: TermId) -> Emitted {
    let diagnostic = emit_diagnostic(u, reason);
    Emitted {
        artifact: u.empty_list(),
        diagnostics: vec![diagnostic],
    }
}

/// `:92-98`.
fn artifact_refs(
    u: &mut Universe,
    emits: TermId,
    emitter: TermId,
    compiler_facts: &[TermId],
) -> Vec<ArtifactRef> {
    let mut terms = Vec::new();
    for fact in compiler_facts {
        let Some((name, parts)) = u.functor(*fact) else {
            continue;
        };
        if name != "call" || parts.len() != 2 {
            continue;
        }
        let (target, arguments) = (parts[0], parts[1]);
        let (Some(relation), Some(arguments)) = (u.unary(target, "ref"), u.as_list(arguments))
        else {
            continue;
        };
        if relation != emits || arguments.len() != 3 {
            continue;
        }
        let (Some(found), Some(name), Some(output)) = (
            u.unary(arguments[0], "ref"),
            u.unary(arguments[1], "const"),
            u.unary(arguments[2], "ref"),
        ) else {
            continue;
        };
        if found != emitter {
            continue;
        }
        terms.push(u.compound("artifact_ref", vec![name, output]));
    }
    let terms = prolog_sort(u, terms);
    terms
        .into_iter()
        .filter_map(|term| {
            let (_, args) = u.functor(term)?;
            Some(ArtifactRef {
                name: args[0],
                output: args[1],
            })
        })
        .collect()
}

/// `:211-229`.
fn artifact_ref_diagnostics(
    u: &mut Universe,
    emitter: TermId,
    refs: &[ArtifactRef],
) -> Vec<TermId> {
    if refs.is_empty() {
        return vec![u.compound("emitter_has_no_outputs", vec![emitter])];
    }
    let mut names = Vec::new();
    for left in refs {
        for right in refs {
            if left.name == right.name && left.output != right.output {
                names.push(left.name);
            }
        }
    }
    let names = prolog_sort(u, names);
    names
        .into_iter()
        .map(|name| u.compound("duplicate_artifact_name", vec![name]))
        .collect()
}

/// `:240-248`.
fn materialize_artifact_ref(u: &mut Universe, artifact: &ArtifactRef, rows: &[TermId]) -> TermId {
    let mut arguments = Vec::new();
    for row in rows {
        let Some((name, parts)) = u.functor(*row) else {
            continue;
        };
        if name != "call" || parts.len() != 2 {
            continue;
        }
        let (target, row_arguments) = (parts[0], parts[1]);
        if u.unary(target, "ref") == Some(artifact.output) {
            arguments.push(row_arguments);
        }
    }
    let arguments = prolog_sort(u, arguments);
    let arguments = u.list(&arguments);
    u.compound("artifact", vec![artifact.name, artifact.output, arguments])
}

/// `:119-157`.
fn dl7_emitter_rows(
    u: &mut Universe,
    outputs: &[TermId],
    view: &CompilerView,
) -> Result<(Vec<TermId>, Vec<TermId>), Stop> {
    let parts = checked_parts(u, view.runtime_program)?;
    let dependencies = output_dependency_closure(u, &parts.rules, outputs);
    let emitter_rules: Vec<TermId> = parts
        .rules
        .iter()
        .copied()
        .filter(|rule| rule_heads_relation(u, *rule, &dependencies))
        .collect();

    let logical = logical_program_rows_calls(
        u,
        &view.compiler_facts,
        &view.logical_program_rows,
        Some(&dependencies),
    )?;
    let graph_calls = logical_program_graph_calls(u, view.runtime_program, Some(&dependencies))?;
    if !logical.diagnostics.is_empty() {
        return Ok((vec![], logical.diagnostics));
    }

    // :142-153
    let mut seeds: Vec<TermId> = Vec::new();
    for source in [
        view.compiler_facts.as_slice(),
        parts.seeds.as_slice(),
        logical.calls.as_slice(),
    ] {
        seeds.extend(source.iter().copied().filter(|call| {
            call_relation(u, *call).is_some_and(|relation| dependencies.contains(&relation))
        }));
    }
    seeds.extend(graph_calls);
    let seeds = prolog_sort(u, seeds);

    let relevant_relations: Vec<TermId> = parts
        .relations
        .iter()
        .copied()
        .filter(|declaration| declaration_has_relation(u, *declaration, &dependencies))
        .collect();

    let (closure, evaluation_diagnostics) = run_emitter_rules(u, &emitter_rules, &seeds)?;
    if !evaluation_diagnostics.is_empty() {
        return Ok((vec![], evaluation_diagnostics));
    }

    // :187-193
    let key_diagnostics = validate_functional_rows(u, &relevant_relations, &closure);
    let rows = if key_diagnostics.is_empty() {
        closure
    } else {
        vec![]
    };
    Ok((rows, key_diagnostics))
}

/// `:154`, `evaluate/4` under the v8 kernel.
fn run_emitter_rules(
    u: &mut Universe,
    rules: &[TermId],
    seeds: &[TermId],
) -> Result<(Vec<TermId>, Vec<TermId>), Stop> {
    let mut program =
        eval_program(u, rules).map_err(|_| Stop::Fail("emitter rules are not checked rules"))?;
    for seed in seeds {
        let Some((relation, arguments)) = call_relation_arguments(u, *seed) else {
            return Err(Stop::Fail("seed call/2 expected"));
        };
        program.seeds.push(Row {
            rel: relation,
            args: arguments,
        });
    }
    let closure = run(u, &program);
    let rows = closure
        .0
        .iter()
        .map(|row| {
            let arguments = u.list(&row.args);
            u.compound("call", vec![row.rel, arguments])
        })
        .collect();
    Ok((rows, closure.1))
}

fn run(u: &mut Universe, program: &Program) -> (Vec<Row>, Vec<TermId>) {
    let mut sink = |_: Trace| {};
    let closure = evaluate(u, program, &mut sink);
    let diagnostics = closure
        .diagnostics
        .iter()
        .map(|d| {
            let phase = u.atom(d.phase);
            let none = u.atom("none");
            u.compound("diagnostic", vec![phase, none, d.payload])
        })
        .collect();
    (closure.rows, diagnostics)
}

/// `call(Relation, Arguments)` with the relation term as written.
fn call_relation_arguments(u: &Universe, call: TermId) -> Option<(TermId, Vec<TermId>)> {
    let (name, parts) = u.functor(call)?;
    if name != "call" || parts.len() != 2 {
        return None;
    }
    let (relation, arguments) = (parts[0], parts[1]);
    Some((relation, u.as_list(arguments)?))
}

/// `:163-164`, the `ref/1` payload of a `call/2` row.
fn call_relation(u: &Universe, call: TermId) -> Option<TermId> {
    let (relation, _) = call_relation_arguments(u, call)?;
    u.unary(relation, "ref")
}

/// `:166-167`.
fn declaration_has_relation(u: &Universe, declaration: TermId, relations: &[TermId]) -> bool {
    let Some(("relation", args)) = u.functor(declaration) else {
        return false;
    };
    if args.len() != 3 {
        return false;
    }
    u.unary(args[0], "ref")
        .is_some_and(|relation| relations.contains(&relation))
}

/// `:184-185`.
fn rule_heads_relation(u: &Universe, rule: TermId, relations: &[TermId]) -> bool {
    rule_head_relation(u, rule).is_some_and(|relation| relations.contains(&relation))
}

fn rule_head_relation(u: &Universe, rule: TermId) -> Option<TermId> {
    let (name, parts) = u.functor(rule)?;
    if name != "rule" || parts.len() != 2 {
        return None;
    }
    call_relation(u, parts[0])
}

/// `:169-182`. Least fixpoint over the body relations of the rules that write
/// a relation already in the set.
fn output_dependency_closure(
    u: &mut Universe,
    rules: &[TermId],
    outputs: &[TermId],
) -> Vec<TermId> {
    let mut relations = outputs.to_vec();
    loop {
        let mut next = relations.clone();
        for rule in rules {
            let Some(head) = rule_head_relation(u, *rule) else {
                continue;
            };
            if !relations.contains(&head) {
                continue;
            }
            let Some(("rule", parts)) = u.functor(*rule) else {
                continue;
            };
            let Some(goals) = u.as_list(parts[1]) else {
                continue;
            };
            for goal in goals {
                let Some(("checked_goal", goal_parts)) = u.functor(goal) else {
                    continue;
                };
                if goal_parts.len() != 2 {
                    continue;
                }
                if let Some(body) = call_relation(u, goal_parts[1]) {
                    next.push(body);
                }
            }
        }
        let next = prolog_sort(u, next);
        if next == relations {
            return next;
        }
        relations = next;
    }
}
