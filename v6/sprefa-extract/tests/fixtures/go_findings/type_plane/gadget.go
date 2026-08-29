// The competing "Name" method: proves receivers.go's calls did not resolve
// by luck (a unique corpus name).
package typeplane

type Gadget struct{}

func (g *Gadget) Name() string { return "gadget" }
