# Tutorial: C

> **Status:** kit shipping in the current v1.6.3 tree. Lift adapters planned. Libs under evaluation (native C BLAKE3 binding planned; v1.1 delegated hashing to the Python `blake3` module via subprocess). Embedded verifier planned. LSP plugin planned. Verification via the Rust CLI.

A walkthrough for C developers. **v1.1 is the IR library and JCS canonical JSON emitter; lift adapters land in v1.2.**

## 1. What you'll have at the end

In v1.1: a `.proof` authored directly through the C IR library.

In v1.2: a `.proof` lifted from `assert.h` macros (partial coverage; `assert` discards conditional information at compile time, so the lift is best-effort).

## 2. Prerequisites

- C99-capable toolchain (clang or GCC).
- Python 3.12 with the `blake3` package (v1.1 hashing path).
- Rust toolchain on `PATH` (verifier).
- Z3 on `PATH` (Tier 3 only).

## 3. Install

```bash
cargo install --path implementations/rust/provekit-cli
provekit verify-protocol

cd implementations/c && make
```

## 4. Author

In v1.1, author through `implementations/c/provekit-ir`. The library provides IR types, a JCS canonical JSON emitter, and a BLAKE3-512 wrapper (Python subprocess for v1.1).

## 5. Verify

```bash
provekit prove
```

## 6. Wire your IDE and CI

- **IDE:** C LSP plugin planned.
- **CI:** see [content-addressed CI](../how-to/content-addressed-ci.md).

## What's next

- [docs/how-to/cross-domain-bridges.md](../how-to/cross-domain-bridges.md).
- [docs/explanation/thesis.md](../explanation/thesis.md).

---

*This tutorial is a stub. Major gaps: lift adapters, native BLAKE3 binding, embedded verifier, LSP plugin not yet shipping. The kit today is the IR substrate; ergonomic surface lands in v1.2.*
