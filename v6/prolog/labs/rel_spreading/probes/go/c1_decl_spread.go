// C1 decl spread in Go: struct embedding is the inclusion mechanism.
// EXPECTED: it PROMOTES names, it does not SPLICE fields. A positional
// composite literal proves which one it is.
package main

import "fmt"

type ARow struct {
	Id   int64
	Name string
}

type BRow struct {
	ARow
	Extra int64
}

func main() {
	// (1) promoted field access reads like a splice
	b := BRow{ARow: ARow{Id: 1, Name: "n"}, Extra: 7}
	fmt.Println(b.Id, b.Name, b.Extra)

	// (2) but the positional literal has TWO slots, not three: embedding did
	// not widen the field list
	b2 := BRow{ARow{2, "m"}, 8}
	fmt.Println(b2)

	// (3) the spliced-width literal is refused
	b3 := BRow{3, "o", 9}
	fmt.Println(b3)
}
