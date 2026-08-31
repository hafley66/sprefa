package promoted

import "example.com/promoted/base"

// Every method name below is unique to its owner, so a bound edge names the
// walk that found it.

type Inner struct{}

func (Inner) InnerPing() string { return "" }

func (Inner) Shadowed() string { return "inner" }

type Outer struct {
	Inner
	name string
}

func (Outer) Shadowed() string { return "outer" }

type PtrHolder struct {
	*Inner
}

type Importer struct {
	base.Writer
}

type D5 struct{}

func (D5) Deep5() string { return "" }

type D4 struct{ D5 }

type D3 struct{ D4 }

type D2 struct{ D3 }

type D1 struct{ D2 }

type D0 struct{ D1 }

type LeftArm struct{}

func (LeftArm) Tied() string { return "" }

type RightArm struct{}

func (RightArm) Tied() string { return "" }

type Ambiguous struct {
	LeftArm
	RightArm
}
