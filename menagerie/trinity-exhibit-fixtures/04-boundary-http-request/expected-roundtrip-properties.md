# Fixture 04: expected roundtrip properties

Chain: Python -> Java -> Rust -> Python

## Must hold after chain

1. `concept:http-request` CID (`blake3-512:784dab96...`) is present in the Python lift-out.
2. The function call shape (one URL argument, returns status integer) is preserved structurally across all hops.
3. Library binding MAY differ per hop: Python uses `urllib.request`, Java may use `java.net.http`, Rust may use `reqwest`. This is expected and does not cause a gap -- the concept hub is the stable identity.
4. Loss-records at each hop record which library binding was chosen; these are expected non-empty.
5. Final Python output includes a `concept:http-request` citation comment or a realized `urllib.request` call that maps to the same hub CID.
6. No `CompositionRefusalMemento` -- the chain completes (possibly loudly-bounded-lossy) but not refused.
7. The `fetch_status` function name is preserved via `fn_name_sugar` (R14.5) through all three hops.
8. The return type (integer status code) is preserved; no silent coercion to string or boolean.
9. The URL parameter name `url` survives in the final Python output or is documented in the loss-record.
10. Effect signature includes IO at every hop (the http-request carries an IO effect).

## Concept reference

- `concept:http-request` CID: `blake3-512:784dab96537ebae452cba5fdbcf88e07395d5e0634099055008d819f21d0fb51930fc29877afda069cdf0c1ec893fba5de47b025717fd024919c687381baee43`
- Realize kit: `provekit-realize-python-requests` (Python side)
- HTTP receipt issues: #847, #848, #849

## Harness note

The test harness (to land under #1068's real-toolchain ruling) must start a local HTTP stub
server before invoking the chain. The `url` value should be parameterized as a harness
constant, not hardcoded.
