# provekit-emit-python-pytest

A PEP 1.7.0 **emitter** kit (a `realize` plugin whose target framework is
`pytest`). It takes a contract — its neutral predicates plus the target
function signature — and emits a pytest test module whose `test_*` functions
assert the contract's predicates.

This is the python sibling of `provekit-emit-java-junit` (PR-6) and the inverse
of the python harvester `provekit-lift-py-tests`: the harvester lifts pytest
assertions back into neutral predicates; this emitter materializes neutral
predicates as native pytest assertions.

## The mapping is inline python

The predicate -> assertion mapping lives in
`src/provekit_emit_python_pytest/predicate_table.py`, written in python. The
fact that `concept:eq` spells as `assert a == b` is **python framework
knowledge**, not substrate data. There is **no** catalog memento family for this
mapping and **no** catalog read for the framework spelling.

(An earlier attempt, PR-5 / #1401, tried to externalize the mapping into a
`JUnitAssertionTemplateMemento` catalog family. That was closed as an
architectural mistake. See issue #1403.)

What the kit reads from the catalog: the neutral concept name (`concept:eq`,
etc.), to know *which* predicate it is emitting. The framework spelling stays in
the kit.

## Supported predicates

| neutral predicate      | emitted pytest assertion              |
| ---------------------- | ------------------------------------- |
| `concept:eq(a, b)`     | `assert a == b`                       |
| `concept:ne(a, b)`     | `assert a != b`                       |
| `concept:lt(a, b)`     | `assert a < b`                        |
| `concept:gt(a, b)`     | `assert a > b`                        |
| `concept:le(a, b)`     | `assert a <= b`                       |
| `concept:ge(a, b)`     | `assert a >= b`                       |
| `concept:option-is-some(x)` | `assert x is not None`           |
| `concept:option-is-none(x)` | `assert x is None`               |
| `concept:fallible-err(x)`   | `with pytest.raises(Exception): x()` |

Predicates the kit cannot spell are **not** emitted as vacuously-passing tests;
they are recorded as `unsupported_predicates` (an honest emit-assertion-gap) so
the substrate can account for them.

Each emitted `test_*` function declares per-predicate placeholder locals for the
free variables it references, chosen so the assertion **passes** when run
standalone under pytest (e.g. `lt` gets operands `0` then `1`; `option-is-some`
gets `object()`). The contract is about the *shape* of the assertion, not
concrete runtime values.

## RPC interface (PEP 1.7.0, newline-delimited JSON-RPC over stdio)

```
provekit-emit-python-pytest --rpc
```

Methods:

- `provekit.plugin.describe` — returns a full PEP 1.7.0 plugin memento
  `{envelope, header, metadata}` (NOT a flat object). The loader
  (`provekit-plugin-loader`) recomputes `header.cid` per §6.1 and refuses on
  mismatch, so the kit mints the CID itself (see `plugin_memento.py`). The kit's
  capability summary + supported predicates live in `header.content` (opaque to
  the loader). This is what lets `provekit verify`/`materialize` load the kit
  through the standard plugin loader and invoke it to emit pytest gates.
- `provekit.plugin.invoke` — emit a pytest module from an emit plan in `params`.
- `provekit.plugin.shutdown` — exit.

`describe` result (plugin memento, abbreviated):

```json
{
  "envelope": {"declaredAt": "...", "signature": "ed25519:...", "signer": "ed25519:..."},
  "header": {
    "cid": "blake3-512:...",
    "content": {"name": "provekit-emit-python-pytest", "kind": "realize",
                "target_framework": "pytest",
                "capabilities": {"predicates": ["concept:eq", "..."]}},
    "critical": false,
    "kind": "realize",
    "protocol_versions": ["pep/1.7.0"],
    "provenance_cid": "blake3-512:...",
    "schemaVersion": "1",
    "version": "0.1.0"
  },
  "metadata": {"maintainer": "...", "note": "...", "source_url": "..."}
}
```

Signing is a placeholder (zero-bytes ed25519); full signature verification is
the loader-integration follow-up (§12 out-of-scope for the loader skeleton).

`invoke` params:

```json
{
  "contract_id": "concept:clamp",
  "function": "clamp",
  "params": ["x", "lo", "hi"],
  "param_types": ["int", "int", "int"],
  "predicates": [
    {"kind": "op", "name": "concept:ge",
     "args": [{"kind": "var", "name": "x"}, {"kind": "var", "name": "lo"}]},
    {"kind": "op", "name": "concept:le",
     "args": [{"kind": "var", "name": "x"}, {"kind": "var", "name": "hi"}]}
  ]
}
```

`invoke` result:

```json
{
  "kind": "pytest-test-emission",
  "source": "def test_verifies_ge_0():\n    ...",
  "extension": "py",
  "emitted_artifact_cid": "blake3-512:...",
  "emitted_predicates": ["ge", "le"],
  "unsupported_predicates": [],
  "is_complete": true
}
```

## Tests

```
cd implementations/python/provekit-emit-python-pytest
python3 -m pytest
```

The end-to-end tests emit a module for each supported predicate, write it to a
temp file, and run `pytest` on it in a subprocess — verifying the emitted tests
run **green**, not merely that they parse.
