# Research: Python LSP plugin entry and ForwardPropagator wiring

## Scope

This note documents the current state of the Python LSP plugin in the Sugar monorepo, its entry point, existing infrastructure, and what the ForwardPropagator (#320) will reuse. The goal is to provide enough context for a low-context agent to implement #320 without further investigation.

## Findings

### Python LSP plugin: EXISTS at `provekit-lift-py-tests/src/provekit_lift_py_tests/lsp.py`

The Python LSP plugin already exists as a module within the `provekit-lift-py-tests` package. It implements the full NDJSON-over-stdio LSP protocol with three methods: `initialize`, `parse`, and `shutdown`.

```python
# implementations/python/provekit-lift-py-tests/src/provekit_lift_py_tests/lsp.py:211-237
def main() -> None:
    """Run the LSP plugin main loop (NDJSON over stdio)."""
    while True:
        msg = _recv()
        if msg is None:
            break
        msg_id = msg.get("id")
        method = msg.get("method")
        params = msg.get("params", {})

        if method == "initialize":
            handle_initialize(msg_id)
        elif method == "parse":
            handle_parse(msg_id, params)
        elif method == "shutdown":
            handle_shutdown(msg_id)
        else:
            _send(
                {
                    "jsonrpc": "2.0",
                    "id": msg_id,
                    "error": {
                        "code": -32601,
                        "message": f"method '{method}' not found",
                    },
                }
            )
```

Entry point: `python -m provekit_lift_py_tests.lsp` or `from provekit_lift_py_tests.lsp import main; main()`.

**Gap: no standalone binary.** Unlike Go (`cmd/provekit-lsp-go/main.go`) and PHP (`provekit-lift/src/lspd.php`), the Python LSP has no dedicated binary entry point. It lives as a module inside the test package. For the ForwardPropagator to spawn it as a subprocess, a thin binary wrapper is needed at `implementations/python/bin/provekit-lsp-python`:

```python
#!/usr/bin/env python3
import sys, os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'provekit-lift-py-tests', 'src'))
from provekit_lift_py_tests.lsp import main
main()
```

### Go LSP reference pattern: `cmd/provekit-lsp-go/main.go`

The Go kit uses a dedicated `cmd/` directory with a `main.go` that reads NDJSON from stdin and dispatches to handlers:

```go
// implementations/go/cmd/provekit-lsp-go/main.go:67-96
func main() {
    scanner := bufio.NewScanner(os.Stdin)
    for scanner.Scan() {
        if !handleRequest(string(scanner.Bytes())) {
            return
        }
    }
}

func handleRequest(line string) bool {
    var req rpcRequest
    if err := json.Unmarshal([]byte(line), &req); err != nil {
        return true
    }
    switch req.Method {
    case "initialize":
        handleInit(req.ID)
    case "parse":
        handleParse(req.ID, req.Params)
    case "shutdown":
        handleShutdown(req.ID)
        return false
    default:
        sendError(req.ID, -32601, fmt.Sprintf("unknown method: %s", req.Method))
    }
    return true
}
```

### PHP LSP reference pattern: `provekit-lift/src/lspd.php`

The PHP kit uses a single-file daemon (`lspd.php`) with manual require-once imports:

```php
// implementations/php/provekit-lift/src/lspd.php:1-18
<?php
declare(strict_types=1);

require_once __DIR__ . '/../provekit-ir-symbolic/src/Ir/Term.php';
require_once __DIR__ . '/../provekit-ir-symbolic/src/Ir/Formula.php';
require_once __DIR__ . '/../provekit-ir-symbolic/src/Ir/Declaration.php';
require_once __DIR__ . '/../provekit-ir-symbolic/src/Canonicalizer/Blake3.php';
require_once __DIR__ . '/../provekit-ir-symbolic/src/Canonicalizer/Jcs.php';

use Sugar\Ir\{ContractDecl, BridgeDecl, Collector};
use Sugar\Canonicalizer\{Blake3, Jcs};
```

### Self-contracts: contract JSON shape

The Python self-contracts orchestrator (`provekit-self-contracts.py`) produces `ContractDecl` objects that serialize to JSON with the following shape:

```python
# implementations/python/provekit-self-contracts.py:115-145
def slab_blake3() -> List[ContractDecl]:
    s = make_var("s")
    h_str = lambda x: ctor("blake3_512_of", [x])
    return [
        ContractDecl(
            name="python_blake3_512_of_total_length_eq_139",
            out_binding="out",
            post=eq(
                ctor("string_length", [h_str(s)]),
                num(139),
            ),
        ),
        ContractDecl(
            name="python_blake3_512_of_is_deterministic",
            out_binding="out",
            post=eq(h_str(s), h_str(s)),
        ),
    ]
```

The `ContractDecl` class (from `ir.py`) serializes to JSON with fields: `kind`, `name`, `outBinding`, `pre`, `post`. The ForwardPropagator will consume these as the contract index for call-edge resolution.

### Existing Python IR infrastructure

The `provekit-lift-py-tests` package already provides:

| Module | Purpose |
|--------|---------|
| `ir.py` | `ContractDecl`, `BridgeDecl`, `formula_to_value`, `declarations_to_value`, `call_edges_to_value` |
| `canonicalizer.py` | `encode_jcs`, `jcs_hash` (JCS + BLAKE3-512) |
| `layer2.py` | `lift_file_layer2` (pytest/unittest structural lift) |
| `decorators.py` | `collect_module` (@provekit.contract decorator collection) |
| `lift/pydantic.py` | `lift_pydantic_model` (Pydantic BaseModel lift) |
| `cpython_ctypes_resolver.py` | `resolve_ctypes_calls` (ctypes call-edge resolution) |
| `lsp.py` | NDJSON LSP plugin (initialize/parse/shutdown) |

## Conventions

- Python kit lives at `implementations/python/` with flat layout: `provekit-self-contracts.py` at root, `provekit-lift-py-tests/` as a Python package under `src/`.
- No `pip install -e .` is used; the kit relies on `sys.path` injection. The `mint-python` Makefile target does not run a build step.
- Test runner: `pytest` from within `provekit-lift-py-tests/`. Tests live in `tests/` and use `conftest.py` for fixtures.
- Per `pyproject.toml`, the runtime requirement is `requires-python = ">=3.8"`. The `__pycache__` directory observed during local development reflects whichever 3.x interpreter the developer ran, not a project requirement.
- The LSP module uses `from __future__ import annotations` and type hints throughout.
- NDJSON protocol: one JSON object per line, `\n` terminated, flushed after each write.

## Open questions

1. **Should the LSP binary live at `bin/provekit-lsp-python` or as a new package `provekit-lsp-python/`?**

   Proposed: `bin/provekit-lsp-python` as a thin wrapper script. This mirrors the existing `bin/mint-python-self-contracts` pattern and avoids creating a new package directory. The ForwardPropagator can spawn it directly.

2. **Does the LSP module need a `__main__.py` for `python -m` invocation?**

   Proposed: add `provekit-lift-py-tests/src/provekit_lift_py_tests/__main__.py` that imports and calls `lsp.main()`. This enables `python -m provekit_lift_py_tests` as an alternative entry point.

3. **What ForwardPropagator methods does the Python LSP need to support beyond `parse`?**

   The current `lsp.py` only supports `initialize`, `parse`, and `shutdown`. The ForwardPropagator spec (#308) may require additional methods like `forwardPropagate` or `updateContracts`. These should be added to `lsp.py` when #320 is implemented. See `implementations/go/cmd/provekit-lsp-go/main.go::handleRequest` (lines 78-95) for the RPC dispatch pattern.
