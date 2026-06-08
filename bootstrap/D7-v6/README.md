# D7-v6 method into argument closure

Scope: retire the single residual argument-wrapper shape from D7-v5.
Parent arc: #943.
Base commit: b3553412.
Branch: bootstrap/D7-v6-retire-method-into-arg.

D7-v5 closed the direct Value constructor argument shape.
That covered Value::boolean and Value::integer.
Value::null was already BYTE_IDENTICAL from D7-v3.
Value::string stayed CHARACTERIZED_DIFF.

The remaining string fixture is:
implementations/rust/libsugar/tests/fixtures/proofir/d7_v4_value_string.json.
Its surface is:
return(call:new(Arc::new, [call:String(Value::String, [method:into(s, [])])])).

The original source is:
implementations/rust/sugar-canonicalizer/src/value.rs.
The checked body is:
Arc::new(Value::String(s.into())).

D7-v6 extends only sugar-realize-rust-core.
The touched entrypoint is emit_from_resolved.
The new accepted argument form is:
method:<method_name>(<first_param>, []).

For the real fixture, that means:
method:into(s, []) becomes s.into().
The shape composes under the existing Value::String constructor arm.
The emitted body is:
Arc::new(Value::String(s.into())).

The guard is intentionally narrow.
The method receiver must equal params[0].
The method arg list must be empty.
Non-param receivers are still rejected.
Nested method receivers are still rejected.
Method calls with one or more args are still rejected.
Value::array and Value::object are not touched.

bootstrap/D7-v4/string_source_round_trip_receipt.json is refreshed.
Its verdict is now BYTE_IDENTICAL.
The regenerated source CID equals the original post-rustfmt slice CID.
Both are:
blake3-512:11ae0368cb858bbcd2806358c139bc80027e302997e2dc51067ad35779289f605026442c059e9b413722cbf3754f7d7b76354e2cc5a296f8b12c1fd9df2b1f43.

bootstrap/D7-v6/n_1_cluster_closure_receipt.json records the cluster result.
The closed constructors are Value::null, Value::boolean, Value::integer, and Value::string.
All four are BYTE_IDENTICAL.

The n=1 scalar constructor cluster is closed for the current fixtures.
This is not a module-level source identity claim.
It is the input for D7-v7.
D7-v7 should sweep implementations/rust/sugar-canonicalizer/src/value.rs at module scope.
That sweep should keep Value::array and Value::object as separate debt unless their fixtures close independently.
