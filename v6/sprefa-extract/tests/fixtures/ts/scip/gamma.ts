// scip-ratchet fixture (4c-ii): calls `helper()` with NO same-file def. The
// name-match sees alpha.helper + beta.helper (ambiguous -> no row); scip
// resolves the call to alpha.helper through the import -> ScipOverride.
import { helper } from "./alpha";

export function use(): number {
  return helper();
}
