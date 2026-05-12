# Program Transport Gaps

**GENERATED FILE. DO NOT EDIT.**
Rebuilt by `scripts/mint_language_morphisms.py` from `gaps/*.json` memento files.
To regenerate: run `./mint.sh` from `menagerie/concept-shapes/`.

Each gap is a `TransportGapMemento` -- a content-addressed, machine-readable record of why
a source-language op has no exact morphism into the concept hub, with resolution options.
See `protocol/specs/2026-05-14-transport-gap-and-partial-morphism-protocol.md`.

## Semantic Restrictions

- `concept:div` is integer division only. Floating-point division is out of scope for this node. `python:div` (true division, 5/2==2.5) and `js:`-style polymorphic division do not transport to `concept:div`.
- `concept:mod` is truncated-toward-zero remainder. `python:%` / `python:mod` is floored remainder (follows sign of divisor, not dividend) and does not transport to `concept:mod`.
- `concept:Int` is a fixed-width integer type. Languages with arbitrary-precision integers (`python:Int`, JS-style BigInt) do not transport to the fixed-width concept ops.
- Polymorphic `python:add` / `js:+` dispatch on operand type (integer, float, string); `concept:add` is integer-only. These do not transport.
- `concept:and` and `concept:or` are demoted from the hub: they are McCarthy desugarings of `concept:ite`, not independent primitives (`a && b = ite(a, b, false)`; `a || b = ite(a, true, b)`). Per-language `eq_and_to_ite_desugar` / `eq_or_to_ite_desugar` mementos record this. Languages with a boolean ternary transport `and`/`or` at the `ite` level after desugaring.
- `concept:foreach` is demoted: no common iterator protocol across the 10 languages; cross-language `foreach` transport requires per-language iterator-op morphisms (`<lang>:iter` / `has_next` / `next`) that lifters do not currently emit. `foreach`-using programs correctly produce transport refusals.
- `concept:ushr` is the logical zero-fill shift. It is separate from arithmetic `concept:shr`.
- `concept:source-unit` is a lossless source-bytes plus operational-term wrapper.
- Effect-subset relaxation: if `lang.effects` (as a set) is a subset of `concept.effects`, the morphism is discharged. Concept ops declare the union of all language effects for the same op. The reverse (lang does more than concept promised) is never discharged.

## Minted Coverage

| Concept op | Minted morphisms |
| --- | --- |
| `concept:add` | morphism_c11_add_to_add, morphism_csharp_add_to_add, morphism_go_add_to_add, morphism_zig_add_to_add, morphism_java_add_to_add |
| `concept:sub` | morphism_c11_sub_to_sub, morphism_csharp_sub_to_sub, morphism_go_sub_to_sub, morphism_zig_sub_to_sub, morphism_java_sub_to_sub |
| `concept:mul` | morphism_c11_mul_to_mul, morphism_csharp_mul_to_mul, morphism_go_mul_to_mul, morphism_zig_mul_to_mul, morphism_java_mul_to_mul |
| `concept:div` | morphism_c11_div_to_div, morphism_go_div_to_div, morphism_zig_div_to_div, morphism_java_div_to_div |
| `concept:mod` | morphism_c11_mod_to_mod, morphism_go_mod_to_mod, morphism_zig_mod_to_mod, morphism_java_mod_to_mod |
| `concept:neg` | morphism_c11_neg_to_neg, morphism_go_neg_to_neg, morphism_zig_neg_to_neg, morphism_java_neg_to_neg |
| `concept:bitand` | morphism_c11_bit_and_to_bitand, morphism_go_bitand_to_bitand, morphism_zig_bitand_to_bitand |
| `concept:bitor` | morphism_c11_bit_or_to_bitor, morphism_go_bitor_to_bitor, morphism_zig_bitor_to_bitor |
| `concept:bitxor` | morphism_c11_bit_xor_to_bitxor, morphism_go_bitxor_to_bitxor, morphism_zig_bitxor_to_bitxor |
| `concept:bitnot` | morphism_c11_bit_not_to_bitnot, morphism_go_bitnot_to_bitnot, morphism_zig_bitnot_to_bitnot, morphism_java_bitnot_to_bitnot |
| `concept:shl` | morphism_c11_shl_to_shl, morphism_go_shl_to_shl, morphism_zig_shl_to_shl, morphism_java_shl_to_shl |
| `concept:shr` | morphism_c11_shr_to_shr, morphism_go_shr_to_shr, morphism_zig_shr_to_shr, morphism_java_shr_to_shr |
| `concept:ushr` | morphism_typescript_ushr_to_ushr, morphism_java_ushr_to_ushr |
| `concept:eq` | morphism_c11_eq_to_eq, morphism_csharp_eq_to_eq, morphism_go_eq_to_eq, morphism_zig_eq_to_eq, morphism_java_eq_to_eq, morphism_rust_eq_to_eq |
| `concept:ne` | morphism_c11_ne_to_ne, morphism_csharp_ne_to_ne, morphism_go_ne_to_ne, morphism_zig_ne_to_ne, morphism_java_ne_to_ne |
| `concept:lt` | morphism_c11_lt_to_lt, morphism_go_lt_to_lt, morphism_zig_lt_to_lt, morphism_java_lt_to_lt |
| `concept:le` | morphism_c11_le_to_le, morphism_go_le_to_le, morphism_zig_le_to_le, morphism_java_le_to_le |
| `concept:gt` | morphism_c11_gt_to_gt, morphism_go_gt_to_gt, morphism_zig_gt_to_gt, morphism_java_gt_to_gt |
| `concept:ge` | morphism_c11_ge_to_ge, morphism_go_ge_to_ge, morphism_zig_ge_to_ge, morphism_java_ge_to_ge |
| `concept:not` | morphism_c11_not_to_not, morphism_zig_not_to_not, morphism_java_not_to_not |
| `concept:assign` | morphism_c11_assign_to_assign, morphism_typescript_assign_to_assign, morphism_zig_assign_to_assign |
| `concept:decl` | morphism_c11_decl_to_decl, morphism_typescript_decl_to_decl, morphism_zig_decl_to_decl, morphism_java_decl_to_decl |
| `concept:seq` | morphism_c11_seq_to_seq, morphism_rust_seq_to_seq |
| `concept:skip` | morphism_c11_skip_to_skip, morphism_rust_skip_to_skip |
| `concept:conditional` | morphism_c11_if_to_conditional, morphism_rust_if_to_conditional |
| `concept:ite` | morphism_c11_conditional_to_ite, morphism_java_ite_to_ite |
| `concept:while` | morphism_c11_while_to_while, morphism_csharp_while_to_while, morphism_python_while_to_while, morphism_zig_while_to_while, morphism_ruby_while_to_while, morphism_java_while_to_while, morphism_rust_while_to_while |
| `concept:do` | morphism_c11_do_to_do, morphism_java_do_to_do |
| `concept:for` | morphism_c11_for_to_for, morphism_csharp_for_to_for, morphism_java_for_to_for, morphism_rust_for_to_for |
| `concept:break` | morphism_c11_break_to_break, morphism_csharp_break_to_break, morphism_typescript_break_to_break, morphism_zig_break_to_break, morphism_php_break_to_break, morphism_java_break_to_break |
| `concept:continue` | morphism_c11_continue_to_continue, morphism_csharp_continue_to_continue, morphism_typescript_continue_to_continue, morphism_zig_continue_to_continue, morphism_php_continue_to_continue, morphism_java_continue_to_continue |
| `concept:return` | morphism_c11_return_to_return, morphism_typescript_return_to_return, morphism_zig_return_to_return, morphism_java_return_to_return, morphism_rust_return_to_return |
| `concept:call` | morphism_c11_call_to_call |
| `concept:index` | morphism_c11_array_subscript_to_index |
| `concept:member` | morphism_c11_member_to_member |
| `concept:deref` | morphism_c11_deref_to_deref |
| `concept:addr` | morphism_c11_addr_of_to_addr, morphism_zig_addr_to_addr |
| `concept:new` | morphism_csharp_new_to_new |
| `concept:cast` | morphism_c11_cast_to_cast |
| `concept:throw` | morphism_typescript_throw_to_throw, morphism_php_throw_to_throw, morphism_java_throw_to_throw |
| `concept:postinc` | morphism_c11_post_inc_to_postinc |
| `concept:postdec` | morphism_c11_post_dec_to_postdec |
| `concept:preinc` | morphism_c11_pre_inc_to_preinc |
| `concept:predec` | morphism_c11_pre_dec_to_predec |
| `concept:source-unit` | none |

## Gaps

| Language | Concept op | Source spec | Reason |
| --- | --- | --- | --- |
| `python` | `concept:add` | `op_add.spec.json` | python:add is polymorphic (dispatches on operand type: int, float, str, list); concept:add is integer-only |
| `typescript` | `concept:add` | `op_add.spec.json` | ts:+ is polymorphic (number | string concatenation); concept:add is integer-only |
| `ruby` | `concept:add` | `op_add.spec.json` | precondition mismatch: got `{"kind":"atomic","name":"true","args":[]}` want `{"kind":"atomic","name":"no_signed_overflow","args":[{"kind":"op","name":"add","args":[{"kind":"var","name":"lhs"},{"kind":"var","name":"rhs"}]}]}` |
| `php` | `concept:add` | `op_add.spec.json` | precondition mismatch: got `{"kind":"atomic","name":"true","args":[]}` want `{"kind":"atomic","name":"no_signed_overflow","args":[{"kind":"op","name":"add","args":[{"kind":"var","name":"lhs"},{"kind":"var","name":"rhs"}]}]}` |
| `rust` | `concept:add` | `op_add.spec.json` | precondition mismatch: got `{"kind":"atomic","name":"no_signed_overflow_or_panic","args":[{"kind":"op","name":"add","args":[{"kind":"var","name":"lhs"},{"kind":"var","name":"rhs"}]}]}` want `{"kind":"atomic","name":"no_signed_overflow","args":[{"kind":"op","name":"add","args":[{"kind":"var","name":"lhs"},{"kind":"var","name":"rhs"}]}]}` |
| `python` | `concept:sub` | `op_sub.spec.json` | python:sub is polymorphic (dispatches on operand type); concept:sub is integer-only |
| `typescript` | `concept:sub` | `op_sub.spec.json` | precondition mismatch: got `{"kind":"atomic","name":"true","args":[]}` want `{"kind":"atomic","name":"no_signed_overflow","args":[{"kind":"op","name":"sub","args":[{"kind":"var","name":"lhs"},{"kind":"var","name":"rhs"}]}]}` |
| `ruby` | `concept:sub` | `op_sub.spec.json` | precondition mismatch: got `{"kind":"atomic","name":"true","args":[]}` want `{"kind":"atomic","name":"no_signed_overflow","args":[{"kind":"op","name":"sub","args":[{"kind":"var","name":"lhs"},{"kind":"var","name":"rhs"}]}]}` |
| `php` | `concept:sub` | `op_sub.spec.json` | precondition mismatch: got `{"kind":"atomic","name":"true","args":[]}` want `{"kind":"atomic","name":"no_signed_overflow","args":[{"kind":"op","name":"sub","args":[{"kind":"var","name":"lhs"},{"kind":"var","name":"rhs"}]}]}` |
| `rust` | `concept:sub` | `op_sub.spec.json` | precondition mismatch: got `{"kind":"atomic","name":"no_signed_overflow_or_panic","args":[{"kind":"op","name":"sub","args":[{"kind":"var","name":"lhs"},{"kind":"var","name":"rhs"}]}]}` want `{"kind":"atomic","name":"no_signed_overflow","args":[{"kind":"op","name":"sub","args":[{"kind":"var","name":"lhs"},{"kind":"var","name":"rhs"}]}]}` |
| `python` | `concept:mul` | `op_mul.spec.json` | python:mul is polymorphic (int * str repeats, etc.); concept:mul is integer-only |
| `typescript` | `concept:mul` | `op_mul.spec.json` | precondition mismatch: got `{"kind":"atomic","name":"true","args":[]}` want `{"kind":"atomic","name":"no_signed_overflow","args":[{"kind":"op","name":"mul","args":[{"kind":"var","name":"lhs"},{"kind":"var","name":"rhs"}]}]}` |
| `ruby` | `concept:mul` | `op_mul.spec.json` | precondition mismatch: got `{"kind":"atomic","name":"true","args":[]}` want `{"kind":"atomic","name":"no_signed_overflow","args":[{"kind":"op","name":"mul","args":[{"kind":"var","name":"lhs"},{"kind":"var","name":"rhs"}]}]}` |
| `php` | `concept:mul` | `op_mul.spec.json` | precondition mismatch: got `{"kind":"atomic","name":"true","args":[]}` want `{"kind":"atomic","name":"no_signed_overflow","args":[{"kind":"op","name":"mul","args":[{"kind":"var","name":"lhs"},{"kind":"var","name":"rhs"}]}]}` |
| `rust` | `concept:mul` | `op_mul.spec.json` | precondition mismatch: got `{"kind":"atomic","name":"no_signed_overflow_or_panic","args":[{"kind":"op","name":"mul","args":[{"kind":"var","name":"lhs"},{"kind":"var","name":"rhs"}]}]}` want `{"kind":"atomic","name":"no_signed_overflow","args":[{"kind":"op","name":"mul","args":[{"kind":"var","name":"lhs"},{"kind":"var","name":"rhs"}]}]}` |
| `csharp` | `concept:div` | `op_div.spec.json` | precondition mismatch: got `{"kind":"atomic","name":"not_zero","args":[{"kind":"var","name":"rhs"}]}` want `{"kind":"atomic","name":"true","args":[]}` |
| `python` | `concept:div` | `not-supported` | operation not in supported set for this language |
| `typescript` | `concept:div` | `not-supported` | operation not in supported set for this language |
| `ruby` | `concept:div` | `not-supported` | operation not in supported set for this language |
| `php` | `concept:div` | `not-supported` | operation not in supported set for this language |
| `rust` | `concept:div` | `op_div.spec.json` | precondition mismatch: got `{"kind":"atomic","name":"nonzero","args":[{"kind":"var","name":"rhs"}]}` want `{"kind":"atomic","name":"true","args":[]}` |
| `csharp` | `concept:mod` | `op_mod.spec.json` | precondition mismatch: got `{"kind":"atomic","name":"not_zero","args":[{"kind":"var","name":"rhs"}]}` want `{"kind":"atomic","name":"true","args":[]}` |
| `python` | `concept:mod` | `op_mod.spec.json` | python:mod is floored remainder (follows sign of divisor); concept:mod is truncated-toward-zero remainder (follows sign of dividend) |
| `typescript` | `concept:mod` | `op_mod.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `ruby` | `concept:mod` | `op_mod.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `php` | `concept:mod` | `op_mod.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `rust` | `concept:mod` | `op_rem.spec.json` | precondition mismatch: got `{"kind":"atomic","name":"nonzero","args":[{"kind":"var","name":"rhs"}]}` want `{"kind":"atomic","name":"true","args":[]}` |
| `csharp` | `concept:neg` | `op_neg.spec.json` | precondition mismatch: got `{"kind":"atomic","name":"true","args":[]}` want `{"kind":"atomic","name":"no_signed_overflow","args":[{"kind":"op","name":"neg","args":[{"kind":"var","name":"value"}]}]}` |
| `python` | `concept:neg` | `op_neg.spec.json` | python:neg is polymorphic (dispatches on operand type); concept:neg is integer-only |
| `typescript` | `concept:neg` | `op_neg.spec.json` | precondition mismatch: got `{"kind":"atomic","name":"true","args":[]}` want `{"kind":"atomic","name":"no_signed_overflow","args":[{"kind":"op","name":"neg","args":[{"kind":"var","name":"value"}]}]}` |
| `ruby` | `concept:neg` | `op_neg.spec.json` | precondition mismatch: got `{"kind":"atomic","name":"true","args":[]}` want `{"kind":"atomic","name":"no_signed_overflow","args":[{"kind":"op","name":"neg","args":[{"kind":"var","name":"value"}]}]}` |
| `php` | `concept:neg` | `op_neg.spec.json` | precondition mismatch: got `{"kind":"atomic","name":"true","args":[]}` want `{"kind":"atomic","name":"no_signed_overflow","args":[{"kind":"op","name":"neg","args":[{"kind":"var","name":"value"}]}]}` |
| `rust` | `concept:neg` | `op_neg.spec.json` | precondition mismatch: got `{"kind":"atomic","name":"no_signed_overflow_or_panic","args":[{"kind":"op","name":"neg","args":[{"kind":"var","name":"value"}]}]}` want `{"kind":"atomic","name":"no_signed_overflow","args":[{"kind":"op","name":"neg","args":[{"kind":"var","name":"value"}]}]}` |
| `csharp` | `concept:bitand` | `op_bitand.spec.json` | wp mismatch: got `"lhs & rhs (bitwise AND)"` want `"integer bitwise and expression"` |
| `python` | `concept:bitand` | `op_bitand.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `typescript` | `concept:bitand` | `op_bitand.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `ruby` | `concept:bitand` | `op_bitand.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `php` | `concept:bitand` | `op_bitand.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `java` | `concept:bitand` | `op_bitand.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `rust` | `concept:bitand` | `op_bit_and.spec.json` | wp mismatch: got `"integer bitwise and"` want `"integer bitwise and expression"` |
| `csharp` | `concept:bitor` | `op_bitor.spec.json` | wp mismatch: got `"lhs | rhs (bitwise OR)"` want `"integer bitwise or expression"` |
| `python` | `concept:bitor` | `op_bitor.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `typescript` | `concept:bitor` | `op_bitor.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `ruby` | `concept:bitor` | `op_bitor.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `php` | `concept:bitor` | `op_bitor.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `java` | `concept:bitor` | `op_bitor.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `rust` | `concept:bitor` | `op_bit_or.spec.json` | wp mismatch: got `"integer bitwise or"` want `"integer bitwise or expression"` |
| `csharp` | `concept:bitxor` | `op_bitxor.spec.json` | wp mismatch: got `"lhs ^ rhs (bitwise XOR)"` want `"integer bitwise xor expression"` |
| `python` | `concept:bitxor` | `op_bitxor.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `typescript` | `concept:bitxor` | `op_bitxor.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `ruby` | `concept:bitxor` | `op_bitxor.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `php` | `concept:bitxor` | `op_bitxor.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `java` | `concept:bitxor` | `op_bitxor.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `rust` | `concept:bitxor` | `op_bit_xor.spec.json` | wp mismatch: got `"integer bitwise xor"` want `"integer bitwise xor expression"` |
| `csharp` | `concept:bitnot` | `op_bitnot.spec.json` | wp mismatch: got `"~value (bitwise complement)"` want `"integer bitwise complement expression"` |
| `python` | `concept:bitnot` | `not-supported` | operation not in supported set for this language |
| `typescript` | `concept:bitnot` | `op_bitnot.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]}]` |
| `ruby` | `concept:bitnot` | `not-supported` | operation not in supported set for this language |
| `php` | `concept:bitnot` | `op_bitnot.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]}]` |
| `rust` | `concept:bitnot` | `op_bit_not.spec.json` | wp mismatch: got `"integer bitwise not"` want `"integer bitwise complement expression"` |
| `csharp` | `concept:shl` | `op_shl.spec.json` | effect signature mismatch: got `{"effects":[]}` want `{"effects":[{"kind":"effect-signature","name":"Panic"}]}` |
| `python` | `concept:shl` | `not-supported` | operation not in supported set for this language |
| `typescript` | `concept:shl` | `op_shl.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `ruby` | `concept:shl` | `not-supported` | operation not in supported set for this language |
| `php` | `concept:shl` | `op_shl.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `rust` | `concept:shl` | `op_shl.spec.json` | wp mismatch: got `"integer shift left"` want `"integer left shift expression"` |
| `csharp` | `concept:shr` | `op_shr.spec.json` | effect signature mismatch: got `{"effects":[]}` want `{"effects":[{"kind":"effect-signature","name":"Panic"}]}` |
| `python` | `concept:shr` | `not-supported` | operation not in supported set for this language |
| `typescript` | `concept:shr` | `op_shr.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `ruby` | `concept:shr` | `not-supported` | operation not in supported set for this language |
| `php` | `concept:shr` | `op_shr.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `rust` | `concept:shr` | `op_shr.spec.json` | wp mismatch: got `"integer shift right"` want `"integer right shift expression"` |
| `c11` | `concept:ushr` | `not-supported` | operation not in supported set for this language |
| `csharp` | `concept:ushr` | `op_ushr.spec.json` | no candidate source operation spec |
| `go` | `concept:ushr` | `not-supported` | operation not in supported set for this language |
| `python` | `concept:ushr` | `not-supported` | operation not in supported set for this language |
| `zig` | `concept:ushr` | `not-supported` | operation not in supported set for this language |
| `ruby` | `concept:ushr` | `not-supported` | operation not in supported set for this language |
| `php` | `concept:ushr` | `not-supported` | operation not in supported set for this language |
| `rust` | `concept:ushr` | `not-supported` | operation not in supported set for this language |
| `python` | `concept:eq` | `op_eq.spec.json` | no candidate source operation spec |
| `typescript` | `concept:eq` | `op_eq.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `ruby` | `concept:eq` | `op_eq.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `php` | `concept:eq` | `op_eq.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `python` | `concept:ne` | `op_ne.spec.json` | no candidate source operation spec |
| `typescript` | `concept:ne` | `op_ne.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `ruby` | `concept:ne` | `op_ne.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `php` | `concept:ne` | `op_ne.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `rust` | `concept:ne` | `op_ne.spec.json` | wp mismatch: got `"integer disequality comparison"` want `"integer not-equal comparison"` |
| `csharp` | `concept:lt` | `op_lt.spec.json` | wp mismatch: got `"lhs < rhs"` want `"integer less-than comparison"` |
| `python` | `concept:lt` | `op_lt.spec.json` | no candidate source operation spec |
| `typescript` | `concept:lt` | `op_lt.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `ruby` | `concept:lt` | `op_lt.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `php` | `concept:lt` | `op_lt.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `rust` | `concept:lt` | `op_lt.spec.json` | wp mismatch: got `"integer less than comparison"` want `"integer less-than comparison"` |
| `csharp` | `concept:le` | `op_le.spec.json` | wp mismatch: got `"lhs <= rhs"` want `"integer less-than-or-equal comparison"` |
| `python` | `concept:le` | `op_le.spec.json` | no candidate source operation spec |
| `typescript` | `concept:le` | `op_le.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `ruby` | `concept:le` | `op_le.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `php` | `concept:le` | `op_le.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `rust` | `concept:le` | `op_le.spec.json` | wp mismatch: got `"integer less than or equal comparison"` want `"integer less-than-or-equal comparison"` |
| `csharp` | `concept:gt` | `op_gt.spec.json` | wp mismatch: got `"lhs > rhs"` want `"integer greater-than comparison"` |
| `python` | `concept:gt` | `op_gt.spec.json` | no candidate source operation spec |
| `typescript` | `concept:gt` | `op_gt.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `ruby` | `concept:gt` | `op_gt.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `php` | `concept:gt` | `op_gt.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `rust` | `concept:gt` | `op_gt.spec.json` | wp mismatch: got `"integer greater than comparison"` want `"integer greater-than comparison"` |
| `csharp` | `concept:ge` | `op_ge.spec.json` | wp mismatch: got `"lhs >= rhs"` want `"integer greater-than-or-equal comparison"` |
| `python` | `concept:ge` | `op_ge.spec.json` | no candidate source operation spec |
| `typescript` | `concept:ge` | `op_ge.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `ruby` | `concept:ge` | `op_ge.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `php` | `concept:ge` | `op_ge.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` |
| `rust` | `concept:ge` | `op_ge.spec.json` | wp mismatch: got `"integer greater than or equal comparison"` want `"integer greater-than-or-equal comparison"` |
| `csharp` | `concept:not` | `op_not.spec.json` | wp mismatch: got `"!value"` want `"boolean negation"` |
| `go` | `concept:not` | `op_not.spec.json` | wp mismatch: got `"logical negation"` want `"boolean negation"` |
| `python` | `concept:not` | `op_not.spec.json` | arity_shape or slot policy mismatch: got `{"kind":"named","slots":[{"name":"operand"}]}` want `{"kind":"named","slots":[{"name":"value"}]}` |
| `typescript` | `concept:not` | `op_not.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]}]` want `[{"kind":"ctor","name":"Bool","args":[]}]` |
| `ruby` | `concept:not` | `op_not.spec.json` | arity_shape or slot policy mismatch: got `{"kind":"named","slots":[{"name":"operand"}]}` want `{"kind":"named","slots":[{"name":"value"}]}` |
| `php` | `concept:not` | `op_not.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"Bool","args":[]}]` |
| `rust` | `concept:not` | `op_not.spec.json` | arity_shape or slot policy mismatch: got `null` want `{"kind":"named","slots":[{"name":"value"}]}` |
| `csharp` | `concept:assign` | `op_assign.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` want `[{"kind":"ctor","name":"LValue","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` |
| `go` | `concept:assign` | `op_assign.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Identifier","args":[]},{"kind":"ctor","name":"Term","args":[]}]` want `[{"kind":"ctor","name":"LValue","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` |
| `python` | `concept:assign` | `op_assign.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"LValue","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` |
| `ruby` | `concept:assign` | `op_assign.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"LValue","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` |
| `php` | `concept:assign` | `op_assign.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"LValue","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` |
| `java` | `concept:assign` | `op_assign.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` want `[{"kind":"ctor","name":"LValue","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` |
| `rust` | `concept:assign` | `op_assign.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Place","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` want `[{"kind":"ctor","name":"LValue","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` |
| `csharp` | `concept:decl` | `op_decl.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"String","args":[]},{"kind":"ctor","name":"Int","args":[]}]` want `[{"kind":"ctor","name":"String","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` |
| `go` | `concept:decl` | `op_decl.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Identifier","args":[]},{"kind":"ctor","name":"Term","args":[]}]` want `[{"kind":"ctor","name":"String","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` |
| `python` | `concept:decl` | `op_decl.spec.json` | no candidate source operation spec |
| `ruby` | `concept:decl` | `op_decl.spec.json` | no candidate source operation spec |
| `php` | `concept:decl` | `op_decl.spec.json` | no candidate source operation spec |
| `rust` | `concept:decl` | `op_let.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Pattern","args":[]},{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Stmt","args":[]}]` want `[{"kind":"ctor","name":"String","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` |
| `csharp` | `concept:seq` | `op_seq.spec.json` | operation-contract mismatch |
| `go` | `concept:seq` | `op_seq.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Term","args":[]},{"kind":"ctor","name":"Term","args":[]}]` want `[{"kind":"ctor","name":"Stmt","args":[]},{"kind":"ctor","name":"Stmt","args":[]}]` |
| `python` | `concept:seq` | `op_seq.spec.json` | effect signature mismatch: got `{"effects":[]}` want `{"effects":[{"kind":"effect-polymorphic","rule":"union(first.effects, second.effects)"}]}` |
| `typescript` | `concept:seq` | `op_seq.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Stmt","args":[]}]` want `[{"kind":"ctor","name":"Stmt","args":[]},{"kind":"ctor","name":"Stmt","args":[]}]` |
| `zig` | `concept:seq` | `op_seq.spec.json` | operation-contract mismatch |
| `ruby` | `concept:seq` | `op_seq.spec.json` | effect signature mismatch: got `{"effects":[]}` want `{"effects":[{"kind":"effect-polymorphic","rule":"union(first.effects, second.effects)"}]}` |
| `php` | `concept:seq` | `op_seq.spec.json` | effect signature mismatch: got `{"effects":[]}` want `{"effects":[{"kind":"effect-polymorphic","rule":"union(first.effects, second.effects)"}]}` |
| `java` | `concept:seq` | `op_seq.spec.json` | operation-contract mismatch |
| `csharp` | `concept:skip` | `op_skip.spec.json` | wp mismatch: got `"post"` want `"state unchanged"` |
| `go` | `concept:skip` | `op_skip.spec.json` | return sort mismatch: got `{"kind":"ctor","name":"Unit","args":[]}` want `{"kind":"ctor","name":"Stmt","args":[]}` |
| `python` | `concept:skip` | `op_skip.spec.json` | no candidate source operation spec |
| `typescript` | `concept:skip` | `op_skip.spec.json` | no candidate source operation spec |
| `zig` | `concept:skip` | `op_skip.spec.json` | operation-contract mismatch |
| `ruby` | `concept:skip` | `op_skip.spec.json` | no candidate source operation spec |
| `php` | `concept:skip` | `op_skip.spec.json` | no candidate source operation spec |
| `java` | `concept:skip` | `op_skip.spec.json` | wp mismatch: got `"empty statement / absent branch"` want `"state unchanged"` |
| `c11` | `concept:conditional` | `op_conditional.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Bool","args":[]},{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` want `[{"kind":"ctor","name":"Bool","args":[]},{"kind":"ctor","name":"Stmt","args":[]},{"kind":"ctor","name":"Stmt","args":[]}]` |
| `csharp` | `concept:conditional` | `op_if.spec.json` | operation-contract mismatch |
| `go` | `concept:conditional` | `op_if.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Term","args":[]},{"kind":"ctor","name":"Bool","args":[]},{"kind":"ctor","name":"Term","args":[]},{"kind":"ctor","name":"Term","args":[]}]` want `[{"kind":"ctor","name":"Bool","args":[]},{"kind":"ctor","name":"Stmt","args":[]},{"kind":"ctor","name":"Stmt","args":[]}]` |
| `python` | `concept:conditional` | `op_if.spec.json` | effect signature mismatch: got `{"effects":[]}` want `{"effects":[{"kind":"effect-polymorphic","rule":"union(then_branch.effects, else_branch.effects)"}]}` |
| `typescript` | `concept:conditional` | `op_if.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Stmt","args":[]},{"kind":"ctor","name":"Stmt","args":[]}]` want `[{"kind":"ctor","name":"Bool","args":[]},{"kind":"ctor","name":"Stmt","args":[]},{"kind":"ctor","name":"Stmt","args":[]}]` |
| `zig` | `concept:conditional` | `op_if.spec.json` | wp mismatch: got `"branch-selected weakest precondition"` want `"cond ? wp(then_branch, post) : wp(else_branch, post)"` |
| `ruby` | `concept:conditional` | `op_if.spec.json` | effect signature mismatch: got `{"effects":[]}` want `{"effects":[{"kind":"effect-polymorphic","rule":"union(then_branch.effects, else_branch.effects)"}]}` |
| `php` | `concept:conditional` | `op_if.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Stmt","args":[]},{"kind":"ctor","name":"Stmt","args":[]}]` want `[{"kind":"ctor","name":"Bool","args":[]},{"kind":"ctor","name":"Stmt","args":[]},{"kind":"ctor","name":"Stmt","args":[]}]` |
| `java` | `concept:conditional` | `op_if.spec.json` | effect signature mismatch: got `{"effects":[]}` want `{"effects":[{"kind":"effect-polymorphic","rule":"union(then_branch.effects, else_branch.effects)"}]}` |
| `csharp` | `concept:ite` | `op_ite.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Bool","args":[]},{"kind":"ctor","name":"Int","args":[]},{"kind":"ctor","name":"Int","args":[]}]` want `[{"kind":"ctor","name":"Bool","args":[]},{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` |
| `go` | `concept:ite` | `not-supported` | operation not in supported set for this language |
| `python` | `concept:ite` | `op_ite-bool.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Bool","args":[]},{"kind":"ctor","name":"Bool","args":[]},{"kind":"ctor","name":"Bool","args":[]}]` want `[{"kind":"ctor","name":"Bool","args":[]},{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` |
| `typescript` | `concept:ite` | `op_ite.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` want `[{"kind":"ctor","name":"Bool","args":[]},{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` |
| `zig` | `concept:ite` | `op_ite.spec.json` | no candidate source operation spec |
| `ruby` | `concept:ite` | `op_ternary.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Bool","args":[]},{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"Bool","args":[]},{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` |
| `php` | `concept:ite` | `op_ternary.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"Bool","args":[]},{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` |
| `rust` | `concept:ite` | `op_ite.spec.json` | wp mismatch: got `"expression conditional used by WP contracts"` want `"ternary expression selected by cond"` |
| `go` | `concept:while` | `op_while.spec.json` | no candidate source operation spec |
| `typescript` | `concept:while` | `op_while.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Stmt","args":[]}]` want `[{"kind":"ctor","name":"Bool","args":[]},{"kind":"ctor","name":"Stmt","args":[]}]` |
| `php` | `concept:while` | `op_while.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Stmt","args":[]}]` want `[{"kind":"ctor","name":"Bool","args":[]},{"kind":"ctor","name":"Stmt","args":[]}]` |
| `csharp` | `concept:do` | `op_do.spec.json` | no candidate source operation spec |
| `go` | `concept:do` | `not-supported` | operation not in supported set for this language |
| `python` | `concept:do` | `not-supported` | operation not in supported set for this language |
| `typescript` | `concept:do` | `not-supported` | operation not in supported set for this language |
| `zig` | `concept:do` | `op_do.spec.json` | no candidate source operation spec |
| `ruby` | `concept:do` | `not-supported` | operation not in supported set for this language |
| `php` | `concept:do` | `not-supported` | operation not in supported set for this language |
| `rust` | `concept:do` | `not-supported` | operation not in supported set for this language |
| `go` | `concept:for` | `op_for.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Term","args":[]},{"kind":"ctor","name":"Term","args":[]},{"kind":"ctor","name":"Term","args":[]},{"kind":"ctor","name":"Term","args":[]}]` want `[{"kind":"ctor","name":"Stmt","args":[]},{"kind":"ctor","name":"Bool","args":[]},{"kind":"ctor","name":"Stmt","args":[]},{"kind":"ctor","name":"Stmt","args":[]}]` |
| `python` | `concept:for` | `op_for.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Stmt","args":[]}]` want `[{"kind":"ctor","name":"Stmt","args":[]},{"kind":"ctor","name":"Bool","args":[]},{"kind":"ctor","name":"Stmt","args":[]},{"kind":"ctor","name":"Stmt","args":[]}]` |
| `typescript` | `concept:for` | `op_for.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Stmt","args":[]},{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Stmt","args":[]},{"kind":"ctor","name":"Stmt","args":[]}]` want `[{"kind":"ctor","name":"Stmt","args":[]},{"kind":"ctor","name":"Bool","args":[]},{"kind":"ctor","name":"Stmt","args":[]},{"kind":"ctor","name":"Stmt","args":[]}]` |
| `zig` | `concept:for` | `op_for.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Stmt","args":[]}]` want `[{"kind":"ctor","name":"Stmt","args":[]},{"kind":"ctor","name":"Bool","args":[]},{"kind":"ctor","name":"Stmt","args":[]},{"kind":"ctor","name":"Stmt","args":[]}]` |
| `ruby` | `concept:for` | `op_for.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Stmt","args":[]}]` want `[{"kind":"ctor","name":"Stmt","args":[]},{"kind":"ctor","name":"Bool","args":[]},{"kind":"ctor","name":"Stmt","args":[]},{"kind":"ctor","name":"Stmt","args":[]}]` |
| `php` | `concept:for` | `op_for.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Stmt","args":[]},{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Stmt","args":[]},{"kind":"ctor","name":"Stmt","args":[]}]` want `[{"kind":"ctor","name":"Stmt","args":[]},{"kind":"ctor","name":"Bool","args":[]},{"kind":"ctor","name":"Stmt","args":[]},{"kind":"ctor","name":"Stmt","args":[]}]` |
| `go` | `concept:break` | `op_break.spec.json` | no candidate source operation spec |
| `python` | `concept:break` | `op_break.spec.json` | effect signature mismatch: got `{"effects":[]}` want `{"effects":[{"kind":"effect-polymorphic","rule":"control-transfer"},{"kind":"effect-signature","name":"Break"}]}` |
| `ruby` | `concept:break` | `op_break.spec.json` | no candidate source operation spec |
| `rust` | `concept:break` | `op_break.spec.json` | effect signature mismatch: got `{"effects":[{"kind":"effect-polymorphic","rule":"control-transfer"}]}` want `{"effects":[{"kind":"effect-polymorphic","rule":"control-transfer"},{"kind":"effect-signature","name":"Break"}]}` |
| `go` | `concept:continue` | `op_continue.spec.json` | no candidate source operation spec |
| `python` | `concept:continue` | `op_continue.spec.json` | effect signature mismatch: got `{"effects":[]}` want `{"effects":[{"kind":"effect-polymorphic","rule":"control-transfer"},{"kind":"effect-signature","name":"Continue"}]}` |
| `ruby` | `concept:continue` | `op_continue.spec.json` | no candidate source operation spec |
| `rust` | `concept:continue` | `op_continue.spec.json` | effect signature mismatch: got `{"effects":[{"kind":"effect-polymorphic","rule":"control-transfer"}]}` want `{"effects":[{"kind":"effect-polymorphic","rule":"control-transfer"},{"kind":"effect-signature","name":"Continue"}]}` |
| `csharp` | `concept:return` | `op_return.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Int","args":[]}]` want `[{"kind":"ctor","name":"Expr","args":[]}]` |
| `go` | `concept:return` | `op_return.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Term","args":[]}]` want `[{"kind":"ctor","name":"Expr","args":[]}]` |
| `python` | `concept:return` | `op_return.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"Expr","args":[]}]` |
| `ruby` | `concept:return` | `op_return.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"Expr","args":[]}]` |
| `php` | `concept:return` | `op_return.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"Expr","args":[]}]` |
| `csharp` | `concept:call` | `op_call.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"String","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` want `[{"kind":"ctor","name":"FnContract","args":[]},{"kind":"ctor","name":"ListOfExpr","args":[]}]` |
| `go` | `concept:call` | `op_call.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Identifier","args":[]},{"kind":"ctor","name":"Term","args":[]}]` want `[{"kind":"ctor","name":"FnContract","args":[]},{"kind":"ctor","name":"ListOfExpr","args":[]}]` |
| `python` | `concept:call` | `op_call.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"String","args":[]},{"kind":"ctor","name":"ListOfValue","args":[]}]` want `[{"kind":"ctor","name":"FnContract","args":[]},{"kind":"ctor","name":"ListOfExpr","args":[]}]` |
| `typescript` | `concept:call` | `op_call.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Args","args":[]}]` want `[{"kind":"ctor","name":"FnContract","args":[]},{"kind":"ctor","name":"ListOfExpr","args":[]}]` |
| `zig` | `concept:call` | `op_call.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"String","args":[]},{"kind":"ctor","name":"ListOfExpr","args":[]}]` want `[{"kind":"ctor","name":"FnContract","args":[]},{"kind":"ctor","name":"ListOfExpr","args":[]}]` |
| `ruby` | `concept:call` | `op_call.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"String","args":[]},{"kind":"ctor","name":"ListOfValue","args":[]}]` want `[{"kind":"ctor","name":"FnContract","args":[]},{"kind":"ctor","name":"ListOfExpr","args":[]}]` |
| `php` | `concept:call` | `op_call.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"String","args":[]},{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"FnContract","args":[]},{"kind":"ctor","name":"ListOfExpr","args":[]}]` |
| `java` | `concept:call` | `op_call.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"String","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` want `[{"kind":"ctor","name":"FnContract","args":[]},{"kind":"ctor","name":"ListOfExpr","args":[]}]` |
| `rust` | `concept:call` | `op_call.spec.json` | effect signature mismatch: got `{"effects":[{"kind":"effect-polymorphic","rule":"callee.effects"}]}` want `{"effects":[{"kind":"effect-polymorphic","rule":"callee.effects"},{"kind":"effect-signature","name":"UnresolvedCall"},{"kind":"effect-signature","name":"Call"},{"kind":"effect-polymorphic","rule":"callee.effects or unresolved_call when unavailable"}]}` |
| `csharp` | `concept:index` | `op_index.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Int","args":[]}]` want `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` |
| `go` | `concept:index` | `op_index.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Term","args":[]},{"kind":"ctor","name":"Term","args":[]}]` want `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` |
| `python` | `concept:index` | `op_subscript.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` |
| `typescript` | `concept:index` | `op_index.spec.json` | effect signature mismatch: got `{"effects":[]}` want `{"effects":[{"kind":"effect-signature","name":"Read"},{"kind":"effect-signature","name":"Reads"},{"kind":"effect-signature","name":"Panic"},{"kind":"effect-signature","name":"MemRead"}]}` |
| `zig` | `concept:index` | `op_index.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Int","args":[]}]` want `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` |
| `ruby` | `concept:index` | `op_index.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` |
| `php` | `concept:index` | `op_index.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"Value","args":[]}]` want `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` |
| `java` | `concept:index` | `op_index.spec.json` | return sort mismatch: got `{"kind":"ctor","name":"Expr","args":[]}` want `{"kind":"ctor","name":"LValue","args":[]}` |
| `rust` | `concept:index` | `op_index.spec.json` | precondition mismatch: got `{"kind":"atomic","name":"in_bounds","args":[{"kind":"var","name":"slice"},{"kind":"var","name":"idx"}]}` want `{"kind":"atomic","name":"true","args":[]}` |
| `csharp` | `concept:member` | `op_member.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"String","args":[]}]` want `[{"kind":"ctor","name":"LValue","args":[]},{"kind":"ctor","name":"FieldName","args":[]}]` |
| `go` | `concept:member` | `op_member.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Term","args":[]},{"kind":"ctor","name":"Identifier","args":[]}]` want `[{"kind":"ctor","name":"LValue","args":[]},{"kind":"ctor","name":"FieldName","args":[]}]` |
| `python` | `concept:member` | `op_member.spec.json` | no candidate source operation spec |
| `typescript` | `concept:member` | `op_member.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"String","args":[]}]` want `[{"kind":"ctor","name":"LValue","args":[]},{"kind":"ctor","name":"FieldName","args":[]}]` |
| `zig` | `concept:member` | `op_field.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"FieldName","args":[]}]` want `[{"kind":"ctor","name":"LValue","args":[]},{"kind":"ctor","name":"FieldName","args":[]}]` |
| `ruby` | `concept:member` | `op_member.spec.json` | no candidate source operation spec |
| `php` | `concept:member` | `op_propfetch.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Value","args":[]},{"kind":"ctor","name":"String","args":[]}]` want `[{"kind":"ctor","name":"LValue","args":[]},{"kind":"ctor","name":"FieldName","args":[]}]` |
| `java` | `concept:member` | `op_member.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"String","args":[]}]` want `[{"kind":"ctor","name":"LValue","args":[]},{"kind":"ctor","name":"FieldName","args":[]}]` |
| `rust` | `concept:member` | `op_member.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Place","args":[]},{"kind":"ctor","name":"FieldName","args":[]}]` want `[{"kind":"ctor","name":"LValue","args":[]},{"kind":"ctor","name":"FieldName","args":[]}]` |
| `rust` | `concept:member` | `op_field.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"FieldName","args":[]}]` want `[{"kind":"ctor","name":"LValue","args":[]},{"kind":"ctor","name":"FieldName","args":[]}]` |
| `csharp` | `concept:deref` | `not-supported` | operation not in supported set for this language |
| `go` | `concept:deref` | `op_deref.spec.json` | precondition mismatch: got `{"kind":"atomic","name":"true","args":[]}` want `{"kind":"connective","op":"and","operands":[{"kind":"atomic","name":"!=","args":[{"kind":"var","name":"ptr"},{"kind":"const","value":"NULL","sort":{"kind":"ctor","name":"Ptr","args":[]}}]},{"kind":"atomic","name":"valid","args":[{"kind":"var","name":"ptr"}]}]}` |
| `python` | `concept:deref` | `not-supported` | operation not in supported set for this language |
| `typescript` | `concept:deref` | `not-supported` | operation not in supported set for this language |
| `zig` | `concept:deref` | `op_deref.spec.json` | precondition mismatch: got `{"kind":"atomic","name":"true","args":[]}` want `{"kind":"connective","op":"and","operands":[{"kind":"atomic","name":"!=","args":[{"kind":"var","name":"ptr"},{"kind":"const","value":"NULL","sort":{"kind":"ctor","name":"Ptr","args":[]}}]},{"kind":"atomic","name":"valid","args":[{"kind":"var","name":"ptr"}]}]}` |
| `ruby` | `concept:deref` | `not-supported` | operation not in supported set for this language |
| `php` | `concept:deref` | `not-supported` | operation not in supported set for this language |
| `java` | `concept:deref` | `not-supported` | operation not in supported set for this language |
| `rust` | `concept:deref` | `op_deref.spec.json` | precondition mismatch: got `{"kind":"atomic","name":"true","args":[]}` want `{"kind":"connective","op":"and","operands":[{"kind":"atomic","name":"!=","args":[{"kind":"var","name":"ptr"},{"kind":"const","value":"NULL","sort":{"kind":"ctor","name":"Ptr","args":[]}}]},{"kind":"atomic","name":"valid","args":[{"kind":"var","name":"ptr"}]}]}` |
| `csharp` | `concept:addr` | `not-supported` | operation not in supported set for this language |
| `go` | `concept:addr` | `op_addr.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Term","args":[]}]` want `[{"kind":"ctor","name":"LValue","args":[]}]` |
| `python` | `concept:addr` | `not-supported` | operation not in supported set for this language |
| `typescript` | `concept:addr` | `not-supported` | operation not in supported set for this language |
| `ruby` | `concept:addr` | `not-supported` | operation not in supported set for this language |
| `php` | `concept:addr` | `not-supported` | operation not in supported set for this language |
| `java` | `concept:addr` | `not-supported` | operation not in supported set for this language |
| `rust` | `concept:addr` | `op_borrow.spec.json` | precondition mismatch: got `{"kind":"atomic","name":"place_live","args":[{"kind":"var","name":"place"}]}` want `{"kind":"atomic","name":"true","args":[]}` |
| `c11` | `concept:new` | `not-supported` | operation not in supported set for this language |
| `go` | `concept:new` | `op_new.spec.json` | no candidate source operation spec |
| `python` | `concept:new` | `not-supported` | operation not in supported set for this language |
| `typescript` | `concept:new` | `op_new.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Args","args":[]}]` want `[{"kind":"ctor","name":"String","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` |
| `zig` | `concept:new` | `op_new.spec.json` | no candidate source operation spec |
| `ruby` | `concept:new` | `not-supported` | operation not in supported set for this language |
| `php` | `concept:new` | `op_new.spec.json` | no candidate source operation spec |
| `java` | `concept:new` | `op_new.spec.json` | java:new returns sort Ref (heap-allocated reference); concept:new (csharp-derived) returns sort Expr (over-generalised). Ref is more precise for an allocation operation; concept:new base should be rebased to java:new. Refusal is loudly bounded per Supra omnia rectum. See #626 R3 follow-up. |
| `rust` | `concept:new` | `op_new.spec.json` | no candidate source operation spec |
| `csharp` | `concept:cast` | `op_cast.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"String","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` want `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` |
| `go` | `concept:cast` | `op_cast.spec.json` | no candidate source operation spec |
| `python` | `concept:cast` | `not-supported` | operation not in supported set for this language |
| `typescript` | `concept:cast` | `op_cast.spec.json` | no candidate source operation spec |
| `zig` | `concept:cast` | `op_cast.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"String","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` want `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` |
| `ruby` | `concept:cast` | `not-supported` | operation not in supported set for this language |
| `php` | `concept:cast` | `op_cast.spec.json` | no candidate source operation spec |
| `java` | `concept:cast` | `op_cast.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"String","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` want `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` |
| `rust` | `concept:cast` | `op_cast.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Sort","args":[]}]` want `[{"kind":"ctor","name":"Expr","args":[]},{"kind":"ctor","name":"Expr","args":[]}]` |
| `c11` | `concept:throw` | `not-supported` | operation not in supported set for this language |
| `csharp` | `concept:throw` | `op_throw.spec.json` | no candidate source operation spec |
| `go` | `concept:throw` | `not-supported` | operation not in supported set for this language |
| `python` | `concept:throw` | `op_raise.spec.json` | effect signature mismatch: got `{"effects":[{"kind":"effect-signature","name":"Panic"}]}` want `{"effects":[{"kind":"effect-signature","name":"Panic"},{"kind":"effect-signature","name":"Panics"}]}` |
| `zig` | `concept:throw` | `op_panic.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Reason","args":[]}]` want `[{"kind":"ctor","name":"Value","args":[]}]` |
| `ruby` | `concept:throw` | `op_raise.spec.json` | effect signature mismatch: got `{"effects":[{"kind":"effect-signature","name":"Panics"}]}` want `{"effects":[{"kind":"effect-signature","name":"Panic"},{"kind":"effect-signature","name":"Panics"}]}` |
| `rust` | `concept:throw` | `op_panic.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Reason","args":[]}]` want `[{"kind":"ctor","name":"Value","args":[]}]` |
| `csharp` | `concept:postinc` | `op_postinc.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Int","args":[]}]` want `[{"kind":"ctor","name":"LValue","args":[]}]` |
| `go` | `concept:postinc` | `not-supported` | operation not in supported set for this language |
| `python` | `concept:postinc` | `not-supported` | operation not in supported set for this language |
| `typescript` | `concept:postinc` | `op_postinc.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]}]` want `[{"kind":"ctor","name":"LValue","args":[]}]` |
| `zig` | `concept:postinc` | `op_postinc.spec.json` | no candidate source operation spec |
| `ruby` | `concept:postinc` | `not-supported` | operation not in supported set for this language |
| `php` | `concept:postinc` | `op_postinc.spec.json` | no candidate source operation spec |
| `java` | `concept:postinc` | `op_postinc.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]}]` want `[{"kind":"ctor","name":"LValue","args":[]}]` |
| `rust` | `concept:postinc` | `op_postinc.spec.json` | no candidate source operation spec |
| `csharp` | `concept:postdec` | `op_postdec.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Int","args":[]}]` want `[{"kind":"ctor","name":"LValue","args":[]}]` |
| `go` | `concept:postdec` | `not-supported` | operation not in supported set for this language |
| `python` | `concept:postdec` | `not-supported` | operation not in supported set for this language |
| `typescript` | `concept:postdec` | `op_postdec.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]}]` want `[{"kind":"ctor","name":"LValue","args":[]}]` |
| `zig` | `concept:postdec` | `op_postdec.spec.json` | no candidate source operation spec |
| `ruby` | `concept:postdec` | `not-supported` | operation not in supported set for this language |
| `php` | `concept:postdec` | `op_postdec.spec.json` | no candidate source operation spec |
| `java` | `concept:postdec` | `op_postdec.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]}]` want `[{"kind":"ctor","name":"LValue","args":[]}]` |
| `rust` | `concept:postdec` | `op_postdec.spec.json` | no candidate source operation spec |
| `csharp` | `concept:preinc` | `op_preinc.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Int","args":[]}]` want `[{"kind":"ctor","name":"LValue","args":[]}]` |
| `go` | `concept:preinc` | `not-supported` | operation not in supported set for this language |
| `python` | `concept:preinc` | `not-supported` | operation not in supported set for this language |
| `typescript` | `concept:preinc` | `op_preinc.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]}]` want `[{"kind":"ctor","name":"LValue","args":[]}]` |
| `zig` | `concept:preinc` | `op_preinc.spec.json` | no candidate source operation spec |
| `ruby` | `concept:preinc` | `not-supported` | operation not in supported set for this language |
| `php` | `concept:preinc` | `op_preinc.spec.json` | no candidate source operation spec |
| `java` | `concept:preinc` | `op_preinc.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]}]` want `[{"kind":"ctor","name":"LValue","args":[]}]` |
| `rust` | `concept:preinc` | `op_preinc.spec.json` | no candidate source operation spec |
| `csharp` | `concept:predec` | `op_predec.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Int","args":[]}]` want `[{"kind":"ctor","name":"LValue","args":[]}]` |
| `go` | `concept:predec` | `not-supported` | operation not in supported set for this language |
| `python` | `concept:predec` | `not-supported` | operation not in supported set for this language |
| `typescript` | `concept:predec` | `op_predec.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]}]` want `[{"kind":"ctor","name":"LValue","args":[]}]` |
| `zig` | `concept:predec` | `op_predec.spec.json` | no candidate source operation spec |
| `ruby` | `concept:predec` | `not-supported` | operation not in supported set for this language |
| `php` | `concept:predec` | `op_predec.spec.json` | no candidate source operation spec |
| `java` | `concept:predec` | `op_predec.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Expr","args":[]}]` want `[{"kind":"ctor","name":"LValue","args":[]}]` |
| `rust` | `concept:predec` | `op_predec.spec.json` | no candidate source operation spec |
| `c11` | `concept:source-unit` | `op_source_unit.spec.json` | operation-contract mismatch |
| `csharp` | `concept:source-unit` | `op_source-unit.spec.json` | wp mismatch: got `"source bytes are recoverable; operational_term is the lifted program"` want `"lossless source wrapper; the source bytes are recoverable and the operational projection is operational_term"` |
| `go` | `concept:source-unit` | `op_source_unit.spec.json` | formal sort mismatch: got `[{"kind":"ctor","name":"Literal","args":[]},{"kind":"ctor","name":"Term","args":[]}]` want `[{"kind":"ctor","name":"String","args":[]},{"kind":"ctor","name":"Stmt","args":[]}]` |
| `python` | `concept:source-unit` | `op_source-unit.spec.json` | wp mismatch: got `"lossless Python source wrapper; project_effects descends to operational_term"` want `"lossless source wrapper; the source bytes are recoverable and the operational projection is operational_term"` |
| `typescript` | `concept:source-unit` | `op_source-unit.spec.json` | wp mismatch: got `"lossless TypeScript source wrapper; source bytes are recoverable and operational_term is the lifted program"` want `"lossless source wrapper; the source bytes are recoverable and the operational projection is operational_term"` |
| `zig` | `concept:source-unit` | `op_source-unit.spec.json` | wp mismatch: got `"lossless Zig source wrapper; bytes are recoverable and operational_term is the lifted program"` want `"lossless source wrapper; the source bytes are recoverable and the operational projection is operational_term"` |
| `ruby` | `concept:source-unit` | `op_source-unit.spec.json` | wp mismatch: got `"lossless Ruby source wrapper; project_effects descends to operational_term"` want `"lossless source wrapper; the source bytes are recoverable and the operational projection is operational_term"` |
| `php` | `concept:source-unit` | `op_source-unit.spec.json` | wp mismatch: got `"lossless PHP source wrapper; source bytes are recoverable and operational_term is the lifted program"` want `"lossless source wrapper; the source bytes are recoverable and the operational projection is operational_term"` |
| `java` | `concept:source-unit` | `op_source-unit.spec.json` | wp mismatch: got `"source bytes are recoverable; operational_term is the lifted program"` want `"lossless source wrapper; the source bytes are recoverable and the operational projection is operational_term"` |
| `rust` | `concept:source-unit` | `op_source_unit.spec.json` | no candidate source operation spec |

T Savo
