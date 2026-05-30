# BZ-SHAPE-007: Polyglot Link Obligation

This specimen promotes the existing Rust <-> Go polyglot linker smoke into
Bug Zoo.

The lab shows the ordinary host shape: a caller can pass a value through normal
Go code and the host test still passes. The exhibit crosses a language
boundary: Go calls a Rust `extern "C"` function whose lifted contract requires
`n > 0`. Because the Go caller has no post-condition establishing that
precondition, the checked-in LinkBundle records a `linker-error` with
`errorKind: unprovable-obligation`.

The fixed specimen keeps the same cross-kit cgo call. The Go caller adds a
scoped post-condition, `post=n>0`, so the checked-in fixed LinkBundle carries
the Go-to-Rust bridge and records zero linker errors.
