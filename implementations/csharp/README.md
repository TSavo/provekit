# ProvekIt: C# peer

C# (.NET 10) implementation of the ProvekIt protocol, peer to the Rust,
C++, Go, and TypeScript reference implementations. Phases 1+2 only:

- **Phase 1: Library** (this directory):
  - `Provekit.Canonicalizer`: JCS-JSON encoder (RFC 8785) + BLAKE3-512
    self-identifying CIDs
  - `Provekit.ProofEnvelope`: deterministic CBOR (RFC 8949 §4.2.1) +
    Ed25519 signing + `.proof` envelope builder
  - `Provekit.ClaimEnvelope`: `MintContract` / `MintBridge` /
    `MintImplication`
- **Phase 2: Kit-authoring API**:
  - `Provekit.IR`: `Must`, `Contract`, `ForAll`, `Exists`, `And`, `Or`,
    `Not`, `Implies`, `Eq`, `Gt`, `Gte`, `Lt`, `Lte`, `Atomic`, `Num`,
    `StrConst`, `Var`, `Ctor`, `Int()`, `String()`, `Bool()`, plus the
    `Collector.BeginCollecting` / `Finish` lifecycle.

Phase 3 (self-contracts) and Phase 4 (the `[DllImport]` bridge demo)
are out of scope for this commit; they slot in once the foundation is
in place.

## Quick start

```bash
cd implementations/csharp
dotnet build
dotnet test
```

All cross-language conformance tests should pass:

- `CanonicalizerConformanceTests`: asserts the canonicalizer's JCS
  bytes for the spec fixture `x > 0` (de Bruijn form) are byte-
  identical to the C++ peer, and the BLAKE3-512 propertyHash is
  `c592f835...23a5`.
- `IrKitConformanceTests`: asserts the kit-authoring API's
  `Gt(Var("x"), Num(0))` produces the v1.1.0 IR-JSON-shape bytes that
  match the Rust peer byte-for-byte.
- `ProofEnvelopeTests`: CBOR shortest-form integers, Ed25519 round-
  trip, deterministic .proof envelopes.
- `ClaimEnvelopeTests`: full mint flow for contract / bridge /
  implication mementos with proper hash derivation.

## NuGet dependencies

| Package              | Purpose                              |
|----------------------|--------------------------------------|
| `Blake3` (1.x)       | BLAKE3-512 with 64-byte XOF output   |
| `NSec.Cryptography`  | Ed25519 (libsodium-backed)           |
| `xunit` 2.x          | Test framework                       |
| `Microsoft.NET.Test.Sdk` | dotnet test runner host           |

CBOR is hand-rolled (~70 LOC) to mirror the Rust/C++ peers exactly;
trying to drive a third-party CBOR library to RFC 8949 §4.2.1
determinism is more code than the encoder itself.

## Conformance

The protocol is the bytes. Cross-language hash agreement is verified
by:

1. The `x > 0` canonicalizer fixture (raw `Value` AST, alphabetized
   keys) hashes to `blake3-512:c592f835...23a5` per spec.
2. The kit's `Gt(Var("x"), Num(0))` serialized through the kit's
   `FormulaToValue` → JCS path hashes to
   `blake3-512:3e28aae8...95110`, matching the Rust peer byte-for-byte
   (the value was captured from the Rust crate before pinning).
3. BLAKE3-512 of `"hello"` and the empty input are pinned against the
   Rust peer's `blake3` crate output (64-byte XOF).

If a NuGet upgrade ever silently changes any of these, the conformance
tests fail and the regression is caught at the byte level.

## Layout

```
Provekit.sln
Provekit.Canonicalizer/      # JCS + BLAKE3
Provekit.IR/                 # Kit authoring API
Provekit.ProofEnvelope/      # CBOR + Ed25519 + .proof builder
Provekit.ClaimEnvelope/      # Mint contract/bridge/implication
Provekit.Tests/              # xUnit conformance tests
README.md
```
