# provekit-ir (C)

C kit for ProvekIt protocol v1.1.0.

## Build

```bash
make test
```

## Design

- **Tagged unions** for `Sort`, `Term`, `Formula`, `Declaration`
- **Tree ownership**: every constructor takes ownership of its children; caller
  must ensure terms are not shared across multiple parents (no ref-counting in
  v1.1; deep-copy helpers planned for v1.2)
- **JCS emitter**: object keys sorted alphabetically with `qsort`; strings
  escaped per RFC 8785; UTF-8 > U+001F emitted verbatim
- **BLAKE3**: delegates to the Python `blake3` module via `popen` in v1.1;
  native C BLAKE3 binding planned for v1.2

## Cross-language conformance

The JCS bytes emitted by this kit match the Rust, Go, Java, and Python kits
for the same IR tree (verified against pinned test vectors).
