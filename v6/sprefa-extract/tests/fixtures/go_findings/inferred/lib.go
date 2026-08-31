// The inferred-receiver corpus: every name below is deliberately shared
// across files where a bare corpus name search would be ambiguous, so an
// edge through an inferred binding proves the callee's declared result type
// did the narrowing.
package inferred

type Thing struct{}

func (t *Thing) Ring() string { return "ring" }
func (t *Thing) Clone() *Thing {
	return t
}

type Other struct{}

func (o *Other) Bell() string { return "bell" }

func NewThing() *Thing { return &Thing{} }
func Two() (*Thing, *Other) {
	return &Thing{}, &Other{}
}
func MightFail() (*Thing, error) {
	return nil, nil
}
func NewErr() error { return nil }
