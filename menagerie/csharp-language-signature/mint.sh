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
for spec in $(ls "$SPEC_DIR"/sort_*.spec.json 2>/dev/null | xargs -I{} basename {} | sort); do
  mint_one sort "$spec"
done
for spec in $(ls "$SPEC_DIR"/op_*.spec.json 2>/dev/null | xargs -I{} basename {} | sort); do
  mint_one algorithm "$spec"
done
for spec in $(ls "$SPEC_DIR"/eff_*.spec.json 2>/dev/null | xargs -I{} basename {} | sort); do
  mint_one algorithm "$spec"
done
for spec in $(ls "$SPEC_DIR"/eq_*.spec.json 2>/dev/null | xargs -I{} basename {} | sort); do
  mint_one equation "$spec"
done
for spec in $(ls "$SPEC_DIR"/effsig_*.spec.json 2>/dev/null | xargs -I{} basename {} | sort); do
  mint_one effect-signature "$spec"
done
mint_one language-signature language_signature_csharp.spec.json
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
