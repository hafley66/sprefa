// Consumer (a different language than the backend). Call sites use the spec
// operationId verbatim, so a spec op resolves to these call sites cross-repo.

function getPet(id: number): Promise<Response> {
  return fetch(`/pets/${id}`);
}

function createPet(): Promise<Response> {
  return fetch(`/pets`, { method: "POST" });
}

// the actual consumers (handlers, in your words): they call the operations.
export function loadPetPage(id: number) {
  return getPet(id);
}

export function onCreateClicked() {
  return createPet();
}
