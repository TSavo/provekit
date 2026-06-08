# D7-v7 value module sweep

Scope: sugar-canonicalizer::value.
Parent arc: #943.
Base commit: 63a0bc1a.
Branch: bootstrap/D7-v7-value-module-sweep.

D7-v6 closed the scalar constructor cluster.
That cluster contains Value::null, Value::boolean, Value::integer, and Value::string.
D7-v7 reruns the same lift, bridge, resolve, realize, rustfmt, and byte compare path at module scope.
The source file is implementations/rust/sugar-canonicalizer/src/value.rs.

This is the D7 terminus claim for sugar-canonicalizer::value.
D7 terminus reached for sugar-canonicalizer::value at commit <merge-sha-of-this-PR>.
The claim is scoped to functions whose current lift yields a D7 bridge-compatible single-term fixture.
Non-goal functions are characterized below and excluded from the terminus check.

The walker command used for each function was:
cd implementations/rust && target/debug/sugar-walk-emit term sugar-canonicalizer/src/value.rs <fn> /private/tmp/d7-v7-walk/<fn>.raw.json

The in-scope functions are:
null.
boolean.
integer.
string.

The non-goal functions are:
kind.
array.
object.

| fn | verdict | fixture CID | cluster_canonical_cid | cluster_cardinality | note |
| --- | --- | --- | --- | --- | --- |
| kind | NON_GOAL | n/a | n/a | n/a | raw match_expr contains var nodes; not bridge-compatible under local_cid_fixture_check |
| null | BYTE_IDENTICAL | blake3-512:754c6ff2c9a0ff92d96bcce0d1269385809944e613c11696df0458703a4e5e187714866b9e35171eb09d7753b66274a5e0d51a99ff1e0a0be3d9da6c1082d23f | n/a | 4 scalar constructors | D7-v3 and v7 agree after rustfmt |
| boolean | BYTE_IDENTICAL | blake3-512:fe4f9d22916f33696f6a318c559948f445c0e61f353370c8d8a007e64bc56057e937fd844685e7120fa6346a0a4abd1cd2301190020b7c4bfec41fccb77abe89 | n/a | 4 scalar constructors | D7-v5 direct param shape remains closed |
| integer | BYTE_IDENTICAL | blake3-512:e69f9d9cf2ffac684c58502e416d722b4a94bf2269f7ac135010520e57a65660e7093b517bf17f01bfe1a574794881f3e7d28c091fb0a6b5507cddb2ec9e279a | n/a | 4 scalar constructors | D7-v5 direct param shape remains closed |
| string | BYTE_IDENTICAL | blake3-512:87a7b7bedc6615347bf6e01483804337dbeba211bc7bb5566e56f5d38e52d44b5d422af48d5a68a98cd88fd1a9011b940b229e53cdcb91f878f8e0aa2b731b60 | n/a | 4 scalar constructors | D7-v6 method:into arg shape remains closed |
| array | NON_GOAL | n/a | n/a | n/a | Value::Array is explicit D7-v6 out-of-scope debt |
| object | NON_GOAL | n/a | n/a | n/a | lift refuses with unsupported-value-closure |

The #971 inventory is a libsugar/src bind inventory.
value.rs lives under sugar-canonicalizer, so it has no useful #971 cluster_canonical_cid.
The table keeps the cluster fields explicit and marks them n/a for that reason.
The local scalar cluster cardinality is still useful: 4.

Committed D7-v7 fixtures:
implementations/rust/libsugar/tests/fixtures/proofir/d7_v7_value_null.json.
implementations/rust/libsugar/tests/fixtures/proofir/d7_v7_value_boolean.json.
implementations/rust/libsugar/tests/fixtures/proofir/d7_v7_value_integer.json.
implementations/rust/libsugar/tests/fixtures/proofir/d7_v7_value_string.json.

Committed D7-v7 receipts:
bootstrap/D7-v7/value_null_source_round_trip_receipt.json.
bootstrap/D7-v7/value_boolean_source_round_trip_receipt.json.
bootstrap/D7-v7/value_integer_source_round_trip_receipt.json.
bootstrap/D7-v7/value_string_source_round_trip_receipt.json.
bootstrap/D7-v7/value_module_sweep_receipt.json.

The integration test is implementations/rust/libsugar/tests/d7_v7_module_sweep.rs.
It recomputes each fixture CID.
It resolves through libsugar::local_cid_fixture_check.
It realizes through sugar_realize_rust_core::emit_from_resolved.
It extracts the original function slice from value.rs.
It rustfmts both sides.
It byte-compares the post-rustfmt strings.
It checks the committed receipt for drift.

Module verdict: TERMINUS.
byte_identical_count: 4.
characterized_diff_count: 0.
non_goal_count: 3.

The non-goal accounting matters.
kind is not a scalar constructor fixture because its raw term contains match_expr plus var nodes.
array is a Value::Array constructor surface, and v6 explicitly left Value::Array outside the closure.
object still refuses during lift with unsupported value expression Expr::Closure.
No lifter, realizer, or substrate extension was made in D7-v7.

This is the n=1 case of the cycle invariance theorem on a real libsugar submodule.
For the in-scope value.rs functions, rustfmt(realize_rust(local_cid_fixture_check(lift_rust(fn)))) equals rustfmt(fn) byte-for-byte.
