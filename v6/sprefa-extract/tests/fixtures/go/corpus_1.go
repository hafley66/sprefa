package corpus

// Expected fact (v5-parity-pinned): Get's ret sig should NOT carry the
// receiver-declared type parameter name T, but v5 go_fn_type and this port
// both emit sig{owner=Get,slot=ret,pos=0,ty="T"} because the exclusion set
// only reads the method's own type_parameters field, and a receiver form
// `func (g Gen[T]) Get() T` declares T inside the receiver's type_arguments.
type Gen[T any] struct {
	V T
}

func (g Gen[T]) Get() T {
	return g.V
}
