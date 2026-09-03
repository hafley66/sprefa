package caller

import "example.com/grind/store"

// `picked` is bound by a map index, which the receiver walk cannot type, so
// the reassignment from `sh.Widget()` is the first binding the walk can read.
func Reassign(sh *store.Shop, shelf map[string]*store.Widget) string {
	picked, ok := shelf["a"]
	if !ok {
		picked = sh.Widget()
	}
	return picked.Tag()
}

// A multi-value reassignment: `found` types from the call's first result.
func ReassignPair(sh *store.Shop, shelf map[string]*store.Widget) string {
	found, ok := shelf["b"]
	if !ok {
		found, ok = sh.Lookup("b")
	}
	if !ok {
		return ""
	}
	return found.Tag()
}

// A compound operator never binds; the call inside the index target still
// records its own site.
func Compound(sh *store.Shop, counts map[string]int) {
	counts[sh.Widget().Tag()] += 1
}

// A two-name reassignment: `receipt` types from the call's SECOND result,
// so the position must skip the `,` token between the targets.
func ReassignSecond(sh *store.Shop, shelf map[string]*store.Widget) int {
	sold, ok := shelf["c"]
	var receipt *store.Receipt
	if !ok {
		sold, receipt = sh.Sell("c")
	}
	_ = sold
	return receipt.Total()
}

// A paired `:=` with one call per name: each name types from its own call.
func PairedDeclare(sh *store.Shop) int {
	sold, receipt := sh.Widget(), sh.Receipt()
	_ = sold
	return receipt.Total()
}
