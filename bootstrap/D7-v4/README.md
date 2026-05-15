# D7-v4 Value constructor widening receipts

Scope: diagnostic-only widening after D7-v3.
Parent arc: #943.
Base commit: fba9a51d.
Branch: bootstrap/D7-v4-widen-value-methods.

D7-v3 reached BYTE_IDENTICAL for impl Value::null.
D7-v4 widens the receipt discipline to three adjacent constructors.
It does not extend provekit-walk.
It does not extend provekit-realize-rust-core.
It does not mint concept hubs, touch substrate, or mint new ops.

Targets in implementations/rust/provekit-canonicalizer/src/value.rs:
impl Value::boolean.
impl Value::integer.
impl Value::string.

The checked-in source currently uses Value::Bool for boolean.
The checked-in integer parameter is named n, and string is generic: string<S: Into<String>>(s: S).
The original slices in the receipts are taken from the checked-in file.

Trimmed fixture paths:
implementations/rust/libprovekit/tests/fixtures/proofir/d7_v4_value_boolean.json.
implementations/rust/libprovekit/tests/fixtures/proofir/d7_v4_value_integer.json.
implementations/rust/libprovekit/tests/fixtures/proofir/d7_v4_value_string.json.

| method | verdict | dominant class | fixture CID |
| --- | --- | --- | --- |
| boolean | CHARACTERIZED_DIFF | stub-body | blake3-512:b2c54035098e0bc287eca146fea6f697d56547d13d2e620cc38cbfda6b2f65d1994eeebe64aa3d8f3ac91843c45ab423b5917e95f2d62bc88f76500dd5f45b85 |
| integer | CHARACTERIZED_DIFF | stub-body | blake3-512:ca7c544cf2e01f70d422e41d75151d13d2adae92dafdec36d9368ae132ce2510954c4fd04ec49e50c36124f98a00edc4e6162f7c7d800427844d251f837c8f28 |
| string | CHARACTERIZED_DIFF | stub-body | blake3-512:8b3c7680416cb010f77eb30f5ef2c9284c3feadb16e3f5be7c48c44596557e46a94e5c7502cc0c106d1d39ec4ffcddd34431363c3f9e5e3e708e6cd09091b9c2 |

The walker already emits widened nested constructor surfaces.
boolean trims to return(call:new(Arc::new, [call:Bool(Value::Bool, [b])])).
integer trims to return(call:new(Arc::new, [call:Integer(Value::Integer, [n])])).
string trims to return(call:new(Arc::new, [call:String(Value::String, [method:into(s, [])])])).

The bridge fixture keeps only existing return and call:new catalog ops.
The widened nested constructor surface is preserved inside the ListOfExpr literal.
That keeps the fixture bridge-compatible without adding ProofIR ops.

provekit_realize_rust_core::emit_from_resolved still recognizes Value::Null only.
For these three widened literal shapes it falls through to the stub-body path.
The regenerated source body is panic!("provekit-bind canonical: literal").
That is the expected D7-v4 deliverable.

bootstrap/D7-v4/boolean_source_round_trip_receipt.json.
bootstrap/D7-v4/integer_source_round_trip_receipt.json.
bootstrap/D7-v4/string_source_round_trip_receipt.json.

The D7-v4 integration test is implementations/rust/libprovekit/tests/d7_v4_widen.rs.
It recomputes the fixture CID, resolves the ProofIR term, realizes Rust, rustfmts both sides, compares bytes, classifies the diff, and checks the committed receipt.
It accepts BYTE_IDENTICAL, CHARACTERIZED_DIFF, or BLOCKED.
It does not assert byte identity.

v5 should retire these stub-body differences under the #964/#962 self-host follow-up.
That retirement should consume the existing widened literal shapes.
It should not use this diagnostic receipt as license to add substrate concepts.
