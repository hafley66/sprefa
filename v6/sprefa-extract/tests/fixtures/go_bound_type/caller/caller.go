package caller

import (
	"example.com/bound/callee"
	"example.com/bound/types"
)

var _ = types.Widget{}

// Call binds one value from a single-result call, then calls through it.
func Call() {
	w := callee.NewWidget()
	w.Ping()
}

// CallPair binds one value from a multi-value define, then calls through it.
func CallPair() {
	w, _ := callee.GivePair()
	w.Ping()
}
