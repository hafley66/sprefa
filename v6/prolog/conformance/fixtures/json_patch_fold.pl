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


% ONE program, nine sessions, one keyed json fold. metric_doc/2 is keyed on
% session, so each session's tick is the scenario its folded fixture ran alone
% and no session sees another's rows.
%   alpha    RFC 7396 s2, `Target[Name] = MergePatch(Target[Name], Value)`: a
%            key the patch does not mention survives, a key it mentions is
%            replaced. Two ticks.
%   beta     the stored text json_patch returns is NOT key-sorted (measured on
%            the pinned @libsql/client 3.45.1:
%            json_patch('{"b":1}','{"a":2}') -> {"b":1,"a":2}) while the oracle
%            keysorts. This session grades that gap.
%   gamma    RFC 7396 s2, the recursive call: an object value merges into the
%            object already at that key rather than replacing it.
%   delta    RFC 7396 s1, "if the patch is anything other than an object, the
%            result will always be to replace the entire target": an ARRAY
%            value replaces wholesale, element-by-element merging is not
%            merge-patch.
%   epsilon  RFC 7396 s2, `else: return Patch`: a patch that is not an object
%            replaces the whole target, scalar or array alike.
%   zeta     RFC 7396 s2, `if Target is not an Object: Target = {}`: the
%            contents of a non-object target are discarded, not merged into.
%   eta      RFC 7396 s1/s2, "Null values in the merge patch ... indicate the
%            removal of existing values": the json-null stand-in `none` has a
%            meaning (the decision, 2026-08-11) so json_patch composes instead
%            of throwing, and a patch key set to `none` removes that key.
%   theta    defect 1 of that same decision, the round-trip break:
%            canonical_json_text/2 (compiler write side) and value_json/2
%            (oracle write side) used to render the atom `none` as the
%            FOUR-CHARACTER STRING "none", so a stored {"a":null} came back as
%            {"a":"none"}. A stored null now round-trips byte-identical and an
%            unrelated patch key leaves it intact.
%   iota     defect 2: json_patch/2 used to throw json_patch_null_unruled when
%            a patch carried the stand-in. The runtime lowers to SQLite
%            json_patch/2, whose merge-patch clause for a null VALUE removes
%            the key; the oracle json_merge_patch/3 mirrors it
%            (body.pl json_merge_patch_pair).
% folded 2026-08-20 from json_patch_fold_merges_arrival_documents,
% json_patch_fold_result_is_key_sorted,
% json_patch_merges_nested_objects_recursively,
% json_patch_replaces_arrays_wholesale,
% json_patch_non_object_patch_replaces_the_document,
% json_patch_non_object_target_becomes_empty, json_patch_null_value_sets_key,
% json_null_round_trips_byte_identical (json_null_is_none.pl),
% json_patch_null_value_composes (json_null_is_none.pl).
fixture(json_patch_fold_rfc7396_clauses,
  prog([ col_type(metric_sample/2, session, text),
         col_type(metric_sample/2, patch, json),
         kind(metric_sample/2, log), keep(metric_sample/2, all),
         col_type(metric_doc/2, session, text),
         col_type(metric_doc/2, snapshot, json),
         keyed(metric_doc/2, [1]) ],
       [ (metric_doc(SessionId, Next) <+ metric_sample(SessionId, Patch),
                                         pre(metric_doc(SessionId, Prior)),
                                         Next := json_patch(Prior, Patch)) ]),
  [ metric_doc(alpha,   {cpu: 1}),
    metric_doc(beta,    {zeta: 1}),
    metric_doc(gamma,   {cpu: {user: 1, sys: 2}}),
    metric_doc(delta,   {tags: [red, green]}),
    metric_doc(epsilon, {cpu: 1}),
    metric_doc(zeta,    [7, 8]),
    metric_doc(eta,     {cpu: 1}),
    metric_doc(theta,   {a: none, b: 1}),
    metric_doc(iota,    {a: 1, b: 2}) ],
  [ [ +metric_sample(alpha,   {mem: 2}) ],
    [ +metric_sample(alpha,   {cpu: 9}) ],
    [ +metric_sample(beta,    {alpha_key: 2}) ],
    [ +metric_sample(gamma,   {cpu: {sys: 9}}) ],
    [ +metric_sample(delta,   {tags: [blue]}) ],
    [ +metric_sample(epsilon, [7, 8]) ],
    [ +metric_sample(zeta,    {cpu: 1}) ],
    [ +metric_sample(eta,     {cpu: none}) ],
    [ +metric_sample(theta,   {c: 2}) ],
    [ +metric_sample(iota,    {a: none}) ] ],
  [ final(metric_doc/2,
          [ metric_doc(alpha,   obj([cpu-9, mem-2])),
            metric_doc(beta,    obj([alpha_key-2, zeta-1])),
            metric_doc(delta,   obj([tags-[blue]])),
            metric_doc(epsilon, [7, 8]),
            metric_doc(eta,     obj([])),
            metric_doc(gamma,   obj([cpu-obj([sys-9, user-1])])),
            metric_doc(iota,    obj([b-2])),
            metric_doc(theta,   obj([a-none, b-1, c-2])),
            metric_doc(zeta,    obj([cpu-1])) ]),
    deltas(metric_doc/2,
           [ [ -metric_doc(alpha, {cpu: 1}),
               +metric_doc(alpha, obj([cpu-1, mem-2])) ],
             [ -metric_doc(alpha, obj([cpu-1, mem-2])),
               +metric_doc(alpha, obj([cpu-9, mem-2])) ],
             [ -metric_doc(beta, {zeta: 1}),
               +metric_doc(beta, obj([alpha_key-2, zeta-1])) ],
             [ -metric_doc(gamma, {cpu: {user: 1, sys: 2}}),
               +metric_doc(gamma, obj([cpu-obj([sys-9, user-1])])) ],
             [ -metric_doc(delta, {tags: [red, green]}),
               +metric_doc(delta, obj([tags-[blue]])) ],
             [ -metric_doc(epsilon, {cpu: 1}),
               +metric_doc(epsilon, [7, 8]) ],
             [ -metric_doc(zeta, [7, 8]),
               +metric_doc(zeta, obj([cpu-1])) ],
             [ -metric_doc(eta, {cpu: 1}),
               +metric_doc(eta, obj([])) ],
             [ -metric_doc(theta, {a: none, b: 1}),
               +metric_doc(theta, obj([a-none, b-1, c-2])) ],
             [ -metric_doc(iota, {a: 1, b: 2}),
               +metric_doc(iota, obj([b-2])) ],
             [] ]) ]).
