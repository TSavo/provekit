package ir

// MarshalDeclarations serializes a slice of Declarations to v1.1.0
// IR-JSON: top-level `{kind:"contract", name, outBinding, pre?, post?,
// inv?}` for contracts, plus `{kind:"bridge", ...}` for bridges. HTML
// escaping is disabled to match the C++ reference kit's emitter and
// JavaScript's default JSON.stringify behavior (so atomic predicates
// like `>` and `<` and strings containing `&` survive round-trip).
//
// The cross-kit equivalence contract: the IR emitted by Go's symbolic
// primitives produces JSON byte-equivalent to the C++ reference kit's
// output for the same logical claim. Both feed the same JCS canonical
// bytes into the contract-memento body and hash to the same propertyHash.
func MarshalDeclarations(decls []Declaration) ([]byte, error) {
	return encodeJSON(decls)
}
