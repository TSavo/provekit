package ir

// MarshalDeclarations serializes a slice of Declarations to JSON in the
// same field order as the TypeScript kit, byte-for-byte for equivalent
// claims. HTML escaping is disabled to match JavaScript's default
// `JSON.stringify` behavior (so atomic predicates like `>` and `<` and
// strings containing `&` survive round-trip).
//
// The cross-kit equivalence contract: the IR data structure emitted by
// Go's symbolic primitives produces the same JSON as TS's, which feeds
// the AST canonicalizer the same input on both sides.
//
// Note: the canonical hash contract (per docs/specs/2026-04-29-ast-canonicalizer.md)
// is CBOR over the canonicalized FOL form, not raw IR JSON. JSON
// byte-equivalence here is a sanity proxy that the input to the
// canonicalizer matches across kits, not the load-bearing hash itself.
func MarshalDeclarations(decls []Declaration) ([]byte, error) {
	return encodeJSON(decls)
}
