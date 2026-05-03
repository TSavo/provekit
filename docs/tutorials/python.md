# Tutorial: Python

> **Status:** kit shipping (v1.1.0). Lift adapter shipping: `pydantic`. Layer-2 structural lift shipping (walks pytest/unittest with bounded loops, helper inlining, `@pytest.mark.parametrize`). Decorator macro shipping: `@provekit.contract`. LSP plugin shipping. Verification via the Rust CLI.

A walkthrough for Python developers. By the end you have a `.proof` catalog lifted from existing `pydantic.BaseModel` schemas (or pytest tests), verified via the Rust CLI, with red squigglies in your editor via the LSP plugin.

## 1. What you'll have at the end

- A `.proof` file alongside your Python package.
- Mementos derived from your existing `pydantic` `Field` constraints, pytest tests, or `@provekit.contract` decorators — no parallel spec.
- A handshake report from `provekit prove`.
- LSP-driven squigglies in your editor on contract violations.

## 2. Prerequisites

- Python 3.12+.
- Rust toolchain on `PATH` (verifier subprocess).
- Z3 on `PATH` (Tier 3 only).

## 3. Install

```bash
# the canonical verifier (Rust CLI)
cargo install provekit
provekit verify-protocol

# the Python kit
pip install provekit
```

The Python kit lives at [implementations/python/](../../implementations/python/). The canonicalizer is pure Python and byte-identical to the Rust canonicalizer for all conformance tests.

## 4. Lift your first contract

If your codebase already uses `pydantic`:

```python
from pydantic import BaseModel, Field

class User(BaseModel):
    email: str = Field(..., pattern=r"^[^@]+@[^@]+\.[^@]+$")
    age: int = Field(..., ge=0, le=150)
```

Run the lift adapter:

```bash
provekit-lift-py
```

The lifter walks `BaseModel` field annotations, canonicalizes constraints into IR (the same IR that Bean Validation `@Min`/`@Max`/`@Pattern` produces), and emits a `.proof`.

**Layer-2 structural lift** also picks up your pytest test files: bounded loops, helper inlining, multi-assertion characterization, `@pytest.mark.parametrize` blocks become first-class IR.

For functions without an existing annotation library, author directly with `@provekit.contract`:

```python
from provekit import contract

@contract(pre="x >= 0", post="result >= x")
def add_one_or_more(x: int) -> int:
    return x + 1
```

## 5. Verify

```bash
provekit prove
```

Same handshake, same discharge breakdown shape as the [Rust tutorial step 4](rust.md#step-4-verify).

## 6. Wire your IDE and CI

- **IDE:** install the LSP plugin. See [docs/how-to/ide-integration/](../how-to/ide-integration/) for editor-specific wire-up. The plugin implements the ProvekIt NDJSON LSP plugin protocol.
- **CI:** see [docs/how-to/ci-integration/github-actions.md](../how-to/ci-integration/github-actions.md).

## What's next

- [docs/how-to/publishing-a-proof.md](../how-to/publishing-a-proof.md) — ship the `.proof` alongside your PyPI package.
- [docs/how-to/cross-domain-bridges.md](../how-to/cross-domain-bridges.md) — bind a Python implementation to a reference contract.
- [docs/reference/per-adapter-coverage.md](../reference/per-adapter-coverage.md) — what `pydantic` and Layer-2 lift see and miss.
- [docs/explanation/thesis.md](../explanation/thesis.md).

---

*This tutorial is a stub. Contributions welcome — see [docs/contributing/overview.md](../contributing/overview.md). Known gaps: end-to-end runnable example, LSP install per editor.*
