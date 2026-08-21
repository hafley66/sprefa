:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% Template bounds live inside the parameter parens (ruling
% template_bound_spelling): `rel pair(T: json_encodable)(first: T, second: T).`
% A template mints declarations only (ruling generic_template_rules), so each
% fixture below carries the minted instance as an ordinary column type and
% grades the carrier rel's own arrivals.

fixture(bounded_template_ground_instance,
  prog([ interface_decl(json_encodable, []),
         rel_template([pair], [type_parameter('T', [json_encodable])],
                      [column(first, 'T'), column(second, 'T')]),
         col_type(edge/2, id, int),
         col_type(edge/2, endpoints, pair(int)),
         keyed(edge/2, [1]) ],
       [ (carry(Id, Endpoints) <- edge(Id, Endpoints)) ]),
  [],
  [[+edge(1, obj([first-10, second-20]))],
   [-edge(1, obj([first-10, second-20]))]],
  [ final(edge/2, []), ticks(2) ]).

% Two bounded parameters, one template, distinct arguments.
fixture(two_bounded_parameters_mint_one_instance,
  prog([ interface_decl(json_encodable, []),
         rel_template([span],
                      [type_parameter('Start', [json_encodable]),
                       type_parameter('Label', [json_encodable])],
                      [column(start, 'Start'), column(label, 'Label')]),
         col_type(marker/2, id, int),
         col_type(marker/2, extent, span(int, text)),
         keyed(marker/2, [1]) ],
       [ (carry(Id, Extent) <- marker(Id, Extent)) ]),
  [],
  [[+marker(1, obj([start-5, label-"head"]))],
   [-marker(1, obj([start-5, label-"head"]))]],
  [ final(marker/2, []), ticks(2) ]).

% Phase D of plans/2026-08-14-type-system-unity.md: an application whose
% argument is ITSELF a minted instance discharges the bound when the inner
% instance does. Checking the bound against the application SPELLING threw
% generic_bound_unsatisfied(wrap(int), json_encodable) here.
%
% The two levels use DIFFERENT templates on purpose. `pair(pair(int))` names
% the outer columns `first`/`second`, the same names the inner instance
% carries, and lower.pl:2752 dictionary_render_expr/3 leaves the outer column
% UNQUOTED inside the correlated subquery, so SQLite binds it to the child
% ref view instead. That defect is not about templates: two plain type_decls
% whose parent column names equal the child's reproduce it exactly.
fixture(nested_bounded_template_instance,
  prog([ interface_decl(json_encodable, []),
         rel_template([wrap], [type_parameter('T', [json_encodable])],
                      [column(value, 'T')]),
         rel_template([couple],
                      [type_parameter('Left', [json_encodable]),
                       type_parameter('Right', [json_encodable])],
                      [column(first, 'Left'), column(second, 'Right')]),
         col_type(index/2, id, int),
         col_type(index/2, nested, couple(wrap(int), wrap(text))),
         keyed(index/2, [1]) ],
       [ (carry(Id, Nested) <- index(Id, Nested)) ]),
  [],
  [[+index(1, obj([first-obj([value-1]), second-obj([value-"a"])]))],
   [-index(1, obj([first-obj([value-1]), second-obj([value-"a"])]))]],
  [ final(index/2, []), ticks(2) ]).

% A parameter with no bound sits beside a bounded one in the same parens.
fixture(mixed_bounded_and_free_parameters,
  prog([ interface_decl(json_encodable, []),
         rel_template([entry],
                      [type_parameter('Key', [json_encodable]),
                       type_parameter('Value', [])],
                      [column(key, 'Key'), column(value, 'Value')]),
         col_type(cell/2, id, int),
         col_type(cell/2, slot, entry(text, int)),
         keyed(cell/2, [1]) ],
       [ (carry(Id, Slot) <- cell(Id, Slot)) ]),
  [],
  [[+cell(1, obj([key-"alpha", value-7]))],
   [-cell(1, obj([key-"alpha", value-7]))]],
  [ final(cell/2, []), ticks(2) ]).

% An unsatisfied bound names the template, the application and the failing
% argument position, not just the leaf type.
fixture(unsatisfied_bound_names_the_path,
  prog([ interface_decl(addressable, []),
         rel_template([box], [type_parameter('T', [addressable])],
                      [column(value, 'T')]),
         col_type(file/2, id, int),
         col_type(file/2, path, text),
         keyed(file/2, [1]),
         col_type(holder/2, id, int),
         col_type(holder/2, boxed, box(file)),
         keyed(holder/2, [1]) ],
       [ (carry(Id, Boxed) <- holder(Id, Boxed)) ]),
  [],
  [[+holder(1, obj([value-1]))]],
  [ throws(unsupported_construct(
               generic_bound_unsatisfied(file, addressable,
                   path([template(box), application(box(file)),
                         argument(1, file)])))) ]).

% An interface name no `interface` declaration introduces is named before any
% instance is minted.
fixture(unknown_bound_interface_is_named,
  prog([ rel_template([box], [type_parameter('T', [missing_capability])],
                      [column(value, 'T')]),
         col_type(holder/2, id, int),
         col_type(holder/2, boxed, box(text)),
         keyed(holder/2, [1]) ],
       [ (carry(Id, Boxed) <- holder(Id, Boxed)) ]),
  [],
  [[+holder(1, obj([value-"a"]))]],
  [ throws(unsupported_construct(
               interface_unknown(missing_capability))) ]).

% Relation-arrow sugar on templates (parse_dl_dcg.pl rel_stmt generic arm)
% folds into an ordinary trailing `return` column before rel_template/3 is
% minted, so a template's Specs already carries it here. These two fixtures
% do not exercise the parser (fixture/5 is a pre-parsed AST, engine.pl:711);
% they pin that 0_generic_expand.pl and engine.pl treat that trailing column
% like any other, which is the contract the arrow fold depends on.
fixture(unbounded_template_arrow_return_column,
  prog([ rel_template([counter], [type_parameter('Node', [])],
                      [column(node, 'Node'), column(return, int)]),
         col_type(cursor/2, id, int),
         col_type(cursor/2, pos, counter(text)),
         keyed(cursor/2, [1]) ],
       [ (carry(Id, Pos) <- cursor(Id, Pos)) ]),
  [],
  [[+cursor(1, obj([node-"a", return-7]))],
   [-cursor(1, obj([node-"a", return-7]))]],
  [ final(cursor/2, []), ticks(2) ]).

fixture(bounded_template_arrow_return_column,
  prog([ interface_decl(json_encodable, []),
         rel_template([mapper],
                      [type_parameter('In', [json_encodable]),
                       type_parameter('Out', [json_encodable])],
                      [column(input, 'In'), column(return, 'Out')]),
         col_type(conversion/2, id, int),
         col_type(conversion/2, applied, mapper(int, text)),
         keyed(conversion/2, [1]) ],
       [ (carry(Id, Applied) <- conversion(Id, Applied)) ]),
  [],
  [[+conversion(1, obj([input-5, return-"ok"]))],
   [-conversion(1, obj([input-5, return-"ok"]))]],
  [ final(conversion/2, []), ticks(2) ]).
