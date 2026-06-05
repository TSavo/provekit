# pytest-witness-dummy

Witnessed-correctness demo. The pytest-witness seat is the proofchain-native
correctness instrument: `k(I)=t` where pytest is `k`, the code is `I`, the
observed pass/fail is `t`.

```
# from this directory:
provekit mint  --project . --out .provekit/imports
provekit prove .
#   -> discharged: 1  (verdict: witnessed by recompute -- the test was re-run on
#                      the pinned code and the witness CID reproduced)

# break the implementation and re-prove (no re-mint):
sed -i '' 's/a + b/a + b + 1/' impl.py
provekit prove .
#   -> violations: 1  ("witness REFUSED by recompute: code CID mismatch")
```

`prove` reads the discharge command from this project's
`.provekit/lift/python-pytest-witness/manifest.toml` (`discharge_command` +
`witness_tool`) -- no env vars. A wrong implementation has a different code CID
and, when run, yields a `failed` outcome, so it can neither borrow nor mint a
passing witness.
