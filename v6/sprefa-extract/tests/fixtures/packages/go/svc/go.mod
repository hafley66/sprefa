module example.com/svc

go 1.21

require (
	example.com/lib v0.0.0
	golang.org/x/net v0.20.0
)

replace example.com/lib => ../lib
