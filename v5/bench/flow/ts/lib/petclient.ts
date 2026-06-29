// GENERATED from openapi.yaml — do not edit. One exported function per
// operationId (the codegen rhythm: export name == operationId verbatim). This is
// the package the consuming codebase imports; its defs are the operation symbols.

export function getPet(id: number): Promise<Response> {
  return fetch(`/pets/${id}`);
}

export function createPet(): Promise<Response> {
  return fetch(`/pets`, { method: "POST" });
}
