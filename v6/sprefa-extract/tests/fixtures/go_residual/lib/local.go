package lib

func callOwnDirName() *Base { return NewThing() }

func callPointerConversion(m *Mutable) string {
	converted := (*Base)(m)
	return converted.BasePing()
}
