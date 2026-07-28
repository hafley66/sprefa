% lowering.pl : what the shorthand becomes, as DATA. DDL text, match-path
% join SQL, and the three candidate decl spellings with their expansions.
%
% Nothing here executes SQL; the strings are the deliverable (lab protocol:
% sqlite/rx lowering is described, not mocked). Text shape follows the real
% emitter, v6/prolog/compile/lower.pl:304-333 (quoted identifiers, NOT NULL
% columns, WITHOUT ROWID primary keys), so a later emitter change is a visible
% diff against these strings rather than a silent divergence.

:- module(lowering,
          [ table_ddl/2, all_ddl/1, elem_ddl/1,
            match_path_sql/3, inline_json_sql/3,
            spelling/2, spelling_text/2, spelling_constructs/2,
            spelling_tables/2, normalized_tables/1,
            policy_bundle/2, resolve_policy/2,
            policy_ddl/3,
            coexistence_spelling/2, coexistence_text/2,
            coexistence_assignments/2, coexistence_policy_tokens/2,
            ref_mode/3, policy_match_path_sql/4 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(schema).

% Both policy bundles have four named positions. A declaration resolves only
% when it carries a policy word. There is deliberately no decl(Name) clause.
policy_bundle(value,
              policy(content_hash, immutable, support_zero, set)).
policy_bundle(entity,
              policy(extrinsic_id, mutable_history, explicit_retire, keyed)).

resolve_policy(decl(_, Policy), Bundle) :-
    policy_bundle(Policy, Bundle).

% Deep value refs close over immutable values. An entity id inside a value is
% an identity scalar: matching may follow it, while value rendering stops at
% the id. This keeps a mutable entity from changing an already interned value.
ref_mode(value, value, deep).
ref_mode(entity, value, deep).
ref_mode(entity, entity, deep).
ref_mode(value, entity, identity).

% Physical tables differ by policy. Value rows carry a unique semantic hash
% beside their dense integer key. Entity rows keep current state plus history.
policy_ddl(value, Type, [CurrentDdl, ContentDdl]) :-
    struct_type(Type, Fields),
    maplist(column_def(Type), Fields, FieldDefs),
    atomic_list_concat(FieldDefs, ', ', FieldSql),
    format(atom(CurrentDdl),
           'CREATE TABLE "~w_value" ("id" INTEGER NOT NULL, "semantic" TEXT NOT NULL UNIQUE, ~w, PRIMARY KEY ("id")) WITHOUT ROWID',
           [Type, FieldSql]),
    maplist(quoted, Fields, QuotedFields),
    atomic_list_concat(QuotedFields, ', ', UniqueSql),
    format(atom(ContentDdl),
           'CREATE UNIQUE INDEX "~w_value_content" ON "~w_value" (~w)',
           [Type, Type, UniqueSql]).
policy_ddl(entity, Type, [CurrentDdl, HistoryDdl]) :-
    struct_type(Type, Fields),
    maplist(column_def(Type), Fields, FieldDefs),
    atomic_list_concat(FieldDefs, ', ', FieldSql),
    format(atom(CurrentDdl),
           'CREATE TABLE "~w_entity" ("id" INTEGER NOT NULL, ~w, PRIMARY KEY ("id")) WITHOUT ROWID',
           [Type, FieldSql]),
    format(atom(HistoryDdl),
           'CREATE TABLE "~w_entity_history" ("id" INTEGER NOT NULL, "tick" INTEGER NOT NULL, ~w, PRIMARY KEY ("id", "tick")) WITHOUT ROWID',
           [Type, FieldSql]).

% ═══ DDL ════════════════════════════════════════════════════════════════════
% Every value table is (id, content columns...) with the id as PRIMARY KEY and
% the content columns UNIQUE: the intern table IS a keyed set rel, and the
% UNIQUE index is what "same content = same row" means in SQL terms.

table_ddl(Type, [TableDdl, UniqueDdl]) :-
    struct_type(Type, Fields),
    maplist(column_def(Type), Fields, FieldDefs),
    atomic_list_concat(FieldDefs, ', ', FieldSql),
    format(atom(TableDdl),
           'CREATE TABLE "~w" ("id" INTEGER NOT NULL, ~w, PRIMARY KEY ("id")) WITHOUT ROWID',
           [Type, FieldSql]),
    maplist(quoted, Fields, QuotedFields),
    atomic_list_concat(QuotedFields, ', ', UniqueSql),
    format(atom(UniqueDdl),
           'CREATE UNIQUE INDEX "~w_content" ON "~w" (~w)',
           [Type, Type, UniqueSql]).

column_def(Type, Field, Def) :-
    field_spec(Type, Field, Spec),
    sql_type(Spec, SqlType),
    format(atom(Def), '"~w" ~w NOT NULL', [Field, SqlType]).

sql_type(text, 'TEXT').
sql_type(int, 'INTEGER').
sql_type(struct(_), 'INTEGER').
sql_type(enum(_), 'INTEGER').
sql_type(list(_), 'INTEGER').

quoted(Name, Quoted) :- format(atom(Quoted), '"~w"', [Name]).

all_ddl(Ddl) :-
    findall(Type, struct_type(Type, _), Types),
    findall(Line, ( member(Type, Types), table_ddl(Type, Lines), member(Line, Lines) ), Ddl).

% The edge table for the indexed list modelling. Q4's who-points-to-who table
% is an ordinary rel: three columns, a two-column key, nothing special.
elem_ddl([Table, Index]) :-
    Table = 'CREATE TABLE "list_elem" ("list" INTEGER NOT NULL, "index" INTEGER NOT NULL, "item" INTEGER NOT NULL, PRIMARY KEY ("list", "index")) WITHOUT ROWID',
    Index = 'CREATE INDEX "list_elem_item" ON "list_elem" ("item")'.

% ═══ match-path lowering ════════════════════════════════════════════════════
% match_path_sql(RootType, Steps, Sql). A step is Field (struct/scalar hop) or
% Field-Tag (enum hop, naming the variant table). Depth = number of hops.

match_path_sql(RootType, Steps, Sql) :-
    format(atom(From), '"~w" r0', [RootType]),
    walk_steps(RootType, r0, 0, Steps, [], Joins, Select),
    ( Joins == [] -> FromSql = From
    ; atomic_list_concat(Joins, ' ', JoinSql),
      format(atom(FromSql), '~w ~w', [From, JoinSql]) ),
    format(atom(Sql), 'SELECT ~w FROM ~w WHERE r0."id" = ?', [Select, FromSql]).

policy_match_path_sql(Policy, RootType, Steps, Sql) :-
    policy_bundle(Policy, _),
    match_path_sql(RootType, Steps, Sql).

walk_steps(Type, Alias, _, [Field], Joins, Joins, Select) :-
    atom(Field),
    field_spec(Type, Field, Spec),
    memberchk(Spec, [text, int]), !,
    format(atom(Select), '~w."~w"', [Alias, Field]).
walk_steps(Type, Alias, Depth, [Step | Rest], Joins0, Joins, Select) :-
    step_target(Type, Step, Field, NextType),
    NextDepth is Depth + 1,
    atom_concat(r, NextDepth, NextAlias),
    format(atom(Join), 'JOIN "~w" ~w ON ~w."id" = ~w."~w"',
           [NextType, NextAlias, NextAlias, Alias, Field]),
    append(Joins0, [Join], Joins1),
    walk_steps(NextType, NextAlias, NextDepth, Rest, Joins1, Joins, Select).

step_target(Type, Field-Tag, Field, VariantTable) :- !,
    field_spec(Type, Field, enum(Enum)),
    variant_of(Enum, Tag, VariantTable, _).
step_target(Type, Field, Field, NextType) :-
    field_spec(Type, Field, struct(NextType)).

% The inline-json1 counterpart, for the depth-cost table. This is what the
% CURRENT compound-column punt would have to emit (compound values stored as
% json1 text, lower.pl:262-276 builds them with json_object/json_array).
inline_json_sql(RootType, JsonPath, Sql) :-
    format(atom(Sql),
           'SELECT json_extract(r0."body", \'~w\') FROM "~w" r0 WHERE r0."id" = ?',
           [JsonPath, RootType]).

% ═══ the three candidate decl spellings ═════════════════════════════════════
% Same worked example in each: the route tree with an enum body, plus the
% shared view. spelling_tables/2 is the hand-written expansion each surface
% claims to denote; the entry file grades that all three expand to the SAME
% table set, which is what "shorthand" has to mean.

spelling(a, 'json braces').
spelling(b, 'prolog functors').
spelling(c, 'sql rels, no new construct').

spelling_text(a,
'rel route { path: text, body: body, children: [route] } value.
enum body { page { view: view }, redirect { to: text } }
rel view { title: text, tags: [text] } value.').

spelling_text(b,
'rel route(path: text, body: body, children: list(route)) value.
rel body(page(view: view) ; redirect(to: text)) value.
rel view(title: text, tags: list(text)) value.').

spelling_text(c,
'rel route(id, path, body, children) set key(2, 3, 4).
rel body_page(id, view) set key(2).
rel body_redirect(id, to) set key(2).
rel view(id, title, tags) set key(2, 3).
rel cons(id, head, tail) set key(2, 3).
rel nil(id) set key(1).').

% What each spelling asks the reader to hold in their head, beyond the rel
% decl that already exists in v6/prolog/compile/SYNTAX.md.
spelling_constructs(a, [brace_field_block, bracket_list_type, enum_keyword,
                        variant_brace_block, value_policy_word]).
spelling_constructs(b, [named_column_types, list_type_functor, variant_functor_alternation,
                        value_policy_word]).
spelling_constructs(c, []).

% table(Name, Columns, key(Positions)). Each spelling's expansion is written
% out SEPARATELY, in the order that surface reads, and the entry file grades
% that the three msort to the same set. Writing one list and aliasing it three
% times would make that check vacuous.

% (a) reads top-down: route, then the enum's two variant blocks, then view.
%     `[route]` expands to the shared cons/nil pair.
spelling_tables(a,
    [ table(route,         [id, path, body, children], key([2, 3, 4])),
      table(body_page,     [id, view],                 key([2])),
      table(body_redirect, [id, to],                   key([2])),
      table(view,          [id, title, tags],          key([2, 3])),
      table(cons,          [id, head, tail],           key([2, 3])),
      table(nil,           [id],                       key([1])) ]).

% (b) reads the same three decls; the alternation in the body rel is the enum.
spelling_tables(b,
    [ table(route,         [id, path, body, children], key([2, 3, 4])),
      table(body_page,     [id, view],                 key([2])),
      table(body_redirect, [id, to],                   key([2])),
      table(view,          [id, title, tags],          key([2, 3])),
      table(cons,          [id, head, tail],           key([2, 3])),
      table(nil,           [id],                       key([1])) ]).

% (c) IS the expansion; its decl lines map one to one, in file order.
spelling_tables(c,
    [ table(route,         [id, path, body, children], key([2, 3, 4])),
      table(body_page,     [id, view],                 key([2])),
      table(body_redirect, [id, to],                   key([2])),
      table(view,          [id, title, tags],          key([2, 3])),
      table(cons,          [id, head, tail],           key([2, 3])),
      table(nil,           [id],                       key([1])) ]).

normalized_tables(Tables) :- spelling_tables(c, Tables0), msort(Tables0, Tables).

% ═══ explicit coexistence decompositions ═══════════════════════════════════
% The same route tree is assigned route=entity and every body/view value
% table=value. Each surface reaches that assignment without a default.

coexistence_spelling(decl_word, 'declaration word per type').
coexistence_spelling(use_site, 'body use-site word').
coexistence_spelling(hybrid, 'declaration plus use-site word').

coexistence_text(decl_word,
'rel route(id, path, body, children) entity.
rel body_page(id, view) value.
rel body_redirect(id, to) value.
rel view(id, title, tags) value.').

coexistence_text(use_site,
'rel route(id, path, body, children).
rel body_page(id, view).
rel body_redirect(id, to).
rel view(id, title, tags).
route(entity, Route, Path, Body, Children) <- route_wanted(Route, Path, Body, Children).
body_page(value, Body, View) <- page_wanted(Body, View).
body_redirect(value, Body, To) <- redirect_wanted(Body, To).
view(value, View, Title, Tags) <- view_wanted(View, Title, Tags).').

coexistence_text(hybrid,
'rel route(id, path, body, children) policy(entity, value).
rel body_page(id, view) value.
rel body_redirect(id, to) value.
rel view(id, title, tags) value.
route(entity, Route, Path, Body, Children) <- route_wanted(Route, Path, Body, Children).').

coexistence_assignments(decl_word,
    [route-entity, body_page-value, body_redirect-value, view-value]).
coexistence_assignments(use_site,
    [route-entity, body_page-value, body_redirect-value, view-value]).
coexistence_assignments(hybrid,
    [route-entity, body_page-value, body_redirect-value, view-value]).

% Count of explicit value/entity choice words in each shown source.
coexistence_policy_tokens(decl_word, 4).
coexistence_policy_tokens(use_site, 4).
coexistence_policy_tokens(hybrid, 6).
