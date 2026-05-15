# D7-v1 Value::null source round-trip receipt

Scope: one production function only.
Target crate: provekit-canonicalizer.
Target function: impl Value::null.
Source path: implementations/rust/provekit-canonicalizer/src/value.rs.

D7-v0 proved that this fixture round-trips byte-identically at the ProofIR layer.
D7-v1 measures the next layer only.
The measured equation is:
rustfmt(realize_rust(proofir_resolve(fixture))) == rustfmt(original_source)

The fixture is implementations/rust/libprovekit/tests/fixtures/proofir/d7_v0_value_null.json.
The fixture CID is blake3-512:f78e468e6f80e305c8abb4f1b5ccbe54cdea54bf3d5104a63970be8500f7f0a5e7a467fa2cf3bcd2894502ff045c4aede5dbc83f3b76d5818a0aeb2fcacaca3e.
The bridge resolves the term as return(call:new(literal("new"), literal(["Null"]))).
The resolved root sort is Stmt.
The nested call result sort is Expr.
The literal callee sort is FnContract.
The literal argument-list sort is ListOfExpr.

The current Rust realizer surface is flat.
It accepts function, params, param_types, return_type, concept_name, and mode.
It does not accept a ResolvedTerm body tree.
It does not walk nested ProofIR ops.
It chooses one body template by concept name.
If no template matches, it emits a panic stub.

The D7-v1 extraction used the best fields available from the fixture and resolved term.
function: null.
params: [].
param_types: [].
return_type: Arc < Value >, carried by the D7-v0 loss record.
concept_name: return, recovered from the resolved root op CID.
mode: null.

The library call was:
provekit_realize_rust_core::emit_stub_with_mode("null", &[], &[], "Arc < Value >", "return", None)

The regenerated source before rustfmt was:
pub fn null() -> Arc < Value > {
    panic!("provekit-bind canonical: return")
}

The original source slice was extracted from value.rs as just the method definition.
The original body is Arc::new(Value::Null).
Both sides were normalized with rustfmt --edition 2021 over stdin.
No rustfmt.toml or .rustfmt.toml was present at the repository root.

The post-rustfmt byte comparison is false.
The verdict is CHARACTERIZED_DIFF.
The unified diff has one hunk.
The hunk class is stub-body.
No formatter-noise hunk appeared.
No name-difference hunk appeared independently of the stub.
No structural-difference hunk appeared independently of the stub.

The empirical body gap is the stub.
The realizer emitted the stub because there is no template for the extracted root concept return.
The realizer also has no term-tree input path that could lower return(call:new(...)).
This is a source-layer realization gap, not a ProofIR bridge gap.
The D7-v0 bridge receipt remains byte-identical at its layer.

The dominant D7 debt ticket is #964.
D7-v0 recorded ffi-call-unresolved-effect detail new.
That is the operation needed to reconstitute Arc::new(Value::Null) as a body expression.
#962 is secondary because D7-v0 also recorded trait-path-truncated for Arc :: new and Value :: Null.
#963 is secondary because the return type Arc < Value > came from the loss record, not from resolved sort structure.
#961 is not active for this method-level source diff because derive loss is outside the extracted null method.
#965 is not active because the extracted method has no comments.

For this single function, the source-layer terminus claim requires debt retirement.
The immediate blocker is #964 plus a realizer path that can consume ResolvedTerm bodies.
Byte identity should not be claimed from D7-v1.
The receipt is bootstrap/D7-v1/value_null_source_round_trip_receipt.json.
