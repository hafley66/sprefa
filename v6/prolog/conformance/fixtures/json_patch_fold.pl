% json_patch_fold.pl : RFC 7396 JSON Merge Patch as the streaming `scan`
% operator over a keyed edge head, candidate B of
% plans/2026-08-09-scan-into-json-research.md.
%
% FAIL-FIRST RECEIPT (json-as-value-in-scan arc, base 26f3f25f): with no
% json_patch row in registry.pl every fixture below was WRONG, not stopped.
% The oracle left json_patch(Prior, Patch) unevaluated as a compound term and
% compile_expr's generic compound arm wrapped the same call in the json1
% tagged-term encoding, so the store held
% {"fn":"json_patch","args":[...]} against the oracle's own term text.
%
% RFC 7396 clause per fixture is named in each header.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% RFC 7396 §2, `Target[Name] = MergePatch(Target[Name], Value)`: a key the
% patch does not mention survives, a key it mentions is replaced.
fixture(json_patch_fold_merges_arrival_documents,
  prog([ col_type(metric_sample/2, session, text),
         col_type(metric_sample/2, patch, json),
         kind(metric_sample/2, log), keep(metric_sample/2, all),
         col_type(metric_doc/2, session, text),
         col_type(metric_doc/2, snapshot, json),
         keyed(metric_doc/2, [1]) ],
       [ (metric_doc(SessionId, Next) <+ metric_sample(SessionId, Patch),
                                         pre(metric_doc(SessionId, Prior)),
                                         Next := json_patch(Prior, Patch)) ]),
  [ metric_doc(alpha, {cpu: 1}) ],
  [ [ +metric_sample(alpha, {mem: 2}) ],
    [ +metric_sample(alpha, {cpu: 9}) ] ],
  [ final(metric_doc/2, [ metric_doc(alpha, obj([cpu-9, mem-2])) ]),
    deltas(metric_doc/2,
           [ [ -metric_doc(alpha, {cpu: 1}),
               +metric_doc(alpha, obj([cpu-1, mem-2])) ],
             [ -metric_doc(alpha, obj([cpu-1, mem-2])),
               +metric_doc(alpha, obj([cpu-9, mem-2])) ],
             [] ]) ]).

% The stored text json_patch returns is NOT key-sorted (measured on the pinned
% @libsql/client 3.45.1: json_patch('{"b":1}','{"a":2}') -> {"b":1,"a":2}),
% while the oracle keysorts. This fixture is the one that grades that gap.
fixture(json_patch_fold_result_is_key_sorted,
  prog([ col_type(metric_sample/2, session, text),
         col_type(metric_sample/2, patch, json),
         kind(metric_sample/2, log), keep(metric_sample/2, all),
         col_type(metric_doc/2, session, text),
         col_type(metric_doc/2, snapshot, json),
         keyed(metric_doc/2, [1]) ],
       [ (metric_doc(SessionId, Next) <+ metric_sample(SessionId, Patch),
                                         pre(metric_doc(SessionId, Prior)),
                                         Next := json_patch(Prior, Patch)) ]),
  [ metric_doc(alpha, {zeta: 1}) ],
  [ [ +metric_sample(alpha, {alpha_key: 2}) ] ],
  [ final(metric_doc/2, [ metric_doc(alpha, obj([alpha_key-2, zeta-1])) ]),
    deltas(metric_doc/2,
           [ [ -metric_doc(alpha, {zeta: 1}),
               +metric_doc(alpha, obj([alpha_key-2, zeta-1])) ],
             [] ]) ]).

% RFC 7396 §2, the recursive call: an object value merges into the object
% already at that key rather than replacing it.
fixture(json_patch_merges_nested_objects_recursively,
  prog([ col_type(metric_sample/2, session, text),
         col_type(metric_sample/2, patch, json),
         kind(metric_sample/2, log), keep(metric_sample/2, all),
         col_type(metric_doc/2, session, text),
         col_type(metric_doc/2, snapshot, json),
         keyed(metric_doc/2, [1]) ],
       [ (metric_doc(SessionId, Next) <+ metric_sample(SessionId, Patch),
                                         pre(metric_doc(SessionId, Prior)),
                                         Next := json_patch(Prior, Patch)) ]),
  [ metric_doc(alpha, {cpu: {user: 1, sys: 2}}) ],
  [ [ +metric_sample(alpha, {cpu: {sys: 9}}) ] ],
  [ final(metric_doc/2,
          [ metric_doc(alpha, obj([cpu-obj([sys-9, user-1])])) ]) ]).

% RFC 7396 §1, "if the patch is anything other than an object, the result will
% always be to replace the entire target": an ARRAY value replaces wholesale,
% element by element merging is not merge-patch.
fixture(json_patch_replaces_arrays_wholesale,
  prog([ col_type(metric_sample/2, session, text),
         col_type(metric_sample/2, patch, json),
         kind(metric_sample/2, log), keep(metric_sample/2, all),
         col_type(metric_doc/2, session, text),
         col_type(metric_doc/2, snapshot, json),
         keyed(metric_doc/2, [1]) ],
       [ (metric_doc(SessionId, Next) <+ metric_sample(SessionId, Patch),
                                         pre(metric_doc(SessionId, Prior)),
                                         Next := json_patch(Prior, Patch)) ]),
  [ metric_doc(alpha, {tags: [red, green]}) ],
  [ [ +metric_sample(alpha, {tags: [blue]}) ] ],
  [ final(metric_doc/2, [ metric_doc(alpha, obj([tags-[blue]])) ]) ]).

% RFC 7396 §2, `else: return Patch`: a patch that is not an object replaces
% the whole target, scalar or array alike.
fixture(json_patch_non_object_patch_replaces_the_document,
  prog([ col_type(metric_sample/2, session, text),
         col_type(metric_sample/2, patch, json),
         kind(metric_sample/2, log), keep(metric_sample/2, all),
         col_type(metric_doc/2, session, text),
         col_type(metric_doc/2, snapshot, json),
         keyed(metric_doc/2, [1]) ],
       [ (metric_doc(SessionId, Next) <+ metric_sample(SessionId, Patch),
                                         pre(metric_doc(SessionId, Prior)),
                                         Next := json_patch(Prior, Patch)) ]),
  [ metric_doc(alpha, {cpu: 1}) ],
  [ [ +metric_sample(alpha, [7, 8]) ] ],
  [ final(metric_doc/2, [ metric_doc(alpha, [7, 8]) ]) ]).

% RFC 7396 §2, `if Target is not an Object: Target = {}`: the contents of a
% non-object target are discarded, not merged into.
fixture(json_patch_non_object_target_becomes_empty,
  prog([ col_type(metric_sample/2, session, text),
         col_type(metric_sample/2, patch, json),
         kind(metric_sample/2, log), keep(metric_sample/2, all),
         col_type(metric_doc/2, session, text),
         col_type(metric_doc/2, snapshot, json),
         keyed(metric_doc/2, [1]) ],
       [ (metric_doc(SessionId, Next) <+ metric_sample(SessionId, Patch),
                                         pre(metric_doc(SessionId, Prior)),
                                         Next := json_patch(Prior, Patch)) ]),
  [ metric_doc(alpha, [7, 8]) ],
  [ [ +metric_sample(alpha, {cpu: 1}) ] ],
  [ final(metric_doc/2, [ metric_doc(alpha, obj([cpu-1])) ]) ]).

% RFC 7396 §1/§2, "Null values in the merge patch ... indicate the removal of
% existing values": the json-null stand-in `none` now has a meaning (the
% decision, 2026-08-11) so json_patch composes instead of throwing. A patch
% key set to `none` removes that key, which is what SQLite json_patch/2 emits.
fixture(json_patch_null_value_sets_key,
  prog([ col_type(metric_sample/2, session, text),
         col_type(metric_sample/2, patch, json),
         kind(metric_sample/2, log), keep(metric_sample/2, all),
         col_type(metric_doc/2, session, text),
         col_type(metric_doc/2, snapshot, json),
         keyed(metric_doc/2, [1]) ],
       [ (metric_doc(SessionId, Next) <+ metric_sample(SessionId, Patch),
                                          pre(metric_doc(SessionId, Prior)),
                                          Next := json_patch(Prior, Patch)) ]),
  [ metric_doc(alpha, {cpu: 1}) ],
  [ [ +metric_sample(alpha, {cpu: none}) ] ],
  [ final(metric_doc/2, [ metric_doc(alpha, obj([])) ]) ]).
