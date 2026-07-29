// C4 width subtyping in Go: is the wider (embedding) struct accepted where
// the narrower one is wanted? EXPECTED: refused for struct types.
package main

import "fmt"

type Narrow struct {
	Id int64
}

type Wide struct {
	Narrow
	Extra int64
}

func takesNarrow(row Narrow) {
	fmt.Println(row.Id)
}

func main() {
	w := Wide{Narrow{1}, 2}

	// (1) the embedding struct where the embedded one is wanted: refused
	takesNarrow(w)

	// (2) the promoted field must be named explicitly
	takesNarrow(w.Narrow)

	// (3) a structurally identical but separately declared struct: refused
	type AlsoNarrow struct {
		Id int64
	}
	takesNarrow(AlsoNarrow{1})
}
