import sugar


# A library author DECLARES that this function's body is a boundary they want
# a verifiable contract for. The verify-facing `python-contracts` surface gates
# its body-derived function-contract emission on this declaration (mirroring
# Go's `//sugar:boundary` pragma + AnnotatedOnly); the `python-bind` surface
# emits the library-sugar-binding-entry declaration catalog for it.
@sugar.boundary(concept="concept:mul")
def double(x: int) -> int:
    return x * 2
