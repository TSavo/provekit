# Fixture 06: expected roundtrip properties

Chain: Python -> Java -> Rust -> Python

## Must hold after chain (per R14.5)

1. Final Python source defines a function named exactly `factorial` (not `fn_0`, `f`, or any mangled form).
2. Final Python source defines a function named exactly `sum_squares`.
3. Final Python source defines a function named exactly `is_even`.
4. `factorial(5)` returns `120`; `sum_squares(4)` returns `30`; `is_even(6)` returns `True`; `is_even(7)` returns `False`.
5. At every bind step: `fn_name_sugar` is present in the wire-format payload alongside citations.
6. At every lower step: `fn_name_sugar` is recovered to populate the realize-request's user-visible name.
7. CID canonical bytes strip both `function` field and `fn_name_sugar` -- so CID is name-independent and stable.
8. A name-mangled intermediate (e.g., Rust or Java function has a language-specific casing) is acceptable in intermediate hops, provided the final Python output restores the original name.
9. Loss-record entries for naming conventions (e.g., snake_case vs camelCase intermediate) are acceptable and expected.
10. No `CompositionRefusalMemento` due to name conflicts.

## R14.5 reference

- Ruling: `docs/plans/2026-05-17-realization-tag-kinds-and-marketplace-ruling.md` section 2.5
- Implementation: PR #1153 (bind/lower thread fn_name through wire citations as fn_name_sugar)
- The split: CID-canonical strips both `function` and `fn_name_sugar`; wire-format strips only `function`.
