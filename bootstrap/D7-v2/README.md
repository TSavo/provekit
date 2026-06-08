# D7-v2 Value::null resolved-term realizer receipt

Scope: one production function only.
Target crate: provekit-canonicalizer.
Target function: impl Value::null.
Source path: implementations/rust/provekit-canonicalizer/src/value.rs.

D7-v1 measured the source-layer gap after ProofIR resolution.
That receipt showed a CHARACTERIZED_DIFF dominated by stub-body.
The regenerator emitted panic!("provekit-bind canonical: return").
The resolved body was return(call:new(literal("new"), literal(["Null"]))).
The flat realizer API could not walk that body tree.

D7-v2 adds one tight realizer extension in provekit-realize-rust-core.
The new entrypoint is emit_from_resolved.
It accepts the resolved term JSON used by the D7-v1 resolve step.
It recognizes exactly the D7 Value::null body shape.
It lowers return(<expr>) as a Rust trailing expression.
It lowers call:new(literal("new"), literal(["Null"])) as new(Value::Null).
It lowers literal(["Null"]) as Value::Null for this fixture.
Every unsupported resolved concept or literal shape still falls back to the stub.

The intended regenerated source body is:
new(Value::Null)

The original source body remains:
Arc::new(Value::Null)

The expected post-rustfmt diff shape is:
-    Arc::new(Value::Null)
+    new(Value::Null)

That diff is intentional for D7-v2.
The missing Arc:: receiver prefix is not retired here.
It is the next debt class tracked by #962.
The empirical root cause is trait-path-truncated.
The resolved call:new node carries the constructor call.
It does not carry the receiver path needed to rebuild Arc::new.

This is still a CHARACTERIZED_DIFF, not BYTE_IDENTICAL.
The important change is the dominant class.
D7-v1 was dominated by stub-body.
D7-v2 is dominated by name-difference.
That means the realizer now consumes the resolved body tree for this op family.
The old panic stub no longer explains the Value::null source diff.

This is progress because it narrows the remaining failure.
Before D7-v2, source regeneration stopped at the root return concept.
After D7-v2, regeneration reaches the constructor expression.
The only visible body gap is the missing receiver name.
That is the #962 trait-path-truncated issue.

Out of scope for this receipt:
Do not change provekit-walk.
Do not change libprovekit::local_cid_fixture_check.
Do not add new concept ops.
Do not generalize beyond return plus call:new plus literal.
Do not claim source-layer byte identity for Value::null.

The receipt is bootstrap/D7-v2/value_null_source_round_trip_receipt.json.
