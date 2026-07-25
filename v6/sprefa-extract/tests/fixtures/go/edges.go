// edges.go: type_edge parity fixture (4d-i-go). Exercises v5's go_edges_from
// (src/graph/typegraph/go.rs:299): struct fields of named types (field),
// struct embeds incl. via pointer (impl), interface type_elem embeds (impl),
// and declared type-parameter constraints (generic) - v5's own
// go_fields_embeds_and_generic_constraints input shape. Method/fn SIGNATURES
// are NOT edge sources (entity-level type_sig covers callables; v5 go's
// type_edge is shape-only), so the one method below contributes no type_edge.
// A qualified ref (time.Time) exercises the qualified_type arm; it names no
// corpus node, so its resolved dst leg is the v6-only zero leg (text stays
// text). ASCII-only so tree-sitter byte spans round-trip cleanly.

package edges

import "time"

type Entity interface {
	Label() string
}

type Pricing interface {
	Entity
	Price() int
}

type Cache struct {
	hits int
}

type Item struct {
	name string
}

type Store struct{}

type Repo[T Entity] struct {
	Store
	*Pricing
	cache Cache
	items []Item
	stamp time.Time
}

type Color int

func (r Repo[T]) Label() string {
	return "repo"
}
