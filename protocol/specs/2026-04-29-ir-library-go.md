# Sugar: Go kit IR — `sugar/ir`

> Author: shared session 2026-04-29 (T + Claude). The Go-side reference
> IR library. Parallel to `@sugar/ir` (TypeScript); same logical
> surface, Go-idiomatic implementation.

## Why this spec exists

The kit standard requires every host language's IR library to expose
the same logical surface — sorts, brands, quantifiers, connectives,
assertions, scope helpers, the `property` constructor — in that
language's native idiom. This spec defines the Go reference.

Go is a load-bearing host language for the framework's reach into
cloud-native, infrastructure, and large-scale backend codebases.
Kubernetes, Docker, etcd, Prometheus, much of HashiCorp's stack,
substantial portions of Google/Cloudflare/Stripe/Uber backend
infrastructure — all Go. Without a Go kit, the framework misses a
significant slice of modern enterprise software.

This spec fixes:
- The Go module's import path and package layout.
- The Go-idiomatic surface for both dialects (type and library).
- The constraints Go's type system imposes (no macros, limited
  generics until 1.18+, no operator overloading).
- The producer pool integrations (gopls, go vet, staticcheck,
  golangci-lint, go test, fuzz, kani-go, Z3 via translator).
- The hash-equivalence contract with other kits.

## Constraints Go imposes (and how the kit handles them)

Go's strengths and weaknesses for IR authoring:

- **No macros / proc-macros.** The Rust kit uses `forall!{}` macros to
  build IR at compile time; the TS kit uses higher-order functions.
  Go has neither macros nor sufficient first-class generics for the
  TS-style approach to be ergonomic. Go's IR uses **builder pattern
  with method chaining** plus generics where they help (Go 1.18+).
- **No operator overloading.** Where TS can write `b !== 0`, Go must
  write `assert.NotEqual(b, Zero)`. Slightly more verbose, but the
  AST canonicalizer normalizes both forms to the same FOL.
- **`go generate` for code generation.** When the IR library needs to
  emit code (e.g., to register property invocations with the
  framework's runtime), Go's idiom is `go generate` directives, not
  build-time macros.
- **Reflection.** Go has runtime reflection via the `reflect`
  package; the kit uses it sparingly (for sort identification at
  runtime) but prefers compile-time approaches.
- **Type assertions over branded types.** Go's brands are runtime-
  enforced via wrapper struct types with private fields, similar to
  TypeScript brands but enforced at construction not at consumption.

Go's strengths for IR authoring:

- **Strong static typing** carries a lot of verification load (similar
  to Rust's strength but with a simpler type system).
- **`go vet` and `staticcheck`** are mature, widely-deployed producers.
- **Build tag / constraint system** is a natural place for IR
  registration metadata.
- **Excellent tooling and language server (gopls)** make IDE
  integration straightforward.
- **`go test` and the testing package** integrate naturally with the
  behavioral memento producer.
- **`go fmt`'s discipline** makes canonical-form authoring less of an
  ergonomic ask than in less opinionated languages.

## Module identity

```
import "github.com/sugar/ir"

// or, after a community fork:
import "github.com/<org>/sugar-ir"
```

Module path is canonical; the framework's kit registry pins the CID.
A repo's `go.mod` may pin the version explicitly. The IR library has
no transitive dependencies beyond the Go standard library and a
small JSON canonicalization package.

## Type-dialect surface

Property authoring as Go types. Verified by gopls + go vet +
staticcheck as producers; mementos emitted on every clean check.

```go
package main

import (
    "github.com/sugar/ir"
)

// Branded primitives via wrapper structs with private fields.
type NonZero[T ir.Numeric] struct {
    v T
    _ ir.Brand[NonZeroBrand]
}

type NonZeroBrand struct{}

// Constructor: runtime check produces the brand.
func NewNonZero[T ir.Numeric](x T) (NonZero[T], error) {
    if x == 0 {
        return NonZero[T]{}, ir.ErrViolation{Brand: "non-zero"}
    }
    return NonZero[T]{v: x}, nil
}

func (n NonZero[T]) Value() T { return n.v }

// Function consumes a NonZero — the type system enforces the
// non-zero precondition; gopls verifies at every call site.
func Divide[T ir.Numeric](a T, b NonZero[T]) T {
    return a / b.Value()
}
```

The framework ships standard brands as part of `sugar/ir`:

```go
// sugar/ir/brands.go
package ir

type NonZero[T Numeric] struct { ... }
type NonEmpty[T any] struct { ... }
type Sorted[T Ordered] struct { ... }
type Validated[T any, S Schema] struct { ... }
type Refined[T any, P Predicate] struct { ... }
type NonNull[T any] struct { ... }    // Go's nil semantics

// Numeric, Ordered, Schema, Predicate are constraint interfaces.
type Numeric interface { ~int | ~int8 | ... | ~float32 | ~float64 }
type Ordered interface { Numeric | ~string }
type Schema interface { Validate(value any) error }
type Predicate interface { Check(value any) bool }
```

Construction always goes through the kit's constructor functions;
the brand fields are unexported, so external code can't forge a
brand without calling the constructor (which performs the check).

## Library-dialect surface

Property authoring as Go values. Builder pattern instead of
higher-order functions.

```go
package divide

import (
    "github.com/sugar/ir"
    "github.com/sugar/ir/assert"
    "github.com/sugar/ir/scope"
)

var DenominatorNonZero = ir.Property{
    Name:  "denominator-nonzero",
    Scope: scope.Function("Calculate"),
    Bindings: ir.Bindings{
        "b": ir.Int,
    },
    Formula: ir.ForAll("b", ir.Int, func(b ir.Term) ir.Formula {
        return assert.NotEqual(b, ir.IntConst(0))
    }),
}

var InputSanitizedBeforeSink = ir.Property{
    Name:  "user-input-sanitized-before-execSync",
    Scope: scope.Module("api"),
    Bindings: ir.Bindings{
        "input": ir.Ref,
        "sink":  ir.Ref,
    },
    Formula: ir.ForAll2(
        "input", ir.Ref,
        "sink", ir.Ref,
        func(input, sink ir.Term) ir.Formula {
            return ir.Implies(
                ir.And(
                    assert.DataFlowsTo(input, sink),
                    assert.KindOf(sink, "execSync"),
                ),
                ir.Exists("path", ir.Ref, func(path ir.Term) ir.Formula {
                    return ir.And(
                        assert.OnPath(path, input, sink),
                        assert.KindOf(path, "sanitize"),
                    )
                }),
            )
        },
    ),
}
```

Properties are package-level `var` declarations. The framework's
discovery mechanism walks the AST at parse time, identifies
`ir.Property{...}` literals, ingests them into the property
registry. Authors can also register dynamically:

```go
func init() {
    ir.Register(DenominatorNonZero)
    ir.Register(InputSanitizedBeforeSink)
}
```

The framework's `sugar prove` command picks up properties via
both static discovery and `init()`-time registration.

## Required exports

Every Go kit IR library MUST export (mapped from the kit standard's
required logical surface):

### Sorts

```go
package ir

var Bool   Sort = sort{kind: "primitive", name: "Bool"}
var Int    Sort = sort{kind: "primitive", name: "Int"}
var Real   Sort = sort{kind: "primitive", name: "Real"}
var String Sort = sort{kind: "primitive", name: "String"}
var Ref    Sort = sort{kind: "primitive", name: "Ref"}
var Node   Sort = sort{kind: "primitive", name: "Node"}
var Edge   Sort = sort{kind: "primitive", name: "Edge"}

func SetOf(elem Sort) Sort
func TupleOf(elems ...Sort) Sort
func FuncOf(domain []Sort, range_ Sort) Sort
```

### Quantifiers

```go
func ForAll(varName string, sort Sort, body func(Term) Formula) Formula
func Exists(varName string, sort Sort, body func(Term) Formula) Formula

// Convenience: 2-arg, 3-arg quantifiers.
func ForAll2(name1 string, sort1 Sort, name2 string, sort2 Sort,
    body func(Term, Term) Formula) Formula

func Exists2(name1 string, sort1 Sort, name2 string, sort2 Sort,
    body func(Term, Term) Formula) Formula

// Bounded quantifier over a known set.
func ForSome(name string, sort Sort, domain Term,
    body func(Term) Formula) Formula
```

### Connectives

```go
func And(formulas ...Formula) Formula
func Or(formulas ...Formula) Formula
func Not(f Formula) Formula
func Implies(antecedent, consequent Formula) Formula
func Iff(a, b Formula) Formula
```

### Assertions

```go
package assert

func Equal(a, b ir.Term) ir.Formula
func NotEqual(a, b ir.Term) ir.Formula
func LessThan(a, b ir.Term) ir.Formula
func LessThanOrEqual(a, b ir.Term) ir.Formula
func GreaterThan(a, b ir.Term) ir.Formula
func GreaterThanOrEqual(a, b ir.Term) ir.Formula

func True(b ir.Term) ir.Formula
func False(b ir.Term) ir.Formula

func Subset(a, b ir.Term) ir.Formula
func Member(x, set ir.Term) ir.Formula

// SAST predicates.
func KindOf(node ir.Term, kind string) ir.Formula
func DataFlowsTo(a, b ir.Term) ir.Formula
func Dominates(a, b ir.Term) ir.Formula
func OnPath(path, from, to ir.Term) ir.Formula

// Temporal predicates.
type TransitionFrom struct { pre ir.Term }
func TransitionFromTerm(pre ir.Term) TransitionFrom
func (t TransitionFrom) To(post ir.Term) ir.Formula
```

### Scope helpers

```go
package scope

func Function(name string) ir.BindingScope
func Module(path string) ir.BindingScope
func Type(name string) ir.BindingScope
func Method(receiverType, methodName string) ir.BindingScope
func Region(start, end ir.Position) ir.BindingScope
func Whenever(predicate ir.Formula) ir.BindingScope

// Go-specific.
func Goroutine(spawnSite ir.Position) ir.BindingScope
func Channel(decl ir.Position) ir.BindingScope
```

### The `Property` type and constructor

```go
type Property struct {
    Name     string
    Scope    BindingScope
    Bindings Bindings
    Formula  Formula
    Hint     CompilationHint  // optional
}

type Bindings map[string]Sort

type CompilationHint string

const (
    HintAuto             CompilationHint = "auto"
    HintDatalogFriendly  CompilationHint = "datalog-friendly"
    HintRequiresSMT      CompilationHint = "requires-smt"
    HintBehavioral       CompilationHint = "behavioral"
)

func Register(p Property)
func Properties() []Property  // returns all registered
```

## Internal representation

Same `IrFormula` data structure as the TypeScript kit, expressed in
Go types:

```go
type Formula interface {
    canonical() canonicalForm
}

// Internal — not exported. Each constructor produces one of:
type forAllFormula struct {
    Sort      Sort
    VarName   string
    Body      Formula
}

type existsFormula struct {
    Sort      Sort
    VarName   string
    Body      Formula
}

type andFormula struct {
    Conjuncts []Formula
}

type orFormula struct {
    Disjuncts []Formula
}

type notFormula struct {
    Body Formula
}

type impliesFormula struct {
    Antecedent Formula
    Consequent Formula
}

type atomicFormula struct {
    Predicate string
    Args      []Term
}

type Term interface {
    canonical() canonicalForm
}

type varTerm struct {
    Name string
    Sort Sort
}

type constTerm struct {
    Value any
    Sort  Sort
}

type ctorTerm struct {
    Name string
    Args []Term
    Sort Sort
}
```

The `canonical()` method on each interface returns a tagged-union
intermediate representation that the AST canonicalizer hashes. The
hash is byte-identical to the TypeScript kit's hash for the same
logical formula — that's the cross-language equivalence contract.

## Producer integrations

The Go kit registers the following producers:

```go
// sugar/ir/producers/
//   gopls/        — type-check-pass producer for gopls
//   govet/        — pattern-match producer for go vet
//   staticcheck/  — pattern-match producer for staticcheck
//   golangcilint/ — composite linter producer
//   gotest/       — behavioral memento producer for go test
//   gofuzz/       — property-test memento producer for go fuzz
//   z3translator/ — translates IR formulas to SMT-LIB; runs Z3
//   datalog/      — translates pattern properties to Datalog/Soufflé
```

Each producer wraps an existing tool. The kit's `sugar prove`
invocation runs them in parallel and aggregates mementos.

The mandate-able floor for Go: `go vet ./... && staticcheck ./... &&
go test ./...` produces a composite memento that holds when all
required producers' verdicts are `holds`. This is a richer floor
than `tsc --strict` because Go's static analyzers are mature and
catch many classes of correctness issues by default.

## Diagnostic translator

Memento failures surface as Go-native errors:

```
./api/divide.go:42:9: sugar-violation: denominator-nonzero
    Z3 counterexample: b = 0
    Suggestion: change b's type to NonZero[float64]
```

For LSP integration via gopls, mementos surface as standard
diagnostics with severity Error and code "SUGAR-V<n>" where
`<n>` is a stable property identifier.

## File extensions

The Go kit handles: `.go`.

It also recognizes Go-specific build artifacts: `go.mod`, `go.sum`,
build tags (`//go:build` directives), `go generate` comments. These
inform the canonicalizer's scope detection and the producer pool's
invocation.

## Cross-language equivalence example

Same logical claim authored in three host languages:

**TypeScript:**

```typescript
const denominatorNonZero = property({
  name: "denominator-nonzero",
  scope: function_("calculate"),
  bindings: { b: Int },
  formula: forAll((b: Int) => assert.notEqual(b, 0)),
});
```

**Rust:**

```rust
sugar::property! {
    name: "denominator-nonzero",
    scope: function!("calculate"),
    bindings: { b: Int },
    formula: forall(b: Int) => b != 0,
}
```

**Go:**

```go
var DenominatorNonZero = ir.Property{
    Name:  "denominator-nonzero",
    Scope: scope.Function("calculate"),
    Bindings: ir.Bindings{"b": ir.Int},
    Formula: ir.ForAll("b", ir.Int, func(b ir.Term) ir.Formula {
        return assert.NotEqual(b, ir.IntConst(0))
    }),
}
```

All three canonicalize to the same FOL form:

```
∀b: Int. ¬(b = 0)
```

Same `propertyHash`. Same memento slot. Cross-validation between
TS-produced, Rust-produced, and Go-produced mementos works
mechanically.

## Acceptance test

The Go kit IR library is correct when:

1. A Go developer can author both type-dialect and library-dialect
   properties using only the kit's exports.
2. The IR formula data structure round-trips through serialization
   without semantic loss.
3. gopls verifies type-dialect properties at edit time without any
   framework-specific extension.
4. The library's exports match the kit-standard's required logical
   surface.
5. The same logical claim authored in Go and in TypeScript produces
   canonicalized FOL forms with matching propertyHashes.
6. The mandate-able floor (`go vet && staticcheck && go test`)
   produces a composite memento that holds for a clean Go codebase.

When these six hold, Go is a fully-supported host language. The
framework reaches Kubernetes, Docker, etcd, Prometheus,
HashiCorp's stack, and the substantial Go-shaped slice of modern
backend engineering.

## Implementation notes

- The kit's reference implementation lives in the framework's
  `implementations/go/` directory. Published to Go modules as
  `github.com/sugar/ir`.
- Brands use Go 1.18+ generics. For Go versions <1.18, fall back to
  per-type brand structs (less ergonomic but functional).
- Property registration via `init()` functions integrates with Go's
  standard initialization order; framework discovery picks up
  registered properties at parse time.
- The AST canonicalizer for Go uses `go/ast` and `go/types` from
  the standard library; the canonicalizer's WASM target compiles
  via tinygo.
- LLM prompt set is Go-idiomatic: teaches goroutine semantics,
  channel ownership, error-handling conventions, the standard
  library's idiomatic patterns, gofmt's discipline.
- Producer integrations wrap existing CLI tools (`go vet`,
  `staticcheck`, etc.) via subprocess invocation; the wrapping is
  thin (~50 lines per producer).

## Spec-as-canonical (meta-note)

Per the recursion principle: this spec is content-addressed. Its
CID is the canonical identity of the Go-kit IR library. Any
implementation that satisfies the spec — including a future LLM-
authored implementation, or a community fork — produces
hash-equivalent mementos. The framework's substrate is the spec,
not any particular implementation.

Two organizations could ship competing Go kit implementations;
their mementos compose at the wrapper level; any divergence
surfaces as cross-validation signal. The kit's identity is the
spec's CID; the implementation is fungible.
