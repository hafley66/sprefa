// sample.go: a small Go fixture exercising every facet (type/call/df).
// ASCII-only so tree-sitter byte spans round-trip cleanly (parity is clean).
// v5 go emits NO const facet: a package-level `const` produces no type_node and
// no const_value (walk_go_entities skips const_declaration; extract leaves
// consts empty). The const here confirms both sides emit nothing for it. A
// const INSIDE a fn body does hit the df const_spec path (go_flow_spec).

package sample

type Engine struct {
	name string
}

type Sizer interface {
	Size() int
}

type Mode int

const Greeting string = "hello"

func Trim(value string) string {
	return value
}

func MakeEngine(name string) Engine {
	trimmed := Trim(name)
	engine := Engine{name: trimmed}
	return engine
}

func (e *Engine) Mode() Mode {
	picked := Mode(0)
	return picked
}
