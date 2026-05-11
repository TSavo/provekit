#!/bin/sh
set -eu
BASE="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
ROOT="$(CDPATH= cd -- "$BASE/../.." && pwd)"
OUT="$BASE/artifacts"
mkdir -p "$OUT"
make -C "$ROOT/implementations/c/provekit-walk-c"
cargo build --manifest-path "$ROOT/implementations/rust/Cargo.toml" -p provekit-cli
"$ROOT/implementations/rust/target/debug/provekit" transport \
  "$BASE/foo.c" \
  --to rust \
  --function foo \
  --out "$OUT" \
  --json | tee "$OUT/transport-report.json"
cmp "$OUT/concept.term.json" "$OUT/roundtrip.concept.term.json"
printf 'roundtrip concept artifact: ok\n'
