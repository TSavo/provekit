# provekit-rpc-java-demo

A working JSON-RPC 2.0 server in java, partially lowered from
`implementations/rust/libprovekit-rpc-cross-platform/src/lib.rs` by the
substrate's lift→bind→lower pipeline.

## Provenance per function

| Function | Origin |
|---|---|
| `ok_response` | **LOWERED** — `provekit lower` output verbatim |
| `error_response` | **LOWERED** — `provekit lower` output verbatim |
| `slot_cid` | **LOWERED** — `provekit lower` output verbatim |
| `content_addressed_name` | **LOWERED** — `provekit lower` output verbatim |
| `blake3_512_cid` | hand-written (rust uses for-loop with `&b` pattern + bit-shifts + `as char` cast; partial lower vocabulary) |
| `initialize_result` | hand-written (rust `json!({...})` with nested array; partial nested macro support) |
| `handle_line` | hand-written (rust uses match w/ tuple return `(Value, bool)` + nested match; needs richer enum/tuple lowering) |
| `run_server` | hand-written (rust uses `while let Some(line)` destructuring; lifter doesn't yet lower it) |
| `stdin_read_line` / `stdout_write_line` / `stderr_write_line` / `json_parse` / `json_serialize` / `blake3_512_of` / `encode_jcs` | boundary primitives — java-io shim realizations |

## Run

```
# Compile (with jackson + bouncycastle on classpath)
CP="$HOME/.m2/repository/com/fasterxml/jackson/core/jackson-databind/2.17.2/jackson-databind-2.17.2.jar:..."
javac -cp "$CP" src/main/java/CrossPlatformRpc.java

# Run as JSON-RPC server on stdio
echo '{"jsonrpc":"2.0","id":1,"method":"initialize"}' | java -cp "src/main/java:$CP" CrossPlatformRpc
```

Produces:

```
provekit-rpc-java-lowered-demo listening on stdio (JSON-RPC 2.0, NDJSON)
{"jsonrpc":"2.0","id":1,"result":{"name":"provekit-rpc-java-lowered-demo",...}}
```

## Pipeline

```
provekit lift implementations/rust/libprovekit-rpc-cross-platform/ --library-bindings --json -o lift.json
provekit lower --target java lift.json --project . --library java-io --family-library json=jackson -o lower.java
```

(The "wrap" + "hand-fill" steps are manual until the lower vocabulary
covers the remaining constructs. The lowered functions are verbatim
output; no edits.)
