// C5 modifier inheritance in Go: does the inclusion mechanism carry the
// source's BEHAVIOR as well as its fields? EXPECTED: yes, and that is
// exactly the hazard. Embedding promotes methods and interface satisfaction
// silently along with the columns.
package main

import "fmt"

type Keyed interface {
	Key() int64
}

type ARow struct {
	Id int64
}

func (a ARow) Key() int64 { return a.Id }

// BRow declares no method and names no interface
type BRow struct {
	ARow
	Extra int64
}

// CRow copies the SAME columns by hand instead of embedding
type CRow struct {
	Id    int64
	Extra int64
}

func wantsKeyed(k Keyed) { fmt.Println(k.Key()) }

func main() {
	b := BRow{ARow{1}, 2}
	// BRow silently satisfies Keyed because of the embedding
	wantsKeyed(b)

	c := CRow{1, 2}
	// the hand-copied columns do not
	wantsKeyed(c)
}
