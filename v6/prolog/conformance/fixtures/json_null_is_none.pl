% json_null_is_none.pl : the decision (2026-08-11) that JSON null IS the atom
% `none` from optional. Two receipts, one per defect the decision closes:
%
% defect 1, the round-trip break: canonical_json_text/2 (compiler write side)
% and value_json/2 (oracle write side) used to render the atom `none` as the
% FOUR-CHARACTER STRING "none", so a stored `{"a":null}` came back as
% `{"a":"none"}`. Both write sides now render the atom `none` as the bare
% JSON literal null, so a document holding a null round-trips byte-identical.
%
% defect 2, the patch refusal: json_patch/2 used to throw
% json_patch_null_unruled when a patch carried the json-null stand-in. The
% decision gives `none` a meaning, so the refusal is gone and json_patch
% composes. The runtime lowers to SQLite json_patch/2, whose merge-patch
% clause for a null VALUE is RFC 7396 (the key is removed); the oracle
% json_merge_patch/3 mirrors that clause here (body.pl json_merge_patch_pair).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% A stored json document holds a json null at `a`; an unrelated patch key
% leaves it intact. The read-back term keeps the value as the atom `none`
% (defect 1), which both write sides now render as the literal null.
fixture(json_null_round_trips_byte_identical,
  prog([ col_type(metric_sample/2, session, text),
         col_type(metric_sample/2, patch, json),
         kind(metric_sample/2, log), keep(metric_sample/2, all),
         col_type(metric_doc/2, session, text),
         col_type(metric_doc/2, snapshot, json),
         keyed(metric_doc/2, [1]) ],
       [ (metric_doc(SessionId, Next) <+ metric_sample(SessionId, Patch),
                                          pre(metric_doc(SessionId, Prior)),
                                          Next := json_patch(Prior, Patch)) ]),
  [ metric_doc(alpha, {a: none, b: 1}) ],
  [ [ +metric_sample(alpha, {c: 2}) ] ],
  [ final(metric_doc/2, [ metric_doc(alpha, obj([a-none, b-1, c-2])) ]) ]).

% defect 2: a patch key set to the json-null stand-in `none` composes instead
% of throwing. Merge-patch treats a null value as removal of that key (RFC
% 7396), which is what SQLite json_patch/2 emits, so the result is the object
% with the key gone.
fixture(json_patch_null_value_composes,
  prog([ col_type(metric_sample/2, session, text),
         col_type(metric_sample/2, patch, json),
         kind(metric_sample/2, log), keep(metric_sample/2, all),
         col_type(metric_doc/2, session, text),
         col_type(metric_doc/2, snapshot, json),
         keyed(metric_doc/2, [1]) ],
       [ (metric_doc(SessionId, Next) <+ metric_sample(SessionId, Patch),
                                          pre(metric_doc(SessionId, Prior)),
                                          Next := json_patch(Prior, Patch)) ]),
  [ metric_doc(alpha, {a: 1, b: 2}) ],
  [ [ +metric_sample(alpha, {a: none}) ] ],
  [ final(metric_doc/2, [ metric_doc(alpha, obj([b-2])) ]) ]).
