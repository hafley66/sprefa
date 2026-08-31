package caller

import "example.com/rangelem/store"

// Tag is a decoy: a free function sharing the element method's name, so a
// corpus-wide name guess cannot fake the element binding.
func Tag() string { return "decoy" }

// RangeCall ranges over a call result and calls through the element.
func RangeCall(sh *store.Shelf) {
	for _, it := range sh.Items() {
		it.Tag()
	}
}

// RangeInferred ranges over a variable a call result bound, then calls through
// the element: the bound type is the written slice shape `[]*store.Item`.
func RangeInferred(sh *store.Shelf) {
	src := sh.Items()
	for _, it := range src {
		it.Tag()
	}
}
