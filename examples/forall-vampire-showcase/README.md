# Forall Vampire Showcase

This showcase demonstrates the invariant 6 routing story: the verifier lifts a quantified first-order obligation and routes it to the configured first-order solver seat. The theorem is not implemented in Sugar.

Target obligation:

```text
((forall x y z. mul(mul(x, y), z) = mul(x, mul(y, z)))
 and (forall x. mul(e, x) = x)
 and (forall x. mul(inv(x), x) = e))
=> (forall x. mul(x, e) = x)
```

The GOOD row is a first-order algebra theorem. In the local gate, z3 times out on the exact SMT-LIB obligation, while Vampire returns `unsat` and `sugar verify` records:

```text
obligationClass = first-order
routedSolver = vampire
status = discharged
dischargingSolver = vampire@...
```

The BAD row is a false universal claim:

```text
forall x. must_hold(x)
```

It routes to the same first-order seat and is refused as `unsatisfied`. The script parses these fields from `.generated/.verify.json`.

Run:

```bash
examples/forall-vampire-showcase/run.sh
```
