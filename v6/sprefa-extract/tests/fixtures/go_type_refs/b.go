package typerefs

// The `internal/ast/ast.go` shape: a method named for a type declared in
// another file of the same package, beside a struct field of that type whose
// own name repeats the type name.
type Node struct{}

func (n *Node) ModifierFlags() ModifierFlags {
	return ModifierFlagsNone
}

type ModifierList struct {
	ModifierFlags ModifierFlags
}

type Wrapper struct {
	List ModifierList
}

// `Snapshot` is declared twice in the corpus, so only package scope can bind
// it: the `internal/project/session.go` shape.
type Session struct {
	Snapshot *Snapshot
}
