// Expected: the c.Write() site resolves to the interface method declaration
// Writer.Write and emits a resolved_edge (dynamic dispatch target = interface
// method). Observed on typescript-go: methods declared on interface types have
// no function node in the parse arm, so every interface-typed call site
// (500 `interface {` declarations across internal/*) resolves to nothing.
package main

type Writer interface {
	Write(p []byte) (int, error)
}

type C struct{ w Writer }

func (c C) caller() {
	c.w.Write([]byte("x"))
}
