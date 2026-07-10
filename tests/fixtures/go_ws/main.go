package main

import (
	"fmt"

	// Aliased import -- exercises the module_binding alias hop against compiler
	// ground truth (loadUser is the local name, api the real package).
	loadUser "example.com/gows/api"
	"example.com/gows/service"
)

func greet(id int) string {
	name := loadUser.FetchUser(id)
	svc := &service.UserService{}
	tagged := svc.GetUser(id)
	tag := loadUser.Helper()
	return "hello " + name + " " + tagged + " " + tag
}

func main() {
	fmt.Println(greet(1))
}
