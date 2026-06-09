# Python Body-Guard Precondition Showcase

This showcase proves the Python mirror of the Rust body-guard precondition
slice.

Claimed slice:

- Source: a real Python function body guard:

```python
def bounded_digit(x: int) -> int:
    if x < 2 or x > 36:
        raise ValueError("x out of range")
    return x
```

- Lift: the guard condition is lifted as the flat precondition
  `x >= 2 and x <= 36`.
- Verification: a GOOD caller using `bounded_digit(16)` discharges; a BAD
  caller using `bounded_digit(1)` is refused at the precondition seam.
- Federation guard: the script asks the Python lifter and the Rust lifter for
  the equivalent `x >= 2 and x <= 36` precondition and asserts their canonical
  FOL CIDs are identical.

Not claimed: non-flat guards, guards with `else`, guards that call helper
predicates, attribute/subscript guards, exception semantics, control-flow
semantics, or effects. The raise is only the syntactic marker for the rejected
branch.

Run:

```sh
examples/python-bodyguard-precondition/run.sh
```

The script leaves real receipts in `.work/good/.verify.json` and
`.work/bad/.verify.json`, plus the CID comparison in `.work/federation.json`.
