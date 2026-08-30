package residual

import "example.com/residual/lib"

func callQualifiedFreeFunc() bool { return lib.IsThing() }

func callThroughShadowedName() string {
	lib := lib.MakeBase()
	return lib.BasePing()
}
