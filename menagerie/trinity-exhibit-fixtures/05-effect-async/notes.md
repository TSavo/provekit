# Fixture 05: notes

No `concept:async` hub node exists in the current catalog. Async is captured via the Python
language-signature's `eff_io` / `effsig_io` effect spec files, not as a named hub concept.

This fixture therefore exercises the effect-propagation path, not concept-CID identity. The
key claim is that the eff_io effect signature in the Python lift-out survives into the
hub representation and is accounted for in each hop's loss-record rather than silently dropped.

`provekit-realize-python-aiosqlite` is the reference realize kit for async Python patterns.
The fixture uses `asyncio` stdlib rather than aiosqlite because the test value is in-memory;
the effect shape is identical.

If a future release mints `concept:async`, this fixture should be updated to assert hub-CID
identity for that concept and the notes.md updated accordingly.
