# provekit-ir-symbolic (Go)

Go kit equivalent of `@provekit/ir/symbolic`. Symbolic primitives that
build IR data structures when called. Running an `.invariant.go`-style
file produces declarations the lifter can consume; canonicalizing those
declarations yields the same byte-deterministic FOL form as the TS kit.

## Status

This module ships the team-lead's collector-style surface:
`Must`, `Describe`, `Bridge`, `BeginCollecting`, plus the symbolic
primitives (`ForAll`, `Exists`, `ParseInt`, arithmetic, atomic
predicates, connectives).

The richer surface from `docs/specs/2026-04-29-ir-library-go.md`
(`Property{}` struct with `Scope`, `Bindings`, `Hint`; `scope.*`
helpers; `Register()` / `Properties()`; brand types) is **not** built
here. That spec describes a more ambitious dialect; the team-lead's
task scoped to TS-parity at the symbolic-emission layer. See
"Spec gap" below.

## Cross-kit equivalence

The IR data structure serializes to JSON in the **same field order** as
the TS kit, byte-for-byte for equivalent claims. See
`ir/canonical_form_test.go` for embedded TS-produced fixtures and the
parity assertions.

JSON byte-equivalence is a sanity proxy. Per
`docs/specs/2026-04-29-ast-canonicalizer.md`, the load-bearing
cross-language hash is CBOR over the canonicalized FOL form (NNF +
de-Bruijn + AC-sorted): not the raw IR JSON. A future canonicalizer
pass consumes either kit's IR and produces matching bytes for hashing.

## Usage

```go
import (
    "fmt"
    ir "github.com/provekit/ir-symbolic/ir"
)

func main() {
    finish := ir.BeginCollecting()

    ir.Describe("parseInt", func() {
        ir.Must("can return zero",
            ir.Exists(ir.String, func(s ir.IrTerm) ir.IrFormula {
                return ir.Eq(ir.ParseInt(s), ir.Num(0))
            }),
        )
    })

    decls := finish()
    fmt.Printf("%d declarations\n", len(decls))
}
```

## Tests

```
go test ./ir/...
```

Coverage:
- `types_test.go`: type construction + JSON marshal field order
- `primitives_test.go`: every primitive's IR shape and sort
- `property_test.go`: collector, describe paths, skips, recovery
- `canonical_form_test.go`: byte-identical JSON parity with TS

## Spec gap

The Go IR-library spec (`docs/specs/2026-04-29-ir-library-go.md`)
describes a property-authoring surface that includes:

- `Property{}` literal type with `Scope`, `Bindings`, `Hint` fields
- `scope.Function(name)`, `scope.Module(path)`, `scope.Method(...)`,
  `scope.Region(...)`, `scope.Whenever(predicate)`,
  `scope.Goroutine(...)`, `scope.Channel(...)`
- `Register(p Property)` / `Properties() []Property`
- Brand types (`NonZero[T]`, `NonEmpty[T]`, `Sorted[T]`, etc.) using
  Go 1.18+ generics
- Producer integrations (`gopls`, `govet`, `staticcheck`, `gotest`,
  `gofuzz`, `z3translator`, `datalog`)
- AST canonicalizer using `go/ast` + `go/types`
- Diagnostic translator surfacing mementos as `gopls` diagnostics

None of those are in this module. They would extend the surface; they
do not contradict it. Build them out as separate work.
