# ProvekIt Zig Kit

Zig-native implementation of the ProvekIt IR and lift tool.

## Structure

```
implementations/zig/
├── provekit-ir/                    # IR types + JCS canonicalizer + BLAKE3
│   ├── build.zig
│   └── src/root.zig
├── provekit-proof-envelope-zig/    # Side B substrate: deterministic CBOR,
│   │                               # Ed25519, .proof envelope build/verify
│   ├── build.zig
│   └── src/{root,cbor,sign,proof}.zig
└── provekit-lift-zig/              # Lift tool: scans .zig -> emits IR JSON
    ├── build.zig
    └── src/main.zig
```

## provekit-proof-envelope-zig

Side B (language-native) crypto substrate. Exposes BLAKE3-512, JCS
(re-exported from provekit-ir), Ed25519 sign/verify, deterministic CBOR
(RFC 8949 §4.2.1), and `.proof` envelope construction.

Cross-kit byte-equivalence with the rust kit's `provekit-proof-envelope`
is pinned by the canonical two-member fixture; same input produces
byte-identical output and the same `blake3-512:...` CID.

Primitive sources: BLAKE3 from `std.crypto.hash.Blake3`, Ed25519 from
`std.crypto.sign.Ed25519` (deterministic mode via `noise=null`), CBOR
hand-rolled (~90 LOC, 5 major types), JCS hand-rolled in provekit-ir
via `std.json.Stringify` with strict alphabetical key order.

```bash
cd implementations/zig/provekit-proof-envelope-zig
zig build test    # 26/26 tests pass, includes cross-kit byte-pin
```

## provekit-ir

The IR library provides Zig unions for all ProvekIt types with custom JSON serialization matching the v1.1.0 grammar.

```zig
const provekit = @import("provekit-ir");

const post = provekit.Atomic("gte", &.{
    provekit.Var("x", Sort.Int),
    provekit.Const(.{ .int = 0 }, Sort.Int),
});
```

### Build

```bash
cd implementations/zig/provekit-ir
zig build test
```

## provekit-lift-zig

Scans Zig source files for provekit annotations and emits IR documents.

### Annotations in Zig

Zig doesn't have attributes, so we use comment conventions:

```zig
//provekit:contract
fn parse_int(s: []const u8) i32 {
    // ...
}

//provekit:implement bafy...js-parseInt-v24
fn js_compatible_parse(s: []const u8) i32 {
    // ...
}

//provekit:verify
fn validate_email(email: []const u8) bool {
    // ...
}
```

### Usage

Standalone mode:
```bash
cd implementations/zig/provekit-lift-zig
zig build run -- --workspace ./src --output ./target/provekit
```

RPC plugin mode (for `provekit mint` integration):
```bash
provekit-lift-zig --rpc
```

### Build

```bash
cd implementations/zig/provekit-lift-zig
zig build
```

The binary installs to `zig-out/bin/provekit-lift-zig`.

## Design Philosophy

- **Zero dependencies**: Only Zig standard library.
- **Union enums**: Zig's `union(enum)` maps perfectly to the CDDL `kind`-tagged unions.
- **Allocator explicit**: All heap usage takes an allocator parameter: no hidden allocations.
- **Comptime-ready**: IR types are plain data; suitable for comptime code generation.

## LSP Plugin

See `examples/lsp-plugins/zig/` for the VS Code language server plugin.
