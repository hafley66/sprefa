// Type specs whose underlying type is not a struct or an interface embed list,
// plus an interface method spec: every named type each one mentions is a
// type edge of the declaring spec.
package shapes

type Item struct {
	name string
}

type Key struct {
	id int
}

type Req struct{}

type Resp struct{}

type Handler func(req Req) Resp

type ItemList []Item

type KeyedItems map[Key]*Item

type ItemAlias = Item

type Visitor interface {
	Visit(item Item) Resp
}
