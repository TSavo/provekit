# D7-v0 Value::null ProofIR bridge receipt

Scope: one production function only.
Target crate: provekit-canonicalizer.
Target function: impl Value::null.
Source path: implementations/rust/provekit-canonicalizer/src/value.rs.

The lift was produced with provekit-walk through provekit-walk-emit term mode.
The command used was:
cd implementations/rust && cargo run --bin provekit-walk-emit -- term provekit-canonicalizer/src/value.rs null /tmp/d7_v0_value_null.raw.json

The raw walk output was a rust-algebra-term envelope.
For this D7-v0 bridge smoke, that envelope was trimmed to one bridge-compatible ProofIR term.
The fixture keeps the target, source, handling, term surface, op CIDs, and loss record from the raw lift.
The fixture path is implementations/rust/libprovekit/tests/fixtures/proofir/d7_v0_value_null.json.

The ProofIR term is return(call:new(Arc::new, [Null])).
The test resolves that term through libprovekit::local_cid_fixture_check.
The test then unresolves it through libprovekit::local_cid_fixture_check.
The reassembled fixture is JCS-canonicalized and compared against the input fixture.

The fixture CID is blake3-512:bcb10be48ad632abc71c406355b6d11b0191a959b523aa755ee00ad7496afa2270ce28821af4abcd5949427026fb16d8d8b38af702b1810dec3bdff810ec8f32.
The round-trip output CID is the same CID.
The byte-identity check passed.
The loss-record preservation check passed.

The carried loss record is intentionally non-empty.
It records procedural-macro detail derive.
It records return-type-user-defined detail Arc < Value >.
It records trait-path-truncated detail Value :: Null.
It records ffi-call-unresolved-effect detail Arc::new.

## Loss-record framing

Source-layer equivalence is evaluated in formatter-normal form. The contract is k(formatter(I)) = t(formatter(I) == formatter(I')), where formatter is the language's canonical formatter (rustfmt for Rust). The formatter is the canonicalizer, so whitespace, blank lines, trailing commas, and hex-vs-decimal literal rendering are NOT substrate concerns. The substrate carries concept:comment but NOT concept:formatting-hint.

The audit gap classes captured in this fixture's loss_record are engineering debt with named retirement tickets:

- #961 procedural-macro -> concept:proc-macro-invocation + concept:derive-attribute
- #962 trait-path-truncated -> concept:fully-qualified-path + rustc name resolution
- #963 return-type-user-defined -> concept:sort with generic args
- #964 ffi-call-unresolved-effect -> concept:effect-occurrence (admissibility-spine primitive)
- #965 trivia comments -> concept:comment (formatter handles formatting; comments survive rustfmt)

D7's terminus claim: for any libprovekit submodule M, rustfmt(realize_rust(local_cid_fixture_check(lift_rust(M)))) == rustfmt(M) byte-for-byte.

This receipt does not claim the lift is lossless.
This receipt does not re-realize Rust source.
This receipt does not compare generated Rust back to the source file.
This receipt does not sweep the whole module.
This receipt does not add or mint substrate definitions.
This receipt does not generalize beyond impl Value::null.
D7-v1 and later remain responsible for source re-realization and broader coverage.
