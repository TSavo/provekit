# numpy showcase showdown

All the verbs over **one** operation, `numpy.add` — the full sugar+contract
lifecycle, kit-side, with the `.proof` as the transport and rust proof-blind.

```sh
./run.sh
```

## The cast

- `.provekit/imports/*.proof` — the **numpy sugar `.proof`** (the transport).
  Carries `numpy.add`'s two faces: `body_text` (materialize) + `ast_template`
  (recognize), symbol-keyed, concept-free.
- `test_numpy_add.py` — numpy's **own** testing vocabulary (`numpy.testing`).
  Two lifters read it: the **consistency** seat (numpy.testing) and the
  **witness** seat (pytest-witness, which *runs* it).
- `boundary.py` — a `@boundary(library="numpy", call="add")` stub for materialize.
- `app.py` — production `np.add` (aliased) for recognize.

## The showdown

| verb | what it does | result |
|---|---|---|
| **lift** | sugar `.proof` + numpy.testing mints the CONTRACT + pytest-witness RUNS the test for the WITNESS | one `.proof` |
| **materialize** | `@boundary(numpy.add)` body ← sugar `body_text` | `return numpy.add(x, y)` |
| **recognize** | `np.add` in `app.py` found from the sugar `.proof`, alias-resolved, anywhere | tag `numpy.add` |
| **prove** | the contract discharges **two ways** | **consistent** (z3) AND **witnessed** (recompute) |

## The degenerate case (contracts contradict)

`np.add(2,3)` asserted `== 5` **and** `== 6` is refused **both** ways:
- **consistency**: the spec contradicts itself → z3 UNSAT → refused.
- **witness**: when actually run, `numpy.add(2,3)` is `5`, so the `== 6`
  assertion *fails* → witness outcome `failed` → refused by recompute.

That is the whole correctness claim, on a real library operation: *the spec is
consistent, AND the witness matches the spec.*

## Environment

The witness lifter **runs numpy's test**, so it needs `numpy` + the kit deps in
a venv (PEP 668: never `--break-system-packages`). `run.sh` provisions
`/tmp/numpy-witness-venv` and points the witness manifest's `command` /
`discharge_command` at it.
