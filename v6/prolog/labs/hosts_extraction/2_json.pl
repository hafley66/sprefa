:- module(hosts_extraction_json,
          [ json_rx_lowering/2,
            json_residue/1
          ]).

json_rx_lowering(
    field_pull,
    "currentBody$.pipe(map(({ep, body}) => ({ep, n: body.stargazers_count})))").
json_rx_lowering(
    array_explode,
    "currentBody$.pipe(mergeMap(({ep, body}) => from(body).pipe(map((item) => ({ep, num: item.number, title: item.title, state: item.state, author: item.user.login})))))").

% The conformance engine consumes canonical JSON values. A shell host returns
% text, so the text-to-value decoder remains a separately named seam.
json_residue(slot_json_text_to_value).
