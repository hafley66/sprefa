package inferred

import "example.com/inferred/sub"

func fromImportQualifiedFunc() string {
	s := sub.NewSub()
	return s.Hello()
}
