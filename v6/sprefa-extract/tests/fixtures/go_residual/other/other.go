package other

type Thing struct{}

func NewThing() *Thing { return nil }

func (t *Thing) BasePing() string { return "" }

func (t *Thing) Make() *Thing { return nil }
