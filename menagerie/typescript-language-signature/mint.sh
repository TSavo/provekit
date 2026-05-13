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
COMPONENT_FILE="$BASE/component-cids.json"
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
for spec in sort_any.spec.json sort_args.spec.json sort_boolean.spec.json sort_expr.spec.json sort_lvalue.spec.json sort_null.spec.json sort_number.spec.json sort_stmt.spec.json sort_string.spec.json sort_unit.spec.json; do
  mint_one sort "$spec"
done
for spec in op_add.spec.json op_and.spec.json op_args.spec.json op_assign.spec.json op_bitand.spec.json op_bitnot.spec.json op_bitor.spec.json op_bitxor.spec.json op_break.spec.json op_call.spec.json op_continue.spec.json op_decl.spec.json op_div.spec.json op_eq.spec.json op_for.spec.json op_ge.spec.json op_gt.spec.json op_if.spec.json op_index.spec.json op_ite.spec.json op_le.spec.json op_lt.spec.json op_member.spec.json op_mod.spec.json op_mul.spec.json op_ne.spec.json op_neg.spec.json op_new.spec.json op_not.spec.json op_nullish.spec.json op_or.spec.json op_pos.spec.json op_postdec.spec.json op_postinc.spec.json op_predec.spec.json op_preinc.spec.json op_return.spec.json op_seq.spec.json op_shl.spec.json op_shr.spec.json op_source-unit.spec.json op_sub.spec.json op_throw.spec.json op_typeof.spec.json op_ushr.spec.json op_while.spec.json; do
  mint_one algorithm "$spec"
done
for spec in eff_io.spec.json eff_opaque_loop.spec.json eff_panic.spec.json eff_read.spec.json eff_unresolved_call.spec.json eff_write.spec.json; do
  mint_one algorithm "$spec"
done
for spec in eq_and_to_ite_desugar.spec.json eq_or_to_ite_desugar.spec.json eq_seq_assoc.spec.json eq_seq_empty_left.spec.json eq_seq_empty_right.spec.json; do
  mint_one equation "$spec"
done
for spec in effsig_call.spec.json effsig_io.spec.json effsig_loop.spec.json effsig_panic.spec.json effsig_read.spec.json effsig_write.spec.json; do
  mint_one effect-signature "$spec"
done
mint_one language-signature language_signature_typescript.spec.json
python3 - "$CID_FILE" "$COMPONENT_FILE" <<'PY'
import json
import sys
from pathlib import Path
rows = []
for line in Path(sys.argv[1]).read_text(encoding="utf-8").splitlines():
    if not line.strip():
        continue
    parts = line.split("\t")
    if len(parts) != 4:
        raise SystemExit(f"bad cids.tsv row: {line}")
    kind, spec, cid, path = parts
    rows.append({"kind": kind, "spec": spec, "cid": cid, "path": path})
Path(sys.argv[2]).write_text(json.dumps(rows, indent=2) + "\n", encoding="utf-8")
PY
