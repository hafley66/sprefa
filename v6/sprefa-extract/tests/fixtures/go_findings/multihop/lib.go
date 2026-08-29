package multihop

// Declared types the multihop chains walk through. Every method name the
// chains resolve is unique to its owner, so a passing assertion proves the
// chain typing did the work.

type Host struct{}

func (Host) FileExists() string { return "" }

type FS interface {
	FileExists() string
}

type Real struct{}

func (Real) FileExists() string { return "" }

type Orch struct{}

func (Orch) FS() Host         { return Host{} }
func (Orch) VFS() FS          { return Real{} }
func (Orch) Name() string     { return "" }
func (Orch) Items() List[Item] { return List[Item]{} }
func (Orch) Self() *Orch      { return nil }
func (Orch) End() string      { return "" }
func (Orch) Ping() string     { return "" }

type Item struct{}

type List[T any] struct{}

func (l List[T]) Fetch() string { return "" }

func (Host) Fetch() string { return "" }

func (Host) Ping() string { return "" }

type Logger struct{}

func (Logger) Write() int { return 0 }

type Cfg struct {
	Log Logger
}

type App struct {
	cfg Cfg
}
