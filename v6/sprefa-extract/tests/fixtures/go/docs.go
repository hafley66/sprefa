// docs.go: doc-facet parity fixture. Exercises v5's walk_go_docs: the
// contiguous line-comment block directly above a type spec (struct /
// interface / alias), function, or method declaration becomes a DocFact
// (go_leading_doc walks prev-sibling comments, requiring row adjacency; a
// blank line breaks the block). The documented entities re-exercise the
// ported facets (type/call/df) on doc-heavy input. ASCII-only so tree-sitter
// byte spans round-trip cleanly (parity is clean).

package docs

// Engine is a tiny string-bearing struct.
type Engine struct {
	name string
}

// Sizer reports a size.
type Sizer interface {
	Size() int
}

// Mode is an int alias.
type Mode int

// Trim returns its input unchanged.
func Trim(value string) string {
	return value
}

// MakeEngine builds an Engine from a name.
// A second doc line, to exercise the multi-line block walk.
func MakeEngine(name string) Engine {
	trimmed := Trim(name)
	engine := Engine{name: trimmed}
	return engine
}

// Mode picks the fast mode.
func (e *Engine) Mode() Mode {
	picked := Mode(0)
	return picked
}
