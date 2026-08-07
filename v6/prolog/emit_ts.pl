% Emit lowered compiler plans as TypeScript modules.

% The four extra exports form the emitter-mode seam.
:- module(emit_ts,
          [ emit_program/5,
            incremental_program_safe/4,
            reconcile_every_tick/2,
            derived_edge_carry_required/3,
            retraction_guard/2 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
% relplan_kind/3 supplies trigger kind to the per-arm resolver.
:- use_module(lower, [ relplan_kind/3, departure_frontier_table_name/2,
                       departure_read_sql/3, struct_type_plans/2,
                       program_text_intern_plan/3,
                       statement_rule_ids/3 ]).
:- use_module(analyze,
              [ body_ref_uses/2, derived_refs/2, rule_head_ref/2,
                program_uses_tick/2, listened_departure_refs/2,
                level_body_pre_ref/2, rel_rule_observers_map/2 ]).
:- use_module('1_host_expand', [compile_host_decl/2, compile_query/2]).
:- use_module('compile/registry', [bind_executor/2, host_execution/3]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

% ═══ small text helpers ══════════════════════════════════════════════════════

lines_block(Lines, Text) :- atomic_list_concat(Lines, '\n', Text).

% Escape backslashes, backticks, and `${` before embedding SQL in a template
% literal. Backslashes must be handled first.
%
% The backslash clause goes FIRST, or it would double the backslashes this
% predicate itself introduces for the other two.
js_template(SqlText, JsLiteral) :-
    atom_string(SqlText, SqlString),
    string_codes(SqlString, Codes),
    js_template_codes(Codes, Escaped),
    atom_codes(Body, Escaped),
    format(atom(JsLiteral), '`~w`', [Body]).

js_template_codes([], []).
js_template_codes([0'\\ | Rest], [0'\\, 0'\\ | More]) :-
    !,
    js_template_codes(Rest, More).
js_template_codes([0'` | Rest], [0'\\, 0'` | More]) :-
    !,
    js_template_codes(Rest, More).
js_template_codes([0'$, 0'{ | Rest], [0'\\, 0'$, 0'{ | More]) :-
    !,
    js_template_codes(Rest, More).
js_template_codes([Code | Rest], [Code | More]) :-
    js_template_codes(Rest, More).

% A plain JS identifier stays bare so existing modules are byte-identical;
% anything else is quoted rather than emitted as a syntax error.
js_object_key(Name, Key) :-
    atom_codes(Name, [First | Rest]),
    (   js_identifier_start(First),
        forall(member(Code, Rest), js_identifier_part(Code))
    ->  Key = Name
    ;   js_string(Name, Key)
    ).

js_identifier_start(Code) :-
    ( Code >= 0'a, Code =< 0'z ; Code >= 0'A, Code =< 0'Z ; Code =:= 0'_ ; Code =:= 0'$ ), !.

js_identifier_part(Code) :-
    ( js_identifier_start(Code) ; Code >= 0'0, Code =< 0'9 ), !.

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
    imports_lines(HasEdgeRules, HasRetention, false, false, false, [], Lines).

imports_lines(_HasEdgeRules, HasRetention, HasStructTypes, HasTextIntern,
              HasOrderedProgram,
              SelfReferentialLevelRefs, Lines) :-
    ( HasRetention == true
    -> RetentionImport = ['  IIncrementalRetentionStatement,']
    ; RetentionImport = []
    ),
    % The level fixpoint's three extra operators, imported only by the
    % programs that emit it, so every other module's import line is unchanged.
    ( SelfReferentialLevelRefs == []
    -> RxImportLine =
       'import { concatMap, forkJoin, map, of, type Observable } from "rxjs";'
    ;  RxImportLine =
       'import { concatMap, EMPTY, expand, forkJoin, last, map, of, type Observable } from "rxjs";'
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
    ( HasTextIntern == true
    -> TextImport = ['import { TextPlane } from "../runtime/textPlane.ts";'],
       TextTypeImports = ['  ITextInternPlan,']
    ;  TextImport = [], TextTypeImports = []
    ),
    append(
    [ [ RxImportLine,
      '',
      RuntimeImport,
      'import { SubscribeCone } from "../runtime/3_subscribe.ts";',
      'import { multisetDiff } from "../runtime/diff.ts";',
      'import { selectRows } from "../runtime/rows.ts";'
      ],
      StructImport,
      TextImport,
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
      '  IRelCatalogRow,',
      '  IRelDelta,',
      '  IRow,',
      '  IRowColumnType,',
      '  IRowValue,',
      '  ISqlSeam,'
      ],
      StructTypeImports,
      TextTypeImports,
      [
      '  ITickDeltas,',
      '  SqlStatement,',
      '} from "../runtime/types.ts";'
      ]
    ], Lines).

% ═══ the declared value plane (STRUCT-AS-ROWS) ══════════════════════════════
% Emitted ONLY for a program that declares a type. Every other module stays
% Programs without struct declarations receive no struct-plane output.

% The plan a direct-mode module has no use for is simply absent, along with
% its import and its statement.
text_intern_plan_lines(none, [], false) :- !.
text_intern_plan_lines(textintern(InternSql, LookupSql, RelColumns),
                       Lines, true) :-
    js_template(InternSql, InternTemplate),
    js_template(LookupSql, LookupTemplate),
    findall(EntryLine,
            ( member(Name-Flags, RelColumns),
              js_string(Name, NameKey),
              atomic_list_concat(Flags, ', ', FlagsText),
              format(atom(EntryLine), '    ~w: [~w],', [NameKey, FlagsText]) ),
            EntryLines),
    format(atom(InternLine), '  internSql: ~w,', [InternTemplate]),
    format(atom(LookupLine), '  lookupSql: ~w,', [LookupTemplate]),
    append(
    [ [ 'export const TEXT_INTERN_PLAN: ITextInternPlan = {',
        InternLine,
        LookupLine,
        '  relColumns: {' ],
      EntryLines,
      [ '  },', '};' ]
    ], Lines).

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
              ref_name(Ref, Name),
              maplist(column_type_ref_entry, ColumnTypes, RefTexts),
              atomic_list_concat(RefTexts, ', ', RefsText),
              js_string(Name, NameKey),
              format(atom(Line), '  ~w: [~w],', [NameKey, RefsText]) ),
            Lines).

column_type_ref_entry(ref(TypeName), Text) :- !, js_string(TypeName, Text).
column_type_ref_entry(_, 'null').

% Relation references normalize inside each emitter mode after that mode has
% opened its tick boundary. Target rows pass through the same arrival
% applicator as authored rows, then parent fields carry the resolved integer
% endpoints. No second externally visible tick or reference-value runtime
% exists.
struct_tick_wrapper_lines(_, _, []).

% Before StructPlane and before any level statement: a rewritten row must
% never reach a statement that would store a string in an id column.
naive_text_intern_lines(false, []) :- !.
naive_text_intern_lines(true,
    [ '    concatMap((before) => TextPlane.intern(seam, TEXT_INTERN_PLAN, arrivals)',
      '      .pipe(map((interned) => { arrivals = interned; return before; }))),'
    ]).

incremental_text_intern_lines(false, []) :- !.
incremental_text_intern_lines(true,
    [ '    concatMap(() => TextPlane.intern(seam, TEXT_INTERN_PLAN, arrivals)',
      '      .pipe(map((interned) => { arrivals = interned; }))),'
    ]).

naive_reference_normalize_lines(false, []) :- !.
naive_reference_normalize_lines(true,
    [ '    concatMap((before) => StructPlane.intern(seam, STRUCT_TYPES, STRUCT_REF_COLUMNS, arrivals,',
      '      (targets) => applyArrivals(seam, targets),',
      '    ).pipe(map((normalized) => { arrivals = normalized; return before; }))),'
    ]).

incremental_reference_normalize_lines(false, []) :- !.
incremental_reference_normalize_lines(true,
    [ '    concatMap(() => StructPlane.intern(seam, STRUCT_TYPES, STRUCT_REF_COLUMNS, arrivals,',
      '      (targets) => IncrementalRuntime.applyArrivals(seam, targets, SUBSCRIBED_RELATIONS),',
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
      'interface IQueryPlanData { readonly rel: string; readonly arity: number; readonly columns: readonly (IRowValue | null)[]; readonly bound: readonly number[]; readonly snapshot: "current" }',
      '',
      'interface IBootStatement {',
      '  rel: string;',
      '  sql: string;',
      '  params: readonly IRowValue[];',
      '}',
      '',
      'type IGenProgramWithBoot = IGenProgram & { readonly boot: readonly IBootStatement[]; readonly finalSelect: Record<string, string>; readonly hostPlans: readonly IHostPlanData[]; readonly bindPlans: readonly IBindPlanData[]; readonly queryPlans: readonly IQueryPlanData[]; readonly subscribedRels: readonly string[]; readonly relCatalog: readonly IRelCatalogRow[]; readonly unsupportedExecution: readonly string[] };'
    ]).

world_plan_lines(plan(_, prog(Decls, Rules), _, _, _, _, SubscribedRels, _), Lines) :-
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
    % compile_query/2 is the ONE definition of the plan term; every query decl
    % reaching here already went through it in prepare_program/5, so calling it
    % again cannot introduce a refusal this door did not already raise.
    findall(QueryPlan,
            ( member(Query, Decls),
              Query = query(_),
              compile_query(Query, QueryPlan)
            ),
            QueryPlans),
    maplist(host_plan_json, HostPlans, HostRows),
    maplist(bind_plan_json, BindPlans, BindRows),
    maplist(query_plan_json, QueryPlans, QueryRows),
    maplist(subscribed_rel_json, SubscribedRels, SubscribedRows),
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
    array_const_line('export const subscribedRels: readonly string[]',
                     SubscribedRows, SubscribedLine),
    array_const_line('export const unsupportedExecution: readonly string[]',
                     Refusals, RefusalLine),
    Lines = [HostLine, BindLine, QueryLine, SubscribedLine, RefusalLine].

% One cone member as the emitted module spells it: the "name/arity" string
% compile.pl:program_plan/2 computed, never re-derived out here.
subscribed_rel_json(Name/Arity, Json) :-
    format(atom(Ref), '~w/~w', [Name, Arity]),
    js_string(Ref, Json).

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

% `columns` is one entry per position of the query atom -- the pinned literal,
% or null where the position is free -- and `bound` lists the pinned positions,
% 0-based. Those positions are the demand keys a later consumer slices on, so
% it reads (rel, arity, columns, bound) off this line and never re-parses the
% surface.
%
% The atom comes out of the POST-expansion Decls of plan/6, so a position an
% expansion phase has not reduced to a literal (a dot chain, an arithmetic
% expression) is free here rather than a key: a key is a value, and this line
% cannot invent one it does not hold.
query_plan_json(query_plan(Name/Arity, columns(Args), snapshot(current)), Json) :-
    js_string(Name, NameJson),
    maplist(query_column_text, Args, ColumnTexts),
    atomic_list_concat(ColumnTexts, ', ', ColumnBody),
    findall(Position,
            ( nth0(Position, Args, Arg), query_column_pinned(Arg) ),
            Bound),
    atomic_list_concat(Bound, ', ', BoundBody),
    format(atom(Json),
           '{ rel: ~w, arity: ~w, columns: [~w], bound: [~w], snapshot: "current" }',
           [NameJson, Arity, ColumnBody, BoundBody]).

query_column_text(Arg, Text) :-
    ( query_column_pinned(Arg)
    -> param_text(Arg, Text)
    ;  Text = null
    ).

query_column_pinned(Arg) :-
    nonvar(Arg),
    ( Arg = bool_lit(_) -> true ; atomic(Arg) ).

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
      '  return values.map((value) => typeof value === "boolean" ? BigInt(value ? 1 : 0) : (typeof value === "number" && Number.isSafeInteger(value) ? BigInt(value) : value));',
      '}'
    ]).

% ═══ the arrival type gate ══════════════════════════════════════════════════
% The TS mirror of 0_type_plane.pl:world_row_shape_violation/3. Ruling
% type_gate_widening = arrival_gate_all_types_all_positions: EVERY declared
% column type is checked, not just the three numeric-ish ones, and the refusal
% NAME is the oracle's own `type_arrival_shape_mismatch` so the two doors
% answer the same program with the same word. The one place types are allowed
% to mix is SQLite affinity's numeric widening: an integer at a REAL column
% widens to a float and is accepted, which is what the engine now does too.
%
% The gate is driven by `relDeclaredColumnTypes`, NOT by `relColumnTypes`.
% The latter carries analyze.pl's INFERRED types (a bare column with an
% integer witness types int), and the engine's gate is decl-driven only, so
% gating on inferred types would refuse programs the reference engine runs.
% A rel with no declaration, or a partially typed one, gets no per-column
% check here -- the same all-or-nothing rule as ref_column_names/4.
%
% The wide-integer scan (ruling wide_int_fate) runs FIRST and is
% decl-independent, matching row_column_violation/8's first pass: an integer
% past Number.MAX_SAFE_INTEGER is refused at whatever column it arrived in,
% including inside a json document, including a rel with no colon types.
%
% NAMED CRACK, and it is a JavaScript one: this side cannot tell the integer
% 1e20 from the float 1.0e20, because JS has one number type. At a `float`
% column the declaration settles it and the scan is skipped; inside a json
% document the SOURCE TEXT settles it and the scan reads number tokens rather
% than parsed values. At a column with NO declaration neither is available,
% so an integral float past 2^53 is refused here and accepted by the engine.
% Declaring the column is the fix; nothing else can be.
arrival_value_guard_lines(
    [ 'const SAFE_INTEGER_LIMIT = 9007199254740991n;',
      '',
      'function wideIntegerWitness(value: unknown): boolean {',
      '  if (typeof value === "bigint") return value < -SAFE_INTEGER_LIMIT || value > SAFE_INTEGER_LIMIT;',
      '  if (typeof value === "number") return Number.isInteger(value) && !Number.isSafeInteger(value);',
      '  return false;',
      '}',
      '',
      '/** A json/list column arrives as the document TEXT, and the text is the',
      ' *  only place the int-versus-float distinction still exists: a JSON',
      ' *  number token with no `.` and no exponent IS an integer, which is',
      ' *  exactly how the prolog reader parses it. String contents are blanked',
      ' *  first so digits inside a string never read as a number. Unparseable',
      ' *  text is not this scan\'s business (the json arm below names it). */',
      'const JSON_NUMBER = /-?\\d+(?:\\.\\d+)?(?:[eE][+-]?\\d+)?/g;',
      '',
      'function wideIntegerInJsonText(value: IRowValue): boolean {',
      '  if (typeof value !== "string") return wideIntegerWitness(value);',
      '  const withoutStrings = value.replace(/"(?:\\\\.|[^"\\\\])*"/g, \'""\');',
      '  for (const token of withoutStrings.match(JSON_NUMBER) ?? []) {',
      '    if (/[.eE]/.test(token)) continue;',
      '    const parsed = BigInt(token);',
      '    if (parsed < -SAFE_INTEGER_LIMIT || parsed > SAFE_INTEGER_LIMIT) return true;',
      '  }',
      '  return false;',
      '}',
      '',
      'function validateArrivals(arrivals: IArrivalBatch): IArrivalBatch {',
      '  return arrivals.map((arrival): IArrivalRow => {',
      '    const types = relColumnTypes[arrival.rel];',
      '    if (types === undefined || types.length !== arrival.row.length) throw new Error(`arrival shape mismatch for ${arrival.rel}`);',
      '    const declared = relDeclaredColumnTypes[arrival.rel];',
      '    const row = arrival.row.map((value, index): IRowValue => {',
      '      const type = declared === undefined ? undefined : declared[index];',
      '      const scanned = type === "json" ? wideIntegerInJsonText(value)',
      '        : type === "float" ? false',
      '        : wideIntegerWitness(value);',
      '      if (scanned) throw new Error(`int_out_of_range ${arrival.rel}[${index}]`);',
      '      if (type === "bool") {',
      '        if (typeof value !== "boolean") throw new Error(`type_arrival_shape_mismatch ${arrival.rel}[${index}] field_not_bool`);',
      '        return value;',
      '      }',
      '      if (type === "float") {',
      '        // SQLite REAL affinity: an integer widens, everything else is refused.',
      '        if (typeof value !== "number" || !Number.isFinite(value)) throw new Error(`type_arrival_shape_mismatch ${arrival.rel}[${index}] field_not_finite_float`);',
      '        return Object.is(value, -0) ? 0 : value;',
      '      }',
      '      if (type === "int") {',
      '        if (typeof value !== "number" || !Number.isInteger(value)) throw new Error(`type_arrival_shape_mismatch ${arrival.rel}[${index}] field_not_int`);',
      '        return value;',
      '      }',
      '      if (type === "text") {',
      '        if (typeof value !== "string") throw new Error(`type_arrival_shape_mismatch ${arrival.rel}[${index}] field_not_text`);',
      '        return value;',
      '      }',
      '      if (type === "json") {',
      '        // The shared reader (compile/scripts/0_json_arrival.pl',
      '        // json_column_term/4) takes a document TEXT or a bare number,',
      '        // and refuses everything else. Same two shapes here.',
      '        if (typeof value === "number") return value;',
      '        if (typeof value !== "string") throw new Error(`type_arrival_shape_mismatch ${arrival.rel}[${index}] field_not_json`);',
      '        try { JSON.parse(value); } catch { throw new Error(`type_arrival_shape_mismatch ${arrival.rel}[${index}] field_not_json`); }',
      '        return value;',
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
    js_object_key(Name, NameKey),
    format(atom(Line), '  ~w: ~w,', [NameKey, ColumnsSql]).

rel_column_types_lines(RelPlans, Lines) :-
    maplist(rel_column_types_entry_line, RelPlans, EntryLines),
    append([ ['const relColumnTypes: Record<string, readonly IRowColumnType[]> = {'],
             EntryLines, ['};'] ], Lines).

rel_column_types_entry_line(relplan(Ref, _Kind, _Columns, _Key, ColumnTypes), Line) :-
    ref_name(Ref, Name),
    maplist(boundary_column_type, ColumnTypes, BoundaryTypes),
    quoted_string_array_text(BoundaryTypes, TypesText),
    js_object_key(Name, NameKey),
    format(atom(Line), '  ~w: ~w,', [NameKey, TypesText]).

% ═══ the catalog rows, the same list the INSERT renders ════════════════════
% Emitted even for a program that never queries `__rel`, so a reload compares.
program_catalog_rows(Name, plan(_, prog(_, Rules), _, _, _, _, _, _), RelPlans, Rows) :-
    lower:catalog_rows(Name, Rules, RelPlans, Rows).

rel_catalog_lines([], []) :- !.
rel_catalog_lines(Rows, Lines) :-
    maplist(rel_catalog_entry_line, Rows, EntryLines),
    append([ ['const relCatalog: readonly IRelCatalogRow[] = ['],
             EntryLines, ['];'] ], Lines).

rel_catalog_entry_line(row(RelId, ParentId, Ordinal, Name, Kind, TypeId, Arity,
                           ModuleId, HId, HSchema, HRule), Line) :-
    js_string(Name, NameText),
    js_string(Kind, KindText),
    js_string(HId, HIdText),
    js_string(HSchema, HSchemaText),
    js_string(HRule, HRuleText),
    format(atom(Line),
           '  { relId: ~w, parentId: ~w, ordinal: ~w, localName: ~w, kind: ~w, typeId: ~w, arity: ~w, moduleId: ~w, hId: ~w, hSchema: ~w, hRule: ~w },',
           [RelId, ParentId, Ordinal, NameText, KindText, TypeId, Arity,
            ModuleId, HIdText, HSchemaText, HRuleText]).

% ═══ the DECLARED column types (ruling type_gate_widening) ═════════════════
% What the program WROTE DOWN, as opposed to what analyze.pl inferred. The
% arrival gate reads this map and only this map, because the reference
% engine's gate is decl-driven: a column with an inferred type but no colon
% has no gate on either door.
%
% Entered all-or-nothing per rel, mirroring 0_type_plane.pl:ref_column_names/4
% -- a partially typed decl would mis-locate its own positions, and the engine
% declines to guess there, so this declines too.
rel_declared_column_types_lines(Decls, RelPlans, Lines) :-
    findall(EntryLine,
            ( member(relplan(Ref, _, _, _, _), RelPlans),
              declared_column_types(Decls, Ref, DeclaredTypes),
              rel_declared_types_entry_line(Ref, DeclaredTypes, EntryLine) ),
            EntryLines),
    append([ ['const relDeclaredColumnTypes: Record<string, readonly string[]> = {'],
             EntryLines, ['};'] ], Lines).

declared_column_types(Decls, Ref, Types) :-
    Ref = _/Arity,
    findall(Type, member(col_type(Ref, _, Type), Decls), Types),
    length(Types, Arity).

rel_declared_types_entry_line(Ref, DeclaredTypes, Line) :-
    ref_name(Ref, Name),
    maplist(gate_column_type, DeclaredTypes, GateTypes),
    quoted_string_array_text(GateTypes, TypesText),
    format(atom(Line), '  ~w: ~w,', [Name, TypesText]).

% The five words the emitted guard switches on. Anything else -- a struct
% type name, or a future column type -- renders as `other` and is left
% unchecked at this seam, which is what the engine's own
% column_value_shape_error/4 does with it too (struct shape is checked by
% compile.pl's check_world_shapes on the static rows).
gate_column_type(int,   int)   :- !.
gate_column_type(float, float) :- !.
gate_column_type(bool,  bool)  :- !.
gate_column_type(text,  text)  :- !.
gate_column_type(json,  json)  :- !.
gate_column_type(list(_), json) :- !.
gate_column_type(_,     other).

boundary_column_type(ref(_), ref) :- !.
% A `json` column keeps its own name at the driver seam. JSON scalars and
% strings cannot be classified by inspecting only the first character.
%
%   json column holding `42`        -> string "42", misses the first-char
%                                     sniff, prints as "42" where the oracle
%                                     prints 42. Fifteen of twenty-three
%                                     value kinds take that path, including
%                                     null, true and every number.
%   text column holding `{"a":1}`   -> HITS the sniff and prints as an
%                                     object where the oracle prints a string.
%
% The type is the only thing that separates those two, and it is already in
% hand here. `rowValueFromSql` needs no new arm (json passes through the same
% default text does); the seam that switches on it is ticklog.ts's encoder.
boundary_column_type(json, json) :- !.
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

boot_entry_line(bootstmt(Rel, Sql, Params), Line) :-
    js_string(Rel, RelText),
    js_template(Sql, Template),
    params_array_text(Params, ParamsText),
    format(atom(Line), '  { rel: ~w, sql: ~w, params: ~w },',
           [RelText, Template, ParamsText]).

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

% ═══ finalSelect (final-state grading leg) ════════════════════════════════════
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
    js_object_key(Name, NameKey),
    format(atom(Line), '  ~w: ~w,', [NameKey, Template]).

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
    js_object_key(Name, NameKey),
    format(atom(Line), '  ~w: { kind: "log", addSql: ~w, delSql: null },', [NameKey, AddTemplate]).
arrival_statement_entry_line(arrivalstmt(Ref, set, AddSql, DelSql, _, _), Line) :-
    ref_name(Ref, Name),
    js_template(AddSql, AddTemplate),
    js_template(DelSql, DelTemplate),
    js_object_key(Name, NameKey),
    format(atom(Line), '  ~w: { kind: "set", addSql: ~w, delSql: ~w },', [NameKey, AddTemplate, DelTemplate]).

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

incremental_relation_lines(RelPlans, Rules, ArrivalStatements, DeltaStatements,
                           DepartureRefs, Lines) :-
    rel_rule_observers_map(Rules, ObserverMap),
    maplist(incremental_relation_entry_line(RelPlans, ObserverMap, ArrivalStatements,
                                            DepartureRefs),
            DeltaStatements, EntryLines),
    append(
        [ ['const INCREMENTAL_RELATIONS: readonly IIncrementalRelationPlan[] = ['],
          EntryLines,
          ['];']
        ], Lines).

incremental_relation_entry_line(RelPlans, ObserverMap, ArrivalStatements, DepartureRefs,
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
    % ruleObservers is emitted on EVERY relation entry, empty array when no
    % rule reads this rel's event tables, so the runtime's boot-time skip has
    % a per-rel observer set to test against.
    (   memberchk(Ref-Observers, ObserverMap)
    ->  true
    ;   Observers = []
    ),
    rel_ref_text_list(Observers, ObserverRefTexts),
    quoted_string_array_text(ObserverRefTexts, ObserversText),
    format(atom(Line),
           '  { rel: "~w", kind: "~w", tableName: "~w", deltaTableName: "~w", frontierTableName: "~w", nextFrontierTableName: "~w", columns: ~w, columnTypes: ~w, keyIndices: [~w], arrivalAddSql: ~w, arrivalDelSql: ~w, boundarySql: ~w~w, ruleObservers: ~w },',
           [Name, Kind, Name, DeltaTable, FrontierTable, NextFrontierTable,
            ColumnsText, ColumnTypesText, KeyIndicesText, ArrivalAddTemplate, ArrivalDelTemplate,
            BoundaryTemplate, DepartureField, ObserversText]).

rel_ref_text_list([], []) :- !.
rel_ref_text_list([Name/Arity | Rest], [Text | More]) :-
    format(atom(Text), '~w/~w', [Name, Arity]),
    rel_ref_text_list(Rest, More).

position_index(Position, Index) :- Index is Position - 1.

incremental_edge_statement_lines(Program, EdgeStatements, RelPlans, Lines) :-
    maplist(edge_statement_head_ref, EdgeStatements, HeadRefs),
    statement_rule_ids(Program, HeadRefs, RuleIds),
    maplist(incremental_edge_statement_entry_line(RelPlans), EdgeStatements, RuleIds, EntryLines),
    append(
        [ ['const INCREMENTAL_EDGE_STATEMENTS: readonly IIncrementalEdgeStatement[] = ['],
          EntryLines,
          ['];']
        ], Lines).

edge_statement_head_ref(edgestmt(HeadRef, _, _, _, _, _, _, _), HeadRef).

incremental_edge_statement_entry_line(RelPlans,
        edgestmt(HeadRef, _TriggerRef, HeadColumns, KeyColumns, _ProjectSql,
                 _WriteSql, DeltaProjectSql, _EdgeTriggerKind), RuleId, Line) :-
    ref_name(HeadRef, HeadName),
    relplan_kind(RelPlans, HeadRef, HeadKind),
    format(atom(DeltaTable), '__delta_~w', [HeadName]),
    quoted_string_array_text(HeadColumns, ColumnsText),
    key_indices(HeadColumns, KeyColumns, KeyIndices),
    atomic_list_concat(KeyIndices, ', ', KeyIndicesText),
    js_template(DeltaProjectSql, DeltaProjectTemplate),
    format(atom(Line),
           '  { headRel: "~w", ruleId: "~w", headKind: "~w", headTableName: "~w", headDeltaTableName: "~w", headColumns: ~w, keyIndices: [~w], projectSql: ~w },',
           [HeadName, RuleId, HeadKind, HeadName, DeltaTable, ColumnsText,
            KeyIndicesText, DeltaProjectTemplate]).

incremental_level_statement_lines(Program, LevelStatements, RelPlans, Lines) :-
    maplist(level_statement_head_ref, LevelStatements, HeadRefs),
    statement_rule_ids(Program, HeadRefs, RuleIds),
    maplist(incremental_level_statement_entry_line(RelPlans),
            LevelStatements, RuleIds, EntryLines),
    append(
        [ ['const INCREMENTAL_LEVEL_STATEMENTS: readonly IIncrementalLevelStatement[] = ['],
          EntryLines,
          ['];']
        ], Lines).

level_statement_head_ref(levelstmt(HeadRef, _, _, _, _, _), HeadRef).

incremental_level_statement_entry_line(RelPlans,
        levelstmt(HeadRef, DeleteSql, InsertSqls, DeltaInsertSql, RefCountSql,
                  AggregateSql), RuleId, Line) :-
    ref_name(HeadRef, HeadName),
    format(atom(DeltaTable), '__delta_~w', [HeadName]),
    memberchk(relplan(HeadRef, _, HeadColumns, _, _), RelPlans),
    quoted_string_array_text(HeadColumns, ColumnsText),
    optional_sql_template(DeltaInsertSql, DeltaInsertTemplate),
    maplist(quote_ident_local, HeadColumns, QuotedHeadColumns),
    atomic_list_concat(QuotedHeadColumns, ', ', HeadColumnsSql),
    format(atom(SelectSql), 'SELECT ~w FROM "~w"', [HeadColumnsSql, HeadName]),
    js_template(SelectSql, SelectTemplate),
    % A REAL newline, not the two-character sequence `\n`. These three joins
    % used to emit backslash-n and rely on the JS template literal to turn it
    % into a newline -- which is the same conflation js_template/2 above stopped
    % making. A template literal carries a raw newline fine, and the emitted
    % statement text is now the text sqlite receives, byte for byte.
    atomic_list_concat([DeleteSql | InsertSqls], ';\n', RecomputeSql),
    js_template(RecomputeSql, RecomputeTemplate),
    ref_count_sql_text(RefCountSql, RefCountText, ExpandText, DredText,
                       FixpointIrText),
    aggregate_sql_text(AggregateSql, AggregateText),
    format(atom(Line),
           '  { headRel: "~w", ruleId: "~w", headDeltaTableName: "~w", headColumns: ~w, insertSql: ~w, selectSql: ~w, recomputeSql: ~w, supportSql: ~w, expandSql: ~w, dredSql: ~w, fixpointIr: ~w, aggregateSql: ~w },',
           [HeadName, RuleId, DeltaTable, ColumnsText, DeltaInsertTemplate,
            SelectTemplate, RecomputeTemplate, RefCountText, ExpandText,
            DredText, FixpointIrText, AggregateText]).

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

ref_count_sql_text(none, null, null, null, null) :- !.
ref_count_sql_text(refcountsql(ClearSql, SeedSql, UpdateSql, StageRetractSql,
                               CollectZeroSql, ClearNewSql, FillNewSql,
                               StageAddSql, StageFrontierSql,
                               StageNextFrontierSql, InsertNewSql, ExpandPlan,
                               DredPlan, FixpointIr),
                 Text, ExpandText, DredText, FixpointIrText) :-
    maplist(js_template,
            [ClearSql, SeedSql, UpdateSql, StageRetractSql, CollectZeroSql,
             ClearNewSql, FillNewSql, StageAddSql, StageFrontierSql,
             StageNextFrontierSql, InsertNewSql],
            Templates),
    atomic_list_concat(Templates, ', ', Joined),
    format(atom(Text), '[~w]', [Joined]),
    expand_sql_text(ExpandPlan, ExpandText),
    dred_sql_text(DredPlan, DredText),
    fixpoint_ir_text(FixpointIr, FixpointIrText).

expand_sql_text(none, null) :- !.
expand_sql_text(expandplan(ClearASql, ClearBSql, SeedSqls, HopABSql, HopBASql,
                           AbsorbASql, AbsorbBSql),
                Text) :-
    maplist(js_template, [ClearASql, ClearBSql, HopABSql, HopBASql,
                          AbsorbASql, AbsorbBSql],
            [ClearATemplate, ClearBTemplate, HopABTemplate, HopBATemplate,
             AbsorbATemplate, AbsorbBTemplate]),
    maplist(js_template, SeedSqls, SeedTemplates),
    atomic_list_concat(SeedTemplates, ', ', SeedJoined),
    format(atom(Text),
           '{ clearASql: ~w, clearBSql: ~w, seedSqls: [~w], hopABSql: ~w, hopBASql: ~w, absorbASql: ~w, absorbBSql: ~w }',
           [ClearATemplate, ClearBTemplate, SeedJoined, HopABTemplate,
            HopBATemplate, AbsorbATemplate, AbsorbBTemplate]).

dred_sql_text(none, null) :- !.
dred_sql_text(dredplan(ClearPingSql, ClearPongSql, ClearConeSql,
                       AssertSeedSqls, AssertHopABSql, AssertHopBASql,
                       CommitASql, CommitBSql, ArrivalASql, ArrivalBSql,
                       DredSeedSqls, DredHopABSql, DredHopBASql,
                       ConeAbsorbASql, ConeAbsorbBSql, ConeTrimSql,
                       HeadDeleteSql, RederiveSeedSqls, ReviveHopABSql,
                       ReviveHopBASql, ConeDropASql, ConeDropBSql,
                       StageRetractSql, HeadCountSql),
              Text) :-
    maplist(js_template,
            [ClearPingSql, ClearPongSql, ClearConeSql, AssertHopABSql,
             AssertHopBASql, CommitASql, CommitBSql, ArrivalASql, ArrivalBSql,
             DredHopABSql, DredHopBASql, ConeAbsorbASql, ConeAbsorbBSql,
             ConeTrimSql, HeadDeleteSql, ReviveHopABSql, ReviveHopBASql,
             ConeDropASql, ConeDropBSql, StageRetractSql, HeadCountSql],
            [ClearPingT, ClearPongT, ClearConeT, AssertHopABT, AssertHopBAT,
             CommitAT, CommitBT, ArrivalAT, ArrivalBT, DredHopABT, DredHopBAT,
             ConeAbsorbAT, ConeAbsorbBT, ConeTrimT, HeadDeleteT, ReviveHopABT,
             ReviveHopBAT, ConeDropAT, ConeDropBT, StageRetractT, HeadCountT]),
    sql_template_array(AssertSeedSqls, AssertSeedText),
    sql_template_array(DredSeedSqls, DredSeedText),
    sql_template_array(RederiveSeedSqls, RederiveSeedText),
    format(atom(Text),
           '{ clearPingSql: ~w, clearPongSql: ~w, clearConeSql: ~w, assertSeedSqls: ~w, assertHopABSql: ~w, assertHopBASql: ~w, commitASql: ~w, commitBSql: ~w, arrivalASql: ~w, arrivalBSql: ~w, dredSeedSqls: ~w, dredHopABSql: ~w, dredHopBASql: ~w, coneAbsorbASql: ~w, coneAbsorbBSql: ~w, coneTrimSql: ~w, headDeleteSql: ~w, rederiveSeedSqls: ~w, reviveHopABSql: ~w, reviveHopBASql: ~w, coneDropASql: ~w, coneDropBSql: ~w, stageRetractSql: ~w, headCountSql: ~w }',
           [ClearPingT, ClearPongT, ClearConeT, AssertSeedText, AssertHopABT,
            AssertHopBAT, CommitAT, CommitBT, ArrivalAT, ArrivalBT,
            DredSeedText, DredHopABT, DredHopBAT, ConeAbsorbAT, ConeAbsorbBT,
            ConeTrimT, HeadDeleteT, RederiveSeedText, ReviveHopABT,
            ReviveHopBAT, ConeDropAT, ConeDropBT, StageRetractT, HeadCountT]).

sql_template_array(Sqls, Text) :-
    maplist(js_template, Sqls, Templates),
    atomic_list_concat(Templates, ', ', Joined),
    format(atom(Text), '[~w]', [Joined]).

% lower.pl:level_fixpoint_ir/4 printed as an object literal, additive beside
% expandSql/dredSql (plans/2026-08-07-plan-ir-offload-contract.md §2.4).
fixpoint_ir_text(none, null) :- !.
fixpoint_ir_text(fixpointir(Storage, Assert, Dred, Revive, Expand), Text) :-
    Assert = fixplan(ref(HeadName, _), Columns, ColumnTypes, _, _, _, _),
    quoted_string_array_text(Columns, ColumnsText),
    quoted_string_array_text(ColumnTypes, TypesText),
    fixpoint_term_array_text(fixpoint_storage_text, Storage, StorageText),
    maplist(fixpoint_walk_text, [Assert, Dred, Revive, Expand],
            [AssertText, DredText, ReviveText, ExpandText]),
    format(atom(Text),
           '{ head: { rel: "~w", columns: ~w, types: ~w }, storage: ~w, assert: ~w, dred: ~w, revive: ~w, expand: ~w }',
           [HeadName, ColumnsText, TypesText, StorageText, AssertText, DredText,
            ReviveText, ExpandText]).

% lower.pl:ir_column_class/3. Named keys, so the interning contract adds one
% without moving anything an executor already reads.
fixpoint_storage_text(relstorage(ref(Name, Arity), ColumnClasses), Text) :-
    fixpoint_term_array_text(fixpoint_column_class_text, ColumnClasses,
                             ClassesText),
    format(atom(Text), '{ rel: "~w", arity: ~w, columns: ~w }',
           [Name, Arity, ClassesText]).

fixpoint_column_class_text(colclass(Column, Type, StorageClass, Collation,
                                    Encoding), Text) :-
    js_string(Column, ColumnText),
    fixpoint_collation_text(Collation, CollationText),
    fixpoint_encoding_text(Encoding, EncodingText),
    format(atom(Text),
           '{ name: ~w, type: "~w", storage: "~w", collation: ~w, encoding: ~w }',
           [ColumnText, Type, StorageClass, CollationText, EncodingText]).

fixpoint_collation_text(none, null) :- !.
fixpoint_collation_text(Collation, Text) :- js_string(Collation, Text).

fixpoint_encoding_text(direct, '{ kind: "direct" }') :- !.
fixpoint_encoding_text(dict(Target), Text) :-
    format(atom(Text), '{ kind: "dict", rel: "~w" }', [Target]).

% `stop` carries both admission tests: the seeds' and the hop's differ on the
% over-delete and revive walks (lower.pl:level_dred_plan/4).
fixpoint_walk_text(fixplan(_, _, _, Seeds, Hops, stop(SeedProbe, HopProbe),
                           Emit), Text) :-
    fixpoint_arm_array_text(Seeds, SeedsText),
    fixpoint_arm_array_text(Hops, HopsText),
    fixpoint_probe_text(SeedProbe, SeedProbeText),
    fixpoint_probe_text(HopProbe, HopProbeText),
    fixpoint_emit_text(Emit, EmitText),
    format(atom(Text),
           '{ seeds: ~w, hop: ~w, stop: { seed: ~w, hop: ~w }, emit: ~w }',
           [SeedsText, HopsText, SeedProbeText, HopProbeText, EmitText]).

fixpoint_arm_array_text(Arms, Text) :-
    maplist(fixpoint_arm_text, Arms, ArmTexts),
    atomic_list_concat(ArmTexts, ', ', Joined),
    format(atom(Text), '[~w]', [Joined]).

fixpoint_emit_text(none, null) :- !.
fixpoint_emit_text(order(Order), Text) :- js_string(Order, Text).

fixpoint_probe_text(none, null) :- !.
fixpoint_probe_text(probe(Kind, Target), Text) :-
    fixpoint_probe_target(Target, TargetName),
    format(atom(Text), '{ kind: "~w", target: "~w" }', [Kind, TargetName]).

fixpoint_probe_target(ref_count, refCount) :- !.
fixpoint_probe_target(Target, Target).

fixpoint_arm_text(arm(Sources, Equalities, Filters, Project, SelfIndex),
                  Text) :-
    fixpoint_term_array_text(fixpoint_source_text, Sources, SourcesText),
    fixpoint_term_array_text(fixpoint_equality_text, Equalities,
                             EqualitiesText),
    fixpoint_term_array_text(fixpoint_filter_text, Filters, FiltersText),
    fixpoint_term_array_text(fixpoint_expr_text, Project, ProjectText),
    fixpoint_self_index_text(SelfIndex, SelfIndexText),
    format(atom(Text),
           '{ sources: ~w, equalities: ~w, filters: ~w, project: ~w, selfIndex: ~w }',
           [SourcesText, EqualitiesText, FiltersText, ProjectText,
            SelfIndexText]).

fixpoint_term_array_text(Renderer, Terms, Text) :-
    maplist(Renderer, Terms, Texts),
    atomic_list_concat(Texts, ', ', Joined),
    format(atom(Text), '[~w]', [Joined]).

fixpoint_self_index_text(none, null) :- !.
fixpoint_self_index_text(Index, Index).

fixpoint_source_text(src(Index, Source), Text) :-
    fixpoint_source_kind_text(Source, KindText),
    format(atom(Text), '{ index: ~w, source: ~w }', [Index, KindText]).

fixpoint_source_kind_text(rel(ref(Name, Arity)), Text) :- !,
    format(atom(Text), '{ kind: "rel", rel: "~w", arity: ~w }', [Name, Arity]).
fixpoint_source_kind_text(rel_or_retracted(ref(Name, Arity)), Text) :- !,
    format(atom(Text), '{ kind: "relOrRetracted", rel: "~w", arity: ~w }',
           [Name, Arity]).
fixpoint_source_kind_text(delta(ref(Name, Arity), Sign, liveness(Liveness)),
                          Text) :- !,
    format(atom(Text),
           '{ kind: "delta", rel: "~w", arity: ~w, sign: ~w, liveness: "~w" }',
           [Name, Arity, Sign, Liveness]).
fixpoint_source_kind_text(wave(Slot), Text) :- !,
    format(atom(Text), '{ kind: "wave", slot: "~w" }', [Slot]).
fixpoint_source_kind_text(cone, '{ kind: "cone" }').

fixpoint_equality_text(eq(Left, Right), Text) :-
    fixpoint_expr_text(Left, LeftText),
    fixpoint_expr_text(Right, RightText),
    format(atom(Text), '{ left: ~w, right: ~w }', [LeftText, RightText]).

fixpoint_filter_text(cmp(Operator, Left, Right), Text) :- !,
    js_string(Operator, OperatorText),
    fixpoint_expr_text(Left, LeftText),
    fixpoint_expr_text(Right, RightText),
    format(atom(Text), '{ kind: "cmp", op: ~w, left: ~w, right: ~w }',
           [OperatorText, LeftText, RightText]).
fixpoint_filter_text(eq_lit(Left, Literal), Text) :-
    fixpoint_expr_text(Left, LeftText),
    fixpoint_expr_text(Literal, LiteralText),
    format(atom(Text), '{ kind: "eqLit", left: ~w, right: ~w }',
           [LeftText, LiteralText]).

fixpoint_expr_text(col(Index, Ordinal), Text) :- !,
    format(atom(Text), '{ kind: "col", index: ~w, ordinal: ~w }',
           [Index, Ordinal]).
fixpoint_expr_text(lit(Literal), Text) :- !,
    fixpoint_literal_text(Literal, Text).
% `type` is compile_expr/4's result type: `/` over two ints is SQLite integer
% division, over anything else a REAL divide (lower.pl:arithmetic_rendering/6).
fixpoint_expr_text(arith(Operator, Left, Right, Type), Text) :- !,
    js_string(Operator, OperatorText),
    fixpoint_expr_text(Left, LeftText),
    fixpoint_expr_text(Right, RightText),
    format(atom(Text),
           '{ kind: "arith", op: ~w, type: "~w", left: ~w, right: ~w }',
           [OperatorText, Type, LeftText, RightText]).
fixpoint_expr_text(concat(Parts), Text) :-
    fixpoint_term_array_text(fixpoint_expr_text, Parts, PartsText),
    format(atom(Text), '{ kind: "concat", parts: ~w }', [PartsText]).

fixpoint_literal_text(text(Value), Text) :- !,
    js_string(Value, ValueText),
    format(atom(Text), '{ kind: "lit", type: "text", value: ~w }', [ValueText]).
fixpoint_literal_text(Literal, Text) :-
    Literal =.. [TypeName, Value],
    format(atom(Text), '{ kind: "lit", type: "~w", value: ~w }',
           [TypeName, Value]).

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
           '{ scopeClearSql: ~w, scopeSeedSql: [~w], deleteScopedSql: ~w, insertScopedSql: [~w], deltaMaintained: false }',
           [ScopeClearTemplate, ScopeSeedJoined, DeleteScopedTemplate,
            InsertScopedJoined]).
aggregate_sql_text(avgsql(_ScopeColumns, _ScopeTypes, ScopeClearSql, ScopeSeedSqls,
                          DeleteScopedSql, InsertScopedSqls, _BootSqls), Text) :-
    js_template(ScopeClearSql, ScopeClearTemplate),
    maplist(js_template, ScopeSeedSqls, ScopeSeedTemplates),
    atomic_list_concat(ScopeSeedTemplates, ', ', ScopeSeedJoined),
    js_template(DeleteScopedSql, DeleteScopedTemplate),
    maplist(js_template, InsertScopedSqls, InsertScopedTemplates),
    atomic_list_concat(InsertScopedTemplates, ', ', InsertScopedJoined),
    format(atom(Text),
           '{ scopeClearSql: ~w, scopeSeedSql: [~w], deleteScopedSql: ~w, insertScopedSql: [~w], deltaMaintained: true }',
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

plan_pre_refs(plan(_, prog(_, Rules), _, _, _, _, _, _), Refs) :-
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
    % Real newline; see the note at the recompute join.
    atomic_list_concat(SnapshotStatements, ';\n', SnapshotSql),
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
recompute_levels_fn_lines(_, [], Lines) :- !,
    Lines =
    [ 'function recomputeLevels(seam: ISqlSeam): Observable<void> {',
      '  void seam;',
      '  return of(undefined);',
      '}'
    ].
% THE LEVEL FIXPOINT. One DELETE-then-INSERT-per-clause pass gives a
% self-referential head exactly as many derivation rounds as it has clauses,
% from an empty table, every tick: a two-clause fold reaches two links and
% stops, whatever the data says (sprefa-lab-foldwall/FOLDWALL.md measured the
% ceiling tracking clause count exactly). Both doors that call this function
% carry the ceiling -- runOrderedTick, which any seq/1 or pre/1 program takes,
% and runNaiveTick.
%
% So the DELETE runs ONCE and the INSERT set repeats until a round adds no
% row. strat.pl:topo_order_group/2 refuses mutual recursion inside a stratum
% (recursive_stratum) and exempts only the self-edge, so a DIRECT self-read is
% the whole recursion surface, and a program with none of them still reaches
% its answer in the single pass below -- which is why that clause stays and
% keeps every non-recursive module's emitted text unchanged.
%
% Every clause is INSERT OR IGNORE, so a round can only add rows and the count
% is monotone; datalog closure over a finite store is what makes it stop.
recompute_levels_fn_lines(SelfReferentialLevelRefs, LevelStatements, Lines) :-
    SelfReferentialLevelRefs \== [],
    LevelStatements \== [],
    !,
    findall(DeleteSql,
            member(levelstmt(_, DeleteSql, _, _, _, _), LevelStatements),
            DeleteSqls),
    findall(InsertSql,
            ( member(levelstmt(_, _, InsertSqls, _, _, _), LevelStatements),
              member(InsertSql, InsertSqls) ),
            RoundInsertSqls),
    % Real newline; see the note at the recompute join.
    atomic_list_concat(DeleteSqls, ';\n', JoinedDeleteSql),
    atomic_list_concat(RoundInsertSqls, ';\n', JoinedInsertSql),
    js_template(JoinedDeleteSql, DeleteTemplate),
    js_template(JoinedInsertSql, InsertTemplate),
    level_row_count_sql(LevelStatements, CountSql),
    js_template(CountSql, CountTemplate),
    format(atom(DeleteLine), '  const deleteSql = ~w;', [DeleteTemplate]),
    format(atom(InsertLine), '  const insertSql = ~w;', [InsertTemplate]),
    format(atom(CountLine), '  const countSql = ~w;', [CountTemplate]),
    Lines =
    [ 'function recomputeLevels(seam: ISqlSeam): Observable<void> {',
      DeleteLine,
      InsertLine,
      CountLine,
      '  return seam.runner.executeMultiple(seam.db, deleteSql).pipe(',
      '    map(() => -1),',
      '    expand((priorRows) => seam.runner.executeMultiple(seam.db, insertSql).pipe(',
      '      concatMap(() => seam.runner.scalar(seam.db, countSql)),',
      '      concatMap((rows) => (rows === priorRows ? EMPTY : of(rows))),',
      '    )),',
      '    last(),',
      '    map(() => undefined),',
      '  );',
      '}'
    ].
recompute_levels_fn_lines(_, LevelStatements, Lines) :-
    LevelStatements \== [],
    % InsertSqls is a LIST (lower.pl:level_statement_group/3 -- one entry per
    % rule clause sharing this head, so a multi-clause head's rows all
    % INSERT after exactly one DELETE, never one DELETE per clause); flattens
    % to the identical [Delete, Insert] sequence as before for the common
    % single-clause case.
    findall(Sql, ( member(levelstmt(_, DeleteSql, InsertSqls, _, _, _), LevelStatements), ( Sql = DeleteSql ; member(Sql, InsertSqls) ) ), Sqls),
    % Real newline; see the note at the recompute join.
    atomic_list_concat(Sqls, ';\n', JoinedSql),
    js_template(JoinedSql, SqlTemplate),
    format(atom(SqlLine), '  const sql = ~w;', [SqlTemplate]),
    Lines =
    [ 'function recomputeLevels(seam: ISqlSeam): Observable<void> {',
      SqlLine,
      '  return seam.runner.executeMultiple(seam.db, sql);',
      '}'
    ].

% ISqlRunner.scalar/2 reads the first column of the first row, so the round
% count is one SELECT with no row shape to decode.
level_row_count_sql(LevelStatements, Sql) :-
    findall(CountExpr,
            ( member(levelstmt(HeadRef, _, _, _, _, _), LevelStatements),
              ref_name(HeadRef, HeadName),
              quote_ident_local(HeadName, QuotedHead),
              format(atom(CountExpr), '(SELECT count(*) FROM ~w)',
                     [QuotedHead]) ),
            CountExprs),
    atomic_list_concat(CountExprs, ' + ', SummedExpr),
    format(atom(Sql), 'SELECT ~w', [SummedExpr]).

% A level head that reads ITSELF positively. strat.pl:topo_order_group/2 drops
% exactly this edge from its Kahn order (`DependsOnRef \== HeadRef`) and
% refuses every other cycle, so this is the complete set of heads whose SQL
% needs more than one derivation round.
self_referential_level_refs(Rules, Refs) :-
    findall(HeadRef,
            ( member(Rule, Rules), Rule = (_ <- Body),
              rule_head_ref(Rule, HeadRef),
              body_ref_uses(Body, Uses),
              memberchk(use(HeadRef, _, pos, _), Uses) ),
            Refs0),
    sort(Refs0, Refs).

% ═══ buildDeltas ═════════════════════════════════════════════════════════════

build_deltas_fn_lines(RelPlans, EdgeStatements, _RetentionStatements,
                      DepartureRefs, Lines) :-
    maplist(diff_local_line, RelPlans, DiffLines),
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

% Retention runs between the before and after snapshots, so multisetDiff must
% retain reclaimed rows as deletions; no keep-specific suppression is needed.
diff_local_line(relplan(Ref, _Kind, _Columns, _Key, _ColumnTypes), Line) :-
    ref_name(Ref, Name),
    format(atom(Line), '  const ~w = multisetDiff(before.~w, after.~w);',
           [Name, Name, Name]).

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
                        HasStructTypes, HasTextIntern, Lines) :- !,
    departure_stage_naive_lines(DepartureRefs, DepartureStageLines),
    format(atom(NameCommentLine), '  // ~w: no edge rules -- absorb arrivals, recompute levels, diff.', [Name]),
    retention_tick_lines(HasRetention, RetentionLines),
    advance_tick_naive_line(UsesTick, AdvanceTickLines),
    naive_text_intern_lines(HasTextIntern, TextInternLines),
    naive_reference_normalize_lines(HasStructTypes, NormalizeLines),
    append(
    [ [ 'function runNaiveTick(seam: ISqlSeam, arrivals: IArrivalBatch): Observable<ITickDeltas> {',
        '  return readSnapshot(seam).pipe('
      ],
      AdvanceTickLines,
      TextInternLines,
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
                        DepartureRefs, HasStructTypes, HasTextIntern, Lines) :-
    EdgeStatements \== [],
    departure_stage_naive_lines(DepartureRefs, DepartureStageLines),
    advance_tick_naive_line(UsesTick, AdvanceTickLines),
    naive_text_intern_lines(HasTextIntern, TextInternLines),
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
      TextInternLines,
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

run_ordered_tick_fn_lines(false, _, _, _, _, _, _, []) :- !.
run_ordered_tick_fn_lines(true, Name, HasRetention, UsesTick, DepartureRefs,
                          HasStructTypes, HasTextIntern, Lines) :-
    departure_stage_naive_lines(DepartureRefs, DepartureStageLines),
    advance_tick_naive_line(UsesTick, AdvanceTickLines),
    naive_text_intern_lines(HasTextIntern, TextInternLines),
    naive_reference_normalize_lines(HasStructTypes, NormalizeLines),
    retention_tick_lines_ordered(HasRetention, RetentionLines),
    format(atom(NameCommentLine),
           '  // ~w: ordered process_occurrences with evolving pre snapshots.',
           [Name]),
    append(
    [ [ 'function runOrderedTick(seam: ISqlSeam, arrivals: IArrivalBatch): Observable<ITickDeltas> {',
        '  return readSnapshot(seam).pipe('
      ],
      AdvanceTickLines,
      TextInternLines,
      NormalizeLines,
      [ '    concatMap((before) => applyArrivals(seam, arrivals).pipe(map(() => before))),',
        '    concatMap((before) => snapshotOrderedPre(seam).pipe(map(() => before))),',
        '    concatMap((before) => recomputeLevels(seam).pipe(map(() => before))),',
        '    concatMap((before) => readSnapshot(seam).pipe(map((mid) => ({ before, mid })))),',
        '    concatMap(({ before, mid }) => processOrderedOccurrences(seam, before, mid, arrivals).pipe(map((written) => ({ before, mid, written })))),',
        '    concatMap(({ before, mid, written }) => recomputeLevels(seam).pipe(map(() => ({ before, mid, written })))),',
        %% rxjs pipe() typed overloads stop at 9 operators; past that the chain
        %% collapses to Observable<unknown> (first hit when the golden reached
        %% 12 ordered-tick stages). Second .pipe() keeps every stage typed.
        '  ).pipe('
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

% The ordered tick carries the { before, mid, written } triple at this point;
% the bare `before` passthrough above collapses TypeScript's inference to
% Observable<unknown> (first exercised when the golden gained an ordered pre
% rule). Same stage, triple spelled out.
retention_tick_lines_ordered(true,
    ['    concatMap(({ before, mid, written }) => applyNaiveRetention(seam).pipe(map(() => ({ before, mid, written })))),']).
retention_tick_lines_ordered(false, []).

% The referee's own end-of-tick departure staging. It reuses the RUNTIME's
% IncrementalRuntime.stageDepartures over the deltas THIS path computed from
% its two snapshots -- the same table, filled from an independent source, so
% the two pipelines stay comparable while neither borrows the other's answer.
% Between readBoundary and promoteFrontiers, on purpose: the source is the
% tick's NET boundary delta (engine.pl reads DepartureCarry off `Deltas`), and
% promoteFrontiers is what then reports the staged rows as carryPending.
departure_stage_incremental_lines([], []) :- !.
departure_stage_incremental_lines(DepartureRefs,
    ['    concatMap((rels) => IncrementalRuntime.stageDepartures(seam, SUBSCRIBED_RELATIONS, rels).pipe(map(() => rels))),']) :-
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

% ═══ subscribe-cone pruning (ladder step 2, DEFAULT OFF) ═════════════════════
%
% Read once, at module scope: the filters are pure and the emitted arrays never
% change, so a per-tick call would buy nothing. With the flag off every
% SUBSCRIBED_* const IS the array above it, by reference.
%
% incrementalPlan stays UNPRUNED on purpose: it describes the compiled program
% (tests read statements out of it by rel name), where the consts below are the
% tick path's own working lists.
%
% Only the incremental path can honor a cone. The naive referee rebuilds every
% level rel from one fused SQL string and the ordered path replays whole
% relations, so with the flag on those two refuse by name rather than answering
% a pruned question with an unpruned tick.
subscribe_prune_lines(HasRetention, DerivedEdgeCarryRequired, HasOrderedProgram,
                      Lines) :-
    subscribe_prune_tick_path_line(DerivedEdgeCarryRequired, HasOrderedProgram,
                                   TickPathLine),
    ( HasRetention == true
    -> RetentionLine =
       ['const SUBSCRIBED_RETENTION_STATEMENTS = SubscribeCone.retention(SUBSCRIBE_PRUNE, INCREMENTAL_RETENTION_STATEMENTS, subscribedRels, arrivalTargets);']
    ;  RetentionLine = []
    ),
    append(
    [ [ 'const SUBSCRIBE_PRUNE = SubscribeCone.mode();',
        TickPathLine,
        'if (SUBSCRIBE_PRUNE === "on" && SUBSCRIBE_PRUNE_TICK_PATH !== "incremental") {',
        '  throw new Error(`subscribe_prune_unsupported_tick_path ${SUBSCRIBE_PRUNE_TICK_PATH}`);',
        '}',
        'const SUBSCRIBED_RELATIONS = SubscribeCone.relations(SUBSCRIBE_PRUNE, INCREMENTAL_RELATIONS, subscribedRels, arrivalTargets);',
        'const SUBSCRIBED_EDGE_STATEMENTS = SubscribeCone.edges(SUBSCRIBE_PRUNE, INCREMENTAL_EDGE_STATEMENTS, subscribedRels);',
        'const SUBSCRIBED_LEVEL_STATEMENTS = SubscribeCone.levels(SUBSCRIBE_PRUNE, INCREMENTAL_LEVEL_STATEMENTS, subscribedRels);'
      ],
      RetentionLine,
      [ 'const SUBSCRIBED_BOOT = SubscribeCone.boot(SUBSCRIBE_PRUNE, boot, subscribedRels, arrivalTargets);' ]
    ], Lines).

% Typed `string`, not left to inference: a literal-typed const makes the guard
% below a comparison tsgo reports as having no overlap (TS2367).
subscribe_prune_tick_path_line(_, true,
    'const SUBSCRIBE_PRUNE_TICK_PATH: string = "ordered";') :- !.
subscribe_prune_tick_path_line(true, _,
    'const SUBSCRIBE_PRUNE_TICK_PATH: string = "incremental";') :- !.
subscribe_prune_tick_path_line(_, _,
    'const SUBSCRIBE_PRUNE_TICK_PATH: string = EMITTER_MODE;').

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
    ['    concatMap(() => IncrementalRuntime.recomputeLevelsBeforeEdges(seam, SUBSCRIBED_LEVEL_STATEMENTS, SUBSCRIBED_RELATIONS, RECONCILE_EVERY_TICK, arrivals)),'],
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
                              HasStructTypes, HasTextIntern, HasOrderedProgram,
                              Lines) :-
    advance_tick_pipeline_line(UsesTick, AdvanceTickLines),
    incremental_text_intern_lines(HasTextIntern, TextInternLines),
    incremental_reference_normalize_lines(HasStructTypes, NormalizeLines),
    departure_stage_incremental_lines(DepartureRefs, DepartureStageLines),
    pre_edge_level_reconcile_lines(EdgeStatements, PreEdgeReconcileLines, PipeSplitLines),
    ( EdgeStatements == []
    -> MergeLine = '    concatMap(() => of(undefined)),',
       PostEdgeLevelLine = '    concatMap(() => of(undefined)),'
    ;  MergeLine = '    concatMap(() => IncrementalRuntime.mergeNextIntoCurrent(seam, SUBSCRIBED_RELATIONS)),',
       PostEdgeLevelLine = '    concatMap(() => IncrementalRuntime.applyLevelsAfterEdges(seam, SUBSCRIBED_LEVEL_STATEMENTS, SUBSCRIBED_RELATIONS)),'
    ),
    RecomputeLine = '    concatMap(() => IncrementalRuntime.recomputeLevelsAfterEdges(seam, SUBSCRIBED_LEVEL_STATEMENTS, SUBSCRIBED_RELATIONS, RECONCILE_EVERY_TICK)),',
    run_tick_dispatch_lines(DerivedEdgeCarryRequired, HasStructTypes,
                            HasOrderedProgram, DispatchLines),
    ( HasRetention == true
    -> RetentionLines =
       ['    concatMap(() => IncrementalRuntime.applyRetention(seam, SUBSCRIBED_RETENTION_STATEMENTS, SUBSCRIBED_RELATIONS)),']
    ; RetentionLines = []
    ),
    append(
    [ [ 'function runIncrementalTick(seam: ISqlSeam, arrivals: IArrivalBatch): Observable<ITickDeltas> {',
      '  return IncrementalRuntime.prepareTick(seam, SUBSCRIBED_RELATIONS).pipe('
      ],
      AdvanceTickLines,
      TextInternLines,
      NormalizeLines,
      [ '    concatMap(() => IncrementalRuntime.applyArrivals(seam, arrivals, SUBSCRIBED_RELATIONS)),',
      '    concatMap(() => IncrementalRuntime.applyLevelsBeforeEdges(seam, SUBSCRIBED_LEVEL_STATEMENTS, SUBSCRIBED_RELATIONS)),'
      ],
      PreEdgeReconcileLines,
      [ '    concatMap(() => IncrementalRuntime.applyEdges(seam, SUBSCRIBED_EDGE_STATEMENTS, SUBSCRIBED_RELATIONS)),',
      MergeLine,
      PostEdgeLevelLine
      ],
      PipeSplitLines,
      RetentionLines,
      [
      RecomputeLine,
      '    concatMap(() => IncrementalRuntime.readBoundary(seam, SUBSCRIBED_RELATIONS)),'
      ],
      DepartureStageLines,
      [
      '    concatMap((rels) => IncrementalRuntime.promoteFrontiers(seam, SUBSCRIBED_RELATIONS).pipe(',
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
        plan(_, prog(_, Rules), _, _, _, _, _, _), EdgeStatements, Required) :-
    derived_refs(Rules, DerivedRefs),
    ( member(edgestmt(_, TriggerRef, _, _, _, _, _, _), EdgeStatements),
      memberchk(TriggerRef, DerivedRefs)
    -> Required = true
    ;  Required = false
    ).

incremental_program_safe(plan(_, prog(_, Rules), _, _, _, _, _, _),
                         _EdgeStatements, _LevelStatements, Safe) :-
    rules_have_supported_level_bodies(Rules),
    Safe = true.

rules_have_supported_level_bodies([]).
rules_have_supported_level_bodies([_ | Rest]) :-
    rules_have_supported_level_bodies(Rest).

reconcile_every_tick(plan(_, prog(_, Rules), _, _, _, _, _, _), Reconcile) :-
    ( member(Rule, Rules),
      Rule = (_ <- Body),
      body_ref_uses(Body, Uses),
      member(use(_, _, neg, _), Uses)
    -> Reconcile = true
    ;  Reconcile = false
    ).

retraction_guard(plan(_, prog(_, Rules), _, _, _, _, _, _), Guard) :-
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

% `boot` is the ONE field the cone filter reaches from out here: the tick path
% takes its lists from the SUBSCRIBED_* consts, but boot is run by the harness
% off this object.
program_export_lines(Name, InternMode,
    [ 'export const program: IGenProgramWithBoot = {',
      NameLine,
      InternModeLine,
      '  ddl,',
      '  relColumns,',
      '  relColumnTypes,',
      '  arrivalTargets,',
      '  boot: SUBSCRIBED_BOOT,',
      '  finalSelect,',
      '  hostPlans,',
      '  bindPlans,',
      '  queryPlans,',
      '  subscribedRels,',
      '  relCatalog,',
      '  unsupportedExecution,',
      '  tick: runTick,',
      '};'
    ]) :-
    format(atom(NameLine), '  name: "~w",', [Name]),
    format(atom(InternModeLine), '  internMode: "~w",', [InternMode]).

% A database built by one mode is unreadable by the other, so the artifact
% names the mode that built it (interning contract §15.5).
plan_intern_mode(plan(_, _, _, _, _, _, _, InternMode), InternMode).

% ═══ top level ═══════════════════════════════════════════════════════════════

emit_program(Name, Plan, Lowered, BootStatements, Text) :-
    Lowered = lowered(Name, Ddl, ArrivalStatements, EdgeStatements, LevelStatements, DeltaStatements, RelPlans, ArrivalTargets),
    header_lines(Name, HeaderLines),
    ( EdgeStatements == [] -> HasEdgeRules = false ; HasEdgeRules = true ),
    include(is_level_statement, LevelStatements, RuleLevelStatements),
    include(is_retention_statement, LevelStatements, RetentionStatements),
    ( RetentionStatements == [] -> HasRetention = false ; HasRetention = true ),
    Plan = plan(_, prog(PlanDecls, _), _, _, _, _, _, _),
    struct_type_plans(PlanDecls, StructPlans),
    struct_plane_lines(StructPlans, RelPlans, StructPlaneLines, HasStructTypes),
    plan_intern_mode(Plan, InternMode),
    program_text_intern_plan(InternMode, RelPlans, TextInternPlan),
    text_intern_plan_lines(TextInternPlan, TextInternPlanLines, HasTextIntern),
    ( ordered_program(EdgeStatements) -> HasOrderedProgram = true
    ; HasOrderedProgram = false
    ),
    Plan = plan(_, prog(_, SelfRefScanRules), _, _, _, _, _, _),
    self_referential_level_refs(SelfRefScanRules, SelfReferentialLevelRefs),
    imports_lines(HasEdgeRules, HasRetention, HasStructTypes, HasTextIntern,
                  HasOrderedProgram, SelfReferentialLevelRefs, ImportLines),
    local_types_lines(LocalTypeLines),
    world_plan_lines(Plan, WorldPlanLines),
    bind_args_helper_lines(BindArgsHelperLines),
    arrival_value_guard_lines(ArrivalValueGuardLines),
    ( EdgeStatements == [] -> TriggerOccurrencesHelperLines = [] ; trigger_occurrences_helper_lines(TriggerOccurrencesHelperLines) ),
    Plan = plan(_, prog(_, PlanRules), _, _, _, _, _, _),
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
    program_catalog_rows(Name, Plan, RelPlans, CatalogRows),
    rel_catalog_lines(CatalogRows, RelCatalogLines),
    rel_declared_column_types_lines(PlanDecls, RelPlans, RelDeclaredColumnTypesLines),
    arrival_targets_lines(ArrivalTargets, ArrivalTargetsLines),
    boot_lines(BootStatements, BootLines),
    snapshot_type_lines(RelPlans, SnapshotTypeLines),
    read_snapshot_fn_lines(DeltaStatements, ReadSnapshotFnLines),
    final_select_lines(DeltaStatements, FinalSelectLines),
    arrival_statements_lines(ArrivalStatements, ArrivalStatementsLines),
    arrival_statement_fn_lines(Name, ArrivalStatementFnLines),
    incremental_relation_lines(RelPlans, PlanRules, ArrivalStatements, DeltaStatements, DepartureRefs, IncrementalRelationLines),
    incremental_edge_statement_lines(Name, EdgeStatements, RelPlans, IncrementalEdgeStatementLines),
    incremental_level_statement_lines(Name, RuleLevelStatements, RelPlans, IncrementalLevelStatementLines),
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
    recompute_levels_fn_lines(SelfReferentialLevelRefs, RuleLevelStatements,
                              RecomputeLevelsFnLines),
    naive_retention_fn_lines(RetentionStatements, NaiveRetentionFnLines),
    build_deltas_fn_lines(RelPlans, EdgeStatements, RetentionStatements,
                          DepartureRefs, BuildDeltasFnLines),
    Plan = plan(_, TickProg, _, _, _, _, _, _),
    program_uses_tick(TickProg, UsesTick),
    advance_tick_fn_lines(UsesTick, AdvanceTickFnLines),
    run_naive_tick_fn_lines(Name, EdgeStatements, HasRetention, UsesTick,
                            DepartureRefs, HasStructTypes, HasTextIntern,
                            RunNaiveTickFnLines),
    run_ordered_tick_fn_lines(HasOrderedProgram, Name, HasRetention, UsesTick,
                              DepartureRefs, HasStructTypes, HasTextIntern,
                              RunOrderedTickFnLines),
    incremental_program_safe(Plan, EdgeStatements, RuleLevelStatements, IncrementalSafe),
    reconcile_every_tick(Plan, ReconcileEveryTick),
    derived_edge_carry_required(Plan, EdgeStatements, DerivedEdgeCarryRequired),
    retraction_guard(Plan, RetractionGuard),
    incremental_mode_lines(IncrementalSafe, ReconcileEveryTick,
                           IncrementalModeLines),
    subscribe_prune_lines(HasRetention, DerivedEdgeCarryRequired,
                          HasOrderedProgram, SubscribePruneLines),
    run_incremental_tick_fn_lines(EdgeStatements, DerivedEdgeCarryRequired,
                                  HasRetention, UsesTick, DepartureRefs,
                                  HasStructTypes, HasTextIntern,
                                  HasOrderedProgram,
                                  RunIncrementalTickFnLines),
    struct_tick_wrapper_lines(HasStructTypes, Name, StructTickWrapperLines),
    incremental_plan_export_lines(RetractionGuard, HasRetention,
                                  IncrementalPlanExportLines),
    program_export_lines(Name, InternMode, ProgramExportLines),
    Sections0 =
    [ HeaderLines, ImportLines, LocalTypeLines, WorldPlanLines,
      BindArgsHelperLines, ArrivalValueGuardLines, TriggerOccurrencesHelperLines,
      DepartureOccurrencesHelperLines,
      StructPlaneLines, TextInternPlanLines,
      DdlLines, RelColumnsLines, RelColumnTypesLines, RelCatalogLines,
      RelDeclaredColumnTypesLines, ArrivalTargetsLines,
      BootLines, SnapshotTypeLines, ReadSnapshotFnLines, FinalSelectLines,
      ArrivalStatementsLines, ArrivalStatementFnLines,
      IncrementalRelationLines, IncrementalEdgeStatementLines,
      IncrementalLevelStatementLines, IncrementalRetentionStatementLines,
      EdgeConstLines, EdgeFnLines,
      OrderedPreLines, OrderedOccurrenceLines, OrderedCarryLines,
      RecomputeLevelsFnLines, NaiveRetentionFnLines, BuildDeltasFnLines,
      AdvanceTickFnLines, RunNaiveTickFnLines, RunOrderedTickFnLines,
      IncrementalModeLines, SubscribePruneLines, RunIncrementalTickFnLines,
      StructTickWrapperLines, IncrementalPlanExportLines,
      ProgramExportLines
    ],
    exclude(==([]), Sections0, Sections),
    maplist(lines_block, Sections, SectionTexts),
    atomic_list_concat(SectionTexts, '\n\n', Body),
    format(atom(Text), '~w\n', [Body]).

is_level_statement(levelstmt(_, _, _, _, _, _)).
is_retention_statement(retentionstmt(_, _, _)).
