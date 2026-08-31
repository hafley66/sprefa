package lib

import "example.com/fieldpromote/base"

// Inner declares the field; Outer only EMBEDS Inner, so `outer.Part` names
// the field through Go's field promotion, never a direct field of Outer.
type Inner struct {
	Part *base.Widget
}

type Outer struct {
	Inner
}

// NewOuter hands one out.
func NewOuter() *Outer { return &Outer{} }
