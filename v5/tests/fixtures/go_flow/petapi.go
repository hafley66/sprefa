// Package petstore is a minimal stand-in for a generated OpenAPI client: a
// PetAPI interface with one method per operationId, a concrete impl that
// satisfies it structurally, and a shared helper both operations fan into. The
// method names match the spec operationIds (getPet, createPet) so the SCIP
// descriptor name joins the OpenAPI op directly; lowercase keeps that exact
// (Go derives PascalCase in real codegen, which would need name normalization
// that is orthogonal to the dispatch hop under test).
package petstore

// PetAPI is the generated client interface: one method per OpenAPI operationId.
type PetAPI interface {
	getPet(id string) string
	createPet(name string) string
}

// httpExec is the shared helper both operation paths fan into. It is the
// function that should surface as participating in the dataflow of BOTH ops.
func httpExec(route string) string {
	return "GET " + route
}

// petClient satisfies PetAPI. Go records this structurally; scip-go emits the
// satisfaction as an is_implementation relationship, which becomes scip_impl.
type petClient struct{}

func (c petClient) getPet(id string) string {
	return httpExec("/pets/" + id)
}

func (c petClient) createPet(name string) string {
	return httpExec("/pets")
}

// NewPetAPI hands back the interface, so call sites dispatch through PetAPI.
func NewPetAPI() PetAPI {
	return petClient{}
}
