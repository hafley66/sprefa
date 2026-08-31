// scip-ratchet fixture: `probe` has two corpus defs and NEITHER is an import
// binding here, so the module plane has no word and the name match takes the
// same-file `Near.probe` for both sites. scip types the receiver and moves the
// second one to `epsilon.ts`. This file is the ScipIndexLoad witness: the only
// row in the corpus that exists BECAUSE an index was loaded.
import { Far } from "./epsilon";

class Near {
  probe(): number {
    return 1;
  }
}

// No declared return type: the receiver leg cannot trace `far`, which keeps
// this the ScipIndexLoad witness (a receiver-typed call only scip can bind).
function castFar() {
  return new Far();
}

export function reach(): number {
  const near = new Near();
  const far = castFar();
  return near.probe() + far.probe();
}
