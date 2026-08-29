package main

// Wide has 65 implementers: the site stays on the spec edge plus a fanout_cap row.
type Wide interface {
	Ping() string
}

type impl1 struct{}

func (impl1) Ping() string { return "ping" }

type impl2 struct{}

func (impl2) Ping() string { return "ping" }

type impl3 struct{}

func (impl3) Ping() string { return "ping" }

type impl4 struct{}

func (impl4) Ping() string { return "ping" }

type impl5 struct{}

func (impl5) Ping() string { return "ping" }

type impl6 struct{}

func (impl6) Ping() string { return "ping" }

type impl7 struct{}

func (impl7) Ping() string { return "ping" }

type impl8 struct{}

func (impl8) Ping() string { return "ping" }

type impl9 struct{}

func (impl9) Ping() string { return "ping" }

type impl10 struct{}

func (impl10) Ping() string { return "ping" }

type impl11 struct{}

func (impl11) Ping() string { return "ping" }

type impl12 struct{}

func (impl12) Ping() string { return "ping" }

type impl13 struct{}

func (impl13) Ping() string { return "ping" }

type impl14 struct{}

func (impl14) Ping() string { return "ping" }

type impl15 struct{}

func (impl15) Ping() string { return "ping" }

type impl16 struct{}

func (impl16) Ping() string { return "ping" }

type impl17 struct{}

func (impl17) Ping() string { return "ping" }

type impl18 struct{}

func (impl18) Ping() string { return "ping" }

type impl19 struct{}

func (impl19) Ping() string { return "ping" }

type impl20 struct{}

func (impl20) Ping() string { return "ping" }

type impl21 struct{}

func (impl21) Ping() string { return "ping" }

type impl22 struct{}

func (impl22) Ping() string { return "ping" }

type impl23 struct{}

func (impl23) Ping() string { return "ping" }

type impl24 struct{}

func (impl24) Ping() string { return "ping" }

type impl25 struct{}

func (impl25) Ping() string { return "ping" }

type impl26 struct{}

func (impl26) Ping() string { return "ping" }

type impl27 struct{}

func (impl27) Ping() string { return "ping" }

type impl28 struct{}

func (impl28) Ping() string { return "ping" }

type impl29 struct{}

func (impl29) Ping() string { return "ping" }

type impl30 struct{}

func (impl30) Ping() string { return "ping" }

type impl31 struct{}

func (impl31) Ping() string { return "ping" }

type impl32 struct{}

func (impl32) Ping() string { return "ping" }

type impl33 struct{}

func (impl33) Ping() string { return "ping" }

type impl34 struct{}

func (impl34) Ping() string { return "ping" }

type impl35 struct{}

func (impl35) Ping() string { return "ping" }

type impl36 struct{}

func (impl36) Ping() string { return "ping" }

type impl37 struct{}

func (impl37) Ping() string { return "ping" }

type impl38 struct{}

func (impl38) Ping() string { return "ping" }

type impl39 struct{}

func (impl39) Ping() string { return "ping" }

type impl40 struct{}

func (impl40) Ping() string { return "ping" }

type impl41 struct{}

func (impl41) Ping() string { return "ping" }

type impl42 struct{}

func (impl42) Ping() string { return "ping" }

type impl43 struct{}

func (impl43) Ping() string { return "ping" }

type impl44 struct{}

func (impl44) Ping() string { return "ping" }

type impl45 struct{}

func (impl45) Ping() string { return "ping" }

type impl46 struct{}

func (impl46) Ping() string { return "ping" }

type impl47 struct{}

func (impl47) Ping() string { return "ping" }

type impl48 struct{}

func (impl48) Ping() string { return "ping" }

type impl49 struct{}

func (impl49) Ping() string { return "ping" }

type impl50 struct{}

func (impl50) Ping() string { return "ping" }

type impl51 struct{}

func (impl51) Ping() string { return "ping" }

type impl52 struct{}

func (impl52) Ping() string { return "ping" }

type impl53 struct{}

func (impl53) Ping() string { return "ping" }

type impl54 struct{}

func (impl54) Ping() string { return "ping" }

type impl55 struct{}

func (impl55) Ping() string { return "ping" }

type impl56 struct{}

func (impl56) Ping() string { return "ping" }

type impl57 struct{}

func (impl57) Ping() string { return "ping" }

type impl58 struct{}

func (impl58) Ping() string { return "ping" }

type impl59 struct{}

func (impl59) Ping() string { return "ping" }

type impl60 struct{}

func (impl60) Ping() string { return "ping" }

type impl61 struct{}

func (impl61) Ping() string { return "ping" }

type impl62 struct{}

func (impl62) Ping() string { return "ping" }

type impl63 struct{}

func (impl63) Ping() string { return "ping" }

type impl64 struct{}

func (impl64) Ping() string { return "ping" }

type impl65 struct{}

func (impl65) Ping() string { return "ping" }

func pingAll(w Wide) {
	_ = w.Ping()
}
