# python-urlsafe-seam — the CPython base64 marquee

The URL-safe confusion refuted **statically**, on an input the vendor never
tested, from the vendor's own source, zero vendor changes.

## Provenance

- Input: `b"provekit~seam"` — absent from `test.test_base64` (run.sh
  re-checks on every run and fails if the vendor ever adds it).
- Standard encoding: `cHJvdmVraXR+c2VhbQ==` (`+` at position 12).
- URL-safe encoding: `cHJvdmVraXR-c2VhbQ==` (`-` at position 12).
- The walked seam, `Lib/base64.py` (read from the running interpreter's
  stdlib, never imported/executed):
  `_urlsafe_encode_translation = bytes.maketrans(b'+/', b'-_')` and
  `urlsafe_b64encode(s): return b64encode(s).translate(...)`.

`b64encode` delegates to C (`binascii`) — honestly unwalkable, refused by
name. The seam doesn't need it: translate is total, so urlsafe output never
contains `+` or `/`. The lifter value-pins the table, stability-scans the
binding, gates swap-shaped tables (forbidden = from − to), checks the
universe against the vendor's own vectors (∀⊨sample over `test.test_base64`),
and conjoins `str.chars-not-in-set(subject, "+/")` into the callsite's
`#euf#` assertion.

## Twins

- `good/`: asserts the correct urlsafe value → consistency **discharged**.
- `bad/`: asserts the standard-alphabet value (the confusion) → **unsatisfied**
  (z3 unsat). No point row could catch this — the vendor never tested the
  input; only the universe convicts.

The verdict flip is the vacuity witness: a universe that never met the
equality would let both twins discharge.

Verdicts are parsed from real `.verify.json` consistency rows; exit codes
are never trusted.
