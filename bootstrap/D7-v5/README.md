# D7-v5 Value variant argument realizer receipt

Scope: retire the direct one-parameter Value constructor stub-body class after D7-v4.
Parent arc: #943.
Base commit: 355ed128.
Branch: bootstrap/D7-v5-retire-stub-body-variant-arg.

D7-v5 extends implementations/rust/sugar-realize-rust-core/src/lib.rs.
The only new realizer shape is:

return(call:new(receiver, [call:<Variant>(Value::<Variant>, [first_param])]))

The receiver must still be new or an ::new path.
The literal-list payload must contain exactly one constructor surface.
The constructor argument must exactly match the function's first formal parameter.
The accepted Value variants are Value::Bool, Value::Integer, and Value::String.

Unsupported shapes still fall through to the canonical literal stub.
That includes multiple constructor args, nested calls inside the variant arg, computed-expression args, Value::Array, and Value::Object.

| method | v5 verdict | dominant class | regenerated source CID | original post-rustfmt CID |
| --- | --- | --- | --- | --- |
| boolean | BYTE_IDENTICAL | byte-identical | blake3-512:e4f464f54e93db6d8a14c682a3d2eb927d39c776ba0cc1dd62bc480a6de74918f3a25d94715840e957ed2d2d3b497e58cd02db8c3c9be4278b2e170d90d9982a | blake3-512:e4f464f54e93db6d8a14c682a3d2eb927d39c776ba0cc1dd62bc480a6de74918f3a25d94715840e957ed2d2d3b497e58cd02db8c3c9be4278b2e170d90d9982a |
| integer | BYTE_IDENTICAL | byte-identical | blake3-512:7cfdae9bf999160132cede4141307afcd664d0609e24263059761edf6033861abc85a2f437ea2637b598aafca7091d40b745ce24c52fcb605a3cbf4862373baf | blake3-512:7cfdae9bf999160132cede4141307afcd664d0609e24263059761edf6033861abc85a2f437ea2637b598aafca7091d40b745ce24c52fcb605a3cbf4862373baf |
| string | CHARACTERIZED_DIFF | stub-body | blake3-512:5a1eab3177aab96ca611d3a3ea7bf4284f3b7ffbdbe2e4cdf7e1c805ab2253fa0a7fae31b811df532deeed93660944c5dd4d769d0017e54146b34f160e933c48 | blake3-512:11ae0368cb858bbcd2806358c139bc80027e302997e2dc51067ad35779289f605026442c059e9b413722cbf3754f7d7b76354e2cc5a296f8b12c1fd9df2b1f43 |

boolean and integer now reach BYTE_IDENTICAL from the D7-v4 fixtures.
string does not: its actual v4 fixture is return(call:new(Arc::new, [call:String(Value::String, [method:into(s, [])])])).
That is a nested method-call argument, so D7-v5 records it as CHARACTERIZED_DIFF and leaves it for the next step rather than broadening this patch.

The D7-v4 receipt files were refreshed for the current realizer output.
bootstrap/D7-v5/source_round_trip_receipt.json records the mixed v5 outcome.

Next: D7-v6 should retire the string method:into(s, []) argument surface and then add the submodule-level source round-trip test for implementations/rust/sugar-canonicalizer/src/value.rs.
That submodule-level test should cover the closed Value::null, Value::boolean, Value::integer, and Value::string cluster before Value::array or Value::object are attempted.
