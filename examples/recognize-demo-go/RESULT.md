# recognize-demo-go - Go recognize verb pilot

This module is the Go-side peer of `examples/voltron-demo/` for the
recognize verb. It keeps the shim local to the demo:

- `internal-shim-stdlib-http/shim.go` declares two Go sugar functions with
  `//sugar:sugar(...)`.
- `internal-shim-stdlib-http/blake3-512:095398490c8c99e21c0db81e954681cd98094d38a2dc2b3f2bfbdb3bf2a85fc41e80534cdbad220878bea4c12cb993bfda918729a30ea7a43b45d8c830b3f075.proof`
  is the minted binding source.
- `pkg/ingest/ingest.go` and `pkg/persist/persist.go` are user code. Their
  bodies are alpha-equivalent to the shim bodies after the Go Phase A
  canonicalizer normalizes parameter names.

The current Go authoring lifter refuses two-result function signatures, so the
shim uses single-result wrappers:

```go
return mustResponse(http.Get(url))
return mustDB(sql.Open(driverName, dataSourceName))
```

The user functions use the same body shape with different parameter names:

```go
return mustResponse(http.Get(target))
return mustDB(sql.Open(driver, dsn))
```

That keeps the recognized boundary at the real stdlib calls while matching the
kit capability that PR #1584 landed.

## Green path

The demo builds, runs, and passes tests end-to-end:

```text
$ go test ./...
?   	github.com/sugar/recognize-demo-go/cmd/recognize-demo-go	[no test files]
ok  	github.com/sugar/recognize-demo-go/internal/demoe2e	1.099s
?   	github.com/sugar/recognize-demo-go/internal/memsqlite	[no test files]
?   	github.com/sugar/recognize-demo-go/internal-shim-stdlib-http	[no test files]
ok  	github.com/sugar/recognize-demo-go/pkg/ingest	3.637s
ok  	github.com/sugar/recognize-demo-go/pkg/persist	1.945s

$ go run ./cmd/recognize-demo-go
recognize-demo-go round-trip: status=200 body={"user":"alice","type":"signup"}
```

The command path uses `http.Get` and `database/sql.Open`. To keep this demo
self-contained and CGO-free in an offline runner, `internal/memsqlite`
registers a small pure-Go `database/sql` driver as `sqlite` for the in-memory
round trip.

## Shim proof mint

The local shim proof was minted from this demo directory:

```text
$ sugar mint \
    --project /Users/tsavo/sugar-demo-go/examples/recognize-demo-go \
    --surface go-bind \
    --library-bindings \
    --out /Users/tsavo/sugar-demo-go/examples/recognize-demo-go/internal-shim-stdlib-http \
    --no-attest

dispatch: surface=`go-bind` plugin=`go-bind-lift` command=["go", "run", ".sugar/lift/go-bind/go-bind-rpc.go"]
ok: plugin `sugar-lift-go-verify` ready

  catalog CID:        blake3-512:095398490c8c99e21c0db81e954681cd98094d38a2dc2b3f2bfbdb3bf2a85fc41e80534cdbad220878bea4c12cb993bfda918729a30ea7a43b45d8c830b3f075
  contractSetCid:     blake3-512:d53d18c23212ea7b6300594bb89bce60218f6eff2b9d628b8cc42d3e79bbd5ab09994845815cc7185113418f9fc2edc7606b06f0d57a6d581e7cff5b290f3229
  proof bytes:        4224
  .proof file:        /Users/tsavo/sugar-demo-go/examples/recognize-demo-go/internal-shim-stdlib-http/blake3-512:095398490c8c99e21c0db81e954681cd98094d38a2dc2b3f2bfbdb3bf2a85fc41e80534cdbad220878bea4c12cb993bfda918729a30ea7a43b45d8c830b3f075.proof
```

## Recognizer pilot

`sugar recognize` resolves the recognizer through project config and the
surface manifest. The demo registers `go-bind` in `.sugar/config.toml` and
the executable route lives at `.sugar/lift/go-bind/manifest.toml`; the CLI
does not take or read shim proof paths.

```text
$ sugar recognize \
    --project /Users/tsavo/sugar-demo-go/examples/recognize-demo-go \
    --source pkg/ingest/ingest.go \
    --source pkg/persist/persist.go

dispatch: surface=`go-bind` sources=2
recognize: 2 tag(s) emitted
  [0] concept:http-get @ pkg/ingest/ingest.go:5 (fn=FetchURL, exact)
  [1] concept:sql-open @ pkg/persist/persist.go:9 (fn=OpenStore, exact)
```

Two tags from idiomatic user-code function bodies, derived purely from the
demo-local shim's published sugar templates. The `template_cid` equality is
over the Go kit's `body_source.ast_template`, not string equality over source
text.

## Write and prove

`recognize --write` minted bridge and implication contract mementos under the
demo's proof pool:

```text
$ sugar recognize \
    --write \
    --target go \
    --project /Users/tsavo/sugar-demo-go/examples/recognize-demo-go \
    --source pkg/ingest/ingest.go \
    --source pkg/persist/persist.go

dispatch: surface=`go-bind` sources=2
recognize: 2 tag(s) emitted
  [0] concept:http-get @ pkg/ingest/ingest.go:5 (fn=FetchURL, exact)
  [1] concept:sql-open @ pkg/persist/persist.go:9 (fn=OpenStore, exact)
write: minted 2 bridge(s) + 2 implication contract(s) into /Users/tsavo/sugar-demo-go/examples/recognize-demo-go/.sugar/recognize/blake3-512:6a7e3dc383dbd6c0a9b17aeea0e47ff75f9027c405f8b17bc9dab6a2b4dc98463b4a1ed313b16645c220a671cb65d9162d93bfedce50f071f15dfa87bea389de.proof
```

The proof pool then discharges the two recognize callsites:

```text
$ sugar prove /Users/tsavo/sugar-demo-go/examples/recognize-demo-go
Sugar verifier report
  total callsites : 2
  discharged      : 2
  violations      : 0
  load errors     : 0

  [discharged] FetchURL  (go -> recognize-demo-go-stdlib-http)
      reason: vacuous: no precondition on target (publisher post-only)
  [discharged] OpenStore  (go -> recognize-demo-go-stdlib-http)
      reason: vacuous: no precondition on target (publisher post-only)
```

The Go recognizer kit finds the demo-local shim proof from its own project
semantics and returns tags over RPC. `recognize --write` emits the
bridge/contract proof consumed by `prove`.
