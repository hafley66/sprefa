package lib

type Base struct{ Inner Inner }

func (b *Base) BasePing() string { return "" }

type Inner struct{}

func (i Inner) InnerPing() string { return "" }

type (
	Alias  = Base
	Hop    = Alias
	Widget = Inner
)

type Holder struct {
	One   Base
	Items []Base
	Feed  chan Base
}

type Factory struct{}

func (f *Factory) Make() *Base { return nil }

type Shell struct{ Gear *Factory }

func MakeBase() *Base { return nil }

func Pair() (*Base, Inner) { return nil, Inner{} }

func IsThing() bool { return true }

func NewThing() *Base { return nil }

type Mutable Base
