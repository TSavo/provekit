# Fixture 05: expected roundtrip properties

Chain: Python -> Java -> Rust -> Python

## Must hold after chain

1. Final Python source compiles and runs without error.
2. `asyncio.run(main())` produces `2` in the final Python output.
3. The async effect (`eff_io` + async/await surface) is present in the Python lift-out effect signature.
4. The effect signature (`effsig_io` in the Python specs catalog) survives into the hub representation.
5. Java hop: if Java async (CompletableFuture / virtual threads) is not realized, the hop
   produces a loudly-bounded-lossy output with an explicit `LossRecord` entry naming
   the async-to-blocking translation. This is acceptable; the chain does NOT refuse.
6. Rust hop: Rust async (tokio/async-std) is a valid realization path; OR blocking translation
   with loss-record entry is acceptable.
7. Final Python output either (a) preserves async/await shape exactly, or (b) translates to
   equivalent synchronous form with a chain-level LossRecord entry naming the async-drop.
8. In case (b): the synchronous final output still produces `2` with identical observable behavior.
9. No `CompositionRefusalMemento` -- effect-subset relaxation rule applies: if the target
   language effect set is a subset of the concept effect set, the morphism is discharged.
10. The `fetch_value` and `main` function names are preserved via `fn_name_sugar` (R14.5).

## Effect reference

- `eff_io.spec.json` in `menagerie/python-language-signature/specs/`
- `effsig_io.spec.json` in the same directory
- Effect-subset relaxation rule: documented in `menagerie/concept-shapes/transport-gaps.md`
- `provekit-realize-python-aiosqlite` is the async Python realize kit (aiosqlite pattern)
