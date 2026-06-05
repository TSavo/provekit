# numpy vendor: ship a `.proof` + a witness package

You are the numpy maintainer. You want to start shipping correctness: a signed
`.proof` of your surface and a witness package of your passing tests, so any
consumer can `verify` it — recomputing, trusting nothing.

`./run.sh` does exactly that, with **no code changes to numpy and no hand-written
shim**:

```
numpy.proof:  13M, 2909 sugar members          # all of numpy's python surface
witness: passed -> .provekit/witnesses/<cid>.witness
[pass] <cid>  (signature+content-address:package)
       oracle resolved via package; rust recomputed the CID and it matched
```

## The three legs

- **Sugar** — the universal lifter reads numpy's installed python source and lifts
  every module-level function as sugar (symbol = qualified path, e.g.
  `lib._function_base_impl.rot90`). **2909 functions, ~16s, one `.proof`.** No
  `@sugar` tag, no shim, no edits. Lean SourceMemento mode: the `.proof` carries
  CIDs + spans, not inline bodies — the body is resolved on demand from the
  installed numpy and recompute-verified. (`numpy.add` is a C ufunc with no python
  body; it is simply not among the python functions lifted. The thousands that are
  python all lift.)

- **Witness** — the pytest-witness kit RUNS a numpy-consumer test and
  content-addresses the run into a signed `WitnessMemento` (the `.proof` carries
  the signed pointer, zero body). The run body is written to a CID-named witness
  **package** (`<cid>.witness`), deployed separately — audit material, not ship
  material.

- **Verify (the consumer)** — all verification lives in the rust CLI. The kit
  oracle (python) is **untrusted**: over RPC it only RESOLVES the witness body;
  rust blake3's it itself and compares to the pinned CID. A body that does not
  recompute is refused, loudly — broken oracle (wrong content for the CID) vs
  drift (an honest re-run that differs) are distinguished.

## Scope (no silent caps)

This demo sugar-lifts **all** of numpy but runs **one** consumer test for the
witness. The witness flow is identical per test; point the pytest-witness surface
at more tests to scale toward "ship a witness for the whole suite."

## Run

```sh
./run.sh        # builds /tmp/numpy-witness-venv on first run
```
