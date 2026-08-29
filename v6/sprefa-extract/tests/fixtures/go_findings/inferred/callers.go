package inferred

func fromSamePkgFunc() string {
	t := NewThing()
	return t.Ring()
}

func fromMethodResult(w *Thing) string {
	t := w.Clone()
	return t.Ring()
}

func fromPairFirst() string {
	t, err := MightFail()
	_ = err.Error()
	return t.Ring()
}

func fromMultiAssignIndex() string {
	a, b := Two()
	_ = a.Ring()
	return b.Bell()
}

func chainResolvesInSourceOrder() string {
	a := NewThing()
	b := a.Clone()
	return b.Ring()
}

func unboundCalleeStaysInferred() string {
	x := undefinedCallee()
	return x.Ring()
}

func interfaceResultStaysInferred() string {
	e := NewErr()
	return e.Error()
}
