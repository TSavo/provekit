#!/bin/sh
set -eu
BASE="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
ROOT="$(CDPATH= cd -- "$BASE/../.." && pwd)"
RUST_DIR="$ROOT/implementations/rust"
PROVEKIT="$RUST_DIR/target/debug/provekit"
SPEC_DIR="$BASE/specs"
CATALOG_REAL="$BASE/catalog"
CATALOG_ARG="$BASE/dev/../catalog"
CID_FILE="$BASE/cids.tsv"
mkdir -p "$BASE/dev"
rm -rf "$CATALOG_REAL"
cargo build --manifest-path "$RUST_DIR/Cargo.toml" -p provekit-cli -p provekit-ir-compiler-maude
: > "$CID_FILE"
mint_one() {
  kind="$1"
  spec="$2"
  out=$("$PROVEKIT" mint "$kind" --spec "$SPEC_DIR/$spec" --unsigned --catalog "$CATALOG_ARG")
  printf '%s\t%s\t%s\n' "$kind" "$spec" "$out" | tee -a "$CID_FILE"
}
for spec in sort_stmt.spec.json sort_expr.spec.json sort_lvalue.spec.json sort_int.spec.json sort_ptr.spec.json sort_bool.spec.json sort_unit.spec.json sort_fncontract.spec.json sort_fieldname.spec.json sort_listofstmt.spec.json sort_listofexpr.spec.json sort_addr.spec.json sort_value.spec.json sort_reason.spec.json sort_bottom.spec.json; do
  mint_one sort "$spec"
done
for spec in op_skip.spec.json op_seq.spec.json op_if.spec.json op_while.spec.json op_for.spec.json op_switch.spec.json op_call.spec.json op_return.spec.json op_break.spec.json op_continue.spec.json op_deref.spec.json op_member.spec.json op_add.spec.json op_sub.spec.json op_mul.spec.json op_eq.spec.json op_lt.spec.json op_le.spec.json op_and.spec.json op_or.spec.json op_not.spec.json op_assign.spec.json op_neg.spec.json op_source_unit.spec.json op_opaque.spec.json op_decl.spec.json op_case.spec.json op_default.spec.json op_label.spec.json op_goto.spec.json op_do.spec.json op_cast.spec.json op_array_subscript.spec.json op_conditional.spec.json op_compound_literal.spec.json op_init_list.spec.json op_string_literal.spec.json op_char_literal.spec.json op_float_literal.spec.json op_imaginary_literal.spec.json op_null.spec.json op_sizeof_expr.spec.json op_sizeof_type.spec.json op_alignof_expr.spec.json op_alignof_type.spec.json op_typeof_expr.spec.json op_typeof_type.spec.json op_offsetof.spec.json op_builtin_types_compatible_p.spec.json op_builtin_choose_expr.spec.json op_generic_selection.spec.json op_stmt_expr.spec.json op_addr_label.spec.json op_asm_link_edge.spec.json op_div.spec.json op_mod.spec.json op_shl.spec.json op_shr.spec.json op_bit_and.spec.json op_bit_or.spec.json op_bit_xor.spec.json op_gt.spec.json op_ge.spec.json op_ne.spec.json op_comma.spec.json op_bit_not.spec.json op_addr_of.spec.json op_pre_inc.spec.json op_post_inc.spec.json op_pre_dec.spec.json op_post_dec.spec.json op_plus.spec.json op_unexposed_stmt.spec.json op_unexposed_expr.spec.json op_binary_operator.spec.json op_unary_operator.spec.json op_bop_add.spec.json op_bop_sub.spec.json op_bop_mul.spec.json op_bop_div.spec.json op_bop_mod.spec.json op_bop_shl.spec.json op_bop_shr.spec.json op_bop_bitand.spec.json op_bop_bitor.spec.json op_bop_bitxor.spec.json op_bop_eq.spec.json op_bop_ne.spec.json op_bop_lt.spec.json op_bop_le.spec.json op_bop_gt.spec.json op_bop_ge.spec.json op_bop_logand.spec.json op_bop_logor.spec.json op_bop_comma.spec.json op_uop_neg.spec.json op_uop_lognot.spec.json op_uop_deref.spec.json op_uop_bitnot.spec.json op_uop_addr_of.spec.json op_uop_pre_inc.spec.json op_uop_post_inc.spec.json op_uop_pre_dec.spec.json op_uop_post_dec.spec.json op_uop_plus.spec.json op_compound_assign_add.spec.json op_compound_assign_sub.spec.json op_compound_assign_mul.spec.json op_compound_assign_div.spec.json op_compound_assign_mod.spec.json op_compound_assign_shl.spec.json op_compound_assign_shr.spec.json op_compound_assign_bitand.spec.json op_compound_assign_bitor.spec.json op_compound_assign_bitxor.spec.json eff_op_read.spec.json eff_op_write.spec.json eff_op_input.spec.json eff_op_output.spec.json eff_op_trap.spec.json; do
  mint_one algorithm "$spec"
done
for spec in eq_seq_assoc.spec.json eq_seq_skip_left.spec.json eq_seq_skip_right.spec.json eq_if_true.spec.json eq_if_false.spec.json eq_if_idemp.spec.json eq_while_false.spec.json eq_for_desugar.spec.json eq_and_false.spec.json eq_and_true.spec.json eq_or_true.spec.json eq_or_false.spec.json eq_not_not.spec.json eff_eq_read_after_write.spec.json; do
  mint_one equation "$spec"
done
for spec in effsig_memread.spec.json effsig_memwrite.spec.json effsig_io.spec.json effsig_trap.spec.json; do
  mint_one effect-signature "$spec"
done
mint_one language-signature language_signature_c11.spec.json
