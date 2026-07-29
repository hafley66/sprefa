// C2 collision in Go: two embedded structs contributing the same field name.
// EXPECTED: the DECLARATION is accepted silently; the collision only surfaces
// at a use site, as an ambiguous selector.
package main

import "fmt"

type ARow struct {
	Shared int64
	OnlyA  int64
}

type BRow struct {
	Shared int64
	OnlyB  int64
}

// declared with a colliding promoted name: accepted
type Merged struct {
	ARow
	BRow
}

func main() {
	m := Merged{ARow{1, 2}, BRow{3, 4}}

	// the non-colliding promoted names work
	fmt.Println(m.OnlyA, m.OnlyB)

	// the colliding one is refused at the USE site only
	fmt.Println(m.Shared)
}
