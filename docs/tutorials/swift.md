# Tutorial: Swift

> **Status:** kit shipping via PR #76. Bridge IR v1.1.0 9-field shape supported. Self-contracts package, libs, lift adapters, embedded verifier, and LSP plugin all planned (deferred until kit accumulates a runtime surface beyond conformance). Verification via the Rust CLI.

A walkthrough for Swift developers. **v1.1 is the conformance kit; lift adapters and authoring ergonomics are planned.**

## 1. What you'll have at the end

In v1.1: a `.proof` file authored directly through `Sources/Provekit/IR.swift`. The conformance harness at `Sources/ConformanceRunner/main.swift` validates byte-identical emission against the canonical Rust output for `eq_atomic`, `pattern1_bounded_loop`, `contract_decl`, `bridge_decl`.

In a later release: a `.proof` file lifted from Swift property wrappers and macros (Swift 5.9+).

## 2. Prerequisites

- Swift 5.9+ toolchain.
- Rust toolchain on `PATH` (verifier).
- Z3 on `PATH` (Tier 3 only).

## 3. Install

```bash
cargo install provekit
provekit verify-protocol

cd implementations/swift && swift build
```

## 4. Author

In v1.1, author directly through `IR.swift`. The kit provides IR types, JCS canonical JSON via `Jcs.encode`, and BLAKE3-512 hashing.

```swift
import Provekit

let formula = IrFormula.atomic(...)
let cid = try IR.canonicalize(formula).hash()
```

## 5. Verify

```bash
provekit prove
```

## 6. Wire your IDE and CI

- **IDE:** Swift LSP plugin planned.
- **CI:** see [docs/how-to/ci-integration/github-actions.md](../how-to/ci-integration/github-actions.md).

## What's next

- [docs/how-to/cross-domain-bridges.md](../how-to/cross-domain-bridges.md).
- [docs/explanation/thesis.md](../explanation/thesis.md).

---

*This tutorial is a stub. Major gaps: lift adapters, self-contracts package, embedded verifier, LSP plugin, decorator macros all not yet shipping. The kit today is a conformance kit, not yet a productivity kit.*
