// knip: silent. rail: invisible to every bucket, because it defines nothing and
// `file_defs` is the gate on all three. It exists to put a ZERO-DEF hop between
// the entry and barrelTarget.ts, so reaching that file takes two import edges
// and no call name at any point.
export * from './barrelTarget.ts';
