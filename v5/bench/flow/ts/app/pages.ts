// Consumer codebase: imports the generated package and calls its operations.
// Every import + callsite here is a resolved ref to a generated symbol, so the
// blast radius of an operation is exactly these occurrences (compiler-resolved,
// not name-matched).

import { getPet, createPet } from "../lib/petclient";

export function loadPetPage(id: number) {
  return getPet(id);
}

export function refreshPet(id: number) {
  return getPet(id); // second callsite of the same operation
}

export function onCreateClicked() {
  return createPet();
}
