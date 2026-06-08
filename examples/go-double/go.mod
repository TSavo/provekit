// Standalone example module. It is intentionally NOT part of the
// implementations/go module tree: the Sugar Go lifter consumes these
// sources via go/parser (it lifts the AST, it does not compile them), so the
// example demonstrates contract extraction without pulling testify into the
// implementations/go build. `sugar verify` lifts `double.go` (the
// function-contract) and `double_test.go` (the harvested `Double(3) == 6`
// callsite) and discharges the body through z3.
module example.com/go-double

go 1.22

require github.com/stretchr/testify v1.9.0
