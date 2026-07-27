% ts_reach_gen.pl : bench program + driver for the prolog->TypeScript compiler.
% Load AFTER books/v6/dl_to_ts.pl (its program/2 and driver/2 are replaced by
% these); run gen(bench_reach) from the repo root, then:
%   node books/v6/gen/bench_reach.ts <layers> <width>
% prints the harness CSV line (setup = full derive with both roots, retract =
% recompute with root 1 only; the generated engine is the NAIVE fixpoint the
% compiler emits, measured honestly).

program(bench_reach,
  [ ( reach(Node)  <= root(Node) )
  , ( reach(Child) <= edge(Parent, Child), reach(Parent) )
  , ( found(Node)  <~ reach(Node) )
  ]).

driver(bench_reach,
  [ 'const layers = Number(process.argv[2]);'
  , 'const width = Number(process.argv[3]);'
  , 'const edges: Fact[] = [];'
  , 'for (let w = 0; w < width; w++) {'
  , '  const id = 2 + w;'
  , '  edges.push(["edge", 0, id]);'
  , '  if (w % 3 === 0) edges.push(["edge", 1, id]);'
  , '}'
  , 'for (let l = 1; l < layers; l++) {'
  , '  for (let w = 0; w < width; w++) {'
  , '    const id = 2 + l * width + w;'
  , '    const prev = 2 + (l - 1) * width;'
  , '    edges.push(["edge", prev + w, id], ["edge", prev + (w + 1) % width, id]);'
  , '  }'
  , '}'
  , 'let t0 = performance.now();'
  , 'const before = tick([["root", 0], ["root", 1], ...edges]).effects.length;'
  , 'const setupMs = performance.now() - t0;'
  , 't0 = performance.now();'
  , 'const after = tick([["root", 1], ...edges]).effects.length;'
  , 'const retractMs = performance.now() - t0;'
  , 'const nodes = 2 + layers * width;'
  , 'console.log(`CSV,swi-ts,${nodes},${edges.length},${before - after},${setupMs.toFixed(3)},${retractMs.toFixed(3)}`);'
  ]).
