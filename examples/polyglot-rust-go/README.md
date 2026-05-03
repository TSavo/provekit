# polyglot-rust-go

Demonstrates ProvekIt's cross-language predicate-level correctness verification: a Go program calls a Rust function via cgo, the linker derives a bridge for the call edge, and either emits a `linker-error` memento (when the caller has no post-condition establishing the callee's precondition) or produces a clean link bundle (when the caller satisfies the obligation). The `link-bundle.json` artifact is content-addressed and byte-identical across consecutive runs over the same source.
