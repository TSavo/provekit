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
| `concept:seq` | morphism_c11_seq_to_seq, morphism_csharp_seq_to_seq, morphism_python_seq_to_seq, morphism_zig_seq_to_seq, morphism_ruby_seq_to_seq, morphism_php_seq_to_seq, morphism_java_seq_to_seq, morphism_rust_seq_to_seq |
| `concept:skip` | morphism_c11_skip_to_skip, morphism_csharp_skip_to_skip, morphism_zig_skip_to_skip, morphism_java_skip_to_skip, morphism_rust_skip_to_skip |
| `concept:conditional` | morphism_c11_if_to_conditional, morphism_csharp_if_to_conditional, morphism_python_if_to_conditional, morphism_zig_if_to_conditional, morphism_ruby_if_to_conditional, morphism_rust_if_to_conditional |
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
| `concept:throw` | morphism_python_throw_to_throw, morphism_typescript_throw_to_throw, morphism_php_throw_to_throw, morphism_java_throw_to_throw |
| `concept:postinc` | morphism_c11_post_inc_to_postinc |
| `concept:postdec` | morphism_c11_post_dec_to_postdec |
| `concept:preinc` | morphism_c11_pre_inc_to_preinc |
| `concept:predec` | morphism_c11_pre_dec_to_predec |
| `concept:source-unit` | morphism_c11_source_unit_to_source_unit, morphism_csharp_source_unit_to_source_unit, morphism_python_source_unit_to_source_unit, morphism_typescript_source_unit_to_source_unit, morphism_zig_source_unit_to_source_unit, morphism_ruby_source_unit_to_source_unit, morphism_php_source_unit_to_source_unit, morphism_java_source_unit_to_source_unit |

## Gaps

| Language | Concept op | Gap kind | Gap memento | Resolution options |
| --- | --- | --- | --- | --- |
| `c11` | `concept:conditional` | `sort-mismatch` | `gap_c11_conditional_to_concept_conditional.json` | add-representation-map, accept-permanent |
| `c11` | `concept:new` | `missing-source-op` | `gap_c11_new_to_concept_new.json` | accept-permanent |
| `c11` | `concept:throw` | `missing-source-op` | `gap_c11_throw_to_concept_throw.json` | accept-permanent |
| `c11` | `concept:ushr` | `missing-source-op` | `gap_c11_ushr_to_concept_ushr.json` | accept-permanent |
| `csharp` | `concept:addr` | `missing-source-op` | `gap_csharp_addr_to_concept_addr.json` | accept-permanent |
| `csharp` | `concept:assign` | `sort-mismatch` | `gap_csharp_assign_to_concept_assign.json` | add-representation-map, accept-permanent |
| `csharp` | `concept:bitand` | `sort-mismatch` | `gap_csharp_bitand_to_concept_bitand.json` | add-representation-map, accept-permanent |
| `csharp` | `concept:bitnot` | `sort-mismatch` | `gap_csharp_bitnot_to_concept_bitnot.json` | add-representation-map, accept-permanent |
| `csharp` | `concept:bitor` | `sort-mismatch` | `gap_csharp_bitor_to_concept_bitor.json` | add-representation-map, accept-permanent |
| `csharp` | `concept:bitxor` | `sort-mismatch` | `gap_csharp_bitxor_to_concept_bitxor.json` | add-representation-map, accept-permanent |
| `csharp` | `concept:call` | `sort-mismatch` | `gap_csharp_call_to_concept_call.json` | add-representation-map, accept-permanent |
| `csharp` | `concept:cast` | `sort-mismatch` | `gap_csharp_cast_to_concept_cast.json` | add-representation-map, accept-permanent |
| `csharp` | `concept:decl` | `sort-mismatch` | `gap_csharp_decl_to_concept_decl.json` | add-representation-map, accept-permanent |
| `csharp` | `concept:deref` | `missing-source-op` | `gap_csharp_deref_to_concept_deref.json` | accept-permanent |
| `csharp` | `concept:div` | `sort-mismatch` | `gap_csharp_div_to_concept_div.json` | add-representation-map, accept-permanent |
| `csharp` | `concept:do` | `missing-source-op` | `gap_csharp_do_to_concept_do.json` | accept-permanent |
| `csharp` | `concept:ge` | `sort-mismatch` | `gap_csharp_ge_to_concept_ge.json` | add-representation-map, accept-permanent |
| `csharp` | `concept:gt` | `sort-mismatch` | `gap_csharp_gt_to_concept_gt.json` | add-representation-map, accept-permanent |
| `csharp` | `concept:index` | `sort-mismatch` | `gap_csharp_index_to_concept_index.json` | add-representation-map, accept-permanent |
| `csharp` | `concept:ite` | `sort-mismatch` | `gap_csharp_ite_to_concept_ite.json` | add-representation-map, accept-permanent |
| `csharp` | `concept:le` | `sort-mismatch` | `gap_csharp_le_to_concept_le.json` | add-representation-map, accept-permanent |
| `csharp` | `concept:lt` | `sort-mismatch` | `gap_csharp_lt_to_concept_lt.json` | add-representation-map, accept-permanent |
| `csharp` | `concept:member` | `sort-mismatch` | `gap_csharp_member_to_concept_member.json` | add-representation-map, accept-permanent |
| `csharp` | `concept:mod` | `sort-mismatch` | `gap_csharp_mod_to_concept_mod.json` | add-representation-map, accept-permanent |
| `csharp` | `concept:neg` | `sort-mismatch` | `gap_csharp_neg_to_concept_neg.json` | add-representation-map, accept-permanent |
| `csharp` | `concept:not` | `sort-mismatch` | `gap_csharp_not_to_concept_not.json` | add-representation-map, accept-permanent |
| `csharp` | `concept:postdec` | `sort-mismatch` | `gap_csharp_postdec_to_concept_postdec.json` | add-representation-map, accept-permanent |
| `csharp` | `concept:postinc` | `sort-mismatch` | `gap_csharp_postinc_to_concept_postinc.json` | add-representation-map, accept-permanent |
| `csharp` | `concept:predec` | `sort-mismatch` | `gap_csharp_predec_to_concept_predec.json` | add-representation-map, accept-permanent |
| `csharp` | `concept:preinc` | `sort-mismatch` | `gap_csharp_preinc_to_concept_preinc.json` | add-representation-map, accept-permanent |
| `csharp` | `concept:return` | `sort-mismatch` | `gap_csharp_return_to_concept_return.json` | add-representation-map, accept-permanent |
| `csharp` | `concept:shl` | `effect-mismatch` | `gap_csharp_shl_to_concept_shl.json` | accept-permanent |
| `csharp` | `concept:shr` | `effect-mismatch` | `gap_csharp_shr_to_concept_shr.json` | accept-permanent |
| `csharp` | `concept:throw` | `missing-source-op` | `gap_csharp_throw_to_concept_throw.json` | accept-permanent |
| `csharp` | `concept:ushr` | `missing-source-op` | `gap_csharp_ushr_to_concept_ushr.json` | accept-permanent |
| `go` | `concept:addr` | `sort-mismatch` | `gap_go_addr_to_concept_addr.json` | add-representation-map, accept-permanent |
| `go` | `concept:assign` | `sort-mismatch` | `gap_go_assign_to_concept_assign.json` | add-representation-map, accept-permanent |
| `go` | `concept:break` | `missing-source-op` | `gap_go_break_to_concept_break.json` | accept-permanent |
| `go` | `concept:call` | `sort-mismatch` | `gap_go_call_to_concept_call.json` | add-representation-map, accept-permanent |
| `go` | `concept:cast` | `missing-source-op` | `gap_go_cast_to_concept_cast.json` | accept-permanent |
| `go` | `concept:conditional` | `sort-mismatch` | `gap_go_conditional_to_concept_conditional.json` | add-representation-map, accept-permanent |
| `go` | `concept:continue` | `missing-source-op` | `gap_go_continue_to_concept_continue.json` | accept-permanent |
| `go` | `concept:decl` | `sort-mismatch` | `gap_go_decl_to_concept_decl.json` | add-representation-map, accept-permanent |
| `go` | `concept:deref` | `sort-mismatch` | `gap_go_deref_to_concept_deref.json` | add-representation-map, accept-permanent |
| `go` | `concept:do` | `missing-source-op` | `gap_go_do_to_concept_do.json` | accept-permanent |
| `go` | `concept:for` | `sort-mismatch` | `gap_go_for_to_concept_for.json` | add-representation-map, accept-permanent |
| `go` | `concept:index` | `sort-mismatch` | `gap_go_index_to_concept_index.json` | add-representation-map, accept-permanent |
| `go` | `concept:ite` | `missing-source-op` | `gap_go_ite_to_concept_ite.json` | accept-permanent |
| `go` | `concept:member` | `sort-mismatch` | `gap_go_member_to_concept_member.json` | add-representation-map, accept-permanent |
| `go` | `concept:new` | `missing-source-op` | `gap_go_new_to_concept_new.json` | accept-permanent |
| `go` | `concept:not` | `sort-mismatch` | `gap_go_not_to_concept_not.json` | add-representation-map, accept-permanent |
| `go` | `concept:postdec` | `missing-source-op` | `gap_go_postdec_to_concept_postdec.json` | accept-permanent |
| `go` | `concept:postinc` | `missing-source-op` | `gap_go_postinc_to_concept_postinc.json` | accept-permanent |
| `go` | `concept:predec` | `missing-source-op` | `gap_go_predec_to_concept_predec.json` | accept-permanent |
| `go` | `concept:preinc` | `missing-source-op` | `gap_go_preinc_to_concept_preinc.json` | accept-permanent |
| `go` | `concept:return` | `sort-mismatch` | `gap_go_return_to_concept_return.json` | add-representation-map, accept-permanent |
| `go` | `concept:seq` | `sort-mismatch` | `gap_go_seq_to_concept_seq.json` | add-representation-map, accept-permanent |
| `go` | `concept:skip` | `sort-mismatch` | `gap_go_skip_to_concept_skip.json` | add-representation-map, accept-permanent |
| `go` | `concept:source-unit` | `sort-mismatch` | `gap_go_source_unit_to_concept_source_unit.json` | add-representation-map, accept-permanent |
| `go` | `concept:throw` | `missing-source-op` | `gap_go_throw_to_concept_throw.json` | accept-permanent |
| `go` | `concept:ushr` | `missing-source-op` | `gap_go_ushr_to_concept_ushr.json` | accept-permanent |
| `go` | `concept:while` | `missing-source-op` | `gap_go_while_to_concept_while.json` | accept-permanent |
| `java` | `concept:addr` | `missing-source-op` | `gap_java_addr_to_concept_addr.json` | accept-permanent |
| `java` | `concept:assign` | `sort-mismatch` | `gap_java_assign_to_concept_assign.json` | add-representation-map, accept-permanent |
| `java` | `concept:bitand` | `sort-mismatch` | `gap_java_bitand_to_concept_bitand.json` | add-representation-map, accept-permanent |
| `java` | `concept:bitor` | `sort-mismatch` | `gap_java_bitor_to_concept_bitor.json` | add-representation-map, accept-permanent |
| `java` | `concept:bitxor` | `sort-mismatch` | `gap_java_bitxor_to_concept_bitxor.json` | add-representation-map, accept-permanent |
| `java` | `concept:call` | `sort-mismatch` | `gap_java_call_to_concept_call.json` | add-representation-map, accept-permanent |
| `java` | `concept:cast` | `sort-mismatch` | `gap_java_cast_to_concept_cast.json` | add-representation-map, accept-permanent |
| `java` | `concept:conditional` | `effect-mismatch` | `gap_java_conditional_to_concept_conditional.json` | accept-permanent |
| `java` | `concept:deref` | `missing-source-op` | `gap_java_deref_to_concept_deref.json` | accept-permanent |
| `java` | `concept:index` | `sort-mismatch` | `gap_java_index_to_concept_index.json` | add-representation-map, accept-permanent |
| `java` | `concept:member` | `sort-mismatch` | `gap_java_member_to_concept_member.json` | add-representation-map, accept-permanent |
| `java` | `concept:new` | `divergent-semantics` | `gap_java_new_to_concept_new.json` | partial-morphism, accept-permanent |
| `java` | `concept:postdec` | `sort-mismatch` | `gap_java_postdec_to_concept_postdec.json` | add-representation-map, accept-permanent |
| `java` | `concept:postinc` | `sort-mismatch` | `gap_java_postinc_to_concept_postinc.json` | add-representation-map, accept-permanent |
| `java` | `concept:predec` | `sort-mismatch` | `gap_java_predec_to_concept_predec.json` | add-representation-map, accept-permanent |
| `java` | `concept:preinc` | `sort-mismatch` | `gap_java_preinc_to_concept_preinc.json` | add-representation-map, accept-permanent |
| `php` | `concept:add` | `sort-mismatch` | `gap_php_add_to_concept_add.json` | add-representation-map, accept-permanent |
| `php` | `concept:addr` | `missing-source-op` | `gap_php_addr_to_concept_addr.json` | accept-permanent |
| `php` | `concept:assign` | `sort-mismatch` | `gap_php_assign_to_concept_assign.json` | add-representation-map, accept-permanent |
| `php` | `concept:bitand` | `sort-mismatch` | `gap_php_bitand_to_concept_bitand.json` | add-representation-map, accept-permanent |
| `php` | `concept:bitnot` | `sort-mismatch` | `gap_php_bitnot_to_concept_bitnot.json` | add-representation-map, accept-permanent |
| `php` | `concept:bitor` | `sort-mismatch` | `gap_php_bitor_to_concept_bitor.json` | add-representation-map, accept-permanent |
| `php` | `concept:bitxor` | `sort-mismatch` | `gap_php_bitxor_to_concept_bitxor.json` | add-representation-map, accept-permanent |
| `php` | `concept:call` | `sort-mismatch` | `gap_php_call_to_concept_call.json` | add-representation-map, accept-permanent |
| `php` | `concept:cast` | `missing-source-op` | `gap_php_cast_to_concept_cast.json` | accept-permanent |
| `php` | `concept:conditional` | `sort-mismatch` | `gap_php_conditional_to_concept_conditional.json` | add-representation-map, accept-permanent |
| `php` | `concept:decl` | `missing-source-op` | `gap_php_decl_to_concept_decl.json` | accept-permanent |
| `php` | `concept:deref` | `missing-source-op` | `gap_php_deref_to_concept_deref.json` | accept-permanent |
| `php` | `concept:div` | `missing-source-op` | `gap_php_div_to_concept_div.json` | accept-permanent |
| `php` | `concept:do` | `missing-source-op` | `gap_php_do_to_concept_do.json` | accept-permanent |
| `php` | `concept:eq` | `sort-mismatch` | `gap_php_eq_to_concept_eq.json` | add-representation-map, accept-permanent |
| `php` | `concept:for` | `sort-mismatch` | `gap_php_for_to_concept_for.json` | add-representation-map, accept-permanent |
| `php` | `concept:ge` | `sort-mismatch` | `gap_php_ge_to_concept_ge.json` | add-representation-map, accept-permanent |
| `php` | `concept:gt` | `sort-mismatch` | `gap_php_gt_to_concept_gt.json` | add-representation-map, accept-permanent |
| `php` | `concept:index` | `sort-mismatch` | `gap_php_index_to_concept_index.json` | add-representation-map, accept-permanent |
| `php` | `concept:ite` | `sort-mismatch` | `gap_php_ite_to_concept_ite.json` | add-representation-map, accept-permanent |
| `php` | `concept:le` | `sort-mismatch` | `gap_php_le_to_concept_le.json` | add-representation-map, accept-permanent |
| `php` | `concept:lt` | `sort-mismatch` | `gap_php_lt_to_concept_lt.json` | add-representation-map, accept-permanent |
| `php` | `concept:member` | `sort-mismatch` | `gap_php_member_to_concept_member.json` | add-representation-map, accept-permanent |
| `php` | `concept:mod` | `sort-mismatch` | `gap_php_mod_to_concept_mod.json` | add-representation-map, accept-permanent |
| `php` | `concept:mul` | `sort-mismatch` | `gap_php_mul_to_concept_mul.json` | add-representation-map, accept-permanent |
| `php` | `concept:ne` | `sort-mismatch` | `gap_php_ne_to_concept_ne.json` | add-representation-map, accept-permanent |
| `php` | `concept:neg` | `sort-mismatch` | `gap_php_neg_to_concept_neg.json` | add-representation-map, accept-permanent |
| `php` | `concept:new` | `missing-source-op` | `gap_php_new_to_concept_new.json` | accept-permanent |
| `php` | `concept:not` | `sort-mismatch` | `gap_php_not_to_concept_not.json` | add-representation-map, accept-permanent |
| `php` | `concept:postdec` | `missing-source-op` | `gap_php_postdec_to_concept_postdec.json` | accept-permanent |
| `php` | `concept:postinc` | `missing-source-op` | `gap_php_postinc_to_concept_postinc.json` | accept-permanent |
| `php` | `concept:predec` | `missing-source-op` | `gap_php_predec_to_concept_predec.json` | accept-permanent |
| `php` | `concept:preinc` | `missing-source-op` | `gap_php_preinc_to_concept_preinc.json` | accept-permanent |
| `php` | `concept:return` | `sort-mismatch` | `gap_php_return_to_concept_return.json` | add-representation-map, accept-permanent |
| `php` | `concept:shl` | `sort-mismatch` | `gap_php_shl_to_concept_shl.json` | add-representation-map, accept-permanent |
| `php` | `concept:shr` | `sort-mismatch` | `gap_php_shr_to_concept_shr.json` | add-representation-map, accept-permanent |
| `php` | `concept:skip` | `missing-source-op` | `gap_php_skip_to_concept_skip.json` | accept-permanent |
| `php` | `concept:sub` | `sort-mismatch` | `gap_php_sub_to_concept_sub.json` | add-representation-map, accept-permanent |
| `php` | `concept:ushr` | `missing-source-op` | `gap_php_ushr_to_concept_ushr.json` | accept-permanent |
| `php` | `concept:while` | `sort-mismatch` | `gap_php_while_to_concept_while.json` | add-representation-map, accept-permanent |
| `python` | `concept:add` | `polymorphic-source-op` | `gap_python_add_to_concept_add.json` | partial-morphism, accept-permanent |
| `python` | `concept:addr` | `missing-source-op` | `gap_python_addr_to_concept_addr.json` | accept-permanent |
| `python` | `concept:assign` | `sort-mismatch` | `gap_python_assign_to_concept_assign.json` | add-representation-map, accept-permanent |
| `python` | `concept:bitand` | `sort-mismatch` | `gap_python_bitand_to_concept_bitand.json` | add-representation-map, accept-permanent |
| `python` | `concept:bitnot` | `missing-source-op` | `gap_python_bitnot_to_concept_bitnot.json` | accept-permanent |
| `python` | `concept:bitor` | `sort-mismatch` | `gap_python_bitor_to_concept_bitor.json` | add-representation-map, accept-permanent |
| `python` | `concept:bitxor` | `sort-mismatch` | `gap_python_bitxor_to_concept_bitxor.json` | add-representation-map, accept-permanent |
| `python` | `concept:break` | `effect-mismatch` | `gap_python_break_to_concept_break.json` | accept-permanent |
| `python` | `concept:call` | `sort-mismatch` | `gap_python_call_to_concept_call.json` | add-representation-map, accept-permanent |
| `python` | `concept:cast` | `missing-source-op` | `gap_python_cast_to_concept_cast.json` | accept-permanent |
| `python` | `concept:continue` | `effect-mismatch` | `gap_python_continue_to_concept_continue.json` | accept-permanent |
| `python` | `concept:decl` | `missing-source-op` | `gap_python_decl_to_concept_decl.json` | accept-permanent |
| `python` | `concept:deref` | `missing-source-op` | `gap_python_deref_to_concept_deref.json` | accept-permanent |
| `python` | `concept:div` | `missing-source-op` | `gap_python_div_to_concept_div.json` | accept-permanent |
| `python` | `concept:do` | `missing-source-op` | `gap_python_do_to_concept_do.json` | accept-permanent |
| `python` | `concept:eq` | `missing-source-op` | `gap_python_eq_to_concept_eq.json` | accept-permanent |
| `python` | `concept:for` | `sort-mismatch` | `gap_python_for_to_concept_for.json` | add-representation-map, accept-permanent |
| `python` | `concept:ge` | `missing-source-op` | `gap_python_ge_to_concept_ge.json` | accept-permanent |
| `python` | `concept:gt` | `missing-source-op` | `gap_python_gt_to_concept_gt.json` | accept-permanent |
| `python` | `concept:index` | `sort-mismatch` | `gap_python_index_to_concept_index.json` | add-representation-map, accept-permanent |
| `python` | `concept:ite` | `sort-mismatch` | `gap_python_ite_to_concept_ite.json` | add-representation-map, accept-permanent |
| `python` | `concept:le` | `missing-source-op` | `gap_python_le_to_concept_le.json` | accept-permanent |
| `python` | `concept:lt` | `missing-source-op` | `gap_python_lt_to_concept_lt.json` | accept-permanent |
| `python` | `concept:member` | `missing-source-op` | `gap_python_member_to_concept_member.json` | accept-permanent |
| `python` | `concept:mod` | `divergent-semantics` | `gap_python_mod_to_concept_mod.json` | partial-morphism, accept-permanent |
| `python` | `concept:mul` | `polymorphic-source-op` | `gap_python_mul_to_concept_mul.json` | partial-morphism, accept-permanent |
| `python` | `concept:ne` | `missing-source-op` | `gap_python_ne_to_concept_ne.json` | accept-permanent |
| `python` | `concept:neg` | `polymorphic-source-op` | `gap_python_neg_to_concept_neg.json` | partial-morphism, accept-permanent |
| `python` | `concept:new` | `sort-mismatch` | `gap_python_new_to_concept_new.json` | add-representation-map, accept-permanent |
| `python` | `concept:not` | `arity-shape-mismatch` | `gap_python_not_to_concept_not.json` | re-spec-target-op, accept-permanent |
| `python` | `concept:postdec` | `missing-source-op` | `gap_python_postdec_to_concept_postdec.json` | accept-permanent |
| `python` | `concept:postinc` | `missing-source-op` | `gap_python_postinc_to_concept_postinc.json` | accept-permanent |
| `python` | `concept:predec` | `missing-source-op` | `gap_python_predec_to_concept_predec.json` | accept-permanent |
| `python` | `concept:preinc` | `missing-source-op` | `gap_python_preinc_to_concept_preinc.json` | accept-permanent |
| `python` | `concept:return` | `sort-mismatch` | `gap_python_return_to_concept_return.json` | add-representation-map, accept-permanent |
| `python` | `concept:shl` | `missing-source-op` | `gap_python_shl_to_concept_shl.json` | accept-permanent |
| `python` | `concept:shr` | `missing-source-op` | `gap_python_shr_to_concept_shr.json` | accept-permanent |
| `python` | `concept:skip` | `missing-source-op` | `gap_python_skip_to_concept_skip.json` | accept-permanent |
| `python` | `concept:sub` | `polymorphic-source-op` | `gap_python_sub_to_concept_sub.json` | partial-morphism, accept-permanent |
| `python` | `concept:throw` | `effect-mismatch` | `gap_python_throw_to_concept_throw.json` | accept-permanent |
| `python` | `concept:ushr` | `missing-source-op` | `gap_python_ushr_to_concept_ushr.json` | accept-permanent |
| `ruby` | `concept:add` | `sort-mismatch` | `gap_ruby_add_to_concept_add.json` | add-representation-map, accept-permanent |
| `ruby` | `concept:addr` | `missing-source-op` | `gap_ruby_addr_to_concept_addr.json` | accept-permanent |
| `ruby` | `concept:assign` | `sort-mismatch` | `gap_ruby_assign_to_concept_assign.json` | add-representation-map, accept-permanent |
| `ruby` | `concept:bitand` | `sort-mismatch` | `gap_ruby_bitand_to_concept_bitand.json` | add-representation-map, accept-permanent |
| `ruby` | `concept:bitnot` | `missing-source-op` | `gap_ruby_bitnot_to_concept_bitnot.json` | accept-permanent |
| `ruby` | `concept:bitor` | `sort-mismatch` | `gap_ruby_bitor_to_concept_bitor.json` | add-representation-map, accept-permanent |
| `ruby` | `concept:bitxor` | `sort-mismatch` | `gap_ruby_bitxor_to_concept_bitxor.json` | add-representation-map, accept-permanent |
| `ruby` | `concept:break` | `missing-source-op` | `gap_ruby_break_to_concept_break.json` | accept-permanent |
| `ruby` | `concept:call` | `sort-mismatch` | `gap_ruby_call_to_concept_call.json` | add-representation-map, accept-permanent |
| `ruby` | `concept:cast` | `missing-source-op` | `gap_ruby_cast_to_concept_cast.json` | accept-permanent |
| `ruby` | `concept:continue` | `missing-source-op` | `gap_ruby_continue_to_concept_continue.json` | accept-permanent |
| `ruby` | `concept:decl` | `missing-source-op` | `gap_ruby_decl_to_concept_decl.json` | accept-permanent |
| `ruby` | `concept:deref` | `missing-source-op` | `gap_ruby_deref_to_concept_deref.json` | accept-permanent |
| `ruby` | `concept:div` | `missing-source-op` | `gap_ruby_div_to_concept_div.json` | accept-permanent |
| `ruby` | `concept:do` | `missing-source-op` | `gap_ruby_do_to_concept_do.json` | accept-permanent |
| `ruby` | `concept:eq` | `sort-mismatch` | `gap_ruby_eq_to_concept_eq.json` | add-representation-map, accept-permanent |
| `ruby` | `concept:for` | `sort-mismatch` | `gap_ruby_for_to_concept_for.json` | add-representation-map, accept-permanent |
| `ruby` | `concept:ge` | `sort-mismatch` | `gap_ruby_ge_to_concept_ge.json` | add-representation-map, accept-permanent |
| `ruby` | `concept:gt` | `sort-mismatch` | `gap_ruby_gt_to_concept_gt.json` | add-representation-map, accept-permanent |
| `ruby` | `concept:index` | `sort-mismatch` | `gap_ruby_index_to_concept_index.json` | add-representation-map, accept-permanent |
| `ruby` | `concept:ite` | `sort-mismatch` | `gap_ruby_ite_to_concept_ite.json` | add-representation-map, accept-permanent |
| `ruby` | `concept:le` | `sort-mismatch` | `gap_ruby_le_to_concept_le.json` | add-representation-map, accept-permanent |
| `ruby` | `concept:lt` | `sort-mismatch` | `gap_ruby_lt_to_concept_lt.json` | add-representation-map, accept-permanent |
| `ruby` | `concept:member` | `missing-source-op` | `gap_ruby_member_to_concept_member.json` | accept-permanent |
| `ruby` | `concept:mod` | `sort-mismatch` | `gap_ruby_mod_to_concept_mod.json` | add-representation-map, accept-permanent |
| `ruby` | `concept:mul` | `sort-mismatch` | `gap_ruby_mul_to_concept_mul.json` | add-representation-map, accept-permanent |
| `ruby` | `concept:ne` | `sort-mismatch` | `gap_ruby_ne_to_concept_ne.json` | add-representation-map, accept-permanent |
| `ruby` | `concept:neg` | `sort-mismatch` | `gap_ruby_neg_to_concept_neg.json` | add-representation-map, accept-permanent |
| `ruby` | `concept:new` | `missing-source-op` | `gap_ruby_new_to_concept_new.json` | accept-permanent |
| `ruby` | `concept:not` | `arity-shape-mismatch` | `gap_ruby_not_to_concept_not.json` | re-spec-target-op, accept-permanent |
| `ruby` | `concept:postdec` | `missing-source-op` | `gap_ruby_postdec_to_concept_postdec.json` | accept-permanent |
| `ruby` | `concept:postinc` | `missing-source-op` | `gap_ruby_postinc_to_concept_postinc.json` | accept-permanent |
| `ruby` | `concept:predec` | `missing-source-op` | `gap_ruby_predec_to_concept_predec.json` | accept-permanent |
| `ruby` | `concept:preinc` | `missing-source-op` | `gap_ruby_preinc_to_concept_preinc.json` | accept-permanent |
| `ruby` | `concept:return` | `sort-mismatch` | `gap_ruby_return_to_concept_return.json` | add-representation-map, accept-permanent |
| `ruby` | `concept:shl` | `missing-source-op` | `gap_ruby_shl_to_concept_shl.json` | accept-permanent |
| `ruby` | `concept:shr` | `missing-source-op` | `gap_ruby_shr_to_concept_shr.json` | accept-permanent |
| `ruby` | `concept:skip` | `missing-source-op` | `gap_ruby_skip_to_concept_skip.json` | accept-permanent |
| `ruby` | `concept:sub` | `sort-mismatch` | `gap_ruby_sub_to_concept_sub.json` | add-representation-map, accept-permanent |
| `ruby` | `concept:throw` | `effect-mismatch` | `gap_ruby_throw_to_concept_throw.json` | accept-permanent |
| `ruby` | `concept:ushr` | `missing-source-op` | `gap_ruby_ushr_to_concept_ushr.json` | accept-permanent |
| `rust` | `concept:add` | `sort-mismatch` | `gap_rust_add_to_concept_add.json` | add-representation-map, accept-permanent |
| `rust` | `concept:addr` | `sort-mismatch` | `gap_rust_addr_to_concept_addr.json` | add-representation-map, accept-permanent |
| `rust` | `concept:assign` | `sort-mismatch` | `gap_rust_assign_to_concept_assign.json` | add-representation-map, accept-permanent |
| `rust` | `concept:bitand` | `sort-mismatch` | `gap_rust_bitand_to_concept_bitand.json` | add-representation-map, accept-permanent |
| `rust` | `concept:bitnot` | `sort-mismatch` | `gap_rust_bitnot_to_concept_bitnot.json` | add-representation-map, accept-permanent |
| `rust` | `concept:bitor` | `sort-mismatch` | `gap_rust_bitor_to_concept_bitor.json` | add-representation-map, accept-permanent |
| `rust` | `concept:bitxor` | `sort-mismatch` | `gap_rust_bitxor_to_concept_bitxor.json` | add-representation-map, accept-permanent |
| `rust` | `concept:break` | `effect-mismatch` | `gap_rust_break_to_concept_break.json` | accept-permanent |
| `rust` | `concept:call` | `effect-mismatch` | `gap_rust_call_to_concept_call.json` | accept-permanent |
| `rust` | `concept:cast` | `sort-mismatch` | `gap_rust_cast_to_concept_cast.json` | add-representation-map, accept-permanent |
| `rust` | `concept:continue` | `effect-mismatch` | `gap_rust_continue_to_concept_continue.json` | accept-permanent |
| `rust` | `concept:decl` | `sort-mismatch` | `gap_rust_decl_to_concept_decl.json` | add-representation-map, accept-permanent |
| `rust` | `concept:deref` | `sort-mismatch` | `gap_rust_deref_to_concept_deref.json` | add-representation-map, accept-permanent |
| `rust` | `concept:div` | `sort-mismatch` | `gap_rust_div_to_concept_div.json` | add-representation-map, accept-permanent |
| `rust` | `concept:do` | `missing-source-op` | `gap_rust_do_to_concept_do.json` | accept-permanent |
| `rust` | `concept:ge` | `sort-mismatch` | `gap_rust_ge_to_concept_ge.json` | add-representation-map, accept-permanent |
| `rust` | `concept:gt` | `sort-mismatch` | `gap_rust_gt_to_concept_gt.json` | add-representation-map, accept-permanent |
| `rust` | `concept:index` | `sort-mismatch` | `gap_rust_index_to_concept_index.json` | add-representation-map, accept-permanent |
| `rust` | `concept:ite` | `sort-mismatch` | `gap_rust_ite_to_concept_ite.json` | add-representation-map, accept-permanent |
| `rust` | `concept:le` | `sort-mismatch` | `gap_rust_le_to_concept_le.json` | add-representation-map, accept-permanent |
| `rust` | `concept:lt` | `sort-mismatch` | `gap_rust_lt_to_concept_lt.json` | add-representation-map, accept-permanent |
| `rust` | `concept:member` | `sort-mismatch` | `gap_rust_member_to_concept_member.json` | add-representation-map, accept-permanent |
| `rust` | `concept:mod` | `sort-mismatch` | `gap_rust_mod_to_concept_mod.json` | add-representation-map, accept-permanent |
| `rust` | `concept:mul` | `sort-mismatch` | `gap_rust_mul_to_concept_mul.json` | add-representation-map, accept-permanent |
| `rust` | `concept:ne` | `sort-mismatch` | `gap_rust_ne_to_concept_ne.json` | add-representation-map, accept-permanent |
| `rust` | `concept:neg` | `sort-mismatch` | `gap_rust_neg_to_concept_neg.json` | add-representation-map, accept-permanent |
| `rust` | `concept:new` | `missing-source-op` | `gap_rust_new_to_concept_new.json` | accept-permanent |
| `rust` | `concept:not` | `arity-shape-mismatch` | `gap_rust_not_to_concept_not.json` | re-spec-target-op, accept-permanent |
| `rust` | `concept:postdec` | `missing-source-op` | `gap_rust_postdec_to_concept_postdec.json` | accept-permanent |
| `rust` | `concept:postinc` | `missing-source-op` | `gap_rust_postinc_to_concept_postinc.json` | accept-permanent |
| `rust` | `concept:predec` | `missing-source-op` | `gap_rust_predec_to_concept_predec.json` | accept-permanent |
| `rust` | `concept:preinc` | `missing-source-op` | `gap_rust_preinc_to_concept_preinc.json` | accept-permanent |
| `rust` | `concept:shl` | `sort-mismatch` | `gap_rust_shl_to_concept_shl.json` | add-representation-map, accept-permanent |
| `rust` | `concept:shr` | `sort-mismatch` | `gap_rust_shr_to_concept_shr.json` | add-representation-map, accept-permanent |
| `rust` | `concept:source-unit` | `missing-source-op` | `gap_rust_source_unit_to_concept_source_unit.json` | accept-permanent |
| `rust` | `concept:sub` | `sort-mismatch` | `gap_rust_sub_to_concept_sub.json` | add-representation-map, accept-permanent |
| `rust` | `concept:throw` | `sort-mismatch` | `gap_rust_throw_to_concept_throw.json` | add-representation-map, accept-permanent |
| `rust` | `concept:ushr` | `missing-source-op` | `gap_rust_ushr_to_concept_ushr.json` | accept-permanent |
| `typescript` | `concept:add` | `polymorphic-source-op` | `gap_typescript_add_to_concept_add.json` | partial-morphism, accept-permanent |
| `typescript` | `concept:addr` | `missing-source-op` | `gap_typescript_addr_to_concept_addr.json` | accept-permanent |
| `typescript` | `concept:bitand` | `sort-mismatch` | `gap_typescript_bitand_to_concept_bitand.json` | add-representation-map, accept-permanent |
| `typescript` | `concept:bitnot` | `sort-mismatch` | `gap_typescript_bitnot_to_concept_bitnot.json` | add-representation-map, accept-permanent |
| `typescript` | `concept:bitor` | `sort-mismatch` | `gap_typescript_bitor_to_concept_bitor.json` | add-representation-map, accept-permanent |
| `typescript` | `concept:bitxor` | `sort-mismatch` | `gap_typescript_bitxor_to_concept_bitxor.json` | add-representation-map, accept-permanent |
| `typescript` | `concept:call` | `sort-mismatch` | `gap_typescript_call_to_concept_call.json` | add-representation-map, accept-permanent |
| `typescript` | `concept:cast` | `missing-source-op` | `gap_typescript_cast_to_concept_cast.json` | accept-permanent |
| `typescript` | `concept:conditional` | `sort-mismatch` | `gap_typescript_conditional_to_concept_conditional.json` | add-representation-map, accept-permanent |
| `typescript` | `concept:deref` | `missing-source-op` | `gap_typescript_deref_to_concept_deref.json` | accept-permanent |
| `typescript` | `concept:div` | `missing-source-op` | `gap_typescript_div_to_concept_div.json` | accept-permanent |
| `typescript` | `concept:do` | `missing-source-op` | `gap_typescript_do_to_concept_do.json` | accept-permanent |
| `typescript` | `concept:eq` | `sort-mismatch` | `gap_typescript_eq_to_concept_eq.json` | add-representation-map, accept-permanent |
| `typescript` | `concept:for` | `sort-mismatch` | `gap_typescript_for_to_concept_for.json` | add-representation-map, accept-permanent |
| `typescript` | `concept:ge` | `sort-mismatch` | `gap_typescript_ge_to_concept_ge.json` | add-representation-map, accept-permanent |
| `typescript` | `concept:gt` | `sort-mismatch` | `gap_typescript_gt_to_concept_gt.json` | add-representation-map, accept-permanent |
| `typescript` | `concept:index` | `effect-mismatch` | `gap_typescript_index_to_concept_index.json` | accept-permanent |
| `typescript` | `concept:ite` | `sort-mismatch` | `gap_typescript_ite_to_concept_ite.json` | add-representation-map, accept-permanent |
| `typescript` | `concept:le` | `sort-mismatch` | `gap_typescript_le_to_concept_le.json` | add-representation-map, accept-permanent |
| `typescript` | `concept:lt` | `sort-mismatch` | `gap_typescript_lt_to_concept_lt.json` | add-representation-map, accept-permanent |
| `typescript` | `concept:member` | `sort-mismatch` | `gap_typescript_member_to_concept_member.json` | add-representation-map, accept-permanent |
| `typescript` | `concept:mod` | `sort-mismatch` | `gap_typescript_mod_to_concept_mod.json` | add-representation-map, accept-permanent |
| `typescript` | `concept:mul` | `sort-mismatch` | `gap_typescript_mul_to_concept_mul.json` | add-representation-map, accept-permanent |
| `typescript` | `concept:ne` | `sort-mismatch` | `gap_typescript_ne_to_concept_ne.json` | add-representation-map, accept-permanent |
| `typescript` | `concept:neg` | `sort-mismatch` | `gap_typescript_neg_to_concept_neg.json` | add-representation-map, accept-permanent |
| `typescript` | `concept:new` | `sort-mismatch` | `gap_typescript_new_to_concept_new.json` | add-representation-map, accept-permanent |
| `typescript` | `concept:not` | `sort-mismatch` | `gap_typescript_not_to_concept_not.json` | add-representation-map, accept-permanent |
| `typescript` | `concept:postdec` | `sort-mismatch` | `gap_typescript_postdec_to_concept_postdec.json` | add-representation-map, accept-permanent |
| `typescript` | `concept:postinc` | `sort-mismatch` | `gap_typescript_postinc_to_concept_postinc.json` | add-representation-map, accept-permanent |
| `typescript` | `concept:predec` | `sort-mismatch` | `gap_typescript_predec_to_concept_predec.json` | add-representation-map, accept-permanent |
| `typescript` | `concept:preinc` | `sort-mismatch` | `gap_typescript_preinc_to_concept_preinc.json` | add-representation-map, accept-permanent |
| `typescript` | `concept:seq` | `sort-mismatch` | `gap_typescript_seq_to_concept_seq.json` | add-representation-map, accept-permanent |
| `typescript` | `concept:shl` | `sort-mismatch` | `gap_typescript_shl_to_concept_shl.json` | add-representation-map, accept-permanent |
| `typescript` | `concept:shr` | `sort-mismatch` | `gap_typescript_shr_to_concept_shr.json` | add-representation-map, accept-permanent |
| `typescript` | `concept:skip` | `missing-source-op` | `gap_typescript_skip_to_concept_skip.json` | accept-permanent |
| `typescript` | `concept:sub` | `sort-mismatch` | `gap_typescript_sub_to_concept_sub.json` | add-representation-map, accept-permanent |
| `typescript` | `concept:while` | `sort-mismatch` | `gap_typescript_while_to_concept_while.json` | add-representation-map, accept-permanent |
| `zig` | `concept:call` | `sort-mismatch` | `gap_zig_call_to_concept_call.json` | add-representation-map, accept-permanent |
| `zig` | `concept:cast` | `sort-mismatch` | `gap_zig_cast_to_concept_cast.json` | add-representation-map, accept-permanent |
| `zig` | `concept:deref` | `sort-mismatch` | `gap_zig_deref_to_concept_deref.json` | add-representation-map, accept-permanent |
| `zig` | `concept:do` | `missing-source-op` | `gap_zig_do_to_concept_do.json` | accept-permanent |
| `zig` | `concept:for` | `sort-mismatch` | `gap_zig_for_to_concept_for.json` | add-representation-map, accept-permanent |
| `zig` | `concept:index` | `sort-mismatch` | `gap_zig_index_to_concept_index.json` | add-representation-map, accept-permanent |
| `zig` | `concept:ite` | `missing-source-op` | `gap_zig_ite_to_concept_ite.json` | accept-permanent |
| `zig` | `concept:member` | `sort-mismatch` | `gap_zig_member_to_concept_member.json` | add-representation-map, accept-permanent |
| `zig` | `concept:new` | `missing-source-op` | `gap_zig_new_to_concept_new.json` | accept-permanent |
| `zig` | `concept:postdec` | `missing-source-op` | `gap_zig_postdec_to_concept_postdec.json` | accept-permanent |
| `zig` | `concept:postinc` | `missing-source-op` | `gap_zig_postinc_to_concept_postinc.json` | accept-permanent |
| `zig` | `concept:predec` | `missing-source-op` | `gap_zig_predec_to_concept_predec.json` | accept-permanent |
| `zig` | `concept:preinc` | `missing-source-op` | `gap_zig_preinc_to_concept_preinc.json` | accept-permanent |
| `zig` | `concept:throw` | `sort-mismatch` | `gap_zig_throw_to_concept_throw.json` | add-representation-map, accept-permanent |
| `zig` | `concept:ushr` | `missing-source-op` | `gap_zig_ushr_to_concept_ushr.json` | accept-permanent |

T Savo
