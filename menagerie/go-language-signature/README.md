# Go Language Signature

Draft Go source-language signature for `provekit-lift-go`.

The operation algebra is explicitly namespaced with `go:` operation names. The
source lifter emits `function-contract` mementos over this algebra and a
lossless `go:source-unit(bytes, operational_term)` wrapper whose byte slot is a
hex-encoded copy of the original Go source.

Version: `0.1.0-draft`. This is an honest subset: unsupported Go syntax is
reported as a refusal rather than lowered to an unknown operation.

## Operations

The catalog covers arithmetic, comparisons, logical short-circuiting, bitwise
operators, assignment/declaration, sequencing, calls, indexing, member access,
pointer address/deref, loops as opaque-loop effects, and `source-unit`.

## Effects

Effects follow the canonical `Effect` JSON wire shapes used by
`libprovekit::compose`: `reads`, `writes`, `io`, `unsafe`, `panics`,
`unresolved_call`, and `opaque_loop`.
