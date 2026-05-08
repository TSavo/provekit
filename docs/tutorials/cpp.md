# Tutorial: C++

> **Status:** kit + canonicalizer + libs shipping in the current v1.6.2 tree. Lift adapter for C++26 `[[expects:]]` and `[[ensures:]]` planned. `assert.h` walk under evaluation (partial coverage; the macro discards conditional information at compile time). Embedded verifier shipping. LSP plugin planned.

A walkthrough for C++ developers. **v1.1 is the kit; the C++26 contracts lift adapter lands in v1.2.**

## 1. What you'll have at the end

In v1.2: a `.proof` file lifted from your `[[expects:]]` and `[[ensures:]]` attributes.

In v1.1: a `.proof` file authored directly through the C++ kit (header-only IR library).

## 2. Prerequisites

- A C++20-capable toolchain (clang preferred, GCC supported).
- OpenSSL 3.
- nlohmann-json (`brew install nlohmann-json` / `apt install nlohmann-json3-dev`).
- Rust toolchain on `PATH` (verifier).
- Z3 on `PATH` (Tier 3 only).

## 3. Install

```bash
cargo install --path implementations/rust/provekit-cli
provekit verify-protocol

cd implementations/cpp && make
```

The C++ kit lives at [implementations/cpp/provekit-ir-symbolic](../../implementations/cpp/). It's header-only with CMake integration.

## 4. Author or lift

In v1.2 with C++26 contracts:

```cpp
int add_one_or_more(int x)
    [[expects: x >= 0]]
    [[ensures r: r >= x]]
{
    return x + 1;
}
```

```bash
provekit-lift-cpp
```

In v1.1, author directly through the kit. See [implementations/cpp/](../../implementations/cpp/) for sample CMake integration.

## 5. Verify

```bash
provekit prove
```

## 6. Wire your IDE and CI

- **IDE:** C++ LSP plugin planned.
- **CI:** see [content-addressed CI](../how-to/content-addressed-ci.md).

## What's next

- [docs/how-to/cross-domain-bridges.md](../how-to/cross-domain-bridges.md).
- [docs/reference/per-adapter-coverage.md](../reference/per-adapter-coverage.md).
- [docs/explanation/thesis.md](../explanation/thesis.md).

---

*This tutorial is a stub. Major gaps: the v1.2 lift adapter for C++26 contracts is not yet shipping; Boost.Hana metaprograms and Boost.Contract under evaluation; v1.1 authoring path needs a worked example.*
