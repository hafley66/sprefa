% Emit lowered compiler plans as TypeScript modules.

% The three extra exports form the emitter-mode seam.
:- module(emit_ts,
          [ emit_program/5,
            reconcile_every_tick/2,
            derived_edge_carry_required/3,
            retraction_guard/2 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(assoc)).
:- use_module(lower, [ departure_frontier_table_name/2,
                       departure_read_sql/3, struct_type_plans/3, struct_type_plans/4,
                       program_text_intern_plan/3,
                       statement_rule_ids/3, fixpoint_round_cap/1 ]).
:- use_module('0_rel_record').
:- use_module(analyze,
              [ body_ref_uses/2, derived_refs/2, rule_head_ref/2,
                program_uses_tick/2, listened_departure_refs/2,
                level_body_pre_ref/2, rel_rule_observers_map/2 ]).
:- use_module(strat, [recursive_stratum_groups/2, cyclic_head_groups/2]).
:- use_module('next/1_expand/1_host_expand', [compile_host_decl/2, compile_query/2,
                                query_decl/3, host_plan_contract/2]).
% bind_executor/2 left the registry with the bind surface; pinned here so the
% (now unreachable) bind_plan_json path stays byte-identical.
bind_executor(interval, live_interval).
bind_executor(watch,    live_watch).
:- use_module('next/1_expand/0_option_expand', [option_enum_name/2]).
:- use_module('compile/registry', [host_execution/3]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

% ═══ IR version ══════════════════════════════════════════════════════════════
% emit_rust.pl carries the same number under the same spelling, and both runtimes
% refuse a program document whose value is not the one they interpret.
ir_version(1).

% ═══ small text helpers ══════════════════════════════════════════════════════

lines_block(Lines, Text) :- atomic_list_concat(Lines, '\n', Text).

pairs_to_dict([], _{}) :- !.
pairs_to_dict(Pairs, Dict) :- foldl(add_pair, Pairs, _{}, Dict).
add_pair(Name-Value, Acc, Out) :- Out = Acc.put(Name, Value).

% Escape backslashes, backticks, and `${` before embedding SQL in a template
% literal. Backslashes must be handled first.
%
% The backslash clause goes FIRST, or it would double the backslashes this
% predicate itself introduces for the other two.
js_template(SqlText, JsLiteral) :-
    (   js_template_needs_no_escape(SqlText)
    ->  atomic_list_concat(['`', SqlText, '`'], JsLiteral)
    ;   atom_string(SqlText, SqlString),
        string_codes(SqlString, Codes),
        js_template_codes(Codes, Escaped),
        atom_codes(Body, Escaped),
        atomic_list_concat(['`', Body, '`'], JsLiteral)
    ).

% `$` stands in for the `${` clause: a lone `$` costs one slow pass and never a
% wrong byte, where splitting on two characters at once is not expressible.
js_template_needs_no_escape(SqlText) :-
    atomic(SqlText),
    SqlText \== [],
    split_string(SqlText, "\\`$", "", [_]).

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
    (   js_string_needs_no_escape(Value)
    ->  atomic_list_concat(['"', Value, '"'], JsLiteral)
    ;   ( atom(Value) -> atom_codes(Value, Codes) ; string_codes(Value, Codes) ),
        js_string_codes(Codes, Escaped),
        atom_codes(Body, Escaped),
        atomic_list_concat(['"', Body, '"'], JsLiteral)
    ).

% The separator set is js_string_codes/2's escaping clauses; split_string/4
% finds one in C where the clause walk builds a second code list to find none.
js_string_needs_no_escape(Value) :-
    atomic(Value),
    Value \== [],
    split_string(Value, "\"\\\n\r\t", "", [_]).

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
% where it is idiomatic; mixing it into a camel_case function name
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
    atomic_list_concat(['[', Joined, ']'], Text).

quoted_string_array_text(Atoms, Text) :-
    maplist(js_string, Atoms, Quoted),
    atomic_list_concat(Quoted, ', ', Joined),
    atomic_list_concat(['[', Joined, ']'], Text).

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
      '// lower/lower_sql.ts\'s.',
      '//',
      '// The default path stages effective tick changes in indexed TEMP tables,',
      '// executes emitted frontier-side joins for positive level rules, promotes',
      '// edge and post-write level growth across drain ticks, and computes boundary',
      '// changes from the staged stream. Retractions and negative bodies use emitted',
      '// support-count reconciliation.',
      '//',
      '// IGenProgram has no slot for boot-time work (seeding Initial rows before',
      '// tick 1). `boot` is an extra field added beyond the five pinned names',
      '// ("extend by adding fields, never renaming"); v6/tsv2/scripts/',
      '// run-emitted.ts (the reconciliation runner) runs it after DDL and before',
      '// the tick fold.'
    ].

% ═══ imports ═════════════════════════════════════════════════════════════════

imports_lines(HasEdgeRules, HasRetention, Lines) :-
    imports_lines(HasEdgeRules, HasRetention, false, false, false, [], false, Lines).

% Four spellings of one import line, so a program that neither orders its arms
% nor builds a string keeps the exact text it had.
runtime_import_line(false, false,
    'import { IncrementalRuntime } from "../runtime/1_incremental.ts";') :- !.
runtime_import_line(true, false,
    'import { IncrementalRuntime, stage_ordered_frontiers } from "../runtime/1_incremental.ts";') :- !.
runtime_import_line(false, true,
    'import { IncrementalRuntime, intern_then_execute } from "../runtime/1_incremental.ts";') :- !.
runtime_import_line(true, true,
    'import { IncrementalRuntime, intern_then_execute, stage_ordered_frontiers } from "../runtime/1_incremental.ts";').

imports_lines(_HasEdgeRules, HasRetention, HasStructTypes, HasTextIntern,
              HasOrderedProgram,
              SelfReferentialLevelRefs, HasInternWrite, Lines) :-
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
    runtime_import_line(HasOrderedProgram, HasInternWrite, RuntimeImport),
    EnumImport = ['import { EnumPlane } from "../runtime/enumPlane.ts";'],
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
      'import { multiset_diff } from "../runtime/diff.ts";',
      'import { select_rows } from "../runtime/rows.ts";',
      'import { list_at_scalar_seam } from "../runtime/boundary.ts";'
      ],
      StructImport,
      EnumImport,
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
      '  IRowScalar,',
      '  IRowValue,',
      '  ISqlSeam,',
      '  IEnumRefColumns,',
      '  IEnumTypePlan,'
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
           '  { name: ~w, columns: [~w], refs: [~w], key_indices: [~w], conflict_sql: ~w, intern_sql: ~w, lookup_sql: ~w },',
           [NameText, ColumnsText, RefsText, KeyIndicesText,
            ConflictTemplate, InternTemplate, LookupTemplate]).

struct_ref_entry(none, 'null') :- !.
struct_ref_entry(TypeName, Text) :- js_string(TypeName, Text).

struct_ref_column_entries(RelPlans, Lines) :-
    findall(Line,
            ( member(RelPlan, RelPlans),
              relplan_parts(RelPlan, Ref, _, _, _, ColumnTypes),
              memberchk(ref(_), ColumnTypes),
              ref_name(Ref, Name),
              maplist(column_type_ref_entry, ColumnTypes, RefTexts),
              atomic_list_concat(RefTexts, ', ', RefsText),
              js_string(Name, NameKey),
              format(atom(Line), '  ~w: [~w],', [NameKey, RefsText]) ),
            Lines).

column_type_ref_entry(ref(TypeName), Text) :- !, js_string(TypeName, Text).
column_type_ref_entry(_, 'null').

enum_type_plans(Decls, RelPlans, DeltaStatements, Plans) :-
    findall(Name, enum_runtime_name(Decls, Name), Names0),
    sort(Names0, Names), maplist(enum_type_plan(Decls, RelPlans, DeltaStatements), Names, Plans).
enum_runtime_name(Decls, Name) :- member(enum_column(_, _, Name), Decls).
enum_runtime_name(Decls, Name) :- member(option_column(_, _, Element), Decls), option_enum_name(Element, Name).
enum_runtime_name(Decls, Name) :- member(enum_option_payload(_, _, _, Element), Decls), option_enum_name(Element, Name).
enum_type_plan(Decls, RelPlans, DeltaStatements, Name,
               enumtype(Name, Variants, Identity)) :-
    findall(Tag-Variant, enum_variant_plan(Decls, RelPlans, DeltaStatements, Name, Tag, Variant), Pairs0),
    keysort(Pairs0, Pairs), pairs_values(Pairs, Variants),
    enum_identity_plan(Name, Identity).
enum_variant_plan(Decls, RelPlans, DeltaStatements, EnumName, Tag, enumvariant(Tag, VariantName, Fields, FieldTypes, FieldEnums, SelectSql)) :-
    atomic_list_concat([EnumName, '_'], Prefix), member(RelPlan, RelPlans),
    relplan_parts(RelPlan, VariantName/_, _, [id | Fields], _, [_ | FieldTypes0]),
    atom_concat(Prefix, Tag, VariantName), atom_concat(EnumName, '_tag', TagRelation), VariantName \== TagRelation,
    member(deltastmt(VariantName/_, SelectSql, _, _, _), DeltaStatements),
    maplist(boundary_column_type, FieldTypes0, FieldTypes), maplist(enum_variant_field(Decls, VariantName), Fields, FieldEnums).
enum_variant_field(Decls, VariantName, Field, EnumName) :- member(enum_column(VariantName/_, Field, EnumName), Decls), !.
enum_variant_field(Decls, VariantName, Field, EnumName) :- member(enum_option_payload(_, VariantName, Field, Element), Decls), option_enum_name(Element, EnumName), !.
enum_variant_field(_, _, _, null).
enum_ref_columns_map(Decls, RelPlans, Map) :-
    enum_ref_index(Decls, EnumRefs),
    findall(Name-Refs, (member(RelPlan, RelPlans), relplan_parts(RelPlan, Ref, _, Columns, _, _),
      ref_name(Ref, Name), enum_ref_fields(EnumRefs, Ref, Columns, Columns, Refs), member(Field, Refs), Field \== null), Pairs),
    pairs_to_dict(Pairs, Map).

enum_ref_index(Decls, Index) :-
    empty_assoc(Empty),
    foldl(index_enum_ref, Decls, Empty, Index).

index_enum_ref(enum_column(Ref, Column, EnumName), Index0, Index) :- !,
    put_assoc(Ref-Column, Index0, EnumName, Index).
index_enum_ref(option_column(Ref, Column, Element), Index0, Index) :- !,
    option_enum_name(Element, EnumName),
    put_assoc(Ref-Column, Index0, EnumName, Index).
index_enum_ref(_, Index, Index).

enum_ref_fields(_, _, _, [], []).
enum_ref_fields(Index, Ref, All, [Column | Rest], [Field | Fields]) :-
    ( get_assoc(Ref-Column, Index, EnumName)
    -> ( nth0(EndpointIndex, All, id), nth0(CurrentIndex, All, Column), EndpointIndex =\= CurrentIndex
       -> Field = enumref(EnumName, EndpointIndex)
       ;  Field = enumref(EnumName, null)
       )
    ; Field = null ), enum_ref_fields(Index, Ref, All, Rest, Fields).
enum_identity_plan(Name, enumidentity(InternSql, LookupSql)) :-
    enum_identity_table(Name, Table), quote_ident_local(Table, QuotedTable),
    format(atom(InternSql), 'INSERT OR IGNORE INTO ~w ("value") VALUES (?)', [QuotedTable]),
    format(atom(LookupSql), 'SELECT "id", "value" FROM ~w WHERE "value" = ?', [QuotedTable]).
enum_identity_table(Name, Table) :- atomic_list_concat(['__enum_identity_', Name], Table).
enum_identity_ddls(Decls, Ddls) :-
    findall(Ddl, (enum_runtime_name(Decls, Name), enum_identity_table(Name, Table), quote_ident_local(Table, QuotedTable),
                  format(atom(Ddl), 'CREATE TABLE ~w ("id" INTEGER PRIMARY KEY, "value" TEXT NOT NULL UNIQUE)', [QuotedTable])), Ddls0),
    sort(Ddls0, Ddls).
enum_plane_lines([], _, [ 'export const ENUM_TYPES: readonly IEnumTypePlan[] = [];',
                          'export const ENUM_REF_COLUMNS: IEnumRefColumns = {};'], false) :- !.
enum_plane_lines(Plans, RefColumns, Lines, true) :-
    maplist(enum_type_line, Plans, TypeLines), dict_pairs(RefColumns, _, Pairs),
    maplist(enum_ref_line, Pairs, RefLines),
    append([[ 'export const ENUM_TYPES: readonly IEnumTypePlan[] = [' ], TypeLines,
      [ '];', '', 'export const ENUM_REF_COLUMNS: IEnumRefColumns = {' ], RefLines, [ '};' ]], Lines).
enum_type_line(enumtype(Name, Variants, enumidentity(InternSql, LookupSql)), Line) :- js_string(Name, NameText), maplist(enum_variant_text, Variants, VariantTexts), atomic_list_concat(VariantTexts, ', ', VariantsText), js_template(InternSql, InternText), js_template(LookupSql, LookupText), format(atom(Line), '  { name: ~w, variants: [~w], identity: { intern_sql: ~w, lookup_sql: ~w } },', [NameText, VariantsText, InternText, LookupText]).
enum_variant_text(enumvariant(Tag, Rel, Fields, FieldTypes, FieldEnums, SelectSql), Text) :-
    js_string(Tag, TagText), js_string(Rel, RelText), maplist(js_string, Fields, FieldTexts), atomic_list_concat(FieldTexts, ', ', FieldsText), maplist(js_string, FieldTypes, FieldTypeTexts), atomic_list_concat(FieldTypeTexts, ', ', TypesText), maplist(enum_field_text, FieldEnums, EnumTexts), atomic_list_concat(EnumTexts, ', ', EnumsText), js_template(SelectSql, SelectText),
    format(atom(Text), '{ tag: ~w, rel: ~w, fields: [~w], field_types: [~w], field_enums: [~w], select_sql: ~w }', [TagText, RelText, FieldsText, TypesText, EnumsText, SelectText]).
enum_field_text(null, 'null') :- !. enum_field_text(Name, Text) :- js_string(Name, Text).
enum_ref_line(Name-Refs, Line) :- js_string(Name, NameText), maplist(enum_ref_text, Refs, Texts), atomic_list_concat(Texts, ', ', RefsText), format(atom(Line), '  ~w: [~w],', [NameText, RefsText]).
enum_ref_text(null, 'null') :- !. enum_ref_text(enumref(Name, Index), Text) :- js_string(Name, NameText), (Index == null -> IndexText = 'null' ; format(atom(IndexText), '~w', [Index])), format(atom(Text), '{ name: ~w, endpoint_index: ~w }', [NameText, IndexText]).

% Relation references normalize inside each emitter mode after that mode has
% opened its tick boundary. Target rows pass through the same arrival
% applicator as authored rows, then parent fields carry the resolved integer
% endpoints. No second externally visible tick or reference-value runtime
% exists.
struct_tick_wrapper_lines(_, _, []).

% Before StructPlane and before any level statement: a rewritten row must
% never reach a statement that would store a string in an id column.
snapshot_text_intern_lines(false, []) :- !.
snapshot_text_intern_lines(true,
    [ '    concatMap((before) => TextPlane.intern(seam, TEXT_INTERN_PLAN, arrivals)',
      '      .pipe(map((interned) => { arrivals = interned; return before; }))),'
    ]).

incremental_text_intern_lines(false, []) :- !.
incremental_text_intern_lines(true,
    [ '    concatMap(() => TextPlane.intern(seam, TEXT_INTERN_PLAN, arrivals)',
      '      .pipe(map((interned) => { arrivals = interned; }))),'
    ]).

% A target row reaches its table without crossing the arrival door, so the
% plane takes the ingest plan and interns the target's own text columns.
struct_text_plan_argument(false, '').
struct_text_plan_argument(true, ' TEXT_INTERN_PLAN,').

snapshot_reference_normalize_lines(false, _, []) :- !.
snapshot_reference_normalize_lines(true, HasTextIntern,
    [ '    concatMap((before) => StructPlane.intern(seam, STRUCT_TYPES, STRUCT_REF_COLUMNS, arrivals,',
      ApplyLine,
      '    ).pipe(map((normalized) => { arrivals = normalized; return before; }))),'
    ]) :-
    struct_text_plan_argument(HasTextIntern, TextPlanArgument),
    format(atom(ApplyLine), '      (targets) => apply_arrivals(seam, targets),~w',
           [TextPlanArgument]).

incremental_reference_normalize_lines(false, _, []) :- !.
incremental_reference_normalize_lines(true, HasTextIntern,
    [ '    concatMap(() => StructPlane.intern(seam, STRUCT_TYPES, STRUCT_REF_COLUMNS, arrivals,',
      ApplyLine,
      '    ).pipe(map((normalized) => { arrivals = normalized; }))),'
    ]) :-
    struct_text_plan_argument(HasTextIntern, TextPlanArgument),
    format(atom(ApplyLine),
           '      (targets) => IncrementalRuntime.apply_arrivals(seam, targets, SUBSCRIBED_RELATIONS),~w',
           [TextPlanArgument]).
    % `of` is always imported (edge-free MergeLine + recompute fallback need
    % it); tsconfig has no noUnused* flags, so an unused import cannot error.

% ═══ local supporting types ══════════════════════════════════════════════════

local_types_lines(Plan,
    Lines) :-
    ( plan_has_structured_host(Plan)
    -> HostTypes =
       [ 'interface IHostTypeField { readonly name: string; readonly type: string }',
         'interface IHostTypeDescriptor { readonly ref: string; readonly fields: readonly IHostTypeField[] }',
         'interface IHostColumnPlan { readonly name: string; readonly type: string }',
         'interface IHostPlanData { readonly name: string; readonly inputs: readonly IHostColumnPlan[]; readonly outputs: readonly IHostColumnPlan[]; readonly template: string; readonly demand_rel: string; readonly response_rel: string; readonly execution: string; readonly request_type?: IHostTypeDescriptor; readonly response_type?: IHostTypeDescriptor }'
       ]
    ;  HostTypes =
       [ 'interface IHostColumnPlan { readonly name: string; readonly type: string }',
         'interface IHostPlanData { readonly name: string; readonly inputs: readonly IHostColumnPlan[]; readonly outputs: readonly IHostColumnPlan[]; readonly template: string; readonly demand_rel: string; readonly response_rel: string; readonly execution: string }'
       ]
    ),
    append(HostTypes,
      [ 'interface IBindPlanData { readonly name: string; readonly columns: readonly IHostColumnPlan[]; readonly literals: readonly IRowScalar[]; readonly execution: string }',
        'interface IQueryPlanData { readonly rel: string; readonly arity: number; readonly columns: readonly (IRowScalar | null)[]; readonly bound: readonly number[]; readonly snapshot: "current" }',
        '',
        'interface IBootStatement {',
        '  rel: string;',
        '  sql: string;',
        '  params: readonly IRowScalar[];',
        '}',
        '',
        'type IGenProgramWithBoot = IGenProgram & { readonly ir_version: number; readonly boot: readonly IBootStatement[]; readonly final_select: Record<string, string>; readonly host_plans: readonly IHostPlanData[]; readonly bind_plans: readonly IBindPlanData[]; readonly query_plans: readonly IQueryPlanData[]; readonly subscribed_rels: readonly string[]; readonly rel_catalog: readonly IRelCatalogRow[]; readonly rel_physical_names: Record<string, string>; readonly unsupported_execution: readonly string[] };'
      ], Lines).

plan_has_structured_host(plan(_, prog(Decls, _), _, _, _, _, _, _, _)) :-
    member(Decl, Decls),
    Decl = sh_decl(_, Inputs, Outputs, _),
    ( member(col(_, Type), Inputs), structured_host_type(Type)
    ; member(col(_, Type), Outputs), structured_host_type(Type)
    ),
    !.

structured_host_type(Type) :-
    \+ memberchk(Type, [text, int, float, bool]).

world_plan_lines(plan(_, prog(Decls, Rules), _, _, _, _, _, SubscribedRels, _), Lines) :-
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
    % again cannot introduce a unsupported construct this door did not already raise.
    findall(QueryPlan,
            ( member(Query, Decls),
              query_decl(Query, _, _),
              compile_query(Query, QueryPlan)
            ),
            QueryPlans),
    maplist(host_plan_json, HostPlans, HostRows),
    maplist(bind_plan_json, BindPlans, BindRows),
    maplist(query_plan_json, QueryPlans, QueryRows),
    maplist(subscribed_rel_json, SubscribedRels, SubscribedRows),
    % PHASE 2 (plans/2026-07-29-runtime-bridge-header.md): sh hosts and the
    % interval bind EXECUTE in the served runtime, so neither emits a unsupported construct
    % row any more. The const and its slot stay: a future world term with no
    % executor names itself here rather than executing silently.
    Refusals = [],
    array_const_line('export const host_plans: readonly IHostPlanData[]', HostRows,
                     HostLine),
    array_const_line('export const bind_plans: readonly IBindPlanData[]', BindRows,
                     BindLine),
    array_const_line('export const query_plans: readonly IQueryPlanData[]', QueryRows,
                     QueryLine),
    array_const_line('export const subscribed_rels: readonly string[]',
                     SubscribedRows, SubscribedLine),
    array_const_line('export const unsupported_execution: readonly string[]',
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

host_plan_json(HostPlan, Json) :-
    HostPlan = host_plan(Name, Inputs, Outputs, template(Template),
                         demand_ref(DemandName), response_ref(ResponseName), _),
    js_string(Name, NameJson),
    host_columns_json(Inputs, InputsJson),
    host_columns_json(Outputs, OutputsJson),
    js_string(Template, TemplateJson),
    js_string(DemandName, DemandJson),
    js_string(ResponseName, ResponseRelJson),
    host_execution(Name, Template, Executor),
    format(atom(BaseJson),
           '{ name: ~w, inputs: ~w, outputs: ~w, template: ~w, demand_rel: ~w, response_rel: ~w, execution: "~w"',
           [NameJson, InputsJson, OutputsJson, TemplateJson,
            DemandJson, ResponseRelJson, Executor]),
    host_plan_contract(HostPlan,
                       host_contract(RequestType, ResponseType)),
    (   host_contract_is_structured(RequestType, ResponseType)
    ->  host_type_descriptor_json(RequestType, RequestJson),
        host_type_descriptor_json(ResponseType, ResponseJson),
        format(atom(Json), '~w, request_type: ~w, response_type: ~w }',
               [BaseJson, RequestJson, ResponseJson])
    ;   atom_concat(BaseJson, ' }', Json)
    ).

host_contract_is_structured(type_descriptor(_, RequestFields),
                            type_descriptor(_, ResponseFields)) :-
    ( member(field(_, Type), RequestFields), structured_host_type(Type)
    ; member(field(_, Type), ResponseFields), structured_host_type(Type)
    ),
    !.

host_type_descriptor_json(type_descriptor(TypeRef, Fields), Json) :-
    host_type_ref_json(TypeRef, RefJson),
    maplist(host_field_json, Fields, FieldRows),
    atomic_list_concat(FieldRows, ', ', FieldsJson),
    format(atom(Json), '{ ref: ~w, fields: [~w] }', [RefJson, FieldsJson]).

host_type_ref_json(Name/Arity, Json) :-
    format(atom(Ref), '~w/~w', [Name, Arity]),
    js_string(Ref, Json).

host_field_json(field(Name, Type), Json) :-
    js_string(Name, NameJson),
    host_type_json_text(Type, TypeText),
    js_string(TypeText, TypeJson),
    format(atom(Json), '{ name: ~w, type: ~w }', [NameJson, TypeJson]).

host_type_json_text(Type, Text) :-
    ( atom(Type) -> Text = Type
    ; term_to_atom(Type, Text)
    ).

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
    host_type_json_text(Type, TypeText),
    js_string(TypeText, TypeJson),
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
    [ 'function bind_args(values: readonly IRowValue[]): (string | number | bigint | Uint8Array)[] {',
      '  return values.map((value) => {',
      '    if (typeof value === "boolean") return BigInt(value ? 1 : 0);',
      '    if (typeof value === "number") return Number.isSafeInteger(value) ? BigInt(value) : value;',
      '    if (typeof value === "string") return value;',
      '    if (value instanceof Uint8Array) return value;',
      '    throw list_at_scalar_seam("sql_parameter");',
      '  });',
      '}'
    ]).

% ═══ the arrival type gate ══════════════════════════════════════════════════
% The TS mirror of 0_type_plane.pl:world_row_shape_violation/3. Ruling
% type_gate_widening = arrival_gate_all_types_all_positions: EVERY declared
% column type is checked, not just the three numeric-ish ones, and the unsupported construct
% NAME is the oracle's own `type_arrival_shape_mismatch` so the two doors
% answer the same program with the same word. The one place types are allowed
% to mix is SQLite affinity's numeric widening: an integer at a REAL column
% widens to a float and is accepted, which is what the engine now does too.
%
% The gate is driven by `rel_declared_column_types`, NOT by `rel_column_types`.
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
      'function wide_integer_witness(value: unknown): boolean {',
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
      'const JSON_NUMBER = /-?\\d+(?:\\.\\d+)?(?:[e_e][+-]?\\d+)?/g;',
      '',
      'function wide_integer_in_json_text(value: IRowValue): boolean {',
      '  if (typeof value !== "string") return wide_integer_witness(value);',
      '  const without_strings = value.replace(/"(?:\\\\.|[^"\\\\])*"/g, \'""\');',
      '  for (const token of without_strings.match(JSON_NUMBER) ?? []) {',
      '    if (/[.e_e]/.test(token)) continue;',
      '    const parsed = BigInt(token);',
      '    if (parsed < -SAFE_INTEGER_LIMIT || parsed > SAFE_INTEGER_LIMIT) return true;',
      '  }',
      '  return false;',
      '}',
      '',
      'function validate_arrivals(arrivals: IArrivalBatch): IArrivalBatch {',
      '  return arrivals.map((arrival): IArrivalRow => {',
      '    const types = rel_column_types[arrival.rel];',
      '    if (types === undefined || types.length !== arrival.row.length) throw new Error(`arrival shape mismatch for ${arrival.rel}`);',
      '    const declared = rel_declared_column_types[arrival.rel];',
      '    const row = arrival.row.map((value, index): IRowValue => {',
      '      const type = declared === undefined ? undefined : declared[index];',
      '      const scanned = type === "json" ? wide_integer_in_json_text(value)',
      '        : type === "float" ? false',
      '        : wide_integer_witness(value);',
      '      if (scanned) throw new Error(`int_out_of_range ${arrival.rel}[${index}]`);',
      '      if (type === "bytes") {',
      '        if (!(value instanceof Uint8Array)) throw new Error(`type_arrival_shape_mismatch ${arrival.rel}[${index}] field_not_bytes`);',
      '        return value;',
      '      }',
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
% occurrence-fire only the FIRST. `before_rows` is the tick-start snapshot
% (run_tick's own `before`, captured by read_snapshot before this tick's
% apply_arrivals runs); `seen` starts from it and grows as earlier arrivals
% in THIS tick are folded in, exactly mirroring that recursion.
trigger_occurrences_helper_lines(
    [ 'function trigger_occurrences(',
      '  kind: "log" | "set",',
      '  rel_name: string,',
      '  before_rows: readonly IRow[],',
      '  arrivals: IArrivalBatch,',
      '): IArrivalBatch {',
      '  if (kind === "log") return arrivals.filter((arrival) => arrival.rel === rel_name && arrival.sign === "add");',
      '  const seen = new Set<string>(before_rows.map((row) => JSON.stringify(row)));',
      '  const occurrences: IArrivalRow[] = [];',
      '  for (const arrival of arrivals) {',
      '    if (arrival.rel !== rel_name || arrival.sign !== "add") continue;',
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

ddl_entry_line(Sql, Line) :-
    js_template(Sql, Template),
    atomic_list_concat(['  ', Template, ','], Line).

% ═══ rel_columns / arrival_targets ═════════════════════════════════════════════

rel_columns_lines(RelPlans, Lines) :-
    maplist(rel_columns_entry_line, RelPlans, EntryLines),
    append([ ['const rel_columns: Record<string, readonly string[]> = {'], EntryLines, ['};'] ], Lines).

rel_columns_entry_line(RelPlan, Line) :-
    relplan_parts(RelPlan, Ref, _Kind, Columns, _Key, _ColumnTypes),
    ref_name(Ref, Name),
    quoted_string_array_text(Columns, ColumnsSql),
    js_object_key(Name, NameKey),
    atomic_list_concat(['  ', NameKey, ': ', ColumnsSql, ','], Line).

% The reload plan discards a table by NAME, and a rel's physical name moves
% with its shape digest, so the semantic-to-physical map ships with the program.
rel_physical_names_lines(RelPlans, Lines) :-
    maplist(rel_physical_names_entry_line, RelPlans, EntryLines),
    append([ ['const rel_physical_names: Record<string, string> = {'],
             EntryLines, ['};'] ], Lines).

rel_physical_names_entry_line(RelPlan, Line) :-
    relplan_parts(RelPlan, Ref, _Kind, _Columns, _Key, _ColumnTypes),
    ref_name(Ref, Name),
    relplan_storage_name(RelPlan, StorageName),
    js_string(StorageName, StorageText),
    js_object_key(Name, NameKey),
    atomic_list_concat(['  ', NameKey, ': ', StorageText, ','], Line).

rel_column_types_lines(RelPlans, Lines) :-
    maplist(rel_column_types_entry_line, RelPlans, EntryLines),
    append([ ['const rel_column_types: Record<string, readonly IRowColumnType[]> = {'],
             EntryLines, ['};'] ], Lines).

rel_column_types_entry_line(RelPlan, Line) :-
    relplan_parts(RelPlan, Ref, _Kind, _Columns, _Key, ColumnTypes),
    ref_name(Ref, Name),
    maplist(boundary_column_type, ColumnTypes, BoundaryTypes),
    quoted_string_array_text(BoundaryTypes, TypesText),
    js_object_key(Name, NameKey),
    atomic_list_concat(['  ', NameKey, ': ', TypesText, ','], Line).

% ═══ the raw-storage column types (read_stored_snapshot's own view) ═════════
% read_snapshot decodes the boundary/final plane: a list column reads the
% `__list_...` view's array text and carries type `list` so row_value_from_sql
% parses it into Array<T>. read_stored_snapshot reads the raw base table, where
% a list column is the interned surrogate INTEGER id, and shares rel_column_types
% keyed only by declared type -- so the raw path inherits `list` and crashes on
% the first non-empty row. This map names the STORED shape: `list(T)` is the
% surrogate `int`, everything else keeps its boundary type unchanged.
rel_stored_column_types_lines(RelPlans, Lines) :-
    maplist(rel_stored_column_types_entry_line, RelPlans, EntryLines),
    append([ ['const rel_stored_column_types: Record<string, readonly IRowColumnType[]> = {'],
             EntryLines, ['};'] ], Lines).

rel_stored_column_types_entry_line(RelPlan, Line) :-
    relplan_parts(RelPlan, Ref, _Kind, _Columns, _Key, ColumnTypes),
    ref_name(Ref, Name),
    maplist(stored_column_type, ColumnTypes, StoredTypes),
    quoted_string_array_text(StoredTypes, TypesText),
    js_object_key(Name, NameKey),
    atomic_list_concat(['  ', NameKey, ': ', TypesText, ','], Line).

% ═══ the catalog rows, the same list the INSERT renders ════════════════════
% Emitted even for a program that never queries `__rel`, so a reload compares.
% The full catalog_all_rows/10 block (decl + plane), so the emitted const
% carries the plane rows at compile time (ruling catalog_plane_in_const).
program_catalog_rows(Mode, Name, Decls, Rules, RelPlans, DepartureRefs, PreRefs,
                     Types, RuleLevelStatements, Rows) :-
    lower:catalog_all_rows(Mode, Name, Rules, RelPlans, DepartureRefs, PreRefs,
                           Types, RuleLevelStatements, Decls, Rows).

rel_catalog_lines([], []) :- !.
rel_catalog_lines(Rows, Lines) :-
    maplist(rel_catalog_entry_line, Rows, EntryLines),
    append([ ['const rel_catalog: readonly IRelCatalogRow[] = new Array<IRelCatalogRow>('],
             EntryLines, [');'] ], Lines).

rel_catalog_entry_line(row(RelId, ParentId, Ordinal, Name, Kind, TypeId, Arity,
                           ModuleId, HId, HSchema, HRule), Line) :-
    js_string(Name, NameText),
    js_string(Kind, KindText),
    js_string(HId, HIdText),
    js_string(HSchema, HSchemaText),
    js_string(HRule, HRuleText),
    atomic_list_concat(['  { rel_id: ', RelId,
                        ', parent_id: ', ParentId,
                        ', ordinal: ', Ordinal,
                        ', local_name: ', NameText,
                        ', kind: ', KindText,
                        ', type_id: ', TypeId,
                        ', arity: ', Arity,
                        ', module_id: ', ModuleId,
                        ', h_id: ', HIdText,
                        ', h_schema: ', HSchemaText,
                        ', h_rule: ', HRuleText,
                        ' },'], Line).

% ═══ the DECLARED column types (ruling type_gate_widening) ═════════════════
% What the program WROTE DOWN, as opposed to what analyze.pl inferred. The
% arrival gate reads this map and only this map, because the reference
% engine's gate is decl-driven: a column with an inferred type but no colon
% has no gate on either door.
%
% All-or-nothing per rel is relplan_declared/2's own shape; a partially typed
% decl would mis-locate its own positions and the engine declines to guess.
rel_declared_column_types_lines(RelPlans, Lines) :-
    findall(EntryLine,
            ( member(RelPlan, RelPlans),
              relplan_parts(RelPlan, Ref, _, _, _, _),
              relplan_declared(RelPlan, DeclaredTypes),
              rel_declared_types_entry_line(Ref, DeclaredTypes, EntryLine) ),
            EntryLines),
    append([ ['const rel_declared_column_types: Record<string, readonly string[]> = {'],
             EntryLines, ['};'] ], Lines).

rel_declared_types_entry_line(Ref, DeclaredTypes, Line) :-
    ref_name(Ref, Name),
    maplist(gate_column_type, DeclaredTypes, GateTypes),
    quoted_string_array_text(GateTypes, TypesText),
    atomic_list_concat(['  ', Name, ': ', TypesText, ','], Line).

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
gate_column_type(json_list(_), json) :- !.
gate_column_type(bytes, bytes) :- !.
gate_column_type(id(_), int) :- !.
% The stored id is what the gate sees; the elements live in the member rel.
gate_column_type(list(_), int) :- !.
gate_column_type(_,     other).

boundary_column_type(ref(_), ref) :- !.
% An identity endpoint is an INTEGER at SQLite, but it is neither an `int`
% value nor a followed `ref` at the runtime boundary.
boundary_column_type(idref(_), relation_id) :- !.
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
% hand here. `row_value_from_sql` needs no new arm (json passes through the same
% default text does); the seam that switches on it is ticklog.ts's encoder.
boundary_column_type(json, json) :- !.
boundary_column_type(json_list(_), json) :- !.
boundary_column_type(bytes, bytes) :- !.
% F3: the runtime parses the read surface's array text into Array<T> at the
% row seam, so the boundary type names the list rather than borrowing json's.
boundary_column_type(list(_), list) :- !.
boundary_column_type(Type, Type).

% The stored/raw-base-table shape. A list column's raw storage is the interned
% surrogate entity id (an int), which is what the raw `SELECT "sites" FROM
% "tree_bundle"` read_stored_snapshot issues hands back; everything else stores
% its boundary type directly (ref columns keep the surrogate ref id, json and
% json_list keep their TEXT). Only `list(T)` differs from boundary_column_type/2.
stored_column_type(list(_), int) :- !.
stored_column_type(Type, Stored) :- boundary_column_type(Type, Stored).

arrival_targets_lines(ArrivalTargets, Lines) :-
    maplist(ref_name, ArrivalTargets, Names),
    quoted_string_array_text(Names, Sql),
    format(atom(Line), 'const arrival_targets: readonly string[] = ~w;', [Sql]),
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

% ═══ snapshot type + reader (forkJoin over select_rows, one entry per rel) ════

snapshot_type_lines(RelPlans, Lines) :-
    maplist(snapshot_field_line, RelPlans, FieldLines),
    append([ ['type Snapshot = {'], FieldLines, ['};'] ], Lines).

snapshot_field_line(RelPlan, Line) :-
    relplan_parts(RelPlan, Ref, _Kind, _Columns, _Key, _ColumnTypes),
    ref_name(Ref, Name),
    format(atom(Line), '  readonly ~w: readonly IRow[];', [Name]).

% forkJoin({}) (zero keys) completes WITHOUT emitting, same hazard the
% edge-resolver's forkJoin([]) guard already documents (verified against
% rxjs 7.8.2, not assumed) -- a program with zero declared rels (Decls and
% Rules both empty; every phase-C fixture found so far avoids this via
% analyze.pl:declared_refs/2, but nothing upstream forbids it structurally)
% would otherwise stall run_tick's very first concatMap forever. `of({})` is
% the one-value-then-complete fallback, matching recompute_levels_fn_lines/2's
% [] case just below.
read_snapshot_fn_lines([], Lines) :- !,
    Lines =
    [ 'function read_snapshot(seam: ISqlSeam): Observable<Snapshot> {',
      '  void seam;',
      '  return of({} as Snapshot);',
      '}'
    ].
read_snapshot_fn_lines(DeltaStatements, Lines) :-
    DeltaStatements \== [],
    maplist(snapshot_read_entry_line, DeltaStatements, EntryLines),
    append(
        [ ['function read_snapshot(seam: ISqlSeam): Observable<Snapshot> {', '  return forkJoin({'],
          EntryLines,
          ['  });', '}']
        ], Lines).

snapshot_read_entry_line(deltastmt(Ref, SelectSql, _DeltaTable, _BoundarySql, _StoredSelectSql), Line) :-
    ref_name(Ref, Name),
    js_template(SelectSql, Template),
    format(atom(Line), '    ~w: select_rows(seam, ~w, rel_columns.~w!, rel_column_types.~w!),',
           [Name, Template, Name, Name]).

% read_snapshot decodes for the tick log; a consumer that re-BINDS its rows
% into an emitted statement needs the stored ids instead.
read_stored_snapshot_fn_lines(false, _, [], false) :- !.
read_stored_snapshot_fn_lines(true, [], [], false) :- !.
read_stored_snapshot_fn_lines(true, DeltaStatements, Lines, true) :-
    DeltaStatements \== [],
    maplist(stored_snapshot_read_entry_line, DeltaStatements, EntryLines),
    append(
        [ ['type Snapshots = { readonly decoded: Snapshot; readonly stored: Snapshot };',
           '',
           'function read_stored_snapshot(seam: ISqlSeam): Observable<Snapshot> {',
           '  return forkJoin({'],
          EntryLines,
          ['  });',
           '}',
           '',
           'function read_snapshots(seam: ISqlSeam): Observable<Snapshots> {',
           '  return forkJoin({ decoded: read_snapshot(seam), stored: read_stored_snapshot(seam) });',
           '}']
        ], Lines).

stored_snapshot_read_entry_line(
        deltastmt(Ref, _SelectSql, _DeltaTable, _BoundarySql, StoredSelectSql), Line) :-
    ref_name(Ref, Name),
    js_template(StoredSelectSql, Template),
    format(atom(Line), '    ~w: select_rows(seam, ~w, rel_columns.~w!, rel_stored_column_types.~w!),',
           [Name, Template, Name, Name]).

% Which snapshot each tick-chain position reads. `false` reproduces the text
% the emitter wrote before the stored snapshot existed, byte for byte.
tick_head_read_line(false, '  return read_snapshot(seam).pipe(').
tick_head_read_line(true, '  return read_snapshots(seam).pipe(').

tick_stored_before(false, 'before').
tick_stored_before(true, 'before.stored').

ordered_mid_read_line(false,
    '    concatMap((before) => read_snapshot(seam).pipe(map((mid) => ({ before, mid })))),').
ordered_mid_read_line(true,
    '    concatMap((before) => read_stored_snapshot(seam).pipe(map((mid) => ({ before, mid })))),').

% The carry additions are re-bound next tick, so their diff and the boundary
% set they filter against are both taken in the stored plane.
ordered_after_read_lines(false,
    [ '    concatMap(({ before, mid, written }) => read_snapshot(seam).pipe(map((after) => ({ mid, after, written, deltas: build_deltas(before, after) })))),',
      '    concatMap(({ mid, after, written, deltas }) => stage_ordered_frontiers(seam, INCREMENTAL_RELATIONS, ordered_carry_additions(mid, after, deltas, written)).pipe(',
      '      map((post_write_carry): ITickDeltas => ({ rels: deltas.rels, carry_pending: deltas.carry_pending || post_write_carry })),',
      '    )),'
    ]).
ordered_after_read_lines(true,
    [ '    concatMap(({ before, mid, written }) => read_snapshots(seam).pipe(map((after) => ({ mid, after, written, deltas: build_deltas(before.decoded, after.decoded), stored_deltas: build_deltas(before.stored, after.stored) })))),',
      '    concatMap(({ mid, after, written, deltas, stored_deltas }) => stage_ordered_frontiers(seam, INCREMENTAL_RELATIONS, ordered_carry_additions(mid, after.stored, stored_deltas, written)).pipe(',
      '      map((post_write_carry): ITickDeltas => ({ rels: deltas.rels, carry_pending: deltas.carry_pending || post_write_carry })),',
      '    )),'
    ]).

% ═══ final_select (final-state grading leg) ════════════════════════════════════
% The SAME per-rel "read every row" SQL read_snapshot uses (deltastmt's
% SelectAllSql, canonical-text rendered), exported by rel name so a grader
% can compare the program's END state against the oracle's FinalAll. This is
% NOT part of the tick path -- nothing inside tick() reads it, so the
% host_residency criterion (zero full-table reads into JS per tick) is
% untouched; it runs exactly once, after the fold, in the sweep harness.
final_select_lines(DeltaStatements, Lines) :-
    maplist(final_select_entry_line, DeltaStatements, EntryLines),
    append([ ['const final_select: Record<string, string> = {'], EntryLines, ['};'] ], Lines).

final_select_entry_line(deltastmt(Ref, SelectSql, _DeltaTable, _BoundarySql, _StoredSelectSql), Line) :-
    ref_name(Ref, Name),
    js_template(SelectSql, Template),
    js_object_key(Name, NameKey),
    atomic_list_concat(['  ', NameKey, ': ', Template, ','], Line).

% ═══ arrivals ════════════════════════════════════════════════════════════════

arrival_statements_lines(ArrivalStatements, Lines) :-
    maplist(arrival_statement_entry_line, ArrivalStatements, EntryLines),
    append(
        [ ['const ARRIVAL_STATEMENTS: Record<string, { kind: "log" | "set"; add_sql: string; del_sql: string | null }> = {'],
          EntryLines,
          ['};']
        ], Lines).

arrival_statement_entry_line(arrivalstmt(Ref, log, AddSql, none, _, _), Line) :- !,
    ref_name(Ref, Name),
    js_template(AddSql, AddTemplate),
    js_object_key(Name, NameKey),
    format(atom(Line), '  ~w: { kind: "log", add_sql: ~w, del_sql: null },', [NameKey, AddTemplate]).
arrival_statement_entry_line(arrivalstmt(Ref, set, AddSql, DelSql, _, _), Line) :-
    ref_name(Ref, Name),
    js_template(AddSql, AddTemplate),
    js_template(DelSql, DelTemplate),
    js_object_key(Name, NameKey),
    format(atom(Line), '  ~w: { kind: "set", add_sql: ~w, del_sql: ~w },', [NameKey, AddTemplate, DelTemplate]).

arrival_statement_fn_lines(Name, Lines) :-
    format(atom(UndeclaredError), '    throw new Error(`~w: tick received an arrival for undeclared rel \'${arrival.rel}\'`);', [Name]),
    format(atom(RetractLogError), '      throw new Error(`~w: retract from log rel \'${arrival.rel}\' (engine.pl retract_from_log)`);', [Name]),
    format(atom(NoDeleteError), '      throw new Error(`~w: rel \'${arrival.rel}\' has no delete statement`);', [Name]),
    Lines =
    [ 'function arrival_statement(arrival: IArrivalRow): SqlStatement {',
      '  const template = ARRIVAL_STATEMENTS[arrival.rel];',
      '  if (template === undefined) {',
      UndeclaredError,
      '  }',
      '  if (arrival.sign === "del") {',
      '    if (template.kind === "log") {',
      RetractLogError,
      '    }',
      '    if (template.del_sql === null) {',
      NoDeleteError,
      '    }',
      '    return { sql: template.del_sql, args: bind_args(arrival.row) };',
      '  }',
      '  return { sql: template.add_sql, args: bind_args(arrival.row) };',
      '}',
      '',
      'function apply_arrivals(seam: ISqlSeam, arrivals: IArrivalBatch): Observable<unknown> {',
      '  const statements: SqlStatement[] = arrivals.map(arrival_statement);',
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
        deltastmt(Ref, _SelectSql, DeltaTable, BoundarySql, _StoredSelectSql), Line) :-
    ref_name(Ref, Name),
    relplan_storage_name(RelPlans, Ref, StorageName),
    relplan_shape(RelPlans, Ref, Kind, Columns, KeyOrNone, ColumnTypes),
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
    atomic_list_concat(['__frontier_', StorageName], FrontierTable),
    atomic_list_concat(['__next_frontier_', StorageName], NextFrontierTable),
    % departure_frontier_table_name is OPTIONAL on IIncrementalRelationPlan and
    % emitted only for a rel some rule binds with finalize/1, so a program
    % with no departure arm renders the entry it always rendered, character
    % for character.
    (   memberchk(Ref, DepartureRefs)
    ->  atomic_list_concat(['__departure_frontier_', StorageName],
                           DepartureTable),
        atomic_list_concat([', departure_frontier_table_name: "',
                            DepartureTable, '"'], DepartureField)
    ;   DepartureField = ''
    ),
    % rule_observers is emitted on EVERY relation entry, empty array when no
    % rule reads this rel's event tables, so the runtime's boot-time skip has
    % a per-rel observer set to test against.
    (   memberchk(Ref-Observers, ObserverMap)
    ->  true
    ;   Observers = []
    ),
    rel_ref_text_list(Observers, ObserverRefTexts),
    quoted_string_array_text(ObserverRefTexts, ObserversText),
    shared_frontier_field(Ref, RelPlans, SharedField),
    atomic_list_concat(['  { rel: "', Name,
                        '", kind: "', Kind,
                        '", table_name: "', StorageName,
                        '", delta_table_name: "', DeltaTable,
                        '", frontier_table_name: "', FrontierTable,
                        '", next_frontier_table_name: "', NextFrontierTable,
                        '", columns: ', ColumnsText,
                        ', column_types: ', ColumnTypesText,
                        ', key_indices: [', KeyIndicesText,
                        '], arrival_add_sql: ', ArrivalAddTemplate,
                        ', arrival_del_sql: ', ArrivalDelTemplate,
                        ', boundary_sql: ', BoundaryTemplate,
                        DepartureField, SharedField,
                        ', rule_observers: ', ObserversText,
                        ' },'], Line).

% Emitted only under frontier(shared), so per_rel modules stay byte-identical.
shared_frontier_field(Ref, RelPlans, SharedField) :-
    (   lower:frontier_mode(shared),
        lower:shared_frontier_relation_id(RelPlans, Ref, RelationId)
    ->  format(atom(SharedField),
               ', shared_frontier: { relation_id: ~w }', [RelationId])
    ;   SharedField = ''
    ).

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

edge_statement_head_ref(edgestmt(HeadRef, _, _, _, _, _, _, _, _), HeadRef).

% Absent, not null, when nothing was built: intern(direct) never produces one
% and its emitted bytes must not move (§15.4).
intern_sql_field([], '') :- !.
intern_sql_field(InternSqls, Field) :-
    sql_template_array_text(InternSqls, ArrayText),
    format(atom(Field), ', intern_sql: ~w', [ArrayText]).

support_intern_sql_field([], '') :- !.
support_intern_sql_field(InternSqls, Field) :-
    sql_template_array_text(InternSqls, ArrayText),
    format(atom(Field), ', support_intern_sql: ~w', [ArrayText]).

sql_template_array_text(Sqls, ArrayText) :-
    maplist(js_template, Sqls, Templates),
    atomic_list_concat(Templates, ', ', Joined),
    format(atom(ArrayText), '[~w]', [Joined]).

incremental_edge_statement_entry_line(RelPlans,
        edgestmt(HeadRef, _TriggerRef, HeadColumns, KeyColumns, _ProjectSql,
                 _WriteSql, DeltaProjectSql, _EdgeTriggerKind,
                 edgeinterns(_, DeltaInternSqls)), RuleId, Line) :-
    ref_name(HeadRef, HeadName),
    relplan_storage_name(RelPlans, HeadRef, HeadStorageName),
    relplan_kind(RelPlans, HeadRef, HeadKind),
    format(atom(DeltaTable), '__delta_~w', [HeadStorageName]),
    quoted_string_array_text(HeadColumns, ColumnsText),
    key_indices(HeadColumns, KeyColumns, KeyIndices),
    atomic_list_concat(KeyIndices, ', ', KeyIndicesText),
    js_template(DeltaProjectSql, DeltaProjectTemplate),
    intern_sql_field(DeltaInternSqls, InternField),
    format(atom(Line),
           '  { head_rel: "~w", rule_id: "~w", head_kind: "~w", head_table_name: "~w", head_delta_table_name: "~w", head_columns: ~w, key_indices: [~w], project_sql: ~w~w },',
           [HeadName, RuleId, HeadKind, HeadStorageName, DeltaTable, ColumnsText,
            KeyIndicesText, DeltaProjectTemplate, InternField]).

incremental_level_statement_lines(Program, LevelStatements, RelPlans,
                                  CyclicHeadGroups, Lines) :-
    maplist(level_statement_head_ref, LevelStatements, HeadRefs),
    statement_rule_ids(Program, HeadRefs, RuleIds),
    maplist(incremental_level_statement_entry_line(RelPlans, CyclicHeadGroups),
            LevelStatements, RuleIds, EntryLines),
    append(
        [ ['const INCREMENTAL_LEVEL_STATEMENTS: readonly IIncrementalLevelStatement[] = ['],
          EntryLines,
          ['];']
        ], Lines).

level_statement_head_ref(levelstmt(HeadRef, _, _, _, _, _, _), HeadRef).

incremental_level_statement_entry_line(RelPlans, CyclicHeadGroups,
        levelstmt(HeadRef, DeleteSql, InsertSqls, DeltaInsertSql, RefCountSql,
                  AggregateSql, DeltaInternSqls), RuleId, Line) :-
    ref_name(HeadRef, HeadName),
    relplan_storage_name(RelPlans, HeadRef, HeadStorageName),
    recursion_group_field(CyclicHeadGroups, HeadRef, RecursionGroupField),
    format(atom(DeltaTable), '__delta_~w', [HeadStorageName]),
    relplan_columns(RelPlans, HeadRef, HeadColumns),
    quoted_string_array_text(HeadColumns, ColumnsText),
    optional_sql_template(DeltaInsertSql, DeltaInsertTemplate),
    maplist(quote_ident_local, HeadColumns, QuotedHeadColumns),
    atomic_list_concat(QuotedHeadColumns, ', ', HeadColumnsSql),
    format(atom(SelectSql), 'SELECT ~w FROM "~w"', [HeadColumnsSql, HeadStorageName]),
    js_template(SelectSql, SelectTemplate),
    % A REAL newline, not the two-character sequence `\n`. These three joins
    % used to emit backslash-n and rely on the JS template literal to turn it
    % into a newline -- which is the same conflation js_template/2 above stopped
    % making. A template literal carries a raw newline fine, and the emitted
    % statement text is now the text sqlite receives, byte for byte.
    atomic_list_concat([DeleteSql | InsertSqls], ';\n', RecomputeSql),
    js_template(RecomputeSql, RecomputeTemplate),
    ref_count_sql_text(RefCountSql, RefCountText, ExpandText, DredText,
                       FixpointIrText, SupportInternSqls, SupportCountPlan),
    aggregate_sql_text(AggregateSql, AggregateText),
    intern_sql_field(DeltaInternSqls, InternField),
    support_intern_sql_field(SupportInternSqls, SupportInternField),
    support_count_sql_field(SupportCountPlan, SupportCountField),
    format(atom(Line),
           '  { head_rel: "~w", rule_id: "~w", head_delta_table_name: "~w", head_columns: ~w, insert_sql: ~w, select_sql: ~w, recompute_sql: ~w, support_sql: ~w, expand_sql: ~w, dred_sql: ~w, fixpoint_ir: ~w, aggregate_sql: ~w~w~w~w~w },',
           [HeadName, RuleId, DeltaTable, ColumnsText, DeltaInsertTemplate,
            SelectTemplate, RecomputeTemplate, RefCountText, ExpandText,
            DredText, FixpointIrText, AggregateText, InternField,
            SupportInternField, SupportCountField, RecursionGroupField]).

% Absent under frontier(per_rel), so the field itself never renders and a
% per-rel module keeps its bytes.
support_count_sql_field(none, '') :- !.
support_count_sql_field(supportcount(ClearSql, WriteSqls), Field) :-
    js_template(ClearSql, ClearTemplate),
    maplist(js_template, WriteSqls, WriteTemplates),
    atomic_list_concat(WriteTemplates, ', ', WriteJoined),
    format(atom(Field),
           ', support_count_sql: { clear_sql: ~w, write_sqls: [~w] }',
           [ClearTemplate, WriteJoined]).

% Absent on an acyclic head, so every module of a program with no level cycle
% renders byte-identically to one emitted before outer rounds existed.
recursion_group_field(CyclicHeadGroups, HeadRef, '') :-
    \+ memberchk(HeadRef-_, CyclicHeadGroups), !.
recursion_group_field(CyclicHeadGroups, HeadRef, Field) :-
    memberchk(HeadRef-GroupIndex, CyclicHeadGroups),
    fixpoint_round_cap(RoundCap),
    findall(Name,
            ( member(Ref-GroupIndex, CyclicHeadGroups), ref_name(Ref, Name) ),
            Names),
    atomic_list_concat(Names, ',', JoinedNames),
    format(atom(Field),
           ', recursion_group: { group: ~w, round_cap: ~w, heads: "[~w]" }',
           [GroupIndex, RoundCap, JoinedNames]).

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
           '  { rel: "~w", count: ~w, delete_sql: ~w },',
           [Name, Limit, DeleteTemplate]).

optional_sql_template(none, null) :- !.
optional_sql_template(Sql, Template) :- js_template(Sql, Template).

ref_count_sql_text(none, null, null, null, null, [], none) :- !.
ref_count_sql_text(refcountsql(ClearSql, SeedSql, UpdateSql, StageRetractSql,
                               CollectZeroSql, ClearNewSql, FillNewSql,
                               StageAddSql, StageFrontierSql,
                               StageNextFrontierSql, InsertNewSql, ExpandPlan,
                               DredPlan, FixpointIr, SupportInternSqls,
                               SupportCountPlan),
                 Text, ExpandText, DredText, FixpointIrText, SupportInternSqls,
                 SupportCountPlan) :-
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
                           AbsorbASql, AbsorbBSql, RoundCap),
                Text) :-
    maplist(js_template, [ClearASql, ClearBSql, HopABSql, HopBASql,
                          AbsorbASql, AbsorbBSql],
            [ClearATemplate, ClearBTemplate, HopABTemplate, HopBATemplate,
             AbsorbATemplate, AbsorbBTemplate]),
    maplist(js_template, SeedSqls, SeedTemplates),
    atomic_list_concat(SeedTemplates, ', ', SeedJoined),
    format(atom(Text),
           '{ clear_a_sql: ~w, clear_b_sql: ~w, seed_sqls: [~w], hop_ab_sql: ~w, hop_ba_sql: ~w, absorb_a_sql: ~w, absorb_b_sql: ~w, round_cap: ~w }',
           [ClearATemplate, ClearBTemplate, SeedJoined, HopABTemplate,
            HopBATemplate, AbsorbATemplate, AbsorbBTemplate, RoundCap]).

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
           '{ clear_ping_sql: ~w, clear_pong_sql: ~w, clear_cone_sql: ~w, assert_seed_sqls: ~w, assert_hop_ab_sql: ~w, assert_hop_ba_sql: ~w, commit_a_sql: ~w, commit_b_sql: ~w, arrival_a_sql: ~w, arrival_b_sql: ~w, dred_seed_sqls: ~w, dred_hop_ab_sql: ~w, dred_hop_ba_sql: ~w, cone_absorb_a_sql: ~w, cone_absorb_b_sql: ~w, cone_trim_sql: ~w, head_delete_sql: ~w, rederive_seed_sqls: ~w, revive_hop_ab_sql: ~w, revive_hop_ba_sql: ~w, cone_drop_a_sql: ~w, cone_drop_b_sql: ~w, stage_retract_sql: ~w, head_count_sql: ~w }',
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
    js_shape([head-shape([rel-quoted(HeadName),columns-ColumnsText,types-TypesText]),storage-StorageText,assert-AssertText,dred-DredText,revive-ReviveText,expand-ExpandText], Text).

% lower.pl:ir_column_class/3. Named keys, so the interning contract adds one
% without moving anything an executor already reads.
fixpoint_storage_text(relstorage(ref(Name, Arity), ColumnClasses), Text) :-
    fixpoint_term_array_text(fixpoint_column_class_text, ColumnClasses,
                             ClassesText),
    js_shape([rel-quoted(Name),arity-Arity,columns-ClassesText], Text).

fixpoint_column_class_text(colclass(Column, Type, StorageClass, Collation,
                                    Encoding), Text) :-
    js_string(Column, ColumnText),
    fixpoint_collation_text(Collation, CollationText),
    fixpoint_encoding_text(Encoding, EncodingText),
    js_shape([name-ColumnText,type-quoted(Type),storage-quoted(StorageClass),collation-CollationText,encoding-EncodingText], Text).

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
    js_shape([seeds-SeedsText,hop-HopsText,stop-shape([seed-SeedProbeText,hop-HopProbeText]),emit-EmitText], Text).

fixpoint_arm_array_text(Arms, Text) :-
    maplist(fixpoint_arm_text, Arms, ArmTexts),
    atomic_list_concat(ArmTexts, ', ', Joined),
    format(atom(Text), '[~w]', [Joined]).

fixpoint_emit_text(none, null) :- !.
fixpoint_emit_text(order(Order), Text) :- js_string(Order, Text).

fixpoint_probe_text(none, null) :- !.
fixpoint_probe_text(probe(Kind, Target), Text) :-
    fixpoint_probe_target(Target, TargetName),
    js_shape([kind-quoted(Kind),target-quoted(TargetName)], Text).

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
    js_shape([sources-SourcesText,equalities-EqualitiesText,filters-FiltersText,project-ProjectText,selfIndex-SelfIndexText], Text).

js_shape(Fields, Text) :-
    maplist(js_shape_field, Fields, FieldTexts),
    atomic_list_concat(FieldTexts, ', ', Joined),
    format(atom(Text), '{ ~w }', [Joined]).

js_shape_field(Name-Value, Text) :- js_shape_value(Value, ValueText), format(atom(Text), '~w: ~w', [Name, ValueText]).
js_shape_value(quoted(Value), Text) :- !, js_string(Value, Text).
js_shape_value(shape(Fields), Text) :- !, js_shape(Fields, Text).
js_shape_value(Value, Value).

fixpoint_term_array_text(Renderer, Terms, Text) :-
    maplist(Renderer, Terms, Texts),
    atomic_list_concat(Texts, ', ', Joined),
    format(atom(Text), '[~w]', [Joined]).

fixpoint_self_index_text(none, null) :- !.
fixpoint_self_index_text(Index, Index).

fixpoint_source_text(src(Index, Source), Text) :-
    fixpoint_source_kind_text(Source, KindText),
    js_shape([index-Index,source-KindText], Text).

fixpoint_source_kind_text(rel(ref(Name, Arity)), Text) :- !,
    js_shape([kind-quoted(rel),rel-quoted(Name),arity-Arity], Text).
fixpoint_source_kind_text(rel_or_retracted(ref(Name, Arity)), Text) :- !,
    js_shape([kind-quoted(relOrRetracted),rel-quoted(Name),arity-Arity], Text).
fixpoint_source_kind_text(delta(ref(Name, Arity), Sign, liveness(Liveness)),
                          Text) :- !,
    js_shape([kind-quoted(delta),rel-quoted(Name),arity-Arity,sign-Sign,liveness-quoted(Liveness)], Text).
fixpoint_source_kind_text(wave(Slot), Text) :- !,
    js_shape([kind-quoted(wave),slot-quoted(Slot)], Text).
fixpoint_source_kind_text(cone, Text) :- js_shape([kind-quoted(cone)], Text).

fixpoint_equality_text(eq(Left, Right), Text) :-
    fixpoint_expr_text(Left, LeftText),
    fixpoint_expr_text(Right, RightText),
    js_shape([left-LeftText,right-RightText], Text).

fixpoint_filter_text(cmp(Operator, Left, Right), Text) :- !,
    js_string(Operator, OperatorText),
    fixpoint_expr_text(Left, LeftText),
    fixpoint_expr_text(Right, RightText),
    js_shape([kind-quoted(cmp),op-OperatorText,left-LeftText,right-RightText], Text).
fixpoint_filter_text(eq_lit(Left, Literal), Text) :-
    fixpoint_expr_text(Left, LeftText),
    fixpoint_expr_text(Literal, LiteralText),
    js_shape([kind-quoted(eqLit),left-LeftText,right-LiteralText], Text).

fixpoint_expr_text(col(Index, Ordinal), Text) :- !,
    js_shape([kind-quoted(col),index-Index,ordinal-Ordinal], Text).
fixpoint_expr_text(lit(Literal), Text) :- !,
    fixpoint_literal_text(Literal, Text).
% `type` is compile_expr/4's result type: `/` over two ints is SQLite integer
% division, over anything else a REAL divide (lower.pl:arithmetic_rendering/6).
fixpoint_expr_text(arith(Operator, Left, Right, Type), Text) :- !,
    js_string(Operator, OperatorText),
    fixpoint_expr_text(Left, LeftText),
    fixpoint_expr_text(Right, RightText),
    js_shape([kind-quoted(arith),op-OperatorText,type-quoted(Type),left-LeftText,right-RightText], Text).
fixpoint_expr_text(concat(Parts), Text) :-
    fixpoint_term_array_text(fixpoint_expr_text, Parts, PartsText),
    js_shape([kind-quoted(concat),parts-PartsText], Text).

fixpoint_literal_text(text(Value), Text) :- !,
    js_string(Value, ValueText),
    js_shape([kind-quoted(lit),type-quoted(text),value-ValueText], Text).
fixpoint_literal_text(Literal, Text) :-
    Literal =.. [TypeName, Value],
    js_shape([kind-quoted(lit),type-quoted(TypeName),value-Value], Text).

% The group-scoped aggregate plan (lower.pl level_aggregate_sql/4): clear the
% scope, seed it from this tick's staged deltas, delete the scoped groups
% (RETURNING the -1 events), re-derive them (RETURNING the +1 events).
aggregate_sql_text(none, null) :- !.
aggregate_sql_text(aggsql(_ScopeColumns, _ScopeTypes, ScopeClearSql, ScopeSeedSqls,
                          DeleteScopedSql, InsertScopedSqls, InternSqls), Text) :-
    js_template(ScopeClearSql, ScopeClearTemplate),
    maplist(js_template, ScopeSeedSqls, ScopeSeedTemplates),
    atomic_list_concat(ScopeSeedTemplates, ', ', ScopeSeedJoined),
    js_template(DeleteScopedSql, DeleteScopedTemplate),
    maplist(js_template, InsertScopedSqls, InsertScopedTemplates),
    atomic_list_concat(InsertScopedTemplates, ', ', InsertScopedJoined),
    intern_sql_field(InternSqls, InternField),
    format(atom(Text),
           '{ scope_clear_sql: ~w, scope_seed_sql: [~w], delete_scoped_sql: ~w, insert_scoped_sql: [~w]~w, delta_maintained: false }',
           [ScopeClearTemplate, ScopeSeedJoined, DeleteScopedTemplate,
            InsertScopedJoined, InternField]).
aggregate_sql_text(avgsql(_ScopeColumns, _ScopeTypes, ScopeClearSql, ScopeSeedSqls,
                          DeleteScopedSql, InsertScopedSqls, _BootSqls), Text) :-
    js_template(ScopeClearSql, ScopeClearTemplate),
    maplist(js_template, ScopeSeedSqls, ScopeSeedTemplates),
    atomic_list_concat(ScopeSeedTemplates, ', ', ScopeSeedJoined),
    js_template(DeleteScopedSql, DeleteScopedTemplate),
    maplist(js_template, InsertScopedSqls, InsertScopedTemplates),
    atomic_list_concat(InsertScopedTemplates, ', ', InsertScopedJoined),
    format(atom(Text),
           '{ scope_clear_sql: ~w, scope_seed_sql: [~w], delete_scoped_sql: ~w, insert_scoped_sql: [~w], delta_maintained: true }',
           [ScopeClearTemplate, ScopeSeedJoined, DeleteScopedTemplate,
            InsertScopedJoined]).

quote_ident_local(Name, Quoted) :- format(atom(Quoted), '"~w"', [Name]).

key_indices(HeadColumns, KeyColumns, Indices) :-
    findall(Index0,
            ( member(Column, KeyColumns), nth0(Index0, HeadColumns, Column) ),
            Indices).

% ═══ ordered pre occurrence loop ════════════════════════════════════════════

ordered_edge_statement(edgestmt(_, _, _, _, _, _, _, ordered_arrival, _)).
ordered_edge_statement(edgestmt(_, _, _, _, _, _, _, ordered_departure, _)).

ordered_program(EdgeStatements) :-
    member(Statement, EdgeStatements),
    ordered_edge_statement(Statement),
    !.

plan_pre_refs(plan(_, prog(_, Rules), _, _, _, _, _, _, _), Refs) :-
    findall(Ref,
            ( member((_ <+ Body), Rules),
              level_body_pre_ref(Body, Ref) ),
            Refs0),
    sort(Refs0, Refs).

pre_snapshot_statement(RelPlans, Ref, Statements) :-
    relplan_columns(RelPlans, Ref, Columns),
    relplan_storage_name(RelPlans, Ref, StorageName),
    maplist(quote_ident_local, Columns, QuotedColumns),
    atomic_list_concat(QuotedColumns, ', ', ColumnsSql),
    format(atom(Delete), 'DELETE FROM "__pre_~w"', [StorageName]),
    format(atom(Insert),
           'INSERT INTO "__pre_~w" (~w) SELECT ~w FROM "~w"',
           [StorageName, ColumnsSql, ColumnsSql, StorageName]),
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
      [ 'function snapshot_ordered_pre(seam: ISqlSeam): Observable<void> {',
        SnapshotReturn,
        '}'
      ].

ordered_boundary_carry_line(level, Ref, Line) :-
    ref_name(Ref, Name),
    format(atom(Line),
           '  for (const row of multiset_diff(mid["~w"], after["~w"]).add) { const row_text = JSON.stringify(row); const exact = JSON.stringify(["~w", row]); if (seen.has(exact) || !(boundary_adds.get("~w")?.has(row_text) ?? false)) continue; seen.add(exact); additions.push({ rel: "~w", add: [row], del: [] }); }',
           [Name, Name, Name, Name, Name]).

ordered_carry_lines(false, _, _, []) :- !.
ordered_carry_lines(true, _EdgeStatements, LevelHeadedRefs, Lines) :-
    maplist(ordered_boundary_carry_line(level), LevelHeadedRefs, LevelLines),
    append(
      [ [ 'function ordered_carry_additions(mid: Snapshot, after: Snapshot, boundary: ITickDeltas, written: readonly IOrderedWrite[]): readonly IRelDelta[] {',
          '  const boundary_by_rel = new Map(boundary.rels.map((delta) => [delta.rel, delta]));',
          '  const boundary_adds = new Map([...boundary_by_rel].map(([rel, delta]) => [rel, new Set(delta.add.map((row) => JSON.stringify(row)))]));',
          '  const additions: IRelDelta[] = [];',
          '  const seen = new Set<string>();',
          '  for (const { arm, row } of written) {',
          '    const row_text = JSON.stringify(row);',
          '    const exact = JSON.stringify([arm.head_rel, row]);',
          '    if (seen.has(exact) || !(boundary_adds.get(arm.head_rel)?.has(row_text) ?? false)) continue;',
          '    seen.add(exact);',
          '    additions.push({ rel: arm.head_rel, add: [row], del: [] });',
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
                 WriteSql, _, EdgeTriggerKind,
                 edgeinterns(ProjectInternSqls, _)), Line) :-
    ref_name(HeadRef, HeadName),
    ref_name(TriggerRef, TriggerName),
    relplan_kind(RelPlans, HeadRef, HeadKind),
    relplan_storage_name(RelPlans, HeadRef, HeadStorageName),
    ordered_trigger_kind(EdgeTriggerKind, TriggerKind),
    quoted_string_array_text(HeadColumns, HeadColumnsText),
    key_indices(HeadColumns, KeyColumns, KeyIndices),
    atomic_list_concat(KeyIndices, ', ', KeyIndicesText),
    js_template(ProjectSql, ProjectTemplate),
    js_template(WriteSql, WriteTemplate),
    ( memberchk(HeadRef, PreRefs) -> EvolvesPre = true ; EvolvesPre = false ),
    intern_sql_field(ProjectInternSqls, InternField),
    format(atom(Line),
           '  { trigger_rel: "~w", trigger_kind: "~w", head_rel: "~w", head_table_name: "~w", head_kind: "~w", head_columns: ~w, key_indices: [~w], project_sql: ~w, write_sql: ~w, evolves_pre: ~w~w },',
           [TriggerName, TriggerKind, HeadName, HeadStorageName, HeadKind,
            HeadColumnsText,
            KeyIndicesText, ProjectTemplate, WriteTemplate, EvolvesPre,
            InternField]).

ordered_arrival_accept_line(RelPlans, TriggerRef, Line) :-
    ref_name(TriggerRef, TriggerName),
    relplan_kind(RelPlans, TriggerRef, TriggerKind),
    format(atom(Line),
           '  for (const arrival of trigger_occurrences("~w", "~w", before["~w"], arrivals)) accepted.add(arrival);',
           [TriggerKind, TriggerName, TriggerName]).

ordered_departure_read_entry(RelPlans, TriggerRef, Line) :-
    ref_name(TriggerRef, TriggerName),
    relplan_storage_name(RelPlans, TriggerRef, TriggerStorageName),
    relplan_columns(RelPlans, TriggerRef, TriggerColumns),
    format(atom(DepartureTable), '__departure_frontier_~w', [TriggerStorageName]),
    maplist(quote_ident_local, TriggerColumns, QuotedColumns),
    atomic_list_concat(QuotedColumns, ', ', ColumnsSql),
    format(atom(Sql), 'SELECT ~w FROM "~w" ORDER BY "_phase", "_sequence"',
           [ColumnsSql, DepartureTable]),
    js_template(Sql, SqlTemplate),
    quoted_string_array_text(TriggerColumns, ColumnsText),
    format(atom(Line),
           '  { rel: "~w", sql: ~w, columns: ~w },',
           [TriggerName, SqlTemplate, ColumnsText]).

ordered_carry_read_entry(RelPlans, TriggerRef, Line) :-
    ref_name(TriggerRef, TriggerName),
    relplan_storage_name(RelPlans, TriggerRef, TriggerStorageName),
    relplan_columns(RelPlans, TriggerRef, TriggerColumns),
    maplist(quote_ident_local, TriggerColumns, QuotedColumns),
    atomic_list_concat(QuotedColumns, ', ', ColumnsSql),
    format(atom(Sql),
           'SELECT "_sequence" AS "__sequence", ~w FROM "__frontier_~w" ORDER BY "_phase", "_sequence"',
           [ColumnsSql, TriggerStorageName]),
    js_template(Sql, SqlTemplate),
    quoted_string_array_text(TriggerColumns, ColumnsText),
    format(atom(Line),
           '  { rel: "~w", sql: ~w, columns: ~w },',
           [TriggerName, SqlTemplate, ColumnsText]).

ordered_level_occurrence_line(LevelRef, Line) :-
    ref_name(LevelRef, Name),
    format(atom(Line),
           '  for (const row of multiset_diff(before["~w"], mid["~w"]).add) occurrences.push({ rel: "~w", kind: "arrival", row });',
           [Name, Name, Name]).

edge_statements_intern(EdgeStatements, true) :-
    member(edgestmt(_, _, _, _, _, _, _, _, edgeinterns(InternSqls, _)),
           EdgeStatements),
    InternSqls \== [],
    !.
edge_statements_intern(_, false).

% Two spellings, picked by whether ANY arm builds a string: the direct-mode
% text is the one that was already there, byte for byte.
ordered_arm_interface_line(false,
    'interface IOrderedEdgeArm { readonly trigger_rel: string; readonly trigger_kind: "arrival" | "departure"; readonly head_rel: string; readonly head_table_name: string; readonly head_kind: "log" | "set"; readonly head_columns: readonly string[]; readonly key_indices: readonly number[]; readonly project_sql: string; readonly write_sql: string; readonly evolves_pre: boolean }') :- !.
ordered_arm_interface_line(true,
    'interface IOrderedEdgeArm { readonly trigger_rel: string; readonly trigger_kind: "arrival" | "departure"; readonly head_rel: string; readonly head_table_name: string; readonly head_kind: "log" | "set"; readonly head_columns: readonly string[]; readonly key_indices: readonly number[]; readonly project_sql: string; readonly write_sql: string; readonly evolves_pre: boolean; readonly intern_sql?: readonly string[] }').

ordered_arm_project_line(false,
    '  return forkJoin(arms.map((arm) => seam.runner.execute(seam.db, { sql: arm.project_sql, args: bind_args(occurrence.row) }).pipe(') :- !.
ordered_arm_project_line(true,
    '  return forkJoin(arms.map((arm) => intern_then_execute(seam, arm.intern_sql, { sql: arm.project_sql, args: bind_args(occurrence.row) }).pipe(').

ordered_occurrence_lines(false, _, _, _, _, []) :- !.
ordered_occurrence_lines(true, EdgeStatements, RelPlans, PreRefs,
                         LevelHeadedRefs, Lines) :-
    maplist(ordered_arm_entry_line(RelPlans, PreRefs), EdgeStatements,
            ArmLines),
    edge_statements_intern(EdgeStatements, ArmsIntern),
    ordered_arm_interface_line(ArmsIntern, ArmInterfaceLine),
    ordered_arm_project_line(ArmsIntern, ArmProjectLine),
    findall(TriggerRef,
            ( member(edgestmt(_, TriggerRef, _, _, _, _, _, TriggerKind, _),
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
            ( member(edgestmt(_, TriggerRef, _, _, _, _, _, TriggerKind, _),
                     EdgeStatements),
              ordered_trigger_kind(TriggerKind, departure) ),
            DepartureRefs0),
    sort(DepartureRefs0, OrderedDepartureRefs),
    maplist(ordered_departure_read_entry(RelPlans), OrderedDepartureRefs,
            DepartureReadLines),
    ( DepartureReadLines == []
    -> ReadDepartureBody =
       [ 'function read_ordered_departures(seam: ISqlSeam): Observable<readonly IOrderedOccurrence[]> {',
         '  void seam;',
         '  return of([]);',
         '}'
       ]
    ; ReadDepartureBody =
       [ 'function read_ordered_departures(seam: ISqlSeam): Observable<readonly IOrderedOccurrence[]> {',
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
      [ [ ArmInterfaceLine,
          'interface IOrderedOccurrence { readonly rel: string; readonly kind: "arrival" | "departure"; readonly row: IRow; readonly sequence?: number }',
          'interface IOrderedWrite { readonly arm: IOrderedEdgeArm; readonly row: IRow }',
          '',
          'function quote_ordered_identifier(identifier: string): string {',
          '  return \'"\' + identifier.replaceAll(\'"\', \'""\') + \'"\';',
          '}',
          '',
          'function ordered_pre_write_statement(write: IOrderedWrite): SqlStatement | null {',
          '  const { arm, row } = write;',
          '  if (!arm.evolves_pre) return null;',
          '  const table = quote_ordered_identifier("__pre_" + arm.head_table_name);',
          '  const columns = arm.head_columns.map(quote_ordered_identifier);',
          '  const placeholders = columns.map(() => "?").join(", ");',
          '  if (arm.head_kind === "log") {',
          '    return { sql: "INSERT INTO " + table + " (" + columns.join(", ") + ") VALUES (" + placeholders + ")", args: bind_args(row) };',
          '  }',
          '  const key_indices = new Set(arm.key_indices);',
          '  const key_columns = arm.key_indices.map((index) => columns[index]!);',
          '  const non_key_columns = columns.filter((_column, index) => !key_indices.has(index));',
          '  const conflict = non_key_columns.length === 0',
          '    ? "ON CONFLICT(" + key_columns.join(", ") + ") DO NOTHING"',
          '    : "ON CONFLICT(" + key_columns.join(", ") + ") DO UPDATE SET " + non_key_columns.map((column) => column + " = excluded." + column).join(", ");',
          '  return { sql: "INSERT INTO " + table + " (" + columns.join(", ") + ") VALUES (" + placeholders + ") " + conflict, args: bind_args(row) };',
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
          'function ordered_outside_occurrences(before: Snapshot, arrivals: IArrivalBatch): readonly IOrderedOccurrence[] {',
          '  const accepted = new Set<IArrivalRow>();'
        ],
        AcceptLines,
        [ '  return arrivals.filter((arrival) => accepted.has(arrival)).map((arrival): IOrderedOccurrence => ({ rel: arrival.rel, kind: "arrival", row: arrival.row }));',
          '}',
          ''
        ],
        ReadDepartureBody,
        [ '',
          'function read_ordered_carry(seam: ISqlSeam): Observable<readonly IOrderedOccurrence[]> {',
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
          'function ordered_level_occurrences(before: Snapshot, mid: Snapshot): readonly IOrderedOccurrence[] {',
          '  const occurrences: IOrderedOccurrence[] = [];'
        ],
        LevelOccurrenceLines,
        [ '  return occurrences;',
          '}',
          '',
          'function apply_ordered_occurrence(seam: ISqlSeam, occurrence: IOrderedOccurrence, written: IOrderedWrite[]): Observable<void> {',
          '  const arms = ORDERED_EDGE_ARMS.filter((arm) => arm.trigger_rel === occurrence.rel && arm.trigger_kind === occurrence.kind);',
          '  if (arms.length === 0) return of(undefined);',
          ArmProjectLine,
          '    map((result) => ({ arm, rows: result.rows.map((row) => arm.head_columns.map((column) => row[column] as IRowValue) as IRow) })),',
          '  ))).pipe(',
          '    concatMap((groups) => {',
          '      const writes: IOrderedWrite[] = [];',
          '      const exact = new Set<string>();',
          '      const keyed = new Map<string, IRow>();',
          '      for (const group of groups) {',
          '        for (const row of group.rows) {',
          '          const exact_key = JSON.stringify([group.arm.head_rel, row]);',
          '          if (exact.has(exact_key)) continue;',
          '          exact.add(exact_key);',
          '          if (group.arm.head_kind === "set") {',
          '            const key = JSON.stringify([group.arm.head_rel, group.arm.key_indices.map((index) => row[index])]);',
          '            const prior = keyed.get(key);',
          '            if (prior !== undefined && JSON.stringify(prior) !== JSON.stringify(row)) {',
          '              throw new Error(`keyed conflict in ordered occurrence for ${group.arm.head_rel}: ${key}`);',
          '            }',
          '            keyed.set(key, row);',
          '          }',
          '          writes.push({ arm: group.arm, row });',
          '        }',
          '      }',
          '      if (writes.length === 0) return of(undefined);',
          '      const statements = writes.flatMap((write): readonly SqlStatement[] => {',
          '        const base: SqlStatement = { sql: write.arm.write_sql, args: bind_args(write.row) };',
          '        const pre = ordered_pre_write_statement(write);',
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
          'function process_ordered_occurrences(seam: ISqlSeam, before: Snapshot, mid: Snapshot, arrivals: IArrivalBatch): Observable<readonly IOrderedWrite[]> {',
          '  return forkJoin([read_ordered_carry(seam), read_ordered_departures(seam)]).pipe(',
          '    concatMap(([carry, departures]) => {',
          '      const written: IOrderedWrite[] = [];',
          '      const occurrences = [...carry, ...departures, ...ordered_outside_occurrences(before, arrivals), ...ordered_level_occurrences(before, mid)];',
          '      return occurrences.reduce(',
          '        (work, occurrence) => work.pipe(concatMap(() => apply_ordered_occurrence(seam, occurrence, written))),',
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
% all) still has run_tick_fn_lines call `recompute_levels(seam)` unconditionally
% (that call is not itself gated on LevelStatements), so this needs a real
% zero-op function, not silent failure: `of(undefined)` is the one-void-then-
% complete shape the async-becomes-rxjs law calls for (EMPTY would complete
% without emitting, which starves the caller's `.pipe(map(() => before))` of
% a value and stalls the whole tick chain).
recompute_levels_fn_lines(_, _, [], Lines) :- !,
    Lines =
    [ 'function recompute_levels(seam: ISqlSeam): Observable<void> {',
      '  void seam;',
      '  return of(undefined);',
      '}'
    ].
% @comment-ok: pre-existing fixpoint-ceiling receipts; the hunk shifted when
% the adjacent naive tick predicates were deleted.
% THE LEVEL FIXPOINT. One DELETE-then-INSERT-per-clause pass gives a
% self-referential head exactly as many derivation rounds as it has clauses,
% from an empty table, every tick: a two-clause fold reaches two links and
% stops, whatever the data says (sprefa-lab-foldwall/FOLDWALL.md measured the
% ceiling tracking clause count exactly). The one caller carrying the ceiling
% is run_ordered_tick, which any seq/1 or pre/1 program takes.
%
% So the DELETE runs ONCE and the INSERT set repeats until a round adds no row.
% The round set is recursive_level_refs/2: self-reads PLUS mutual-cycle heads.
%
% Every clause is INSERT OR IGNORE, so a round can only add rows and the count
% is monotone; datalog closure over a finite store is what makes it stop.
recompute_levels_fn_lines(RelPlans, SelfReferentialLevelRefs, LevelStatements, Lines) :-
    SelfReferentialLevelRefs \== [],
    LevelStatements \== [],
    !,
    findall(DeleteSql,
            member(levelstmt(_, DeleteSql, _, _, _, _, _), LevelStatements),
            DeleteSqls),
    findall(InsertSql,
            ( member(levelstmt(_, _, InsertSqls, _, _, _, _), LevelStatements),
              member(InsertSql, InsertSqls) ),
            RoundInsertSqls),
    % Real newline; see the note at the recompute join.
    atomic_list_concat(DeleteSqls, ';\n', JoinedDeleteSql),
    atomic_list_concat(RoundInsertSqls, ';\n', JoinedInsertSql),
    js_template(JoinedDeleteSql, DeleteTemplate),
    js_template(JoinedInsertSql, InsertTemplate),
    level_row_count_sql(RelPlans, LevelStatements, CountSql),
    js_template(CountSql, CountTemplate),
    format(atom(DeleteLine), '  const delete_sql = ~w;', [DeleteTemplate]),
    format(atom(InsertLine), '  const insert_sql = ~w;', [InsertTemplate]),
    format(atom(CountLine), '  const count_sql = ~w;', [CountTemplate]),
    Lines =
    [ 'function recompute_levels(seam: ISqlSeam): Observable<void> {',
      DeleteLine,
      InsertLine,
      CountLine,
      '  return seam.runner.executeMultiple(seam.db, delete_sql).pipe(',
      '    map(() => -1),',
      '    expand((prior_rows) => seam.runner.executeMultiple(seam.db, insert_sql).pipe(',
      '      concatMap(() => seam.runner.scalar(seam.db, count_sql)),',
      '      concatMap((rows) => (rows === prior_rows ? EMPTY : of(rows))),',
      '    )),',
      '    last(),',
      '    map(() => undefined),',
      '  );',
      '}'
    ].
recompute_levels_fn_lines(_, _, LevelStatements, Lines) :-
    LevelStatements \== [],
    % InsertSqls is a LIST (lower.pl:level_statement_group/3 -- one entry per
    % rule clause sharing this head, so a multi-clause head's rows all
    % INSERT after exactly one DELETE, never one DELETE per clause); flattens
    % to the identical [Delete, Insert] sequence as before for the common
    % single-clause case.
    findall(Sql, ( member(levelstmt(_, DeleteSql, InsertSqls, _, _, _, _), LevelStatements), ( Sql = DeleteSql ; member(Sql, InsertSqls) ) ), Sqls),
    % Real newline; see the note at the recompute join.
    atomic_list_concat(Sqls, ';\n', JoinedSql),
    js_template(JoinedSql, SqlTemplate),
    format(atom(SqlLine), '  const sql = ~w;', [SqlTemplate]),
    Lines =
    [ 'function recompute_levels(seam: ISqlSeam): Observable<void> {',
      SqlLine,
      '  return seam.runner.executeMultiple(seam.db, sql);',
      '}'
    ].

% ISqlRunner.scalar/2 reads the first column of the first row, so the round
% count is one SELECT with no row shape to decode.
level_row_count_sql(RelPlans, LevelStatements, Sql) :-
    findall(CountExpr,
            ( member(levelstmt(HeadRef, _, _, _, _, _, _), LevelStatements),
              relplan_storage_name(RelPlans, HeadRef, StorageName),
              quote_ident_local(StorageName, QuotedHead),
              format(atom(CountExpr), '(SELECT count(*) FROM ~w)',
                     [QuotedHead]) ),
            CountExprs),
    atomic_list_concat(CountExprs, ' + ', SummedExpr),
    format(atom(Sql), 'SELECT ~w', [SummedExpr]).

% A level head that reads ITSELF positively: the edge
% strat.pl:group_head_edges/3 drops (`DependsOnRef \== HeadRef`).
self_referential_level_refs(Rules, Refs) :-
    findall(HeadRef,
            ( member(Rule, Rules), Rule = (_ <- Body),
              rule_head_ref(Rule, HeadRef),
              body_ref_uses(Body, Uses),
              memberchk(use(HeadRef, _, pos, _), Uses) ),
            Refs0),
    sort(Refs0, Refs).

recursive_level_refs(Rules, Refs) :-
    self_referential_level_refs(Rules, SelfRefs),
    recursive_stratum_groups(Rules, RecursiveGroups),
    findall(Ref,
            ( member(Group, RecursiveGroups),
              member(Rule, Group),
              rule_head_ref(Rule, Ref) ),
            GroupRefs),
    append(SelfRefs, GroupRefs, Refs0),
    sort(Refs0, Refs).

% ═══ build_deltas ═════════════════════════════════════════════════════════════

build_deltas_fn_lines(RelPlans, EdgeStatements, _RetentionStatements,
                      DepartureRefs, Lines) :-
    maplist(diff_local_line, RelPlans, DiffLines),
    maplist(rel_entry_line, RelPlans, RelEntryLines),
    carry_pending_expr(EdgeStatements, DepartureRefs, CarryExpr),
    format(atom(CarryLine), '    carry_pending: ~w,', [CarryExpr]),
    append(
        [ ['function build_deltas(before: Snapshot, after: Snapshot): ITickDeltas {'],
          DiffLines,
          ['  return {', '    rels: ['],
          RelEntryLines,
          ['    ],', CarryLine, '  };', '}']
        ], Lines).

% Retention runs between the before and after snapshots, so multiset_diff must
% retain reclaimed rows as deletions; no keep-specific suppression is needed.
diff_local_line(RelPlan, Line) :-
    relplan_parts(RelPlan, Ref, _Kind, _Columns, _Key, _ColumnTypes),
    ref_name(Ref, Name),
    format(atom(Line), '  const ~w = multiset_diff(before.~w, after.~w);',
           [Name, Name, Name]).

rel_entry_line(RelPlan, Line) :-
    relplan_parts(RelPlan, Ref, _Kind, _Columns, _Key, _ColumnTypes),
    ref_name(Ref, Name),
    format(atom(Line), '      { rel: "~w", add: ~w.add, del: ~w.del },', [Name, Name, Name]).

% carry_pending (engine.pl q4/R2): true when a row this tick's edge rule(s)
% wrote SHOWS AS A DELTA (an equal-row rewrite is invisible to multiset_diff,
% so no separate no-op check is needed here -- the diff already absorbs it).
% Simplification, matching Phase A's exemplar finding 3: this ignores the
% general "post-write level growth with no edge write" carry source, safe
% for both target fixtures because neither has a level rule reading an
% arrival-driven rel directly without an edge rule in between. A program
% with zero edge rules (demand_laziness_effect_rows) has carry_pending fixed
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
    findall(HeadRef, member(edgestmt(HeadRef, _, _, _, _, _, _, _, _), EdgeStatements), HeadRefs0),
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

snapshot_retention_fn_lines([], []) :- !.
snapshot_retention_fn_lines(_RetentionStatements,
    [ 'function apply_snapshot_retention(seam: ISqlSeam): Observable<void> {',
      '  const statements: SqlStatement[] = INCREMENTAL_RETENTION_STATEMENTS.map((statement) => ({ sql: statement.delete_sql, args: [] }));',
      '  return seam.runner.batch(seam.db, statements).pipe(map(() => undefined));',
      '}'
    ]).

run_ordered_tick_fn_lines(false, _, _, _, _, _, _, []) :- !.
run_ordered_tick_fn_lines(true, Name, HasRetention, UsesTick, DepartureRefs,
                          HasStructTypes, HasTextIntern, Lines) :-
    snapshot_departure_stage_lines(DepartureRefs, DepartureStageLines),
    snapshot_advance_tick_line(UsesTick, AdvanceTickLines),
    snapshot_text_intern_lines(HasTextIntern, TextInternLines),
    snapshot_reference_normalize_lines(HasStructTypes, HasTextIntern, NormalizeLines),
    retention_tick_lines_ordered(HasRetention, RetentionLines),
    tick_head_read_line(HasTextIntern, HeadReadLine),
    tick_stored_before(HasTextIntern, StoredBefore),
    ordered_mid_read_line(HasTextIntern, MidReadLine),
    ordered_after_read_lines(HasTextIntern, AfterReadLines),
    format(atom(ProcessLine),
           '    concatMap(({ before, mid }) => process_ordered_occurrences(seam, ~w, mid, arrivals).pipe(map((written) => ({ before, mid, written })))),',
           [StoredBefore]),
    format(atom(NameCommentLine),
           '  // ~w: ordered process_occurrences with evolving pre snapshots.',
           [Name]),
    append(
    [ [ 'function run_ordered_tick(seam: ISqlSeam, arrivals: IArrivalBatch): Observable<ITickDeltas> {',
        HeadReadLine
      ],
      AdvanceTickLines,
      TextInternLines,
      NormalizeLines,
      [ '    concatMap((before) => apply_arrivals(seam, arrivals).pipe(map(() => before))),',
        '    concatMap((before) => snapshot_ordered_pre(seam).pipe(map(() => before))),',
        '    concatMap((before) => recompute_levels(seam).pipe(map(() => before))),',
        MidReadLine,
        ProcessLine,
        '    concatMap(({ before, mid, written }) => recompute_levels(seam).pipe(map(() => ({ before, mid, written })))),',
        %% rxjs pipe() typed overloads stop at 9 operators; past that the chain
        %% collapses to Observable<unknown> (first hit when the golden reached
        %% 12 ordered-tick stages). Second .pipe() keeps every stage typed.
        '  ).pipe('
      ],
      RetentionLines,
      AfterReadLines,
      %% AfterReadLines has already mapped the chain to ITickDeltas, so the
      %% decode reads `state.rels` and not a `deltas` field no longer there.
      [ '    concatMap((state) => EnumPlane.decode_deltas(seam, ENUM_TYPES, ENUM_REF_COLUMNS, SUBSCRIBED_RELATIONS, state.rels).pipe(',
        '      map((rels): ITickDeltas => ({ ...state, rels })),',
        '    )), '
      ],
      DepartureStageLines,
      [ '  );',
        NameCommentLine,
        '}'
      ]
    ], Lines).

% The ordered tick carries the { before, mid, written } triple at this point;
% the bare `before` passthrough above collapses TypeScript's inference to
% Observable<unknown> (first exercised when the golden gained an ordered pre
% rule). Same stage, triple spelled out.
retention_tick_lines_ordered(true,
    ['    concatMap(({ before, mid, written }) => apply_snapshot_retention(seam).pipe(map(() => ({ before, mid, written })))),']).
retention_tick_lines_ordered(false, []).

% The referee's own end-of-tick departure staging. It reuses the RUNTIME's
% IncrementalRuntime.stage_departures over the deltas THIS path computed from
% its two snapshots -- the same table, filled from an independent source, so
% the two pipelines stay comparable while neither borrows the other's answer.
% Between read_boundary and promote_frontiers, on purpose: the source is the
% tick's NET boundary delta (engine.pl reads DepartureCarry off `Deltas`), and
% promote_frontiers is what then reports the staged rows as carry_pending.
departure_stage_incremental_lines([], []) :- !.
departure_stage_incremental_lines(DepartureRefs,
    ['    concatMap((rels) => IncrementalRuntime.stage_departures(seam, SUBSCRIBED_RELATIONS, rels).pipe(map(() => rels))),']) :-
    DepartureRefs \== [].

snapshot_departure_stage_lines([], []) :- !.
snapshot_departure_stage_lines(DepartureRefs,
    ['    concatMap((deltas) => IncrementalRuntime.stage_departures(seam, INCREMENTAL_RELATIONS, deltas.rels).pipe(map(() => deltas))),']) :-
    DepartureRefs \== [].

incremental_mode_lines(ReconcileEveryTick, [ReconcileLine]) :-
    format(atom(ReconcileLine), 'const RECONCILE_EVERY_TICK = ~w;',
           [ReconcileEveryTick]).

% ═══ subscribe-cone pruning (ladder step 2, DEFAULT OFF) ═════════════════════
% @comment-ok: pre-existing prune-contract doc; only the naive sentence changed.
%
% Read once, at module scope: the filters are pure and the emitted arrays never
% change, so a per-tick call would buy nothing. With the flag off every
% SUBSCRIBED_* const IS the array above it, by reference.
%
% incremental_plan stays UNPRUNED on purpose: it describes the compiled program
% (tests read statements out of it by rel name), where the consts below are the
% tick path's own working lists.
%
% Only the incremental path can honor a cone; the ordered path replays whole
% relations, so with the flag on it refuses by name instead.
subscribe_prune_lines(HasRetention, DerivedEdgeCarryRequired, HasOrderedProgram,
                      Lines) :-
    subscribe_prune_tick_path_line(DerivedEdgeCarryRequired, HasOrderedProgram,
                                   TickPathLine),
    ( HasRetention == true
    -> RetentionLine =
       ['const SUBSCRIBED_RETENTION_STATEMENTS = SubscribeCone.retention(SUBSCRIBE_PRUNE, INCREMENTAL_RETENTION_STATEMENTS, subscribed_rels, arrival_targets);']
    ;  RetentionLine = []
    ),
    append(
    [ [ 'const SUBSCRIBE_PRUNE = SubscribeCone.mode();',
        TickPathLine,
        'if (SUBSCRIBE_PRUNE === "on" && SUBSCRIBE_PRUNE_TICK_PATH !== "incremental") {',
        '  throw new Error(`subscribe_prune_unsupported_tick_path ${SUBSCRIBE_PRUNE_TICK_PATH}`);',
        '}',
        'const SUBSCRIBED_RELATIONS = SubscribeCone.relations(SUBSCRIBE_PRUNE, INCREMENTAL_RELATIONS, subscribed_rels, arrival_targets);',
        'const SUBSCRIBED_EDGE_STATEMENTS = SubscribeCone.edges(SUBSCRIBE_PRUNE, INCREMENTAL_EDGE_STATEMENTS, subscribed_rels);',
        'const SUBSCRIBED_LEVEL_STATEMENTS = SubscribeCone.levels(SUBSCRIBE_PRUNE, INCREMENTAL_LEVEL_STATEMENTS, subscribed_rels);'
      ],
      RetentionLine,
      [ 'const SUBSCRIBED_BOOT = SubscribeCone.boot(SUBSCRIBE_PRUNE, boot, subscribed_rels, arrival_targets);' ]
    ], Lines).

% Typed `string`, not left to inference: a literal-typed const makes the guard
% below a comparison tsgo reports as having no overlap (TS2367).
subscribe_prune_tick_path_line(_, true,
    'const SUBSCRIBE_PRUNE_TICK_PATH: string = "ordered";') :- !.
subscribe_prune_tick_path_line(_, _,
    'const SUBSCRIBE_PRUNE_TICK_PATH: string = "incremental";').

incremental_plan_export_lines(RetractionGuard, HasRetention, Lines) :-
    ( HasRetention == true
    -> RetentionLine = ['  retention: INCREMENTAL_RETENTION_STATEMENTS,']
    ; RetentionLine = []
    ),
    append(
    [ [ 'export const incremental_plan: IIncrementalProgramPlan = {',
      '  reconcile_every_tick: RECONCILE_EVERY_TICK,',
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
    format(atom(GuardLine), '  retraction_guard: "~w",', [RetractionGuard]).

incremental_carry_expr([], 'false') :- !.
incremental_carry_expr(EdgeStatements, Expr) :-
    findall(HeadName,
            ( member(edgestmt(HeadRef, _, _, _, _, _, _, _, _), EdgeStatements),
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
    [ 'function advance_tick(seam: ISqlSeam): Observable<void> {',
      '  return seam.runner.execute(seam.db, `UPDATE "__tick" SET "n" = "n" + 1`).pipe(map(() => undefined));',
      '}'
    ]).

advance_tick_pipeline_line(false, []) :- !.
advance_tick_pipeline_line(true,
    ['    concatMap(() => advance_tick(seam)),']).

snapshot_advance_tick_line(false, []) :- !.
snapshot_advance_tick_line(true,
    ['    concatMap((before) => advance_tick(seam).pipe(map(() => before))),']).

% TICK PHASE ALIGNMENT: the mid-tick level plane an edge body reads must be
% engine.pl's FROZEN MidLevel (`level_closure` over the store AFTER arrivals,
% BEFORE any edge write). apply_levels_before_edges only grows that plane;
% recompute_levels_before_edges runs the retracting half at the same point.
% Emitted ONLY for programs that have edge rules: with no edge rule nothing
% reads the plane mid-tick, the correction is unobservable, and those modules'
% text stays byte-identical to what the previous emitter wrote.
pre_edge_level_reconcile_lines([], [], []) :- !.
pre_edge_level_reconcile_lines(EdgeStatements,
    ['    concatMap(() => IncrementalRuntime.recompute_levels_before_edges(seam, SUBSCRIBED_LEVEL_STATEMENTS, SUBSCRIBED_RELATIONS, RECONCILE_EVERY_TICK, arrivals)),'],
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

% The intern arm is a TENTH operator on an edge-free chain, which degrades the
% whole pipe to Observable<unknown>; same split point, same operator sequence.
tick_pipe_split_lines([], true, ['  ).pipe(']) :- !.
tick_pipe_split_lines(EdgeSplitLines, _, EdgeSplitLines).

run_incremental_tick_fn_lines(EdgeStatements, DerivedEdgeCarryRequired,
                              HasRetention, UsesTick, DepartureRefs,
                              HasStructTypes, HasTextIntern, HasOrderedProgram,
                              Lines) :-
    advance_tick_pipeline_line(UsesTick, AdvanceTickLines),
    incremental_text_intern_lines(HasTextIntern, TextInternLines),
    incremental_reference_normalize_lines(HasStructTypes, HasTextIntern, NormalizeLines),
    departure_stage_incremental_lines(DepartureRefs, DepartureStageLines),
    pre_edge_level_reconcile_lines(EdgeStatements, PreEdgeReconcileLines, EdgeSplitLines),
    tick_pipe_split_lines(EdgeSplitLines, HasTextIntern, PipeSplitLines),
    ( EdgeStatements == []
    -> MergeLine = '    concatMap(() => of(undefined)),',
       PostEdgeLevelLine = '    concatMap(() => of(undefined)),'
    ;  MergeLine = '    concatMap(() => IncrementalRuntime.merge_next_into_current(seam, SUBSCRIBED_RELATIONS)),',
       PostEdgeLevelLine = '    concatMap(() => IncrementalRuntime.apply_levels_after_edges(seam, SUBSCRIBED_LEVEL_STATEMENTS, SUBSCRIBED_RELATIONS)),'
    ),
    RecomputeLine = '    concatMap(() => IncrementalRuntime.recompute_levels_after_edges(seam, SUBSCRIBED_LEVEL_STATEMENTS, SUBSCRIBED_RELATIONS, RECONCILE_EVERY_TICK)),',
    run_tick_dispatch_lines(DerivedEdgeCarryRequired, HasStructTypes,
                            HasOrderedProgram, DispatchLines),
    ( HasRetention == true
    -> RetentionLines =
       ['    concatMap(() => IncrementalRuntime.apply_retention(seam, SUBSCRIBED_RETENTION_STATEMENTS, SUBSCRIBED_RELATIONS)),']
    ; RetentionLines = []
    ),
    append(
    [ [ 'function run_incremental_tick(seam: ISqlSeam, arrivals: IArrivalBatch): Observable<ITickDeltas> {',
      '  return IncrementalRuntime.prepare_tick(seam, SUBSCRIBED_RELATIONS).pipe('
      ],
      AdvanceTickLines,
      TextInternLines,
      NormalizeLines,
      [ '    concatMap(() => IncrementalRuntime.apply_arrivals(seam, arrivals, SUBSCRIBED_RELATIONS)),',
      '    concatMap(() => IncrementalRuntime.apply_levels_before_edges(seam, SUBSCRIBED_LEVEL_STATEMENTS, SUBSCRIBED_RELATIONS)),'
      ],
      PreEdgeReconcileLines,
      [ '    concatMap(() => IncrementalRuntime.apply_edges(seam, SUBSCRIBED_EDGE_STATEMENTS, SUBSCRIBED_RELATIONS)),',
      MergeLine,
      PostEdgeLevelLine
      ],
      PipeSplitLines,
      RetentionLines,
      [
      RecomputeLine,
      '    concatMap(() => IncrementalRuntime.read_boundary(seam, SUBSCRIBED_RELATIONS)),'
      ],
      DepartureStageLines,
      [
      '    concatMap((rels) => IncrementalRuntime.promote_frontiers(seam, SUBSCRIBED_RELATIONS).pipe(',
      '      concatMap((carry_pending) => EnumPlane.decode_deltas(seam, ENUM_TYPES, ENUM_REF_COLUMNS, SUBSCRIBED_RELATIONS, rels).pipe(',
      '        map((decoded): ITickDeltas => ({ rels: decoded, carry_pending })),',
      '      )),',
      '    )),',
      '  );',
      '}',
      ''
      ],
      DispatchLines
    ], Lines).

run_tick_dispatch_lines(_, HasStructTypes, true,
    [ Signature,
      '  return EnumPlane.intern(seam, ENUM_TYPES, ENUM_REF_COLUMNS, arrivals).pipe(',
      '    concatMap((normalized) => run_ordered_tick(seam, validate_arrivals(normalized))),',
      '  );',
      '}'
    ]) :- dispatch_signature(HasStructTypes, Signature), !.
run_tick_dispatch_lines(_, HasStructTypes, false,
    [ Signature,
      '  return EnumPlane.intern(seam, ENUM_TYPES, ENUM_REF_COLUMNS, arrivals).pipe(',
      '    concatMap((normalized) => run_incremental_tick(seam, validate_arrivals(normalized))),',
      '  );',
      '}'
    ]) :- dispatch_signature(HasStructTypes, Signature).

dispatch_signature(_,
    'function run_tick(seam: ISqlSeam, arrivals: IArrivalBatch): Observable<ITickDeltas> {').

derived_edge_carry_required(
        plan(_, prog(_, Rules), _, _, _, _, _, _, _), EdgeStatements, Required) :-
    derived_refs(Rules, DerivedRefs),
    ( member(edgestmt(_, TriggerRef, _, _, _, _, _, _, _), EdgeStatements),
      memberchk(TriggerRef, DerivedRefs)
    -> Required = true
    ;  Required = false
    ).

reconcile_every_tick(plan(_, prog(_, Rules), _, _, _, _, _, _, _), Reconcile) :-
    ( member(Rule, Rules),
      Rule = (_ <- Body),
      body_ref_uses(Body, Uses),
      member(use(_, _, neg, _), Uses)
    -> Reconcile = true
    ;  Reconcile = false
    ).

% Any positive level cycle, direct or mutual: a plain count cannot tell which
% derivations a retraction killed, so the head is reseeded instead.
retraction_guard(plan(_, prog(_, Rules), _, _, _, _, _, _, _), Guard) :-
    recursive_level_refs(Rules, RecursiveRefs),
    ( RecursiveRefs == [] -> Guard = 'plain-count-acyclic'
    ; Guard = 'recursive-cte-reseed'
    ).

% `boot` is the ONE field the cone filter reaches from out here: the tick path
% takes its lists from the SUBSCRIBED_* consts, but boot is run by the harness
% off this object.
program_export_lines(Name, InternMode,
    [ 'export const program: IGenProgramWithBoot = {',
      NameLine,
      IrVersionLine,
      InternModeLine,
      '  ddl,',
      '  rel_columns,',
      '  rel_physical_names,',
      '  rel_column_types,',
      '  arrival_targets,',
      '  boot: SUBSCRIBED_BOOT,',
      '  final_select,',
      '  host_plans,',
      '  bind_plans,',
      '  query_plans,',
      '  subscribed_rels,',
      '  rel_catalog,',
      '  enum_types: ENUM_TYPES,',
      '  enum_ref_columns: ENUM_REF_COLUMNS,',
      '  unsupported_execution,',
      '  tick: run_tick,',
      '};'
    ]) :-
    format(atom(NameLine), '  name: "~w",', [Name]),
    ir_version(IrVersion),
    format(atom(IrVersionLine), '  ir_version: ~w,', [IrVersion]),
    format(atom(InternModeLine), '  internMode: "~w",', [InternMode]).

% A database built by one mode is unreadable by the other, so the artifact
% names the mode that built it (interning contract §15.5).
plan_intern_mode(plan(_, _, _, _, _, _, _, _, InternMode), InternMode).

% ═══ top level ═══════════════════════════════════════════════════════════════

emit_program(Name, Plan, Lowered, BootStatements, Text) :-
    Lowered = lowered(Name, Ddl, ArrivalStatements, EdgeStatements, LevelStatements, DeltaStatements, RelPlans, ArrivalTargets),
    header_lines(Name, HeaderLines),
    ( EdgeStatements == [] -> HasEdgeRules = false ; HasEdgeRules = true ),
    include(is_level_statement, LevelStatements, RuleLevelStatements),
    include(is_retention_statement, LevelStatements, RetentionStatements),
    ( RetentionStatements == [] -> HasRetention = false ; HasRetention = true ),
    Plan = plan(_, prog(PlanDecls, _), LoweringTypes, _, _, _, _, _, _),
    struct_type_plans(PlanDecls, LoweringTypes, RelPlans, StructPlans),
    struct_plane_lines(StructPlans, RelPlans, StructPlaneLines, HasStructTypes),
    enum_type_plans(PlanDecls, RelPlans, DeltaStatements, EnumPlans),
    enum_ref_columns_map(PlanDecls, RelPlans, EnumRefColumns),
    enum_plane_lines(EnumPlans, EnumRefColumns, EnumPlaneLines, _HasEnumTypes),
    plan_intern_mode(Plan, InternMode),
    program_text_intern_plan(InternMode, RelPlans, TextInternPlan),
    text_intern_plan_lines(TextInternPlan, TextInternPlanLines, HasTextIntern),
    ( ordered_program(EdgeStatements) -> HasOrderedProgram = true
    ; HasOrderedProgram = false
    ),
    Plan = plan(_, prog(_, SelfRefScanRules), _, _, _, _, _, _, _),
    recursive_level_refs(SelfRefScanRules, SelfReferentialLevelRefs),
    edge_statements_intern(EdgeStatements, ArmsInternWrite),
    ( HasOrderedProgram == true
    -> HasInternWrite = ArmsInternWrite
    ;  HasInternWrite = false
    ),
    imports_lines(HasEdgeRules, HasRetention, HasStructTypes, HasTextIntern,
                  HasOrderedProgram, SelfReferentialLevelRefs, HasInternWrite,
                  ImportLines),
    local_types_lines(Plan, LocalTypeLines),
    world_plan_lines(Plan, WorldPlanLines),
    bind_args_helper_lines(BindArgsHelperLines),
    arrival_value_guard_lines(ArrivalValueGuardLines),
    ( HasOrderedProgram == true, EdgeStatements \== []
    -> trigger_occurrences_helper_lines(TriggerOccurrencesHelperLines)
    ;  TriggerOccurrencesHelperLines = []
    ),
    Plan = plan(_, prog(_, PlanRules), _, _, _, _, _, _, _),
    listened_departure_refs(PlanRules, DepartureRefs),
    plan_pre_refs(Plan, PreRefs),
    findall(LevelRef,
            ( member((LevelHead <- _), PlanRules),
              functor(LevelHead, LevelName, LevelArity),
              LevelRef = LevelName/LevelArity ),
            LevelRefs0),
    sort(LevelRefs0, LevelHeadedRefs),
    enum_identity_ddls(PlanDecls, EnumIdentityDdls),
    append(Ddl, EnumIdentityDdls, FullDdl),
    ddl_lines(FullDdl, DdlLines),
    rel_columns_lines(RelPlans, RelColumnsLines),
    rel_physical_names_lines(RelPlans, RelPhysicalNamesLines),
    rel_column_types_lines(RelPlans, RelColumnTypesLines),
    rel_stored_column_types_lines(RelPlans, RelStoredColumnTypesLines),
    program_catalog_rows(InternMode, Name, PlanDecls, PlanRules, RelPlans,
                         DepartureRefs, PreRefs, LoweringTypes,
                         RuleLevelStatements, CatalogRows),
    rel_catalog_lines(CatalogRows, RelCatalogLines),
    rel_declared_column_types_lines(RelPlans, RelDeclaredColumnTypesLines),
    arrival_targets_lines(ArrivalTargets, ArrivalTargetsLines),
    boot_lines(BootStatements, BootLines),
    ( HasOrderedProgram == true
    -> snapshot_type_lines(RelPlans, SnapshotTypeLines),
       read_snapshot_fn_lines(DeltaStatements, ReadSnapshotFnLines),
       read_stored_snapshot_fn_lines(HasTextIntern, DeltaStatements,
                                     ReadStoredSnapshotFnLines, _)
    ;  SnapshotTypeLines = [],
       ReadSnapshotFnLines = [],
       ReadStoredSnapshotFnLines = []
    ),
    final_select_lines(DeltaStatements, FinalSelectLines),
    ( HasOrderedProgram == true
    -> arrival_statements_lines(ArrivalStatements, ArrivalStatementsLines),
       arrival_statement_fn_lines(Name, ArrivalStatementFnLines)
    ;  ArrivalStatementsLines = [],
       ArrivalStatementFnLines = []
    ),
    incremental_relation_lines(RelPlans, PlanRules, ArrivalStatements, DeltaStatements, DepartureRefs, IncrementalRelationLines),
    incremental_edge_statement_lines(Name, EdgeStatements, RelPlans, IncrementalEdgeStatementLines),
    cyclic_head_groups(SelfRefScanRules, CyclicHeadGroups),
    incremental_level_statement_lines(Name, RuleLevelStatements, RelPlans,
                                      CyclicHeadGroups,
                                      IncrementalLevelStatementLines),
    incremental_retention_statement_lines(RetentionStatements,
                                          IncrementalRetentionStatementLines),
    ordered_pre_lines(HasOrderedProgram, RelPlans, PreRefs, EdgeStatements,
                      OrderedPreLines),
    ordered_occurrence_lines(HasOrderedProgram, EdgeStatements, RelPlans,
                             PreRefs, LevelHeadedRefs,
                             OrderedOccurrenceLines),
    ordered_carry_lines(HasOrderedProgram, EdgeStatements, LevelHeadedRefs,
                        OrderedCarryLines),
    ( HasOrderedProgram == true
    -> recompute_levels_fn_lines(RelPlans, SelfReferentialLevelRefs, RuleLevelStatements,
                                 RecomputeLevelsFnLines),
       snapshot_retention_fn_lines(RetentionStatements, SnapshotRetentionFnLines),
       build_deltas_fn_lines(RelPlans, EdgeStatements, RetentionStatements,
                             DepartureRefs, BuildDeltasFnLines)
    ;  RecomputeLevelsFnLines = [],
       SnapshotRetentionFnLines = [],
       BuildDeltasFnLines = []
    ),
    Plan = plan(_, TickProg, _, _, _, _, _, _, _),
    program_uses_tick(TickProg, UsesTick),
    advance_tick_fn_lines(UsesTick, AdvanceTickFnLines),
    run_ordered_tick_fn_lines(HasOrderedProgram, Name, HasRetention, UsesTick,
                              DepartureRefs, HasStructTypes, HasTextIntern,
                              RunOrderedTickFnLines),
    reconcile_every_tick(Plan, ReconcileEveryTick),
    derived_edge_carry_required(Plan, EdgeStatements, DerivedEdgeCarryRequired),
    retraction_guard(Plan, RetractionGuard),
    incremental_mode_lines(ReconcileEveryTick, IncrementalModeLines),
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
      StructPlaneLines, EnumPlaneLines, TextInternPlanLines,
      DdlLines, RelColumnsLines, RelPhysicalNamesLines, RelColumnTypesLines, RelStoredColumnTypesLines, RelCatalogLines,
      RelDeclaredColumnTypesLines, ArrivalTargetsLines,
      BootLines, SnapshotTypeLines, ReadSnapshotFnLines,
      ReadStoredSnapshotFnLines, FinalSelectLines,
      ArrivalStatementsLines, ArrivalStatementFnLines,
      IncrementalRelationLines, IncrementalEdgeStatementLines,
      IncrementalLevelStatementLines, IncrementalRetentionStatementLines,
      OrderedPreLines, OrderedOccurrenceLines, OrderedCarryLines,
      RecomputeLevelsFnLines, SnapshotRetentionFnLines, BuildDeltasFnLines,
      AdvanceTickFnLines, RunOrderedTickFnLines,
      IncrementalModeLines, SubscribePruneLines, RunIncrementalTickFnLines,
      StructTickWrapperLines, IncrementalPlanExportLines,
      ProgramExportLines
    ],
    exclude(==([]), Sections0, Sections),
    maplist(lines_block, Sections, SectionTexts),
    atomic_list_concat(SectionTexts, '\n\n', Body),
    format(atom(Text), '~w\n', [Body]).

is_level_statement(levelstmt(_, _, _, _, _, _, _)).
is_retention_statement(retentionstmt(_, _, _)).
