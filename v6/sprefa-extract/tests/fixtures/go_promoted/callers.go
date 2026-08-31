package promoted

func callDepthOne(o Outer) string { return o.InnerPing() }

func callThroughPointer(p PtrHolder) string { return p.InnerPing() }

func callShadowed(o Outer) string { return o.Shadowed() }

func callCrossPackage(i Importer) string { return i.WriteBase() }

func callDepthFour(d D1) string { return d.Deep5() }

func callDepthFive(d D0) string { return d.Deep5() }

func callAmbiguous(a Ambiguous) string { return a.Tied() }

func callInferredReceiver() string {
	o := newOuter()
	return o.InnerPing()
}

func newOuter() Outer { return Outer{} }
