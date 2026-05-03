# polyglot-rust-go

Demonstrates ProvekIt's cross-language predicate-level correctness verification: a Go program calls a Rust function via cgo, the linker derives a bridge for the call edge, and either emits a `linker-error` memento (when the caller has no post-condition establishing the callee's precondition) or produces a clean link bundle (when the caller satisfies the obligation). The `link-bundle.json` artifact is content-addressed and byte-identical across consecutive runs over the same source.

## Generated link bundles (committed)

Both fixtures have a checked-in `link-bundle.json` produced by `provekit link`. These are sample outputs you can read without running the toolchain. The `linkBundleCid` field in each is the content-addressed identity of that bundle; recomputing it locally from the same source must produce the same value (cross-machine determinism is a substrate guarantee per manifesto §11).

- `fixture-fail/link-bundle.json` — failure case with `linker-error` mementos. Today the cgo resolver emits `unresolved-symbol` because the Go caller's preamble (`extern int process(int n);`) lacks a kit-mapping signal (no `#include "rust*.h"` header reference); a follow-up will tune the fixture's preamble so the resolver maps to `rust-kit:process` and the failure shape becomes `unprovable-obligation` (the more interesting demo). The bundle as committed shows real linker behavior on the current fixture.
- `fixture-ok/link-bundle.json` — success case with no linker errors and an empty bridge set (the success-case Go file has no cgo call; everything happens in pure Go).

## Regenerate

```sh
make build-rust          # builds the provekit CLI
go build -o /tmp/provekit-lsp-go ./implementations/go/cmd/provekit-lsp-go
PATH="/tmp:$PATH" ./implementations/rust/target/release/provekit link examples/polyglot-rust-go/fixture-fail/
PATH="/tmp:$PATH" ./implementations/rust/target/release/provekit link examples/polyglot-rust-go/fixture-ok/
```

The CLI writes `link-bundle.json` into each fixture directory.
