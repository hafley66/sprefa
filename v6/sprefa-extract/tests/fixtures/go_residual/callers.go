package residual

import "example.com/residual/lib"

func callAlias(a *lib.Alias) string { return a.BasePing() }

func callAliasChain(h *lib.Hop) string { return h.BasePing() }

func callFieldHop(h lib.Holder) string { return h.One.BasePing() }

func callFieldHopTwice(s lib.Shell) *lib.Base { return s.Gear.Make() }

func callImportRoot() string { return lib.MakeBase().BasePing() }

func callRangeOverField(h lib.Holder) string {
	for _, item := range h.Items {
		return item.BasePing()
	}
	return ""
}

func callRangeIndexOnly(h lib.Holder) string {
	for item := range h.Items {
		return item.BasePing()
	}
	return ""
}

type localFeed struct{ Stream chan lib.Base }

func callRangeOverChannel(f localFeed) string {
	for item := range f.Stream {
		return item.BasePing()
	}
	return ""
}

func callTypeSwitch(v any) string {
	switch narrowed := v.(type) {
	case *lib.Base:
		return narrowed.BasePing()
	}
	return ""
}

func callFieldRead(h lib.Holder) string {
	read := h.One
	return read.BasePing()
}

func callIndexRead(h lib.Holder) string {
	read := h.Items[0]
	return read.BasePing()
}

func callMultiValue() string {
	base, inner := lib.Pair()
	return base.BasePing() + inner.InnerPing()
}

func callTypeAssert(v any) string {
	narrowed := v.(*lib.Base)
	return narrowed.BasePing()
}
