package store

// Item is the element of a collection a caller ranges over.
type Item struct{}

// Tag is the method a caller reaches on a range element.
func (i *Item) Tag() string { return "tag" }

// Shelf hands out a SLICE of items, so a range over the call result needs an
// Elem hop past the call's written result type.
type Shelf struct{}

// Items returns the written slice shape `[]*Item`.
func (s *Shelf) Items() []*Item { return nil }
