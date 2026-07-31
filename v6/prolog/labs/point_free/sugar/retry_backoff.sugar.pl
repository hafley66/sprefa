% corpus 6 sugar. M1 on the attempt counter.
%
%   attempt(Job, scan(Carried, 0, Carried + 1)) <+ failure(Job).
%   backoff(Job, Wait) <- attempt(Job, Tries), Wait := Tries * Tries.
%
% rx lowering:
%   failure$.pipe(groupBy(f => f.job), mergeMap(g => g.pipe(
%     scan(tries => tries + 1, 0),
%     map(tries => ({ job: g.key, wait: tries * tries })))))
sugar(prog(
  [ col_type(failure/1, job, text),
    col_type(attempt/2, job, text),
    col_type(attempt/2, tries, int),
    keyed(attempt/2, [1]),
    col_type(backoff/2, job, text),
    col_type(backoff/2, wait, int)
  ],
  [ (attempt(Job, scan(Carried, 0, Carried + 1)) <+ failure(Job)),
    (backoff(Job, Wait) <- attempt(Job, Tries), Wait := Tries * Tries)
  ])).
