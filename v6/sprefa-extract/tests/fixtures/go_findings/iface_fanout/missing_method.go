package main

// Partial has ONE full implementer: Door covers both methods, Half covers
// only Open, so the Close sites fan out to Door alone and the interface is
// never satisfied by Half.
type Door interface {
	Open() string
	Close() string
}

type Half struct{}

func (Half) Open() string { return "half.open" }

type Full struct{}

func (Full) Open() string  { return "full.open" }
func (Full) Close() string { return "full.close" }

func swing(d Door) string {
	return d.Close()
}
