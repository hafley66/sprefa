// Test fixtures use keyed set updates. Duplicate derivations are represented by
// distinct edges and multiple roots, not repeated insertion of an existing key.
export function snapshot(roots, edges) {
  const alive = new Set(roots);
  const queue = [...roots];
  for (let cursor = 0; cursor < queue.length; cursor++) {
    for (const [parent, child] of edges.values()) {
      if (parent === queue[cursor] && !alive.has(child)) {
        alive.add(child);
        queue.push(child);
      }
    }
  }
  return {
    root: [...roots].sort((a,b) => a-b).map(n => [n]),
    edge: [...edges.values()].sort((a,b) => a[0]-b[0] || a[1]-b[1]),
    alive: [...alive].sort((a,b) => a-b).map(n => [n]),
  };
}

export function trial(name, nodes, dag, cascade, batches) {
  const roots = new Set();
  const edges = new Map();
  const ticks = batches.map(arrivals => {
    for (const {rel, sign, row} of arrivals) {
      const key = rel === "root" ? row[0] : row.join(",");
      const relation = rel === "root" ? roots : edges;
      if (relation.has(key) !== (sign === "del")) throw new Error(`${name}: invalid set update ${JSON.stringify({rel,sign,row})}`);
      if (sign === "del") relation.delete(key);
      else if (rel === "root") roots.add(key);
      else edges.set(key, row);
    }
    return {arrivals, expected: snapshot(roots, edges)};
  });
  return {name, nodes, dag, cascade, ticks};
}

const root = (sign, n) => ({rel:"root", sign, row:[n]});
const edge = (sign, p, c) => ({rel:"edge", sign, row:[p,c]});

export function sequenceFixtures() {
  const cases = [
    trial("two-isolated-roots",2,true,true,[[root("add",0),root("add",1)],[root("del",0)],[root("del",1)]]),
    trial("disconnected-zero-weight-node",2,true,true,[[root("add",0)],[root("del",0)]]),
    trial("diamond-multiple-support", 7, true, true, [
      [root("add",0), root("add",1), ...[[0,2],[0,3],[1,3],[2,4],[3,4],[4,5]].map(([p,c])=>edge("add",p,c))],
      [root("del",0)], [root("del",1)], [root("add",0)], [root("add",1)], [root("del",1)], [root("del",0)],
    ]),
    trial("cycle-final-external-root", 6, false, true, [
      [root("add",0),root("add",1), ...[[0,2],[1,2],[2,3],[3,2],[3,4]].map(([p,c])=>edge("add",p,c))],
      [root("del",0)], [root("del",1)], [root("add",0)], [root("del",0)],
    ]),
    trial("self-loop-final-external-root", 4, false, true, [
      [root("add",0),edge("add",0,2),edge("add",2,2)],
      [root("del",0)], [root("add",0)], [root("del",0)],
    ]),
    trial("edge-delete-reinsert-diamond", 7, true, false, [
      [], [root("add",0)], [edge("add",0,2),edge("add",0,3),edge("add",2,4),edge("add",3,4)],
      [edge("del",2,4)], [edge("del",3,4)], [edge("add",2,4)], [root("add",4)],
      [edge("del",0,2)], [root("del",4)], [edge("add",0,2)], [root("del",0)],
    ]),
    trial("disconnected-cycle-attach-detach", 8, false, false, [
      [edge("add",2,3),edge("add",3,2),edge("add",5,5)],
      [root("add",0)], [edge("add",0,2)], [edge("del",0,2)], [root("add",2)],
      [root("del",2)], [root("add",5)], [root("del",5)], [root("del",0)],
    ]),
  ];
  for (const dag of [true,false]) for (const seed of [1,7,19,42,73,101,409,2026]) {
    let state = seed;
    const next = () => { state = (Math.imul(state,1664525)+1013904223)>>>0; return state>>>8; };
    const roots = new Set(); const edges = new Map(); const batches = [[]];
    for (let tick=0; tick<64; tick++) {
      if (next()%4 === 0) {
        const n = next()%12;
        const sign = roots.has(n) ? "del" : "add";
        sign === "del" ? roots.delete(n) : roots.add(n);
        batches.push([root(sign,n)]);
      } else {
        let p = next()%12, c = next()%12;
        if (dag && p === c) c = (c+1)%12;
        if (dag && p > c) [p,c] = [c,p];
        const key = `${p},${c}`, sign = edges.has(key) ? "del" : "add";
        sign === "del" ? edges.delete(key) : edges.set(key,[p,c]);
        batches.push([edge(sign,p,c)]);
      }
    }
    cases.push(trial(`seed-${seed}-${dag?"dag":"cyclic"}`,12,dag,false,batches));
    const staticEdges = new Map(Array.from({length:10},(_,i)=>[`${i%2},${i+2}`,[i%2,i+2]]));
    for (let i=0;i<24;i++) {
      let p=2+next()%10,c=2+next()%10;
      if (dag && p===c) continue;
      if (dag && p>c) [p,c]=[c,p];
      staticEdges.set(`${p},${c}`,[p,c]);
    }
    cases.push(trial(`cascade-seed-${seed}-${dag?"dag":"cyclic"}`,12,dag,true,[
      [root("add",0),root("add",1),...[...staticEdges.values()].map(([p,c])=>edge("add",p,c))],
      [root("del",0)],[root("del",1)],
    ]));
  }
  return cases;
}
