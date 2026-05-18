# Fixture 04: notes

`concept:http-request` is a boundary contract: its hub CID is stable but its realization is
library-bound per language. This fixture specifically tests that the concept identity survives
library substitution across three hops.

`provekit-realize-python-requests` is the Python-side realize kit. The fixture uses stdlib
`urllib.request` rather than the `requests` library because `requests` is not guaranteed
installed in all test environments; the concept hub CID is the same regardless of which Python
HTTP library is used.

The HTTP trinity receipt (#847, #848, #849) is v0 loudly-bounded-lossy as of 2026-05-17.
This fixture is intentionally written to work within that constraint: it expects
non-empty loss-records and does NOT assert byte-equivalence at the call-site level,
only at the concept CID level.
