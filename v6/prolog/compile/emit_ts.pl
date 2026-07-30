% emit_ts.pl : prints a lowered/8 term as a literal TypeScript module
% conforming to IGenProgram (plans/2026-07-27-tsv2-compile-target-header.md,
% ADDENDUM). P1 emits two execution families. The default incremental family
% stages one tick's effective changes in indexed TEMP tables, runs inline
% delta-side joins for positive non-recursive level rules, and reads boundary
% changes from those tables. The recompute family remains available through
% SPREFA_TSV2_EMITTER_MODE=naive and remains the fallback for negative
% bodies and retraction ticks. P2 carries positive rule frontiers across
% drain ticks in per-relation current/next TEMP tables.
%
% Table-driven, not hand-unrolled: every rel's snapshot read / arrival
% statement / delta entry is one row in a compile-time array or record
% literal, the more "generated code" shape for a compiler meant to scale
% past two or three rels. The one genuinely dynamic-length pieces are
% `arrivals` itself (a runtime value: `.map()`/`.filter()` over it is plain
% array code, "sync stays sync") and the edge-write resolution's `forkJoin`
% over one query per matching arrival row (a real sequential IO fan-out,
% legitimately rx-shaped).
%
% SEAM NAMES: IGenProgram, ISqlSeam, IArrivalBatch, IArrivalRow, IRow,
% IRowValue, ITickDeltas, SqlStatement all imported from
% "../runtime/types.ts" (real file, merged). `IBootStatement` is a LOCAL
% type (the header's IGenProgram has no boot slot; "extend by adding
% fields, never renaming" -- v6/tsv2/scripts/run-emitted.ts confirms who
% runs it and when: after DDL, before the tick fold).

% The four extra exports are the EMITTER MODE seam (rank R8 of
% plans/2026-07-29-prolog-org-review.md). The incremental_mode unit in
% compile/test/plunit_tests.pl asserts which statement family a plan compiles
% to, and it used to reach these as private qualified goals, which
% `just prolog-lint` refuses. They are a real contract, not a test hole: each
% answers a yes/no question about a plan that the emitted module's SHAPE
% depends on, and stating them here makes that contract checkable.
:- module(emit_ts,
          [ emit_program/5,
            incremental_program_safe/4,
            reconcile_every_tick/2,
            derived_edge_carry_required/3,
            retraction_guard/2 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
% relplan_kind/3 (Ref -> log|set): PHASE C2 RULING 2's per-arm resolver needs
% the TRIGGER's kind (to choose the log-unconditional vs set-dedup branch of
% the emitted triggerOccurrences call), and RelPlans is available straight
% off the Lowered term this module already renders -- reused, not
% reimplemented.
:- use_module(lower, [ relplan_kind/3, departure_frontier_table_name/2,
                       departure_read_sql/3, struct_type_plans/2 ]).
:- use_module(analyze,
              [ body_ref_uses/2, derived_refs/2, rule_head_ref/2,
                program_uses_tick/2, listened_departure_refs/2,
                level_body_pre_ref/2 ]).
:- use_module('../1_host_expand', [compile_host_decl/2]).
:- use_module(registry, [bind_executor/2, host_execution/3]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

% ═══ small text helpers ══════════════════════════════════════════════════════

lines_block(Lines, Text) :- atomic_list_concat(Lines, '\n', Text).

js_template(SqlText, JsLiteral) :-
    atom_string(SqlText, SqlString0),
    re_replace("`"/g, "\\`", SqlString0, SqlString1),
    re_replace("\\$\\{"/g, "\\${", SqlString1, SqlString2),
    format(atom(JsLiteral), '`~w`', [SqlString2]).

js_string(Value, JsLiteral) :-
    ( atom(Value) -> atom_codes(Value, Codes) ; string_codes(Value, Codes) ),
    js_string_codes(Codes, Escaped),
    atom_codes(Body, Escaped),
    format(atom(JsLiteral), '"~w"', [Body]).

js_string_codes([], []).
js_string_codes([0'" | Rest], [0'\\, 0'" | More]) :-
    !,
    js_string_codes(Rest, More).
js_string_codes([0'\\ | Rest], [0'\\, 0'\\ | More]) :-
    !,
    js_string_codes(Rest, More).
js_string_codes([0'\n | Rest], [0'\\, 0'n | More]) :-
    !,
    js_string_codes(Rest, More).
js_string_codes([0'\r | Rest], [0'\\, 0'r | More]) :-
    !,
    js_string_codes(Rest, More).
js_string_codes([0'\t | Rest], [0'\\, 0't | More]) :-
    !,
    js_string_codes(Rest, More).
js_string_codes([Code | Rest], [Code | More]) :-
    js_string_codes(Rest, More).

ref_name(Name/_Arity, Name).

upper_snake(Name/_Arity, Upper) :- upcase_atom(Name, Upper).

% snake_case -> PascalCase, for JS function/type names built by combining a
% rel name with a fixed prefix/suffix (e.g. "resolve" + "OpenScope" +
% "Writes") -- SCREAMING_SNAKE (upper_snake/2) is for SQL CONSTANT names,
% where it is idiomatic; mixing it into a camelCase function name
% (`resolveOPEN_SCOPEWrites`) is not.
pascal_case(Name/_Arity, Pascal) :- !, pascal_case(Name, Pascal).
pascal_case(Name, Pascal) :-
    atomic_list_concat(Parts, '_', Name),
    maplist(capitalize_atom, Parts, CapitalizedParts),
    atomic_list_concat(CapitalizedParts, Pascal).

capitalize_atom(Atom, Capitalized) :-
    atom_codes(Atom, [First | Rest]),
    code_type(UpperFirst, to_upper(First)),
    atom_codes(Capitalized, [UpperFirst | Rest]).

param_text(bool_lit(Boolean), Text) :- !, format(atom(Text), '~w', [Boolean]).
param_text(Param, Text) :- number(Param), !, format(atom(Text), '~w', [Param]).
param_text(Param, Text) :- js_string(Param, Text).

params_array_text(Params, Text) :-
    maplist(param_text, Params, ParamTexts),
    atomic_list_concat(ParamTexts, ', ', Joined),
    format(atom(Text), '[~w]', [Joined]).

quoted_string_array_text(Atoms, Text) :-
    maplist(js_string, Atoms, Quoted),
    atomic_list_concat(Quoted, ', ', Joined),
    format(atom(Text), '[~w]', [Joined]).

% Flattens a list of line-groups into one line list, one blank line between
% groups (never trailing).
flatten_with_blank_separators([], []).
flatten_with_blank_separators([Group], Group) :- !.
flatten_with_blank_separators([Group | Rest], Lines) :-
    flatten_with_blank_separators(Rest, RestLines),
    append(Group, [''], WithBlank),
    append(WithBlank, RestLines, Lines).

% ═══ header comment ══════════════════════════════════════════════════════════

header_lines(Name, Lines) :-
    format(atom(TitleLine), '// GENERATED by v6/prolog/compile (tsv2 emitter P3). Do not', []),
    format(atom(NameLine), '// hand-edit; recompile. Program: ~w.', [Name]),
    Lines =
    [ TitleLine, NameLine,
      '// Compiles the reference engine\'s occurrence / keyed-replace / boundary-diff',
      '// semantics (engine.pl) to SQLite + the real v6/tsv2 runtime seam, not',
      '// lower/lowerSql.ts\'s.',
      '//',
      '// The default path stages effective tick changes in indexed TEMP tables,',
      '// executes emitted frontier-side joins for positive level rules, promotes',
      '// edge and post-write level growth across drain ticks, and computes boundary',
      '// changes from the staged stream. Retractions and negative bodies use emitted',
      '// support-count reconciliation. The snapshot path remains selectable with',
      '// SPREFA_TSV2_EMITTER_MODE=naive as a byte-identity referee.',
      '//',
      '// IGenProgram has no slot for boot-time work (seeding Initial rows before',
      '// tick 1). `boot` is an extra field added beyond the five pinned names',
      '// ("extend by adding fields, never renaming"); v6/tsv2/scripts/',
      '// run-emitted.ts (the reconciliation runner) runs it after DDL and before',
      '// the tick fold.'
    ].

% ═══ imports ═════════════════════════════════════════════════════════════════

imports_lines(HasEdgeRules, HasRetention, Lines) :-
    imports_lines(HasEdgeRules, HasRetention, false, false, Lines).

imports_lines(_HasEdgeRules, HasRetention, HasStructTypes, HasOrderedProgram,
              Lines) :-
    ( HasRetention == true
    -> RetentionImport = ['  IIncrementalRetentionStatement,']
    ; RetentionImport = []
    ),
    ( HasOrderedProgram == true
    -> RuntimeImport =
       'import { IncrementalRuntime, stageOrderedFrontiers } from "../runtime/1_incremental.ts";'
    ;  RuntimeImport =
       'import { IncrementalRuntime } from "../runtime/1_incremental.ts";'
    ),
    ( HasStructTypes == true
    -> StructImport = ['import { StructPlane } from "../runtime/structPlane.ts";'],
       StructTypeImports = ['  IStructRefColumns,', '  IStructTypePlan,']
    ;  StructImport = [], StructTypeImports = []
    ),
    append(
    [ [ 'import { concatMap, forkJoin, map, of, type Observable } from "rxjs";',
      '',
      RuntimeImport,
      'import { multisetDiff } from "../runtime/diff.ts";',
      'import { selectRows } from "../runtime/rows.ts";'
      ],
      StructImport,
      [ 'import type {',
      '  IArrivalBatch,',
      '  IArrivalRow,',
      '  IGenProgram,',
      '  IIncrementalEdgeStatement,',
      '  IIncrementalLevelStatement,',
      '  IIncrementalProgramPlan,',
      '  IIncrementalRelationPlan,'
      ],
      RetentionImport,
      [
      '  IRelDelta,',
      '  IRow,',
      '  IRowColumnType,',
      '  IRowValue,',
      '  ISqlSeam,'
      ],
      StructTypeImports,
      [
      '  ITickDeltas,',
      '  SqlStatement,',
      '} from "../runtime/types.ts";'
      ]
    ], Lines).

% ═══ the declared value plane (STRUCT-AS-ROWS) ══════════════════════════════
% Emitted ONLY for a program that declares a type. Every other module stays
% byte-identical to what it was before this arc: no import line, no constant,
% no wrapper -- the storage plane costs exactly nothing where it is unused,
% and the sweep's byte-identity discipline is what checks that claim.

struct_plane_lines([], _, [], false) :- !.
struct_plane_lines(StructPlans, RelPlans, Lines, true) :-
    maplist(struct_type_plan_line, StructPlans, PlanLines),
    struct_ref_column_entries(RelPlans, RefEntryLines),
    append(
    [ [ 'export const STRUCT_TYPES: readonly IStructTypePlan[] = [' ],
      PlanLines,
      [ '];',
        '',
        'export const STRUCT_REF_COLUMNS: IStructRefColumns = {' ],
      RefEntryLines,
      [ '};' ]
    ], Lines).

struct_type_plan_line(structtype(TypeName, Columns, RefTypes, KeyIndices,
                                ConflictSql, InternSql, LookupSql), Line) :-
    js_string(TypeName, NameText),
    maplist(js_string, Columns, ColumnTexts),
    atomic_list_concat(ColumnTexts, ', ', ColumnsText),
    maplist(struct_ref_entry, RefTypes, RefTexts),
    atomic_list_concat(RefTexts, ', ', RefsText),
    atomic_list_concat(KeyIndices, ', ', KeyIndicesText),
    js_template(ConflictSql, ConflictTemplate),
    js_template(InternSql, InternTemplate),
    js_template(LookupSql, LookupTemplate),
    format(atom(Line),
           '  { name: ~w, columns: [~w], refs: [~w], keyIndices: [~w], conflictSql: ~w, internSql: ~w, lookupSql: ~w },',
           [NameText, ColumnsText, RefsText, KeyIndicesText,
            ConflictTemplate, InternTemplate, LookupTemplate]).

struct_ref_entry(none, 'null') :- !.
struct_ref_entry(TypeName, Text) :- js_string(TypeName, Text).

struct_ref_column_entries(RelPlans, Lines) :-
    findall(Line,
            ( member(relplan(Ref, _, _, _, ColumnTypes), RelPlans),
              memberchk(ref(_), ColumnTypes),
              ref_name(Ref, Name), js_string(Name, NameText),
              maplist(column_type_ref_entry, ColumnTypes, RefTexts),
              atomic_list_concat(RefTexts, ', ', RefsText),
              format(atom(Line), '  ~w: [~w],', [NameText, RefsText]) ),
            Lines).

column_type_ref_entry(ref(TypeName), Text) :- !, js_string(TypeName, Text).
column_type_ref_entry(_, 'null').

% Relation references normalize inside each emitter mode after that mode has
% opened its tick boundary. Target rows pass through the same arrival
% applicator as authored rows, then parent fields carry the resolved integer
% endpoints. No second externally visible tick or reference-value runtime
% exists.
struct_tick_wrapper_lines(_, _, []).

naive_reference_normalize_lines(false, []) :- !.
naive_reference_normalize_lines(true,
    [ '    concatMap((before) => StructPlane.intern(seam, STRUCT_TYPES, STRUCT_REF_COLUMNS, arrivals,',
      '      (targets) => applyArrivals(seam, targets),',
      '    ).pipe(map((normalized) => { arrivals = normalized; return before; }))),'
    ]).

incremental_reference_normalize_lines(false, []) :- !.
incremental_reference_normalize_lines(true,
    [ '    concatMap(() => StructPlane.intern(seam, STRUCT_TYPES, STRUCT_REF_COLUMNS, arrivals,',
      '      (targets) => IncrementalRuntime.applyArrivals(seam, targets, INCREMENTAL_RELATIONS),',
      '    ).pipe(map((normalized) => { arrivals = normalized; }))),'
    ]).
    % `of` covers two zero-op shapes, not just the edge-rule forkJoin([])
    % guard it was originally added for: an edge-free tick still needs it for
    % edge_resolver_block/3's `of([])` when EdgeStatements is nonempty, AND
    % recompute_levels_fn_lines/2's no-level-rules fallback below needs
    % `of(undefined)`. Always importing it is simpler than tracking two
    % independent conditions, and tsconfig.json carries no
    % noUnusedLocals/noUnusedParameters flag, so an unused import is not a
    % typecheck error on the (rare) program shape that needs neither.

% ═══ local supporting types ══════════════════════════════════════════════════

local_types_lines(
    [ 'interface IHostColumnPlan { readonly name: string; readonly type: string }',
      'interface IHostPlanData { readonly name: string; readonly inputs: readonly IHostColumnPlan[]; readonly outputs: readonly IHostColumnPlan[]; readonly template: string; readonly demandRel: string; readonly responseRel: string; readonly execution: string }',
      'interface IBindPlanData { readonly name: string; readonly columns: readonly IHostColumnPlan[]; readonly literals: readonly IRowValue[]; readonly execution: string }',
      'interface IQueryPlanData { readonly rel: string; readonly arity: number; readonly snapshot: "current" }',
      '',
      'interface IBootStatement {',
      '  sql: string;',
      '  params: readonly IRowValue[];',
      '}',
      '',
      'type IGenProgramWithBoot = IGenProgram & { readonly boot: readonly IBootStatement[]; readonly finalSelect: Record<string, string>; readonly hostPlans: readonly IHostPlanData[]; readonly bindPlans: readonly IBindPlanData[]; readonly queryPlans: readonly IQueryPlanData[]; readonly unsupportedExecution: readonly string[] };'
    ]).

world_plan_lines(plan(_, prog(Decls, Rules), _, _, _, _), Lines) :-
    findall(HostPlan,
            ( member(Decl, Decls),
              Decl = sh_decl(_, _, _, _),
              compile_host_decl(Decl, HostPlan)
            ),
            HostPlans),
    findall(bind_plan(Name, Columns, Literals),
            ( member(bind_decl(Name, Columns), Decls),
              bind_read_literals(Rules, Name, Columns, Literals)
            ),
            BindPlans),
    findall(query_plan(Name/Arity, snapshot(current)),
            ( member(query(Atom), Decls),
              functor(Atom, Name, Arity)
            ),
            QueryPlans),
    maplist(host_plan_json, HostPlans, HostRows),
    maplist(bind_plan_json, BindPlans, BindRows),
    maplist(query_plan_json, QueryPlans, QueryRows),
    % PHASE 2 (plans/2026-07-29-runtime-bridge-header.md): sh hosts and the
    % interval bind EXECUTE in the served runtime, so neither emits a refusal
    % row any more. The const and its slot stay: a future world term with no
    % executor names itself here rather than executing silently.
    Refusals = [],
    array_const_line('export const hostPlans: readonly IHostPlanData[]', HostRows,
                     HostLine),
    array_const_line('export const bindPlans: readonly IBindPlanData[]', BindRows,
                     BindLine),
    array_const_line('export const queryPlans: readonly IQueryPlanData[]', QueryRows,
                     QueryLine),
    array_const_line('export const unsupportedExecution: readonly string[]',
                     Refusals, RefusalLine),
    Lines = [HostLine, BindLine, QueryLine, RefusalLine].

array_const_line(Prefix, Rows, Line) :-
    atomic_list_concat(Rows, ', ', Body),
    format(atom(Line), '~w = [~w];', [Prefix, Body]).

host_plan_json(
    host_plan(Name, Inputs, Outputs, template(Template),
              demand_ref(DemandName), response_ref(ResponseName), _),
    Json) :-
    js_string(Name, NameJson),
    host_columns_json(Inputs, InputsJson),
    host_columns_json(Outputs, OutputsJson),
    js_string(Template, TemplateJson),
    js_string(DemandName, DemandJson),
    js_string(ResponseName, ResponseJson),
    host_execution(Name, Template, Executor),
    format(atom(Json),
           '{ name: ~w, inputs: ~w, outputs: ~w, template: ~w, demandRel: ~w, responseRel: ~w, execution: "~w" }',
           [NameJson, InputsJson, OutputsJson, TemplateJson,
            DemandJson, ResponseJson, Executor]).

bind_plan_json(bind_plan(Name, Columns, Literals), Json) :-
    js_string(Name, NameJson),
    host_columns_json(Columns, ColumnsJson),
    maplist(bind_literal_json, Literals, LiteralRows),
    atomic_list_concat(LiteralRows, ', ', LiteralBody),
    format(atom(LiteralsJson), '[~w]', [LiteralBody]),
    ( bind_executor(Name, Executor)
    -> true
    ; throw(bind_mismatch(Name, Columns))
    ),
    format(atom(Json),
           '{ name: ~w, columns: ~w, literals: ~w, execution: "~w" }',
           [NameJson, ColumnsJson, LiteralsJson, Executor]).

bind_literal_json(Literal, Json) :-
    ( integer(Literal) -> format(atom(Json), '~w', [Literal])
    ; js_string(Literal, Json)
    ).

% ═══ bind configuration literals (phase 2) ═══════════════════════════════════
% A bind declaration authorizes a world source; the PROGRAM'S OWN RULES say
% which instances of it they consume, as LITERALS in the first column of a body
% atom naming the bind. Column 1 is the configuration column for every bind
% (registry.pl `bind_definition/2` header): `interval(300, Bucket)` asks for a
% 300-second cadence, `watch("src/**/*.ts", Path, Digest)` asks for one watcher
% over that glob. Those literals are the only statement anywhere in the program
% about what the world should push, so they are exactly what the served runtime
% starts -- a program that declares a bind and reads no literal gets no live
% source at all, and says so by emitting an empty list.
%
% This was `bind_read_periods/4` while `interval` was the only bind; the shape
% generalized when `watch` arrived (a glob is a string, not an integer), so the
% type filter widened from integer/1 to "atomic and not a variable". Non-literal
% first columns (a variable, a compound) contribute nothing: a program cannot
% ask the world for a cadence it only computes.
%
% The scan is over the WHOLE rule term (head and body alike): a rule that heads
% the bind rel is already refused at load (bind_and_rule_head), so every
% occurrence reachable here is a read.
bind_read_literals(Rules, Name, Columns, Literals) :-
    length(Columns, Arity),
    findall(Literal,
            ( bind_subterm(Rules, Atom),
              compound(Atom),
              functor(Atom, Name, Arity),
              arg(1, Atom, Literal),
              bind_config_literal(Literal)
            ),
            Raw),
    sort(Raw, Literals).

bind_config_literal(Literal) :-
    nonvar(Literal),
    ( integer(Literal) -> true
    ; string(Literal)  -> true
    ; atom(Literal), Literal \== []
    ).

bind_subterm(Term, Term) :-
    nonvar(Term).
bind_subterm(Term, Sub) :-
    nonvar(Term),
    compound(Term),
    arg(_, Term, Argument),
    bind_subterm(Argument, Sub).

query_plan_json(query_plan(Name/Arity, snapshot(current)), Json) :-
    js_string(Name, NameJson),
    format(atom(Json), '{ rel: ~w, arity: ~w, snapshot: "current" }',
           [NameJson, Arity]).

host_columns_json(Columns, Json) :-
    maplist(host_column_json, Columns, Rows),
    atomic_list_concat(Rows, ', ', Body),
    format(atom(Json), '[~w]', [Body]).

host_column_json(col(Name, Type), Json) :-
    js_string(Name, NameJson),
    js_string(Type, TypeJson),
    format(atom(Json), '{ name: ~w, type: ~w }', [NameJson, TypeJson]).

% ═══ integer bind helper (phase C sweep finding) ═══════════════════════════
% @libsql/client binds a JS `number` parameter as SQLite REAL, not INTEGER --
% verified empirically (a bound `1` lands as the TEXT value "1.0" in a
% TEXT-affinity column, `1n` lands as "1"), a driver behavior, not a bug in
% any SQL text this compiler emits. A TEXT column (lower.pl:column_def/3)
% therefore needs an integer-valued arrival or edge-projected value to cross
% the driver seam as a bigint to keep its digit-for-digit text form; an
% INTEGER column (PHASE C2 RULING 1) round-trips correctly either way
% (verified empirically: both a bound bigint and a bound plain number land
% as SQLite INTEGER storage and read back as a JS number), so applying this
% unconditionally to every bind is simplest and correct for both column
% types. A genuine fractional number (none in this compiler's corpus today)
% passes through unchanged. Used everywhere a raw IRow (an arrival's own
% row, or an edge projection's numbered-placeholder bind) becomes
% SqlStatement args.

% The return type is NOT `readonly` -- @libsql/client's own `InArgs` is
% `InValue[]`, a mutable array type, so a `readonly` array here would fail
% typecheck at every call site despite being correct at runtime (`.map()`
% already returns a fresh mutable array; the annotation was simply too
% narrow).
bind_args_helper_lines(
    [ 'function bindArgs(values: readonly IRowValue[]): (string | number | bigint)[] {',
      '  return values.map((value) => typeof value === "boolean" ? BigInt(value ? 1 : 0) : (typeof value === "number" && Number.isInteger(value) ? BigInt(value) : value));',
      '}'
    ]).

arrival_value_guard_lines(
    [ 'function validateArrivals(arrivals: IArrivalBatch): IArrivalBatch {',
      '  return arrivals.map((arrival): IArrivalRow => {',
      '    const types = relColumnTypes[arrival.rel];',
      '    if (types === undefined || types.length !== arrival.row.length) throw new Error(`arrival shape mismatch for ${arrival.rel}`);',
      '    const row = arrival.row.map((value, index): IRowValue => {',
      '      const type = types[index];',
      '      if (type === "bool") {',
      '        if (typeof value !== "boolean") throw new Error(`bool arrival ${arrival.rel}[${index}] requires true or false`);',
      '        return value;',
      '      }',
      '      if (type === "float") {',
      '        if (typeof value !== "number" || !Number.isFinite(value)) throw new Error(`float arrival ${arrival.rel}[${index}] requires a finite number`);',
      '        return Object.is(value, -0) ? 0 : value;',
      '      }',
      '      return value;',
      '    });',
      '    return { ...arrival, row };',
      '  });',
      '}'
    ]).

% ═══ trigger occurrence helper (PHASE C2 RULING 2) ══════════════════════════
% engine.pl's absorb_arrivals/8 (r7 + q1): a Log rel's `+Row` arrival is
% UNCONDITIONALLY a trigger occurrence (duplicates stack, r7 "one +Row per
% new stamp"); a Set rel's `+Row` arrival is a trigger occurrence ONLY when
% the row was not already present -- an exact-duplicate add is dedup, not a
% new occurrence, and mints no trigger. The dedup check is PROGRESSIVE
% across one tick's own arrival list (engine.pl folds Store0 forward through
% absorb_arrivals/8's own recursion), not just against the tick-start
% snapshot: two identical `+Row` arrivals to a Set rel in the SAME tick
% occurrence-fire only the FIRST. `beforeRows` is the tick-start snapshot
% (runTick's own `before`, captured by readSnapshot before this tick's
% applyArrivals runs); `seen` starts from it and grows as earlier arrivals
% in THIS tick are folded in, exactly mirroring that recursion.
trigger_occurrences_helper_lines(
    [ 'function triggerOccurrences(',
      '  kind: "log" | "set",',
      '  relName: string,',
      '  beforeRows: readonly IRow[],',
      '  arrivals: IArrivalBatch,',
      '): IArrivalBatch {',
      '  if (kind === "log") return arrivals.filter((arrival) => arrival.rel === relName && arrival.sign === "add");',
      '  const seen = new Set<string>(beforeRows.map((row) => JSON.stringify(row)));',
      '  const occurrences: IArrivalRow[] = [];',
      '  for (const arrival of arrivals) {',
      '    if (arrival.rel !== relName || arrival.sign !== "add") continue;',
      '    const key = JSON.stringify(arrival.row);',
      '    if (seen.has(key)) continue;',
      '    seen.add(key);',
      '    occurrences.push(arrival);',
      '  }',
      '  return occurrences;',
      '}'
    ]).

% ═══ ddl ═════════════════════════════════════════════════════════════════════

ddl_lines(Ddl, Lines) :-
    maplist(ddl_entry_line, Ddl, EntryLines),
    append([ ['const ddl: readonly string[] = ['], EntryLines, ['];'] ], Lines).

ddl_entry_line(Sql, Line) :- js_template(Sql, Template), format(atom(Line), '  ~w,', [Template]).

% ═══ relColumns / arrivalTargets ═════════════════════════════════════════════

rel_columns_lines(RelPlans, Lines) :-
    maplist(rel_columns_entry_line, RelPlans, EntryLines),
    append([ ['const relColumns: Record<string, readonly string[]> = {'], EntryLines, ['};'] ], Lines).

rel_columns_entry_line(relplan(Ref, _Kind, Columns, _Key, _ColumnTypes), Line) :-
    ref_name(Ref, Name),
    quoted_string_array_text(Columns, ColumnsSql),
    format(atom(Line), '  ~w: ~w,', [Name, ColumnsSql]).

rel_column_types_lines(RelPlans, Lines) :-
    maplist(rel_column_types_entry_line, RelPlans, EntryLines),
    append([ ['const relColumnTypes: Record<string, readonly IRowColumnType[]> = {'],
             EntryLines, ['};'] ], Lines).

rel_column_types_entry_line(relplan(Ref, _Kind, _Columns, _Key, ColumnTypes), Line) :-
    ref_name(Ref, Name),
    maplist(boundary_column_type, ColumnTypes, BoundaryTypes),
    quoted_string_array_text(BoundaryTypes, TypesText),
    format(atom(Line), '  ~w: ~w,', [Name, TypesText]).

boundary_column_type(ref(_), ref) :- !.
boundary_column_type(Type, Type).

arrival_targets_lines(ArrivalTargets, Lines) :-
    maplist(ref_name, ArrivalTargets, Names),
    quoted_string_array_text(Names, Sql),
    format(atom(Line), 'const arrivalTargets: readonly string[] = ~w;', [Sql]),
    Lines = [Line].

% ═══ boot ════════════════════════════════════════════════════════════════════

boot_lines(BootStatements, Lines) :-
    maplist(boot_entry_line, BootStatements, EntryLines),
    append([ ['const boot: readonly IBootStatement[] = ['], EntryLines, ['];'] ], Lines).

boot_entry_line(bootstmt(Sql, Params), Line) :-
    js_template(Sql, Template),
    params_array_text(Params, ParamsText),
    format(atom(Line), '  { sql: ~w, params: ~w },', [Template, ParamsText]).

% ═══ snapshot type + reader (forkJoin over selectRows, one entry per rel) ════

snapshot_type_lines(RelPlans, Lines) :-
    maplist(snapshot_field_line, RelPlans, FieldLines),
    append([ ['type Snapshot = {'], FieldLines, ['};'] ], Lines).

snapshot_field_line(relplan(Ref, _Kind, _Columns, _Key, _ColumnTypes), Line) :-
    ref_name(Ref, Name),
    format(atom(Line), '  readonly ~w: readonly IRow[];', [Name]).

% forkJoin({}) (zero keys) completes WITHOUT emitting, same hazard the
% edge-resolver's forkJoin([]) guard already documents (verified against
% rxjs 7.8.2, not assumed) -- a program with zero declared rels (Decls and
% Rules both empty; every phase-C fixture found so far avoids this via
% analyze.pl:declared_refs/2, but nothing upstream forbids it structurally)
% would otherwise stall runTick's very first concatMap forever. `of({})` is
% the one-value-then-complete fallback, matching recompute_levels_fn_lines/2's
% [] case just below.
read_snapshot_fn_lines([], Lines) :- !,
    Lines =
    [ 'function readSnapshot(seam: ISqlSeam): Observable<Snapshot> {',
      '  void seam;',
      '  return of({} as Snapshot);',
      '}'
    ].
read_snapshot_fn_lines(DeltaStatements, Lines) :-
    DeltaStatements \== [],
    maplist(snapshot_read_entry_line, DeltaStatements, EntryLines),
    append(
        [ ['function readSnapshot(seam: ISqlSeam): Observable<Snapshot> {', '  return forkJoin({'],
          EntryLines,
          ['  });', '}']
        ], Lines).

snapshot_read_entry_line(deltastmt(Ref, SelectSql, _DeltaTable, _BoundarySql), Line) :-
    ref_name(Ref, Name),
    js_template(SelectSql, Template),
    format(atom(Line), '    ~w: selectRows(seam, ~w, relColumns.~w!, relColumnTypes.~w!),',
           [Name, Template, Name, Name]).

% ═══ finalSelect (EXPRESSION + AGGREGATE LIFT arc, final-state grading leg) ══
% The SAME per-rel "read every row" SQL readSnapshot uses (deltastmt's
% SelectAllSql, canonical-text rendered), exported by rel name so a grader
% can compare the program's END state against the oracle's FinalAll. This is
% NOT part of the tick path -- nothing inside tick() reads it, so the
% host_residency criterion (zero full-table reads into JS per tick) is
% untouched; it runs exactly once, after the fold, in the sweep harness.
final_select_lines(DeltaStatements, Lines) :-
    maplist(final_select_entry_line, DeltaStatements, EntryLines),
    append([ ['const finalSelect: Record<string, string> = {'], EntryLines, ['};'] ], Lines).

final_select_entry_line(deltastmt(Ref, SelectSql, _DeltaTable, _BoundarySql), Line) :-
    ref_name(Ref, Name),
    js_template(SelectSql, Template),
    format(atom(Line), '  ~w: ~w,', [Name, Template]).

% ═══ arrivals ════════════════════════════════════════════════════════════════

arrival_statements_lines(ArrivalStatements, Lines) :-
    maplist(arrival_statement_entry_line, ArrivalStatements, EntryLines),
    append(
        [ ['const ARRIVAL_STATEMENTS: Record<string, { kind: "log" | "set"; addSql: string; delSql: string | null }> = {'],
          EntryLines,
          ['};']
        ], Lines).

arrival_statement_entry_line(arrivalstmt(Ref, log, AddSql, none, _, _), Line) :- !,
    ref_name(Ref, Name),
    js_template(AddSql, AddTemplate),
    format(atom(Line), '  ~w: { kind: "log", addSql: ~w, delSql: null },', [Name, AddTemplate]).
arrival_statement_entry_line(arrivalstmt(Ref, set, AddSql, DelSql, _, _), Line) :-
    ref_name(Ref, Name),
    js_template(AddSql, AddTemplate),
    js_template(DelSql, DelTemplate),
    format(atom(Line), '  ~w: { kind: "set", addSql: ~w, delSql: ~w },', [Name, AddTemplate, DelTemplate]).

arrival_statement_fn_lines(Name, Lines) :-
    format(atom(UndeclaredError), '    throw new Error(`~w: tick received an arrival for undeclared rel \'${arrival.rel}\'`);', [Name]),
    format(atom(RetractLogError), '      throw new Error(`~w: retract from log rel \'${arrival.rel}\' (engine.pl retract_from_log)`);', [Name]),
    format(atom(NoDeleteError), '      throw new Error(`~w: rel \'${arrival.rel}\' has no delete statement`);', [Name]),
    Lines =
    [ 'function arrivalStatement(arrival: IArrivalRow): SqlStatement {',
      '  const template = ARRIVAL_STATEMENTS[arrival.rel];',
      '  if (template === undefined) {',
      UndeclaredError,
      '  }',
      '  if (arrival.sign === "del") {',
      '    if (template.kind === "log") {',
      RetractLogError,
      '    }',
      '    if (template.delSql === null) {',
      NoDeleteError,
      '    }',
      '    return { sql: template.delSql, args: bindArgs(arrival.row) };',
      '  }',
      '  return { sql: template.addSql, args: bindArgs(arrival.row) };',
      '}',
      '',
      'function applyArrivals(seam: ISqlSeam, arrivals: IArrivalBatch): Observable<unknown> {',
      '  const statements: SqlStatement[] = arrivals.map(arrivalStatement);',
      '  return seam.runner.batch(seam.db, statements);',
      '}'
    ].

% ═══ incremental relation plans ══════════════════════════════════════════════

incremental_relation_lines(RelPlans, ArrivalStatements, DeltaStatements,
                           DepartureRefs, Lines) :-
    maplist(incremental_relation_entry_line(RelPlans, ArrivalStatements, DepartureRefs),
            DeltaStatements, EntryLines),
    append(
        [ ['const INCREMENTAL_RELATIONS: readonly IIncrementalRelationPlan[] = ['],
          EntryLines,
          ['];']
        ], Lines).

incremental_relation_entry_line(RelPlans, ArrivalStatements, DepartureRefs,
        deltastmt(Ref, _SelectSql, DeltaTable, BoundarySql), Line) :-
    ref_name(Ref, Name),
    relplan_kind(RelPlans, Ref, Kind),
    memberchk(relplan(Ref, _, Columns, KeyOrNone, ColumnTypes), RelPlans),
    quoted_string_array_text(Columns, ColumnsText),
    maplist(boundary_column_type, ColumnTypes, BoundaryTypes),
    quoted_string_array_text(BoundaryTypes, ColumnTypesText),
    ( KeyOrNone = key(KeyPositions)
    -> maplist(position_index, KeyPositions, KeyIndices)
    ;  KeyIndices = []
    ),
    atomic_list_concat(KeyIndices, ', ', KeyIndicesText),
    ( memberchk(arrivalstmt(Ref, _, _, _, ArrivalAddSql, _), ArrivalStatements)
    -> js_template(ArrivalAddSql, ArrivalAddTemplate)
    ; ArrivalAddTemplate = null
    ),
    ( memberchk(arrivalstmt(Ref, _, _, _, _, ArrivalDelSql), ArrivalStatements),
      ArrivalDelSql \== none
    -> js_template(ArrivalDelSql, ArrivalDelTemplate)
    ; ArrivalDelTemplate = null
    ),
    js_template(BoundarySql, BoundaryTemplate),
    format(atom(FrontierTable), '__frontier_~w', [Name]),
    format(atom(NextFrontierTable), '__next_frontier_~w', [Name]),
    % departureFrontierTableName is OPTIONAL on IIncrementalRelationPlan and
    % emitted only for a rel some rule binds with finalize/1, so a program
    % with no departure arm renders the entry it always rendered, character
    % for character.
    (   memberchk(Ref, DepartureRefs)
    ->  departure_frontier_table_name(Ref, DepartureTable),
        format(atom(DepartureField), ', departureFrontierTableName: "~w"',
               [DepartureTable])
    ;   DepartureField = ''
    ),
    format(atom(Line),
           '  { rel: "~w", kind: "~w", tableName: "~w", deltaTableName: "~w", frontierTableName: "~w", nextFrontierTableName: "~w", columns: ~w, columnTypes: ~w, keyIndices: [~w], arrivalAddSql: ~w, arrivalDelSql: ~w, boundarySql: ~w~w },',
           [Name, Kind, Name, DeltaTable, FrontierTable, NextFrontierTable,
            ColumnsText, ColumnTypesText, KeyIndicesText, ArrivalAddTemplate, ArrivalDelTemplate,
            BoundaryTemplate, DepartureField]).

position_index(Position, Index) :- Index is Position - 1.

incremental_edge_statement_lines(EdgeStatements, RelPlans, Lines) :-
    maplist(incremental_edge_statement_entry_line(RelPlans), EdgeStatements, EntryLines),
    append(
        [ ['const INCREMENTAL_EDGE_STATEMENTS: readonly IIncrementalEdgeStatement[] = ['],
          EntryLines,
          ['];']
        ], Lines).

incremental_edge_statement_entry_line(RelPlans,
        edgestmt(HeadRef, _TriggerRef, HeadColumns, KeyColumns, _ProjectSql,
                 _WriteSql, DeltaProjectSql, _EdgeTriggerKind), Line) :-
    ref_name(HeadRef, HeadName),
    relplan_kind(RelPlans, HeadRef, HeadKind),
    format(atom(DeltaTable), '__delta_~w', [HeadName]),
    quoted_string_array_text(HeadColumns, ColumnsText),
    key_indices(HeadColumns, KeyColumns, KeyIndices),
    atomic_list_concat(KeyIndices, ', ', KeyIndicesText),
    js_template(DeltaProjectSql, DeltaProjectTemplate),
    format(atom(Line),
           '  { headRel: "~w", headKind: "~w", headTableName: "~w", headDeltaTableName: "~w", headColumns: ~w, keyIndices: [~w], projectSql: ~w },',
           [HeadName, HeadKind, HeadName, DeltaTable, ColumnsText,
            KeyIndicesText, DeltaProjectTemplate]).

incremental_level_statement_lines(LevelStatements, RelPlans, Lines) :-
    maplist(incremental_level_statement_entry_line(RelPlans),
            LevelStatements, EntryLines),
    append(
        [ ['const INCREMENTAL_LEVEL_STATEMENTS: readonly IIncrementalLevelStatement[] = ['],
          EntryLines,
          ['];']
        ], Lines).

incremental_level_statement_entry_line(RelPlans,
        levelstmt(HeadRef, DeleteSql, InsertSqls, DeltaInsertSql, SupportSql,
                  AggregateSql), Line) :-
    ref_name(HeadRef, HeadName),
    format(atom(DeltaTable), '__delta_~w', [HeadName]),
    memberchk(relplan(HeadRef, _, HeadColumns, _, _), RelPlans),
    quoted_string_array_text(HeadColumns, ColumnsText),
    optional_sql_template(DeltaInsertSql, DeltaInsertTemplate),
    maplist(quote_ident_local, HeadColumns, QuotedHeadColumns),
    atomic_list_concat(QuotedHeadColumns, ', ', HeadColumnsSql),
    format(atom(SelectSql), 'SELECT ~w FROM "~w"', [HeadColumnsSql, HeadName]),
    js_template(SelectSql, SelectTemplate),
    atomic_list_concat([DeleteSql | InsertSqls], ';\\n', RecomputeSql),
    js_template(RecomputeSql, RecomputeTemplate),
    support_sql_text(SupportSql, SupportText),
    aggregate_sql_text(AggregateSql, AggregateText),
    format(atom(Line),
           '  { headRel: "~w", headDeltaTableName: "~w", headColumns: ~w, insertSql: ~w, selectSql: ~w, recomputeSql: ~w, supportSql: ~w, aggregateSql: ~w },',
           [HeadName, DeltaTable, ColumnsText, DeltaInsertTemplate,
            SelectTemplate, RecomputeTemplate, SupportText, AggregateText]).

incremental_retention_statement_lines([], []) :- !.
incremental_retention_statement_lines(RetentionStatements, Lines) :-
    maplist(incremental_retention_statement_entry_line,
            RetentionStatements, EntryLines),
    append(
        [ ['const INCREMENTAL_RETENTION_STATEMENTS: readonly IIncrementalRetentionStatement[] = ['],
          EntryLines,
          ['];']
        ], Lines).

incremental_retention_statement_entry_line(
        retentionstmt(Ref, Limit, DeleteSql), Line) :-
    ref_name(Ref, Name),
    js_template(DeleteSql, DeleteTemplate),
    format(atom(Line),
           '  { rel: "~w", count: ~w, deleteSql: ~w },',
           [Name, Limit, DeleteTemplate]).

optional_sql_template(none, null) :- !.
optional_sql_template(Sql, Template) :- js_template(Sql, Template).

support_sql_text(none, null) :- !.
support_sql_text(supportsql(ClearSql, SeedSql, UpdateSql, CollectZeroSql, InsertNewSql),
                 Text) :-
    maplist(js_template,
            [ClearSql, SeedSql, UpdateSql, CollectZeroSql, InsertNewSql],
            Templates),
    atomic_list_concat(Templates, ', ', Joined),
    format(atom(Text), '[~w]', [Joined]).

% The group-scoped aggregate plan (lower.pl level_aggregate_sql/4): clear the
% scope, seed it from this tick's staged deltas, delete the scoped groups
% (RETURNING the -1 events), re-derive them (RETURNING the +1 events).
aggregate_sql_text(none, null) :- !.
aggregate_sql_text(aggsql(_ScopeColumns, _ScopeTypes, ScopeClearSql, ScopeSeedSqls,
                          DeleteScopedSql, InsertScopedSqls), Text) :-
    js_template(ScopeClearSql, ScopeClearTemplate),
    maplist(js_template, ScopeSeedSqls, ScopeSeedTemplates),
    atomic_list_concat(ScopeSeedTemplates, ', ', ScopeSeedJoined),
    js_template(DeleteScopedSql, DeleteScopedTemplate),
    maplist(js_template, InsertScopedSqls, InsertScopedTemplates),
    atomic_list_concat(InsertScopedTemplates, ', ', InsertScopedJoined),
    format(atom(Text),
           '{ scopeClearSql: ~w, scopeSeedSql: [~w], deleteScopedSql: ~w, insertScopedSql: [~w] }',
           [ScopeClearTemplate, ScopeSeedJoined, DeleteScopedTemplate,
            InsertScopedJoined]).

quote_ident_local(Name, Quoted) :- format(atom(Quoted), '"~w"', [Name]).

% ═══ edge rule resolution (one resolver function per ARM -- PHASE C2 RULING
% 2: an unmarked_conjunction rule with N body atoms lowers to N edgestmt
% entries, one per atom acting as trigger. A sampled_conjunction does the
% same over its bare trigger atoms; its latest-wrapped atoms are already
% base-table joins inside ProjectSql and DeltaProjectSql. ═════════════════
% ProjectSql (from lower.pl) is already aliased AS HeadColumns
% (lower.pl:edge_statement_single/5 passes HeadColumns, not `none`, to
% head_select_list/4 for exactly this reason), so the resolver reads each
% projected row back by named column access the same way runtime/rows.ts's
% selectRows does -- no string surgery on the SQL text happens in this file.
%
% A trigger occurrence's ProjectSql, once OtherAtoms is nonempty, is a real
% JOIN and can return ZERO, ONE, or MANY rows for a SINGLE triggering
% arrival (the forkJoin/rendezvous case the ruling names: the LAST-arriving
% input's occurrence, joined against every CURRENTLY matching row of the
% other atoms, can multiply). The resolver therefore iterates
% `result.rows` in full, not just its first entry (round 2's shape, correct
% only when there was never another atom to join).
%
% Global Index (this arm's 0-based position in the WHOLE flattened
% EdgeStatements list, not per-head) disambiguates names: multiple rules
% -- or multiple arms of ONE rule -- can share a HeadRef (merge_family.pl's
% `out(Item) <+ event_a(Item)` / `out(Item) <+ event_b(Item)`), which would
% otherwise collide on `resolve<Head>Writes`.

edge_resolver_blocks(EdgeStatements, RelPlans, ConstLines, FnLines) :-
    findall(Index-EdgeStmt, nth0(Index, EdgeStatements, EdgeStmt), IndexedStatements),
    maplist(edge_resolver_block_indexed(RelPlans), IndexedStatements, ConstLineGroups, FnLineGroups),
    flatten_with_blank_separators(ConstLineGroups, ConstLines),
    flatten_with_blank_separators(FnLineGroups, FnLines).

edge_resolver_block_indexed(RelPlans, Index-edgestmt(HeadRef, TriggerRef, HeadColumns, KeyColumns, ProjectSql, WriteSql, _DeltaProjectSql, EdgeTriggerKind), ConstLines, FnLines) :-
    relplan_kind(RelPlans, TriggerRef, TriggerKind),
    relplan_kind(RelPlans, HeadRef, HeadKind),
    memberchk(relplan(TriggerRef, _, TriggerColumns, _, _), RelPlans),
    edge_resolver_block(edgestmt(HeadRef, TriggerRef, HeadColumns, KeyColumns, ProjectSql, WriteSql, _, EdgeTriggerKind), TriggerKind, TriggerColumns, HeadKind, Index, ConstLines, FnLines).

% HeadKind decides how projected rows become SqlStatements (engine.pl
% apply_edge_writes/6, :236-254, unchanged distinction from before this
% ruling): a `set` head upserts by key, LAST WRITE WINS across every
% triggering arrival AND every row a single arrival's join produced (a
% `Map<string, IRow>` keyed by the key-column values, natural overwrite via
% `.set()`); a `log` head APPENDS every projected row unconditionally --
% collapsing through a key Map would be wrong (KeyColumns is `[]` for a Log
% head, so every row would collapse to the SAME key and only the last
% survive, contradicting q1's "duplicate rows are distinct occurrences").
edge_resolver_block(edgestmt(HeadRef, TriggerRef, HeadColumns, KeyColumns, ProjectSql, WriteSql, _DeltaProjectSql, EdgeTriggerKind), TriggerKind, TriggerColumns, HeadKind, Index, ConstLines, FnLines) :-
    ref_name(TriggerRef, TriggerName),
    upper_snake(HeadRef, Upper),
    format(atom(ProjectConst), 'EDGE_~w_~w_PROJECT_SQL', [Upper, Index]),
    format(atom(WriteConst), 'EDGE_~w_~w_WRITE_SQL', [Upper, Index]),
    format(atom(ColumnsConst), 'EDGE_~w_~w_HEAD_COLUMNS', [Upper, Index]),
    js_template(ProjectSql, ProjectTemplate),
    js_template(WriteSql, WriteTemplate),
    quoted_string_array_text(HeadColumns, ColumnsArrayText),
    format(atom(ProjectLine), 'const ~w = ~w;', [ProjectConst, ProjectTemplate]),
    format(atom(WriteLine), 'const ~w = ~w;', [WriteConst, WriteTemplate]),
    format(atom(ColumnsLine), 'const ~w: readonly string[] = ~w;', [ColumnsConst, ColumnsArrayText]),
    departure_resolver_const_lines(EdgeTriggerKind, TriggerRef, TriggerColumns,
                                   Upper, Index, DepartureConstLines),
    ( HeadKind == log
    -> ConstLines0 = [ProjectLine, WriteLine, ColumnsLine]
    ;  format(atom(IndicesConst), 'EDGE_~w_~w_KEY_INDICES', [Upper, Index]),
       key_indices(HeadColumns, KeyColumns, Indices),
       atomic_list_concat(Indices, ', ', IndicesJoined),
       format(atom(IndicesArrayText), '[~w]', [IndicesJoined]),
       format(atom(IndicesLine), 'const ~w: readonly number[] = ~w;', [IndicesConst, IndicesArrayText]),
       ConstLines0 = [ProjectLine, WriteLine, ColumnsLine, IndicesLine]
    ),
    append(ConstLines0, DepartureConstLines, ConstLines),
    pascal_case(HeadRef, Pascal),
    format(atom(FnName), 'resolve~w_~wWrites', [Pascal, Index]),
    format(atom(SigLine), 'function ~w(seam: ISqlSeam, before: Snapshot, arrivals: IArrivalBatch): Observable<readonly SqlStatement[]> {', [FnName]),
    format(atom(TriggerLine), '  const triggerRows = triggerOccurrences("~w", "~w", before.~w, arrivals);', [TriggerKind, TriggerName, TriggerName]),
    format(atom(ForkLine),
           '  return forkJoin(triggerRows.map((arrival) => seam.runner.execute(seam.db, { sql: ~w, args: bindArgs(arrival.row) }))).pipe(',
           [ProjectConst]),
    format(atom(RowsLine), '        const projectedRows = result.rows.map((row) => ~w.map((column) => row[column] as IRowValue) as IRow);', [ColumnsConst]),
    % bindArgs again, not just at the project bind above: a projected value
    % just read back through result.rows may itself be a plain JS number
    % (an INTEGER query-result column, not a TEXT one), and this second bind
    % is the one that actually writes the destination column -- the same
    % "number" -> REAL -> "N.0" hazard applies here independently of the
    % project-side fix (harmless, and still correct, against an INTEGER
    % destination column too -- PHASE C2 RULING 1, verified empirically).
    ( HeadKind == log
    -> format(atom(PushLine), '          written.push({ sql: ~w, args: bindArgs(projectedRow) });', [WriteConst]),
       MapBodyLines =
       [ '      const written: SqlStatement[] = [];',
         '      for (const result of results) {',
         RowsLine,
         '        for (const projectedRow of projectedRows) {',
         PushLine,
         '        }',
         '      }',
         '      return written;'
       ]
    ;  format(atom(IndicesConst), 'EDGE_~w_~w_KEY_INDICES', [Upper, Index]),
       format(atom(KeyLine), '          const key = JSON.stringify(~w.map((index) => projectedRow[index]));', [IndicesConst]),
       format(atom(WriteMapLine),
              '      return [...resolved.values()].map((row): SqlStatement => ({ sql: ~w, args: bindArgs(row) }));',
              [WriteConst]),
       MapBodyLines =
       [ '      const resolved = new Map<string, IRow>();',
         '      for (const result of results) {',
         RowsLine,
         '        for (const projectedRow of projectedRows) {',
         KeyLine,
         '          resolved.set(key, projectedRow);',
         '        }',
         '      }',
         WriteMapLine
       ]
    ),
    % forkJoin([]) COMPLETES WITHOUT EMITTING (verified against rxjs 7.8.2,
    % not assumed) -- a drain tick, or any tick where no arrival matches this
    % trigger, has an empty triggerRows, and without this guard the WHOLE
    % tick() chain silently completes with no ITickDeltas at all for that tick
    % (a real bug caught running the emitted program against the real seam,
    % not by typecheck -- tsgo has no way to know forkJoin([]) is
    % empty-completing rather than []-emitting).
    EmptyGuardLine = '  if (triggerRows.length === 0) return of([]);',
    (   memberchk(EdgeTriggerKind, [departure, ordered_departure])
    ->  % The referee's cross-tick carry: the departure table is written at the
        % END of a naive tick from that tick's own multisetDiff `del` rows, and
        % READ here on the next one. It is the one piece of state the snapshot
        % path keeps between ticks, and it keeps it in SQLite beside the
        % frontier tables rather than in a module variable, so both pipelines
        % lose or keep a pending departure together (they are TEMP tables:
        % neither survives a process restart -- the Ti-carry durability class,
        % match-frontier lab C7, inherited here, not closed).
        format(atom(DepartureSqlConst), 'EDGE_~w_~w_DEPARTURE_SQL', [Upper, Index]),
        format(atom(DepartureColumnsConst), 'EDGE_~w_~w_TRIGGER_COLUMNS', [Upper, Index]),
        format(atom(DepartureReadLine),
               '  return departureOccurrences(seam, ~w, ~w).pipe(',
               [DepartureSqlConst, DepartureColumnsConst]),
        format(atom(DepartureForkLine),
               '      return forkJoin(triggerRows.map((departedRow) => seam.runner.execute(seam.db, { sql: ~w, args: bindArgs(departedRow) }))).pipe(',
               [ProjectConst]),
        append(
            [ [ SigLine,
                '  void before;',
                '  void arrivals;',
                DepartureReadLine,
                '    concatMap((triggerRows) => {',
                '      if (triggerRows.length === 0) return of<readonly SqlStatement[]>([]);',
                DepartureForkLine,
                '        map((results) => {'
              ],
              MapBodyLines,
              [ '        }),',
                '      );',
                '    }),',
                '  );',
                '}'
              ]
            ], FnLines)
    ;   append(
            [ [ SigLine,
                TriggerLine,
                EmptyGuardLine,
                ForkLine,
                '    map((results) => {'
              ],
              MapBodyLines,
              [ '    }),',
                '  );',
                '}'
              ]
            ], FnLines)
    ).

departure_resolver_const_lines(arrival, _, _, _, _, []) :- !.
departure_resolver_const_lines(ordered_arrival, _, _, _, _, []) :- !.
departure_resolver_const_lines(departure, TriggerRef, TriggerColumns, Upper, Index,
                               [SqlLine, ColumnsLine]) :-
    departure_read_sql(TriggerRef, TriggerColumns, Sql),
    js_template(Sql, SqlTemplate),
    format(atom(SqlConst), 'EDGE_~w_~w_DEPARTURE_SQL', [Upper, Index]),
    format(atom(ColumnsConst), 'EDGE_~w_~w_TRIGGER_COLUMNS', [Upper, Index]),
    format(atom(SqlLine), 'const ~w = ~w;', [SqlConst, SqlTemplate]),
    quoted_string_array_text(TriggerColumns, ColumnsArrayText),
    format(atom(ColumnsLine), 'const ~w: readonly string[] = ~w;',
           [ColumnsConst, ColumnsArrayText]).
departure_resolver_const_lines(ordered_departure, TriggerRef, TriggerColumns,
                               Upper, Index, Lines) :-
    departure_resolver_const_lines(departure, TriggerRef, TriggerColumns,
                                   Upper, Index, Lines).

% Emitted once per program that has any departure arm; nothing else changes
% for a program without one.
departure_occurrences_helper_lines(EdgeStatements, Lines) :-
    (   member(edgestmt(_, _, _, _, _, _, _, TriggerKind), EdgeStatements),
        memberchk(TriggerKind, [departure, ordered_departure])
    ->  Lines =
        [ 'function departureOccurrences(seam: ISqlSeam, sql: string, columns: readonly string[]): Observable<readonly IRow[]> {',
          '  return seam.runner.execute(seam.db, sql).pipe(',
          '    map((result) => result.rows.map((row) => columns.map((column) => row[column] as IRowValue) as IRow)),',
          '  );',
          '}'
        ]
    ;   Lines = []
    ).

key_indices(HeadColumns, KeyColumns, Indices) :-
    findall(Index0,
            ( member(Column, KeyColumns), nth0(Index0, HeadColumns, Column) ),
            Indices).

% ═══ ordered pre occurrence loop ════════════════════════════════════════════

ordered_edge_statement(edgestmt(_, _, _, _, _, _, _, ordered_arrival)).
ordered_edge_statement(edgestmt(_, _, _, _, _, _, _, ordered_departure)).

ordered_program(EdgeStatements) :-
    member(Statement, EdgeStatements),
    ordered_edge_statement(Statement),
    !.

plan_pre_refs(plan(_, prog(_, Rules), _, _, _, _), Refs) :-
    findall(Ref,
            ( member((_ <+ Body), Rules),
              level_body_pre_ref(Body, Ref) ),
            Refs0),
    sort(Refs0, Refs).

pre_snapshot_statement(RelPlans, Ref, Statements) :-
    memberchk(relplan(Ref, _, Columns, _, _), RelPlans),
    ref_name(Ref, Name),
    maplist(quote_ident_local, Columns, QuotedColumns),
    atomic_list_concat(QuotedColumns, ', ', ColumnsSql),
    format(atom(Delete), 'DELETE FROM "__pre_~w"', [Name]),
    format(atom(Insert),
           'INSERT INTO "__pre_~w" (~w) SELECT ~w FROM "~w"',
           [Name, ColumnsSql, ColumnsSql, Name]),
    Statements = [Delete, Insert].

ordered_pre_lines(false, _, _, _, []) :- !.
ordered_pre_lines(true, RelPlans, PreRefs, _EdgeStatements, Lines) :-
    maplist(pre_snapshot_statement(RelPlans), PreRefs, SnapshotGroups),
    append(SnapshotGroups, SnapshotStatements),
    atomic_list_concat(SnapshotStatements, ';\\n', SnapshotSql),
    js_template(SnapshotSql, SnapshotTemplate),
    format(atom(SnapshotReturn),
           '  return seam.runner.executeMultiple(seam.db, ~w);',
           [SnapshotTemplate]),
    Lines =
      [ 'function snapshotOrderedPre(seam: ISqlSeam): Observable<void> {',
        SnapshotReturn,
        '}'
      ].

ordered_boundary_carry_line(level, Ref, Line) :-
    ref_name(Ref, Name),
    format(atom(Line),
           '  for (const row of multisetDiff(mid["~w"], after["~w"]).add) { const rowText = JSON.stringify(row); const exact = JSON.stringify(["~w", row]); if (seen.has(exact) || !(boundaryAdds.get("~w")?.has(rowText) ?? false)) continue; seen.add(exact); additions.push({ rel: "~w", add: [row], del: [] }); }',
           [Name, Name, Name, Name, Name]).

ordered_carry_lines(false, _, _, []) :- !.
ordered_carry_lines(true, _EdgeStatements, LevelHeadedRefs, Lines) :-
    maplist(ordered_boundary_carry_line(level), LevelHeadedRefs, LevelLines),
    append(
      [ [ 'function orderedCarryAdditions(mid: Snapshot, after: Snapshot, boundary: ITickDeltas, written: readonly IOrderedWrite[]): readonly IRelDelta[] {',
          '  const boundaryByRel = new Map(boundary.rels.map((delta) => [delta.rel, delta]));',
          '  const boundaryAdds = new Map([...boundaryByRel].map(([rel, delta]) => [rel, new Set(delta.add.map((row) => JSON.stringify(row)))]));',
          '  const additions: IRelDelta[] = [];',
          '  const seen = new Set<string>();',
          '  for (const { arm, row } of written) {',
          '    const rowText = JSON.stringify(row);',
          '    const exact = JSON.stringify([arm.headRel, row]);',
          '    if (seen.has(exact) || !(boundaryAdds.get(arm.headRel)?.has(rowText) ?? false)) continue;',
          '    seen.add(exact);',
          '    additions.push({ rel: arm.headRel, add: [row], del: [] });',
          '  }'
        ],
        LevelLines,
        [ '  return additions.filter((delta) => delta.add.length > 0);',
          '}'
        ]
      ],
      Lines).

ordered_trigger_kind(ordered_departure, departure) :- !.
ordered_trigger_kind(departure, departure) :- !.
ordered_trigger_kind(_, arrival).

ordered_arm_entry_line(RelPlans, PreRefs,
        edgestmt(HeadRef, TriggerRef, HeadColumns, KeyColumns, ProjectSql,
                 WriteSql, _, EdgeTriggerKind), Line) :-
    ref_name(HeadRef, HeadName),
    ref_name(TriggerRef, TriggerName),
    relplan_kind(RelPlans, HeadRef, HeadKind),
    ordered_trigger_kind(EdgeTriggerKind, TriggerKind),
    quoted_string_array_text(HeadColumns, HeadColumnsText),
    key_indices(HeadColumns, KeyColumns, KeyIndices),
    atomic_list_concat(KeyIndices, ', ', KeyIndicesText),
    js_template(ProjectSql, ProjectTemplate),
    js_template(WriteSql, WriteTemplate),
    ( memberchk(HeadRef, PreRefs) -> EvolvesPre = true ; EvolvesPre = false ),
    format(atom(Line),
           '  { triggerRel: "~w", triggerKind: "~w", headRel: "~w", headKind: "~w", headColumns: ~w, keyIndices: [~w], projectSql: ~w, writeSql: ~w, evolvesPre: ~w },',
           [TriggerName, TriggerKind, HeadName, HeadKind, HeadColumnsText,
            KeyIndicesText, ProjectTemplate, WriteTemplate, EvolvesPre]).

ordered_arrival_accept_line(RelPlans, TriggerRef, Line) :-
    ref_name(TriggerRef, TriggerName),
    relplan_kind(RelPlans, TriggerRef, TriggerKind),
    format(atom(Line),
           '  for (const arrival of triggerOccurrences("~w", "~w", before["~w"], arrivals)) accepted.add(arrival);',
           [TriggerKind, TriggerName, TriggerName]).

ordered_departure_read_entry(RelPlans, TriggerRef, Line) :-
    ref_name(TriggerRef, TriggerName),
    memberchk(relplan(TriggerRef, _, TriggerColumns, _, _), RelPlans),
    departure_read_sql(TriggerRef, TriggerColumns, Sql),
    js_template(Sql, SqlTemplate),
    quoted_string_array_text(TriggerColumns, ColumnsText),
    format(atom(Line),
           '  { rel: "~w", sql: ~w, columns: ~w },',
           [TriggerName, SqlTemplate, ColumnsText]).

ordered_carry_read_entry(RelPlans, TriggerRef, Line) :-
    ref_name(TriggerRef, TriggerName),
    memberchk(relplan(TriggerRef, _, TriggerColumns, _, _), RelPlans),
    maplist(quote_ident_local, TriggerColumns, QuotedColumns),
    atomic_list_concat(QuotedColumns, ', ', ColumnsSql),
    format(atom(Sql),
           'SELECT "_sequence" AS "__sequence", ~w FROM "__frontier_~w" ORDER BY "_phase", "_sequence"',
           [ColumnsSql, TriggerName]),
    js_template(Sql, SqlTemplate),
    quoted_string_array_text(TriggerColumns, ColumnsText),
    format(atom(Line),
           '  { rel: "~w", sql: ~w, columns: ~w },',
           [TriggerName, SqlTemplate, ColumnsText]).

ordered_level_occurrence_line(LevelRef, Line) :-
    ref_name(LevelRef, Name),
    format(atom(Line),
           '  for (const row of multisetDiff(before["~w"], mid["~w"]).add) occurrences.push({ rel: "~w", kind: "arrival", row });',
           [Name, Name, Name]).

ordered_occurrence_lines(false, _, _, _, _, []) :- !.
ordered_occurrence_lines(true, EdgeStatements, RelPlans, PreRefs,
                         LevelHeadedRefs, Lines) :-
    maplist(ordered_arm_entry_line(RelPlans, PreRefs), EdgeStatements,
            ArmLines),
    findall(TriggerRef,
            ( member(edgestmt(_, TriggerRef, _, _, _, _, _, TriggerKind),
                     EdgeStatements),
              ordered_trigger_kind(TriggerKind, arrival) ),
            ArrivalRefs0),
    sort(ArrivalRefs0, ArrivalRefs),
    maplist(ordered_arrival_accept_line(RelPlans), ArrivalRefs, AcceptLines),
    maplist(ordered_carry_read_entry(RelPlans), ArrivalRefs, CarryReadLines),
    intersection(ArrivalRefs, LevelHeadedRefs, TriggerLevelRefs),
    maplist(ordered_level_occurrence_line, TriggerLevelRefs,
            LevelOccurrenceLines),
    findall(TriggerRef,
            ( member(edgestmt(_, TriggerRef, _, _, _, _, _, TriggerKind),
                     EdgeStatements),
              ordered_trigger_kind(TriggerKind, departure) ),
            DepartureRefs0),
    sort(DepartureRefs0, OrderedDepartureRefs),
    maplist(ordered_departure_read_entry(RelPlans), OrderedDepartureRefs,
            DepartureReadLines),
    ( DepartureReadLines == []
    -> ReadDepartureBody =
       [ 'function readOrderedDepartures(seam: ISqlSeam): Observable<readonly IOrderedOccurrence[]> {',
         '  void seam;',
         '  return of([]);',
         '}'
       ]
    ; ReadDepartureBody =
       [ 'function readOrderedDepartures(seam: ISqlSeam): Observable<readonly IOrderedOccurrence[]> {',
         '  return forkJoin(ORDERED_DEPARTURE_READS.map((read) => seam.runner.execute(seam.db, read.sql).pipe(',
         '    map((result) => result.rows.map((row): IOrderedOccurrence => ({',
         '      rel: read.rel,',
         '      kind: "departure",',
         '      row: read.columns.map((column) => row[column] as IRowValue) as IRow,',
         '    }))),',
         '  ))).pipe(map((groups) => groups.flat()));',
         '}'
       ]
    ),
    append(
      [ [ 'interface IOrderedEdgeArm { readonly triggerRel: string; readonly triggerKind: "arrival" | "departure"; readonly headRel: string; readonly headKind: "log" | "set"; readonly headColumns: readonly string[]; readonly keyIndices: readonly number[]; readonly projectSql: string; readonly writeSql: string; readonly evolvesPre: boolean }',
          'interface IOrderedOccurrence { readonly rel: string; readonly kind: "arrival" | "departure"; readonly row: IRow; readonly sequence?: number }',
          'interface IOrderedWrite { readonly arm: IOrderedEdgeArm; readonly row: IRow }',
          '',
          'function quoteOrderedIdentifier(identifier: string): string {',
          '  return \'"\' + identifier.replaceAll(\'"\', \'""\') + \'"\';',
          '}',
          '',
          'function orderedPreWriteStatement(write: IOrderedWrite): SqlStatement | null {',
          '  const { arm, row } = write;',
          '  if (!arm.evolvesPre) return null;',
          '  const table = quoteOrderedIdentifier("__pre_" + arm.headRel);',
          '  const columns = arm.headColumns.map(quoteOrderedIdentifier);',
          '  const placeholders = columns.map(() => "?").join(", ");',
          '  if (arm.headKind === "log") {',
          '    return { sql: "INSERT INTO " + table + " (" + columns.join(", ") + ") VALUES (" + placeholders + ")", args: bindArgs(row) };',
          '  }',
          '  const keyIndices = new Set(arm.keyIndices);',
          '  const keyColumns = arm.keyIndices.map((index) => columns[index]!);',
          '  const nonKeyColumns = columns.filter((_column, index) => !keyIndices.has(index));',
          '  const conflict = nonKeyColumns.length === 0',
          '    ? "ON CONFLICT(" + keyColumns.join(", ") + ") DO NOTHING"',
          '    : "ON CONFLICT(" + keyColumns.join(", ") + ") DO UPDATE SET " + nonKeyColumns.map((column) => column + " = excluded." + column).join(", ");',
          '  return { sql: "INSERT INTO " + table + " (" + columns.join(", ") + ") VALUES (" + placeholders + ") " + conflict, args: bindArgs(row) };',
          '}',
          '',
          'const ORDERED_EDGE_ARMS: readonly IOrderedEdgeArm[] = ['
        ],
        ArmLines,
        [ '];',
          '',
          'const ORDERED_DEPARTURE_READS: readonly { readonly rel: string; readonly sql: string; readonly columns: readonly string[] }[] = ['
        ],
        DepartureReadLines,
        [ '];',
          '',
          'const ORDERED_CARRY_READS: readonly { readonly rel: string; readonly sql: string; readonly columns: readonly string[] }[] = ['
        ],
        CarryReadLines,
        [ '];',
          '',
          'function orderedOutsideOccurrences(before: Snapshot, arrivals: IArrivalBatch): readonly IOrderedOccurrence[] {',
          '  const accepted = new Set<IArrivalRow>();'
        ],
        AcceptLines,
        [ '  return arrivals.filter((arrival) => accepted.has(arrival)).map((arrival): IOrderedOccurrence => ({ rel: arrival.rel, kind: "arrival", row: arrival.row }));',
          '}',
          ''
        ],
        ReadDepartureBody,
        [ '',
          'function readOrderedCarry(seam: ISqlSeam): Observable<readonly IOrderedOccurrence[]> {',
          '  if (ORDERED_CARRY_READS.length === 0) return of([]);',
          '  return forkJoin(ORDERED_CARRY_READS.map((read) => seam.runner.execute(seam.db, read.sql).pipe(',
          '    map((result) => result.rows.map((row): IOrderedOccurrence => ({',
          '      rel: read.rel,',
          '      kind: "arrival",',
          '      row: read.columns.map((column) => row[column] as IRowValue) as IRow,',
          '      sequence: Number(row.__sequence),',
          '    }))),',
          '  ))).pipe(map((groups) => groups.flat().sort((left, right) => (left.sequence ?? 0) - (right.sequence ?? 0))));',
          '}',
          '',
          'function orderedLevelOccurrences(before: Snapshot, mid: Snapshot): readonly IOrderedOccurrence[] {',
          '  const occurrences: IOrderedOccurrence[] = [];'
        ],
        LevelOccurrenceLines,
        [ '  return occurrences;',
          '}',
          '',
          'function applyOrderedOccurrence(seam: ISqlSeam, occurrence: IOrderedOccurrence, written: IOrderedWrite[]): Observable<void> {',
          '  const arms = ORDERED_EDGE_ARMS.filter((arm) => arm.triggerRel === occurrence.rel && arm.triggerKind === occurrence.kind);',
          '  if (arms.length === 0) return of(undefined);',
          '  return forkJoin(arms.map((arm) => seam.runner.execute(seam.db, { sql: arm.projectSql, args: bindArgs(occurrence.row) }).pipe(',
          '    map((result) => ({ arm, rows: result.rows.map((row) => arm.headColumns.map((column) => row[column] as IRowValue) as IRow) })),',
          '  ))).pipe(',
          '    concatMap((groups) => {',
          '      const writes: IOrderedWrite[] = [];',
          '      const exact = new Set<string>();',
          '      const keyed = new Map<string, IRow>();',
          '      for (const group of groups) {',
          '        for (const row of group.rows) {',
          '          const exactKey = JSON.stringify([group.arm.headRel, row]);',
          '          if (exact.has(exactKey)) continue;',
          '          exact.add(exactKey);',
          '          if (group.arm.headKind === "set") {',
          '            const key = JSON.stringify([group.arm.headRel, group.arm.keyIndices.map((index) => row[index])]);',
          '            const prior = keyed.get(key);',
          '            if (prior !== undefined && JSON.stringify(prior) !== JSON.stringify(row)) {',
          '              throw new Error(`keyed conflict in ordered occurrence for ${group.arm.headRel}: ${key}`);',
          '            }',
          '            keyed.set(key, row);',
          '          }',
          '          writes.push({ arm: group.arm, row });',
          '        }',
          '      }',
          '      if (writes.length === 0) return of(undefined);',
          '      const statements = writes.flatMap((write): readonly SqlStatement[] => {',
          '        const base: SqlStatement = { sql: write.arm.writeSql, args: bindArgs(write.row) };',
          '        const pre = orderedPreWriteStatement(write);',
          '        return pre === null ? [base] : [base, pre];',
          '      });',
          '      return seam.runner.batch(seam.db, statements).pipe(map(() => {',
          '        written.push(...writes);',
          '        return undefined;',
          '      }));',
          '    }),',
          '  );',
          '}',
          '',
          'function processOrderedOccurrences(seam: ISqlSeam, before: Snapshot, mid: Snapshot, arrivals: IArrivalBatch): Observable<readonly IOrderedWrite[]> {',
          '  return forkJoin([readOrderedCarry(seam), readOrderedDepartures(seam)]).pipe(',
          '    concatMap(([carry, departures]) => {',
          '      const written: IOrderedWrite[] = [];',
          '      const occurrences = [...carry, ...departures, ...orderedOutsideOccurrences(before, arrivals), ...orderedLevelOccurrences(before, mid)];',
          '      return occurrences.reduce(',
          '        (work, occurrence) => work.pipe(concatMap(() => applyOrderedOccurrence(seam, occurrence, written))),',
          '        of(undefined) as Observable<void>,',
          '      ).pipe(map(() => written as readonly IOrderedWrite[]));',
          '    }),',
          '  );',
          '}'
        ]
      ],
      Lines).

% ═══ level recompute ═════════════════════════════════════════════════════════
% Statements joined with the literal two-character escape sequence `\n`
% (NOT an actual newline byte in this Prolog source): embedded inside the
% backtick template literal, that text is what the JS engine itself
% interprets as a newline when the template is evaluated at runtime --
% matches Phase A's exemplar (`.join(";\n")` in real TS source, same
% escape). Keeps the emitted const a single source line.

% A program with zero level rules (every fixture whose Rules list is entirely
% edge rules, or empty outright -- e.g. an EDB-only fixture with no rules at
% all) still has run_tick_fn_lines call `recomputeLevels(seam)` unconditionally
% (that call is not itself gated on LevelStatements), so this needs a real
% zero-op function, not silent failure: `of(undefined)` is the one-void-then-
% complete shape the async-becomes-rxjs law calls for (EMPTY would complete
% without emitting, which starves the caller's `.pipe(map(() => before))` of
% a value and stalls the whole tick chain).
recompute_levels_fn_lines([], Lines) :- !,
    Lines =
    [ 'function recomputeLevels(seam: ISqlSeam): Observable<void> {',
      '  void seam;',
      '  return of(undefined);',
      '}'
    ].
recompute_levels_fn_lines(LevelStatements, Lines) :-
    LevelStatements \== [],
    % InsertSqls is a LIST (lower.pl:level_statement_group/3 -- one entry per
    % rule clause sharing this head, so a multi-clause head's rows all
    % INSERT after exactly one DELETE, never one DELETE per clause); flattens
    % to the identical [Delete, Insert] sequence as before for the common
    % single-clause case.
    findall(Sql, ( member(levelstmt(_, DeleteSql, InsertSqls, _, _, _), LevelStatements), ( Sql = DeleteSql ; member(Sql, InsertSqls) ) ), Sqls),
    atomic_list_concat(Sqls, ';\\n', JoinedSql),
    js_template(JoinedSql, SqlTemplate),
    format(atom(SqlLine), '  const sql = ~w;', [SqlTemplate]),
    Lines =
    [ 'function recomputeLevels(seam: ISqlSeam): Observable<void> {',
      SqlLine,
      '  return seam.runner.executeMultiple(seam.db, sql);',
      '}'
    ].

% ═══ buildDeltas ═════════════════════════════════════════════════════════════

build_deltas_fn_lines(RelPlans, EdgeStatements, RetentionStatements,
                      DepartureRefs, Lines) :-
    findall(Ref, member(retentionstmt(Ref, _, _), RetentionStatements),
            RetentionRefs),
    maplist(diff_local_line(RetentionRefs), RelPlans, DiffLines),
    maplist(rel_entry_line, RelPlans, RelEntryLines),
    carry_pending_expr(EdgeStatements, DepartureRefs, CarryExpr),
    format(atom(CarryLine), '    carryPending: ~w,', [CarryExpr]),
    append(
        [ ['function buildDeltas(before: Snapshot, after: Snapshot): ITickDeltas {'],
          DiffLines,
          ['  return {', '    rels: ['],
          RelEntryLines,
          ['    ],', CarryLine, '  };', '}']
        ], Lines).

diff_local_line(RetentionRefs,
                relplan(Ref, _Kind, _Columns, _Key, _ColumnTypes), Line) :-
    ref_name(Ref, Name),
    ( memberchk(Ref, RetentionRefs)
    -> format(atom(Line),
              '  const ~wDiff = multisetDiff(before.~w, after.~w); const ~w = { add: ~wDiff.add, del: [] };',
              [Name, Name, Name, Name, Name])
    ; format(atom(Line), '  const ~w = multisetDiff(before.~w, after.~w);',
             [Name, Name, Name])
    ).

rel_entry_line(relplan(Ref, _Kind, _Columns, _Key, _ColumnTypes), Line) :-
    ref_name(Ref, Name),
    format(atom(Line), '      { rel: "~w", add: ~w.add, del: ~w.del },', [Name, Name, Name]).

% carryPending (engine.pl q4/R2): true when a row this tick's edge rule(s)
% wrote SHOWS AS A DELTA (an equal-row rewrite is invisible to multisetDiff,
% so no separate no-op check is needed here -- the diff already absorbs it).
% Simplification, matching Phase A's exemplar finding 3: this ignores the
% general "post-write level growth with no edge write" carry source, safe
% for both target fixtures because neither has a level rule reading an
% arrival-driven rel directly without an edge rule in between. A program
% with zero edge rules (demand_laziness_effect_rows) has carryPending fixed
% at `false` -- not a per-tick computation, a structural fact about that
% program shape (no rule ever writes mid-tick). HeadRefs are DEDUPED (sort/2)
% before building conditions: PHASE C2 RULING 2 lets several edgestmt arms
% share one HeadRef (multiple rules, or multiple atoms of one unmarked
% body), and an un-deduped fold would repeat the identical `X.add.length >
% 0 || X.del.length > 0` disjunct once per arm -- harmless logically
% (idempotent OR), just noise in the emitted text.
% A -delta of a LISTENED rel is carry too: engine.pl appends DepartureCarry to
% ArrivalCarry in one CarryOut list, and a non-empty CarryOut is what mints the
% drain tick the departure arm fires on. Without this term the referee's drain
% boundary lies about exactly the ticks this feature exists for.
carry_pending_expr([], [], 'false') :- !.
carry_pending_expr(EdgeStatements, DepartureRefs, Expr) :-
    findall(HeadRef, member(edgestmt(HeadRef, _, _, _, _, _, _, _), EdgeStatements), HeadRefs0),
    sort(HeadRefs0, HeadRefs),
    findall(Cond,
            ( member(HeadRef, HeadRefs),
              ref_name(HeadRef, Name),
              format(atom(Cond), '~w.add.length > 0 || ~w.del.length > 0', [Name, Name]) ),
            HeadConds),
    % A listened rel that is ALSO an edge head already contributes its `del`
    % half through the condition above; repeating it would be a harmless but
    % noisy duplicate disjunct.
    findall(Cond,
            ( member(DepartureRef, DepartureRefs),
              \+ memberchk(DepartureRef, HeadRefs),
              ref_name(DepartureRef, Name),
              format(atom(Cond), '~w.del.length > 0', [Name]) ),
            DepartureConds),
    append(HeadConds, DepartureConds, Conds),
    ( Conds == [] -> Expr = 'false'
    ; atomic_list_concat(Conds, ' || ', Expr) ).

% ═══ tick() + program export ════════════════════════════════════════════════

naive_retention_fn_lines([], []) :- !.
naive_retention_fn_lines(_RetentionStatements,
    [ 'function applyNaiveRetention(seam: ISqlSeam): Observable<void> {',
      '  const statements: SqlStatement[] = INCREMENTAL_RETENTION_STATEMENTS.map((statement) => ({ sql: statement.deleteSql, args: [] }));',
      '  return seam.runner.batch(seam.db, statements).pipe(map(() => undefined));',
      '}'
    ]).

run_naive_tick_fn_lines(Name, [], HasRetention, UsesTick, DepartureRefs,
                        HasStructTypes, Lines) :- !,
    departure_stage_naive_lines(DepartureRefs, DepartureStageLines),
    format(atom(NameCommentLine), '  // ~w: no edge rules -- absorb arrivals, recompute levels, diff.', [Name]),
    retention_tick_lines(HasRetention, RetentionLines),
    advance_tick_naive_line(UsesTick, AdvanceTickLines),
    naive_reference_normalize_lines(HasStructTypes, NormalizeLines),
    append(
    [ [ 'function runNaiveTick(seam: ISqlSeam, arrivals: IArrivalBatch): Observable<ITickDeltas> {',
        '  return readSnapshot(seam).pipe('
      ],
      AdvanceTickLines,
      NormalizeLines,
      [ '    concatMap((before) => applyArrivals(seam, arrivals).pipe(map(() => before))),',
        '    concatMap((before) => recomputeLevels(seam).pipe(map(() => before))),'
      ],
      RetentionLines,
      [ '    concatMap((before) => readSnapshot(seam).pipe(map((after) => buildDeltas(before, after)))),'
      ],
      DepartureStageLines,
      [ '  );',
        NameCommentLine,
        '}'
      ]
    ], Lines).
run_naive_tick_fn_lines(Name, EdgeStatements, HasRetention, UsesTick,
                        DepartureRefs, HasStructTypes, Lines) :-
    EdgeStatements \== [],
    departure_stage_naive_lines(DepartureRefs, DepartureStageLines),
    advance_tick_naive_line(UsesTick, AdvanceTickLines),
    naive_reference_normalize_lines(HasStructTypes, NormalizeLines),
    edge_resolve_call_exprs(EdgeStatements, ResolveCallExprs),
    ( ResolveCallExprs = [SingleCall]
    -> format(atom(EdgeWritesExpr), '~w', [SingleCall])
    ; atomic_list_concat(ResolveCallExprs, ', ', JoinedCalls),
      format(atom(EdgeWritesExpr), 'forkJoin([~w]).pipe(map((groups) => groups.flat()))', [JoinedCalls])
    ),
    format(atom(EdgeWritesLine), '      ~w.pipe(', [EdgeWritesExpr]),
    format(atom(NameCommentLine), '  // ~w: engine.pl process_occurrences -> level_closure -> boundary_deltas.', [Name]),
    retention_tick_lines(HasRetention, RetentionLines),
    append(
    [ [ 'function runNaiveTick(seam: ISqlSeam, arrivals: IArrivalBatch): Observable<ITickDeltas> {',
      '  return readSnapshot(seam).pipe('
      ],
      AdvanceTickLines,
      NormalizeLines,
      [ '    concatMap((before) => applyArrivals(seam, arrivals).pipe(map(() => before))),',
      % TICK PHASE ALIGNMENT: the referee freezes the level plane where
      % engine.pl does -- after arrivals, before the edge batch. The naive
      % recompute is a DELETE + rebuild of every level table, so this one call
      % supplies both halves of MidLevel; the second call below is the oracle's
      % second closure, over the post-write store.
      '    concatMap((before) => recomputeLevels(seam).pipe(map(() => before))),',
      '    concatMap((before) =>',
      EdgeWritesLine,
      '        concatMap((statements) => seam.runner.batch(seam.db, statements)),',
      '        map(() => before),',
      '      ),',
      '    ),',
      '    concatMap((before) => recomputeLevels(seam).pipe(map(() => before))),'
      ],
      RetentionLines,
      [
      '    concatMap((before) => readSnapshot(seam).pipe(map((after) => buildDeltas(before, after)))),'
      ],
      DepartureStageLines,
      [ '  );',
      NameCommentLine,
      '}'
      ]
    ], Lines).

run_ordered_tick_fn_lines(false, _, _, _, _, _, []) :- !.
run_ordered_tick_fn_lines(true, Name, HasRetention, UsesTick, DepartureRefs,
                          HasStructTypes, Lines) :-
    departure_stage_naive_lines(DepartureRefs, DepartureStageLines),
    advance_tick_naive_line(UsesTick, AdvanceTickLines),
    naive_reference_normalize_lines(HasStructTypes, NormalizeLines),
    retention_tick_lines(HasRetention, RetentionLines),
    format(atom(NameCommentLine),
           '  // ~w: ordered process_occurrences with evolving pre snapshots.',
           [Name]),
    append(
    [ [ 'function runOrderedTick(seam: ISqlSeam, arrivals: IArrivalBatch): Observable<ITickDeltas> {',
        '  return readSnapshot(seam).pipe('
      ],
      AdvanceTickLines,
      NormalizeLines,
      [ '    concatMap((before) => applyArrivals(seam, arrivals).pipe(map(() => before))),',
        '    concatMap((before) => snapshotOrderedPre(seam).pipe(map(() => before))),',
        '    concatMap((before) => recomputeLevels(seam).pipe(map(() => before))),',
        '    concatMap((before) => readSnapshot(seam).pipe(map((mid) => ({ before, mid })))),',
        '    concatMap(({ before, mid }) => processOrderedOccurrences(seam, before, mid, arrivals).pipe(map((written) => ({ before, mid, written })))),',
        '    concatMap(({ before, mid, written }) => recomputeLevels(seam).pipe(map(() => ({ before, mid, written })))),'
      ],
      RetentionLines,
      [ '    concatMap(({ before, mid, written }) => readSnapshot(seam).pipe(map((after) => ({ mid, after, written, deltas: buildDeltas(before, after) })))),',
        '    concatMap(({ mid, after, written, deltas }) => stageOrderedFrontiers(seam, INCREMENTAL_RELATIONS, orderedCarryAdditions(mid, after, deltas, written)).pipe(',
        '      map((postWriteCarry): ITickDeltas => ({ rels: deltas.rels, carryPending: deltas.carryPending || postWriteCarry })),',
        '    )),'
      ],
      DepartureStageLines,
      [ '  );',
        NameCommentLine,
        '}'
      ]
    ], Lines).

retention_tick_lines(true,
    ['    concatMap((before) => applyNaiveRetention(seam).pipe(map(() => before))),']).
retention_tick_lines(false, []).

% The referee's own end-of-tick departure staging. It reuses the RUNTIME's
% IncrementalRuntime.stageDepartures over the deltas THIS path computed from
% its two snapshots -- the same table, filled from an independent source, so
% the two pipelines stay comparable while neither borrows the other's answer.
% Between readBoundary and promoteFrontiers, on purpose: the source is the
% tick's NET boundary delta (engine.pl reads DepartureCarry off `Deltas`), and
% promoteFrontiers is what then reports the staged rows as carryPending.
departure_stage_incremental_lines([], []) :- !.
departure_stage_incremental_lines(DepartureRefs,
    ['    concatMap((rels) => IncrementalRuntime.stageDepartures(seam, INCREMENTAL_RELATIONS, rels).pipe(map(() => rels))),']) :-
    DepartureRefs \== [].

departure_stage_naive_lines([], []) :- !.
departure_stage_naive_lines(DepartureRefs,
    ['    concatMap((deltas) => IncrementalRuntime.stageDepartures(seam, INCREMENTAL_RELATIONS, deltas.rels).pipe(map(() => deltas))),']) :-
    DepartureRefs \== [].

incremental_mode_lines(IncrementalSafe, ReconcileEveryTick,
    [ SafeLine, ReconcileLine,
      'const EMITTER_MODE = process.env.SPREFA_TSV2_EMITTER_MODE === "naive" ? "naive" : "incremental";'
    ]) :-
    format(atom(SafeLine), 'const INCREMENTAL_PROGRAM_SAFE = ~w;', [IncrementalSafe]),
    format(atom(ReconcileLine), 'const RECONCILE_EVERY_TICK = ~w;',
           [ReconcileEveryTick]).

incremental_plan_export_lines(RetractionGuard, HasRetention, Lines) :-
    ( HasRetention == true
    -> RetentionLine = ['  retention: INCREMENTAL_RETENTION_STATEMENTS,']
    ; RetentionLine = []
    ),
    append(
    [ [ 'export const incrementalPlan: IIncrementalProgramPlan = {',
      '  safe: INCREMENTAL_PROGRAM_SAFE,',
      '  reconcileEveryTick: RECONCILE_EVERY_TICK,',
      GuardLine,
      '  relations: INCREMENTAL_RELATIONS,',
      '  edges: INCREMENTAL_EDGE_STATEMENTS,',
      '  levels: INCREMENTAL_LEVEL_STATEMENTS,'
      ],
      RetentionLine,
      [
      '};'
      ]
    ], Lines),
    format(atom(GuardLine), '  retractionGuard: "~w",', [RetractionGuard]).

incremental_carry_expr([], 'false') :- !.
incremental_carry_expr(EdgeStatements, Expr) :-
    findall(HeadName,
            ( member(edgestmt(HeadRef, _, _, _, _, _, _, _), EdgeStatements),
              ref_name(HeadRef, HeadName) ),
            HeadNames0),
    sort(HeadNames0, HeadNames),
    findall(Quoted, (member(Name, HeadNames), js_string(Name, Quoted)), QuotedNames),
    atomic_list_concat(QuotedNames, ', ', NamesText),
    format(atom(Expr),
           '[~w].includes(delta.rel) && (delta.add.length > 0 || delta.del.length > 0)',
           [NamesText]).

% now/1's kernel tick, emitted only for programs that read it (every other
% module's text is unchanged). engine.pl:run_ticks/7 counts from 1, so the
% counter is seeded at 0 by the DDL and advanced at the HEAD of each tick,
% before any arrival is absorbed or any arm projects -- the same place the
% oracle fixes Tick for the whole tick. One statement per tick, flat.
advance_tick_fn_lines(false, []) :- !.
advance_tick_fn_lines(true,
    [ 'function advanceTick(seam: ISqlSeam): Observable<void> {',
      '  return seam.runner.execute(seam.db, `UPDATE "__tick" SET "n" = "n" + 1`).pipe(map(() => undefined));',
      '}'
    ]).

advance_tick_pipeline_line(false, []) :- !.
advance_tick_pipeline_line(true,
    ['    concatMap(() => advanceTick(seam)),']).

advance_tick_naive_line(false, []) :- !.
advance_tick_naive_line(true,
    ['    concatMap((before) => advanceTick(seam).pipe(map(() => before))),']).

% TICK PHASE ALIGNMENT: the mid-tick level plane an edge body reads must be
% engine.pl's FROZEN MidLevel (`level_closure` over the store AFTER arrivals,
% BEFORE any edge write). applyLevelsBeforeEdges only grows that plane;
% recomputeLevelsBeforeEdges runs the retracting half at the same point.
% Emitted ONLY for programs that have edge rules: with no edge rule nothing
% reads the plane mid-tick, the correction is unobservable, and those modules'
% text stays byte-identical to what the previous emitter wrote.
pre_edge_level_reconcile_lines([], [], []) :- !.
pre_edge_level_reconcile_lines(EdgeStatements,
    ['    concatMap(() => IncrementalRuntime.recomputeLevelsBeforeEdges(seam, INCREMENTAL_LEVEL_STATEMENTS, INCREMENTAL_RELATIONS, RECONCILE_EVERY_TICK, arrivals)),'],
    % The tick pipeline is emitted as TWO chained pipes when the reconcile line
    % is present, split at the edge boundary: the mid-tick phases (arrivals ->
    % frozen level plane -> edges -> post-write level growth), then the closing
    % phases (retention -> reconcile -> boundary -> carry). rxjs types `pipe`
    % through a fixed overload list that stops at NINE operators; the reconcile
    % line is the tenth, and a tenth silently degrades the whole chain to
    % Observable<unknown>, which tsgo then rejects against the ITickDeltas
    % return type. TYPE boundary only: the operator sequence, and therefore the
    % executed statement sequence, is unchanged. A program with no edge rules
    % takes neither line and its emitted text is byte-identical to what the
    % previous emitter wrote.
    ['  ).pipe(']) :-
    EdgeStatements \== [].

run_incremental_tick_fn_lines(EdgeStatements, DerivedEdgeCarryRequired,
                              HasRetention, UsesTick, DepartureRefs, Lines) :-
    run_incremental_tick_fn_lines(EdgeStatements, DerivedEdgeCarryRequired,
                                  HasRetention, UsesTick, DepartureRefs, false,
                                  false, Lines).

run_incremental_tick_fn_lines(EdgeStatements, DerivedEdgeCarryRequired,
                              HasRetention, UsesTick, DepartureRefs,
                              HasStructTypes, HasOrderedProgram, Lines) :-
    advance_tick_pipeline_line(UsesTick, AdvanceTickLines),
    incremental_reference_normalize_lines(HasStructTypes, NormalizeLines),
    departure_stage_incremental_lines(DepartureRefs, DepartureStageLines),
    pre_edge_level_reconcile_lines(EdgeStatements, PreEdgeReconcileLines, PipeSplitLines),
    ( EdgeStatements == []
    -> MergeLine = '    concatMap(() => of(undefined)),',
       PostEdgeLevelLine = '    concatMap(() => of(undefined)),'
    ;  MergeLine = '    concatMap(() => IncrementalRuntime.mergeNextIntoCurrent(seam, INCREMENTAL_RELATIONS)),',
       PostEdgeLevelLine = '    concatMap(() => IncrementalRuntime.applyLevelsAfterEdges(seam, INCREMENTAL_LEVEL_STATEMENTS, INCREMENTAL_RELATIONS)),'
    ),
    RecomputeLine = '    concatMap(() => IncrementalRuntime.recomputeLevelsAfterEdges(seam, INCREMENTAL_LEVEL_STATEMENTS, INCREMENTAL_RELATIONS, RECONCILE_EVERY_TICK)),',
    run_tick_dispatch_lines(DerivedEdgeCarryRequired, HasStructTypes,
                            HasOrderedProgram, DispatchLines),
    ( HasRetention == true
    -> RetentionLines =
       ['    concatMap(() => IncrementalRuntime.applyRetention(seam, INCREMENTAL_RETENTION_STATEMENTS, INCREMENTAL_RELATIONS)),']
    ; RetentionLines = []
    ),
    append(
    [ [ 'function runIncrementalTick(seam: ISqlSeam, arrivals: IArrivalBatch): Observable<ITickDeltas> {',
      '  return IncrementalRuntime.prepareTick(seam, INCREMENTAL_RELATIONS).pipe('
      ],
      AdvanceTickLines,
      NormalizeLines,
      [ '    concatMap(() => IncrementalRuntime.applyArrivals(seam, arrivals, INCREMENTAL_RELATIONS)),',
      '    concatMap(() => IncrementalRuntime.applyLevelsBeforeEdges(seam, INCREMENTAL_LEVEL_STATEMENTS, INCREMENTAL_RELATIONS)),'
      ],
      PreEdgeReconcileLines,
      [ '    concatMap(() => IncrementalRuntime.applyEdges(seam, INCREMENTAL_EDGE_STATEMENTS, INCREMENTAL_RELATIONS)),',
      MergeLine,
      PostEdgeLevelLine
      ],
      PipeSplitLines,
      RetentionLines,
      [
      RecomputeLine,
      '    concatMap(() => IncrementalRuntime.readBoundary(seam, INCREMENTAL_RELATIONS)),'
      ],
      DepartureStageLines,
      [
      '    concatMap((rels) => IncrementalRuntime.promoteFrontiers(seam, INCREMENTAL_RELATIONS).pipe(',
      '      map((carryPending): ITickDeltas => ({ rels, carryPending })),',
      '    )),',
      '  );',
      '}',
      ''
      ],
      DispatchLines
    ], Lines).

run_tick_dispatch_lines(DerivedEdgeCarryRequired, Lines) :-
    run_tick_dispatch_lines(DerivedEdgeCarryRequired, false, false, Lines).

run_tick_dispatch_lines(_, HasStructTypes, true,
    [ Signature,
      '  arrivals = validateArrivals(arrivals);',
      '  return runOrderedTick(seam, arrivals);',
      '}'
    ]) :- dispatch_signature(HasStructTypes, Signature), !.
run_tick_dispatch_lines(true, HasStructTypes, false,
    [ Signature,
      '  arrivals = validateArrivals(arrivals);',
      '  // Derived edge triggers consume the P1 current/next frontier, including drain carry.',
      '  return runIncrementalTick(seam, arrivals);',
      '}'
    ]) :- dispatch_signature(HasStructTypes, Signature).
run_tick_dispatch_lines(false, HasStructTypes, false,
    [ Signature,
      '  arrivals = validateArrivals(arrivals);',
      '  if (EMITTER_MODE === "naive" || !INCREMENTAL_PROGRAM_SAFE) {',
      '    return runNaiveTick(seam, arrivals);',
      '  }',
      '  return runIncrementalTick(seam, arrivals);',
      '}'
    ]) :- dispatch_signature(HasStructTypes, Signature).

dispatch_signature(_,
    'function runTick(seam: ISqlSeam, arrivals: IArrivalBatch): Observable<ITickDeltas> {').

derived_edge_carry_required(
        plan(_, prog(_, Rules), _, _, _, _), EdgeStatements, Required) :-
    derived_refs(Rules, DerivedRefs),
    ( member(edgestmt(_, TriggerRef, _, _, _, _, _, _), EdgeStatements),
      memberchk(TriggerRef, DerivedRefs)
    -> Required = true
    ;  Required = false
    ).

incremental_program_safe(plan(_, prog(_, Rules), _, _, _, _),
                         _EdgeStatements, _LevelStatements, Safe) :-
    rules_have_supported_level_bodies(Rules),
    Safe = true.

rules_have_supported_level_bodies([]).
rules_have_supported_level_bodies([_ | Rest]) :-
    rules_have_supported_level_bodies(Rest).

reconcile_every_tick(plan(_, prog(_, Rules), _, _, _, _), Reconcile) :-
    ( member(Rule, Rules),
      Rule = (_ <- Body),
      body_ref_uses(Body, Uses),
      member(use(_, _, neg, _), Uses)
    -> Reconcile = true
    ;  Reconcile = false
    ).

retraction_guard(plan(_, prog(_, Rules), _, _, _, _), Guard) :-
    ( member(Rule, Rules),
      Rule = (_ <- Body),
      rule_head_ref(Rule, HeadRef),
      body_ref_uses(Body, Uses),
      member(use(HeadRef, _, pos, _), Uses)
    -> Guard = 'recursive-cte-reseed'
    ;  Guard = 'plain-count-acyclic'
    ).

% Each arm's resolver call passes `before` (PHASE C2 RULING 2: a Set-kind
% trigger's occurrence detection needs the tick-start snapshot to tell a
% genuine new row from a same-tick or standing duplicate -- triggerOccurrences
% above); Index is this arm's 0-based position in the whole flattened
% EdgeStatements list, matching edge_resolver_blocks/4's own naming.
edge_resolve_call_exprs(EdgeStatements, Exprs) :-
    findall(Expr,
            ( nth0(Index, EdgeStatements, edgestmt(HeadRef, _, _, _, _, _, _, _)),
              edge_resolve_call_expr(HeadRef, Index, Expr) ),
            Exprs).

edge_resolve_call_expr(HeadRef, Index, Expr) :-
    pascal_case(HeadRef, Pascal),
    format(atom(Expr), 'resolve~w_~wWrites(seam, before, arrivals)', [Pascal, Index]).

program_export_lines(Name,
    [ 'export const program: IGenProgramWithBoot = {',
      NameLine,
      '  ddl,',
      '  relColumns,',
      '  relColumnTypes,',
      '  arrivalTargets,',
      '  boot,',
      '  finalSelect,',
      '  hostPlans,',
      '  bindPlans,',
      '  queryPlans,',
      '  unsupportedExecution,',
      '  tick: runTick,',
      '};'
    ]) :-
    format(atom(NameLine), '  name: "~w",', [Name]).

% ═══ top level ═══════════════════════════════════════════════════════════════

emit_program(Name, Plan, Lowered, BootStatements, Text) :-
    Lowered = lowered(Name, Ddl, ArrivalStatements, EdgeStatements, LevelStatements, DeltaStatements, RelPlans, ArrivalTargets),
    header_lines(Name, HeaderLines),
    ( EdgeStatements == [] -> HasEdgeRules = false ; HasEdgeRules = true ),
    include(is_level_statement, LevelStatements, RuleLevelStatements),
    include(is_retention_statement, LevelStatements, RetentionStatements),
    ( RetentionStatements == [] -> HasRetention = false ; HasRetention = true ),
    Plan = plan(_, prog(PlanDecls, _), _, _, _, _),
    struct_type_plans(PlanDecls, StructPlans),
    struct_plane_lines(StructPlans, RelPlans, StructPlaneLines, HasStructTypes),
    ( ordered_program(EdgeStatements) -> HasOrderedProgram = true
    ; HasOrderedProgram = false
    ),
    imports_lines(HasEdgeRules, HasRetention, HasStructTypes,
                  HasOrderedProgram, ImportLines),
    local_types_lines(LocalTypeLines),
    world_plan_lines(Plan, WorldPlanLines),
    bind_args_helper_lines(BindArgsHelperLines),
    arrival_value_guard_lines(ArrivalValueGuardLines),
    ( EdgeStatements == [] -> TriggerOccurrencesHelperLines = [] ; trigger_occurrences_helper_lines(TriggerOccurrencesHelperLines) ),
    Plan = plan(_, prog(_, PlanRules), _, _, _, _),
    listened_departure_refs(PlanRules, DepartureRefs),
    plan_pre_refs(Plan, PreRefs),
    findall(LevelRef,
            ( member((LevelHead <- _), PlanRules),
              functor(LevelHead, LevelName, LevelArity),
              LevelRef = LevelName/LevelArity ),
            LevelRefs0),
    sort(LevelRefs0, LevelHeadedRefs),
    departure_occurrences_helper_lines(EdgeStatements, DepartureOccurrencesHelperLines),
    ddl_lines(Ddl, DdlLines),
    rel_columns_lines(RelPlans, RelColumnsLines),
    rel_column_types_lines(RelPlans, RelColumnTypesLines),
    arrival_targets_lines(ArrivalTargets, ArrivalTargetsLines),
    boot_lines(BootStatements, BootLines),
    snapshot_type_lines(RelPlans, SnapshotTypeLines),
    read_snapshot_fn_lines(DeltaStatements, ReadSnapshotFnLines),
    final_select_lines(DeltaStatements, FinalSelectLines),
    arrival_statements_lines(ArrivalStatements, ArrivalStatementsLines),
    arrival_statement_fn_lines(Name, ArrivalStatementFnLines),
    incremental_relation_lines(RelPlans, ArrivalStatements, DeltaStatements, DepartureRefs, IncrementalRelationLines),
    incremental_edge_statement_lines(EdgeStatements, RelPlans, IncrementalEdgeStatementLines),
    incremental_level_statement_lines(RuleLevelStatements, RelPlans, IncrementalLevelStatementLines),
    incremental_retention_statement_lines(RetentionStatements,
                                          IncrementalRetentionStatementLines),
    ( EdgeStatements == []
    -> EdgeConstLines = [], EdgeFnLines = []
    ; edge_resolver_blocks(EdgeStatements, RelPlans, EdgeConstLines, EdgeFnLines)
    ),
    ordered_pre_lines(HasOrderedProgram, RelPlans, PreRefs, EdgeStatements,
                      OrderedPreLines),
    ordered_occurrence_lines(HasOrderedProgram, EdgeStatements, RelPlans,
                             PreRefs, LevelHeadedRefs,
                             OrderedOccurrenceLines),
    ordered_carry_lines(HasOrderedProgram, EdgeStatements, LevelHeadedRefs,
                        OrderedCarryLines),
    recompute_levels_fn_lines(RuleLevelStatements, RecomputeLevelsFnLines),
    naive_retention_fn_lines(RetentionStatements, NaiveRetentionFnLines),
    build_deltas_fn_lines(RelPlans, EdgeStatements, RetentionStatements,
                          DepartureRefs, BuildDeltasFnLines),
    Plan = plan(_, TickProg, _, _, _, _),
    program_uses_tick(TickProg, UsesTick),
    advance_tick_fn_lines(UsesTick, AdvanceTickFnLines),
    run_naive_tick_fn_lines(Name, EdgeStatements, HasRetention, UsesTick,
                            DepartureRefs, HasStructTypes, RunNaiveTickFnLines),
    run_ordered_tick_fn_lines(HasOrderedProgram, Name, HasRetention, UsesTick,
                              DepartureRefs, HasStructTypes,
                              RunOrderedTickFnLines),
    incremental_program_safe(Plan, EdgeStatements, RuleLevelStatements, IncrementalSafe),
    reconcile_every_tick(Plan, ReconcileEveryTick),
    derived_edge_carry_required(Plan, EdgeStatements, DerivedEdgeCarryRequired),
    retraction_guard(Plan, RetractionGuard),
    incremental_mode_lines(IncrementalSafe, ReconcileEveryTick,
                           IncrementalModeLines),
    run_incremental_tick_fn_lines(EdgeStatements, DerivedEdgeCarryRequired,
                                  HasRetention, UsesTick, DepartureRefs,
                                  HasStructTypes, HasOrderedProgram,
                                  RunIncrementalTickFnLines),
    struct_tick_wrapper_lines(HasStructTypes, Name, StructTickWrapperLines),
    incremental_plan_export_lines(RetractionGuard, HasRetention,
                                  IncrementalPlanExportLines),
    program_export_lines(Name, ProgramExportLines),
    Sections0 =
    [ HeaderLines, ImportLines, LocalTypeLines, WorldPlanLines,
      BindArgsHelperLines, ArrivalValueGuardLines, TriggerOccurrencesHelperLines,
      DepartureOccurrencesHelperLines,
      StructPlaneLines,
      DdlLines, RelColumnsLines, RelColumnTypesLines, ArrivalTargetsLines,
      BootLines, SnapshotTypeLines, ReadSnapshotFnLines, FinalSelectLines,
      ArrivalStatementsLines, ArrivalStatementFnLines,
      IncrementalRelationLines, IncrementalEdgeStatementLines,
      IncrementalLevelStatementLines, IncrementalRetentionStatementLines,
      EdgeConstLines, EdgeFnLines,
      OrderedPreLines, OrderedOccurrenceLines, OrderedCarryLines,
      RecomputeLevelsFnLines, NaiveRetentionFnLines, BuildDeltasFnLines,
      AdvanceTickFnLines, RunNaiveTickFnLines, RunOrderedTickFnLines,
      IncrementalModeLines, RunIncrementalTickFnLines,
      StructTickWrapperLines, IncrementalPlanExportLines,
      ProgramExportLines
    ],
    exclude(==([]), Sections0, Sections),
    maplist(lines_block, Sections, SectionTexts),
    atomic_list_concat(SectionTexts, '\n\n', Body),
    format(atom(Text), '~w\n', [Body]).

is_level_statement(levelstmt(_, _, _, _, _, _)).
is_retention_statement(retentionstmt(_, _, _)).
