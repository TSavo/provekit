# polyglot-rust-go

Demonstrates ProvekIt's cross-language predicate-level correctness verification: a Go program calls a Rust function via cgo, the linker derives a bridge for the call edge, and either emits a `linker-error` memento (when the caller has no post-condition establishing the callee's precondition) or produces a clean link bundle (when the caller satisfies the obligation). The `link-bundle.json` artifact is content-addressed and deterministic across consecutive runs over the same source.

## Generated link bundles (committed)

Both fixtures have a checked-in `link-bundle.json` produced by `provekit link`. These are sample outputs you can read without running the toolchain. The `linkBundleCid` field in each is the content-addressed identity of that bundle; recomputing it locally from the same source must produce the same value (cross-machine determinism is a substrate guarantee per manifesto §11).

- `fixture-fail/link-bundle.json`: failure case with a `linker-error` memento. The Go caller's cgo preamble includes `rust_callee.h`, so the resolver maps `C.process(...)` to `rust-kit:process`; the linker finds the Rust callee contract and reports `unprovable-obligation` because the Go caller has no post-condition establishing `n > 0`.
- `fixture-ok/link-bundle.json`: success case with one derived Go-to-Rust bridge and no linker errors. The Go caller still crosses cgo, but its `//provekit:contract post=n>0` annotation binds the parameter scope and discharges the Rust callee's `#[requires(n > 0)]` precondition.

## Regenerate

```sh
make build-rust          # builds the provekit CLI
(cd implementations/go && go build -o /tmp/provekit-lsp-go ./cmd/provekit-lsp-go)
PATH="/tmp:$PATH" ./implementations/rust/target/release/provekit link examples/polyglot-rust-go/fixture-fail/
PATH="/tmp:$PATH" ./implementations/rust/target/release/provekit link examples/polyglot-rust-go/fixture-ok/
```

The CLI writes `link-bundle.json` into each fixture directory.
