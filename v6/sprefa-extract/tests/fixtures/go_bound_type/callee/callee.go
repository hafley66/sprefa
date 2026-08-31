package callee

import "example.com/bound/types"

// NewWidget returns a type written QUALIFIED through this file's own import:
// the return type as written is `*types.Widget`, never bare `Widget`.
func NewWidget() *types.Widget { return &types.Widget{} }

// GivePair returns one in the multi-value form a `w, err :=` define binds.
func GivePair() (*types.Widget, error) { return &types.Widget{}, nil }
