# Tutorial: Go

> **Status:** kit shipping in the current v1.6.3 tree. Lift adapters planned: `go-playground/validator`, `ozzo-validation`. Decorator macros: comment annotations (`//sugar:contract`) under evaluation. Embedded verifier shipping (CGO bridge to Rust canonicalizer; pure-Go canonicalizer planned). LSP plugin planned. Verification via the Rust CLI.

A walkthrough for Go developers. **v1.1 is the kit; lift adapters land in v1.2.** If you can wait, the v1.2 release will pick up `validate:` struct tags from `go-playground/validator` and `ozzo-validation` rule chains automatically. If you can't, you can author IR directly via the kit's API today.

## 1. What you'll have at the end

In v1.2: a `.proof` file lifted from your existing `validate:"required,email"` struct tags.

In v1.1: a `.proof` file authored directly through the Go kit's IR API.

## 2. Prerequisites

- Go 1.22+.
- Rust toolchain on `PATH` (verifier subprocess; CGO bridge for the canonicalizer).
- Z3 on `PATH` (Tier 3 only).

## 3. Install

```bash
cargo install --path implementations/rust/sugar-cli
sugar verify-protocol

cd implementations/go && go build ./...
```

The Go kit lives at [implementations/go/sugar-ir-symbolic](../../implementations/go/). The canonicalizer matches the Rust implementation byte-for-byte.

## 4. Author or lift

In v1.2, the `validate:` struct tag walker:

```go
type User struct {
    Email string `validate:"required,email"`
    Age   int    `validate:"gte=0,lte=150"`
}
```

```bash
sugar-lift-go
```

In v1.1, author directly through the kit's IR API. See [implementations/go/sugar-ir-symbolic/examples/](../../implementations/go/) for sample authoring.

## 5. Verify

```bash
sugar prove
```

## 6. Wire your IDE

- **IDE:** Go LSP plugin planned.

## What's next

- [docs/how-to/cross-domain-bridges.md](../how-to/cross-domain-bridges.md).
- [docs/reference/per-adapter-coverage.md](../reference/per-adapter-coverage.md).
- [docs/explanation/thesis.md](../explanation/thesis.md).

---

*This tutorial is a stub. Major gap: the v1.2 lift adapters and `sugar-lift-go` invocation are not yet shipping. The v1.1 authoring path through the kit API needs a worked example.*
