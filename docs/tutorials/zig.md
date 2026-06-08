# Tutorial: Zig

> **Status:** kit shipping in the current v1.6.3 tree. Lift adapter: comment-based annotations (`//sugar:contract`, `//sugar:implement`, `//sugar:verify`). LSP plugin shipping. Verification via the Rust CLI.

A walkthrough for Zig developers. Zig has no native attribute syntax, so the lift adapter walks comment-block conventions.

## 1. What you'll have at the end

- A `.proof` file alongside your Zig package.
- Mementos derived from `//sugar:contract` and friends.
- LSP-driven squigglies in your editor.

## 2. Prerequisites

- Zig 0.13+ (`std.crypto.blake3` is a build-time dependency).
- Rust toolchain on `PATH` (verifier subprocess).
- Z3 on `PATH` (Tier 3 only).

## 3. Install

```bash
cargo install --path implementations/rust/sugar-cli
sugar verify-protocol

cd implementations/zig && zig build
```

## 4. Annotate

```zig
//sugar:contract pre="x >= 0" post="result >= x"
fn add_one_or_more(x: i32) i32 {
    return x + 1;
}
```

Run the lifter:

```bash
sugar-lift-zig
```

The Zig kit emits JCS canonical IR using `std.crypto.blake3` natively, producing identical hashes to the Rust kit.

## 5. Verify

```bash
sugar prove
```

## 6. Wire your IDE

- **IDE:** install the LSP plugin (`sugar-lift-zig --rpc` implements the NDJSON LSP plugin protocol). See [docs/how-to/ide-integration/](../how-to/ide-integration/).

## What's next

- [docs/how-to/cross-domain-bridges.md](../how-to/cross-domain-bridges.md).
- [docs/reference/per-adapter-coverage.md](../reference/per-adapter-coverage.md).
- [docs/explanation/thesis.md](../explanation/thesis.md).

---

*This tutorial is a stub. Known gaps: end-to-end runnable example, libs (embedded verifier in Zig) under evaluation.*
