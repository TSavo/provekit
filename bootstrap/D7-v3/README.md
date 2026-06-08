# D7-v3 Value::null source round-trip receipt

Scope: one production function only.
Target crate: sugar-canonicalizer.
Target function: impl Value::null.
Source path: implementations/rust/sugar-canonicalizer/src/value.rs.

D7-v3 extends sugar-walk for the existing call:new op family.
The lifter keeps the op name as call:new.
It now threads the receiver-prefixed callee spelling through the first term argument.
For Value::null, that argument changed from new to Arc::new.
No substrate concept was minted.
No concept:fully-qualified-path shape was introduced.

The regenerated fixture is:
implementations/rust/libsugar/tests/fixtures/proofir/d7_v0_value_null.json.
Its term surface is:
return(call:new(Arc::new, [Null])).
The fixture CID is:
blake3-512:bcb10be48ad632abc71c406355b6d11b0191a959b523aa755ee00ad7496afa2270ce28821af4abcd5949427026fb16d8d8b38af702b1810dec3bdff810ec8f32.

The D7-v2 source receipt showed a name-only source diff.
The original body was Arc::new(Value::Null).
The regenerated body was new(Value::Null).
That diff existed because the lifted resolved term carried literal("new").
The Arc:: receiver prefix was only present as a trait-path-truncated loss record.

D7-v3 retires that loss for this call:new body shape.
The new fixture no longer records trait-path-truncated detail Arc :: new.
It still records trait-path-truncated detail Value :: Null.
That remaining enum path loss is outside this chunk.
The fixture still records ffi-call-unresolved-effect detail Arc::new.

sugar-realize-rust-core now consumes the new resolved shape.
It accepts call:new(literal("Arc::new"), literal(["Null"])).
It emits Arc::new(Value::Null).
It also keeps the old bare literal("new") path valid for older fixtures.

The D7-v3 receipt is:
bootstrap/D7-v3/value_null_source_round_trip_receipt.json.
The verdict is BYTE_IDENTICAL.
The post-rustfmt unified diff is empty.
The regenerated source CID matches the original post-rustfmt slice CID.

This reaches the D7 n=1 terminus for Value::null.
The claim is empirical and narrow.
It covers one real production function.
It does not claim module-level source identity.
It does not retire all trait-path-truncated debt.
It does not generalize beyond the call:new receiver-prefix path used here.

Next chunks widen from this single function to module-level source round trips.
Those chunks should keep the same receipt discipline.
They should report the next dominant diff class if byte identity fails.
