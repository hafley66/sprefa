// Widget.Name and Gadget.Name (gadget.go) share a bare name: a plain
// corpus-wide name search is ambiguous on "Name", so every call below
// resolves only if the receiver's declared type narrows the candidate first.
package typeplane

type Widget struct {
	Sub *Widget
}

func (w *Widget) Name() string { return "widget" }

type Box struct {
	Items []Widget
}

func localVar() string {
	var w Widget
	return w.Name()
}

func viaParam(w *Widget) string {
	return w.Name()
}

func viaPointer() string {
	w := &Widget{}
	return w.Name()
}

func viaField(w *Widget) string {
	return w.Sub.Name()
}

func viaSliceElement(b Box) string {
	return b.Items[0].Name()
}

func newWidget() Widget {
	return Widget{}
}

func viaInferred() string {
	w := newWidget()
	return w.Name()
}
