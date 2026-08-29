package multihop

import "example.com/multihop/sub"

func viaStruct(o Orch) string { return o.FS().FileExists() }

func viaIface(o Orch) string { return o.VFS().FileExists() }

func (a App) viaField() int { return a.cfg.Log.Write() }

func viaImport() string { return sub.NewSub().Hello() }

func viaBuiltin(o Orch) string { return o.Name().ToUpper() }

func viaGeneric(o Orch) string { return o.Items().Fetch() }

// Seven Self hops plus the final Ping: eight hops total, the cap.
func viaEight(o Orch) string {
	return o.Self().Self().Self().Self().Self().Self().Self().Ping()
}

// Eight Self hops plus the final Ping: nine hops, past the cap.
func viaNine(o Orch) string {
	return o.Self().Self().Self().Self().Self().Self().Self().Self().Ping()
}
