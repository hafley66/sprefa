// C6 derived source in Go: can a declaration embed a shape that is itself
// computed, and what does a self-referential inclusion do?
package main

import "fmt"

type ARow struct {
	Id int64
}

// (1) embedding a named type declared LATER in the file: accepted, package
// scope is not source-ordered
type Early struct {
	Later
	Extra int64
}

type Later struct {
	Tag string
}

// (2) self-embedding: refused as an invalid recursive type
type SelfRow struct {
	SelfRow
	Extra int64
}

// (3) mutual embedding: refused as an invalid recursive type
type MutualA struct {
	MutualB
	A int64
}

type MutualB struct {
	MutualA
	B int64
}

func main() {
	e := Early{Later{"t"}, 7}
	fmt.Println(e.Tag, e.Extra)
	var s SelfRow
	var m MutualA
	fmt.Println(s, m, ARow{1})
}
