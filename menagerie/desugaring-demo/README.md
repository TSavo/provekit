# Desugaring Demo

This directory documents the first runnable desugaring exhibit. The executable test lives in `implementations/rust/libprovekit/tests/desugar.rs` because it exercises the Rust rewriter directly against the menagerie mementos.

The C# fixture builds a lifted surface term with `csharp:for`, loads the tagged desugaring equations from `menagerie/csharp-language-signature/specs`, rewrites to the core normal form, and asserts that:

- `csharp:for` is gone from the normal form.
- The resulting term is `csharp:seq(init, csharp:while(cond, csharp:seq(body, step)))`.
- The normal-form JCS bytes and CID are deterministic across repeated runs.
- The applied equation carries a discharged WP-preservation obligation.

Run it with:

```sh
cargo test -p libprovekit --test desugar
```
