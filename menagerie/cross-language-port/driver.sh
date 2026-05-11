#!/bin/sh
set -eu
BASE="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
ROOT="$(CDPATH= cd -- "$BASE/../.." && pwd)"
OUT="$BASE/artifacts"
TARGETS="csharp go python typescript zig ruby php java rust"
FUNCTIONS="foo sum_to classify"
mkdir -p "$OUT"
make -C "$ROOT/implementations/c/provekit-walk-c"
cargo build --manifest-path "$ROOT/implementations/rust/Cargo.toml" -p provekit-cli
for fn in $FUNCTIONS; do
  src="$BASE/$fn.c"
  for target in $TARGETS; do
    case "$target" in
      csharp) ext=cs ;;
      typescript) ext=ts ;;
      python) ext=py ;;
      ruby) ext=rb ;;
      rust) ext=rs ;;
      *) ext="$target" ;;
    esac
    dir="$OUT/$fn/$target"
    mkdir -p "$dir"
    "$ROOT/implementations/rust/target/debug/provekit" transport \
      "$src" \
      --to "$target" \
      --function "$fn" \
      --out "$dir" \
      --json > "$dir/transport-report.json"
    cmp "$dir/concept.term.json" "$dir/roundtrip.concept.term.json"
    test -s "$dir/$fn.$ext"
    printf '%s -> %s: roundtrip concept artifact ok\n' "$fn" "$target"
  done
done
python3 - "$OUT" "$ROOT" <<'INNER_PY'
import json
import sys
from pathlib import Path
out = Path(sys.argv[1])
receipts = {}
for report_path in sorted(out.glob('*/*/transport-report.json')):
    report = json.load(open(report_path, encoding='utf-8'))
    key = f"{report['function']}:{report['target_language']}"
    receipts[key] = report.get('morphism_receipts', [])
with open(out / 'receipt-cids.tsv', 'w', encoding='utf-8') as handle:
    handle.write('case\treceipt\tcid\n')
    for key, values in sorted(receipts.items()):
        for item in values:
            name, _, cid = item.partition('=')
            handle.write(f'{key}\t{name}\t{cid}\n')
contract_exhibit = {
    'kind': 'function-contract-transport-exhibit',
    'status': 'worked-contract-artifact',
    'source_contract': 'menagerie/c11-language-signature/example/foo.contract.json',
    'target_contract': 'menagerie/rust-language-signature/example/foo.contract.json',
    'transport_path': ['c11:* -> concept:*', 'concept:* -> rust:*'],
    'lemma': 'paper-13 Lemma 4 proof transport over discharged morphisms',
    'operation_receipts': receipts.get('foo:rust', []),
    'note': 'This records the contract-level exhibit path. The CLI runtime currently transports terms and preserves operation wp; proof envelope emission is a follow-up adapter.'
}
with open(out / 'foo_contract_transport_rust.json', 'w', encoding='utf-8') as handle:
    json.dump(contract_exhibit, handle, indent=2, sort_keys=True)
    handle.write('\n')
summary = {
    'functions': sorted({p.parent.parent.name for p in out.glob('*/*/transport-report.json')}),
    'targets': sorted({p.parent.name for p in out.glob('*/*/transport-report.json')}),
    'receipt_table': 'artifacts/receipt-cids.tsv',
    'contract_exhibit': 'artifacts/foo_contract_transport_rust.json',
}
with open(out / 'summary.json', 'w', encoding='utf-8') as handle:
    json.dump(summary, handle, indent=2, sort_keys=True)
    handle.write('\n')
INNER_PY
printf 'receipt table: %s\n' "$OUT/receipt-cids.tsv"
printf 'contract exhibit: %s\n' "$OUT/foo_contract_transport_rust.json"
