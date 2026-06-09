#!/usr/bin/env bash
# std/core logo showcase: real Rust core tests, zero std source changes.
#
# Claimed proof surface:
#   - `rust-src` provides the pinned toolchain's std/core source and tests.
#   - The lifter sees a selected sound scalar direct call-result slice:
#       * tests/cmp.rs integer call-result equality rows,
#       * tests/mem.rs generic type-arg-keyed size_of/align_of rows,
#       * tests/time.rs finite decimal float method-call equality rows,
#       * tests/fmt/mod.rs exact string method-call equality rows.
#   - `sugar mint` + `sugar verify` must produce only discharged `#euf#`
#     consistency rows for that slice.
#   - The exact vendor tests rerun as the witness axis.
#
# Explicitly NOT claimed:
#   - assertion macros requiring expansion,
#   - float refinements such as NaN/infinity/ordered comparisons,
#   - chars, cfg-sensitive generic width tests, and complex expression terms.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
RUST="$REPO/implementations/rust"
BIN_DIR="$RUST/target/debug"
SUGAR="$BIN_DIR/sugar"
ASSERT_RPC="$BIN_DIR/rust_test_assertions_rpc"
WORK="${STD_CORE_SHOWCASE_WORK:-$HERE/.work}"
PROJECT="$WORK/proof-scope"
WITNESS_TARGET="$WORK/coretests-target"
STD_CORE_RUST_TOOLCHAIN="${STD_CORE_RUST_TOOLCHAIN:-1.96.0}"

echo "SCOPE: Rust std/core own tests, zero std source changes."
echo "SCOPE: claimed slice = scalar direct call-result equality assertions from cmp.rs, type-arg-keyed generic rows from mem.rs, finite float rows from time.rs, exact string rows from fmt/mod.rs."
echo "SCOPE: excluded gaps = macros requiring expansion, NaN/infinity/ordered float refinements, chars, cfg-sensitive generic width tests, complex terms."
echo "SCOPE: pinned Rust toolchain = $STD_CORE_RUST_TOOLCHAIN (std source is not taken from CI's active default)."

ensure_rust_src() {
  local sysroot stdroot
  if command -v rustup >/dev/null 2>&1; then
    echo "== install rust-src for pinned toolchain $STD_CORE_RUST_TOOLCHAIN ==" >&2
    rustup toolchain install "$STD_CORE_RUST_TOOLCHAIN" --profile minimal --component rust-src >/dev/null
  fi
  sysroot="$(rustc "+$STD_CORE_RUST_TOOLCHAIN" --print sysroot)"
  stdroot="$sysroot/lib/rustlib/src/rust/library"
  if [ -f "$stdroot/coretests/tests/cmp.rs" ]; then
    printf '%s\n' "$stdroot"
    return 0
  fi
  if [ ! -f "$stdroot/coretests/tests/cmp.rs" ]; then
    echo "rust-src for pinned toolchain $STD_CORE_RUST_TOOLCHAIN is missing coretests/tests/cmp.rs under $stdroot" >&2
    exit 1
  fi
  printf '%s\n' "$stdroot"
}

if [ "${STD_CORE_SHOWCASE_SKIP_LOCAL_BUILD:-0}" != "1" ]; then
  echo "== build local proof binaries =="
  cargo build --manifest-path "$RUST/Cargo.toml" \
    -p sugar-cli --bin sugar \
    -p sugar-lift-rust-tests --bin rust_test_assertions_rpc >/dev/null
fi

for bin in "$SUGAR" "$ASSERT_RPC"; do
  if [ ! -x "$bin" ]; then
    echo "missing executable: $bin" >&2
    exit 1
  fi
done

STDROOT="$(ensure_rust_src)"
RUSTC_VERSION="$(rustc "+$STD_CORE_RUST_TOOLCHAIN" --version)"
RUSTC_VERBOSE="$(rustc "+$STD_CORE_RUST_TOOLCHAIN" --version --verbose | tr '\n' ';')"
echo "rust-src: $STDROOT"
echo "toolchain: $RUSTC_VERSION"

rm -rf "$WORK"
mkdir -p "$PROJECT/tests/fmt" "$PROJECT/.sugar/lift/rust-test-assertions"
ln -s "$STDROOT/coretests/tests/cmp.rs" "$PROJECT/tests/cmp.rs"
ln -s "$STDROOT/coretests/tests/time.rs" "$PROJECT/tests/time.rs"
ln -s "$STDROOT/coretests/tests/fmt/mod.rs" "$PROJECT/tests/fmt/mod.rs"

python3 - "$STDROOT/coretests/tests/mem.rs" "$PROJECT/tests/mem.rs" <<'PY'
import sys

source, dest = sys.argv[1:]
lines = open(source, encoding="utf-8").read().splitlines()

def extract(fn_name: str) -> list[str]:
    fn_idx = next(
        i for i, line in enumerate(lines)
        if line.startswith(f"fn {fn_name}(")
    )
    start = fn_idx
    while start > 0 and (lines[start - 1].startswith("#[") or lines[start - 1] == ""):
        start -= 1
    out = []
    depth = 0
    seen_open = False
    for line in lines[start:]:
        out.append(line)
        depth += line.count("{")
        if "{" in line:
            seen_open = True
        depth -= line.count("}")
        if seen_open and depth == 0:
            return out
    raise RuntimeError(f"unterminated function {fn_name}")

chunks = [
    "use core::mem::*;",
    "",
    *extract("size_of_basic"),
    "",
    *extract("align_of_basic"),
    "",
]
open(dest, "w", encoding="utf-8").write("\n".join(chunks))
PY

cat > "$PROJECT/.sugar/config.toml" <<TOML
[[plugins]]
name = "rust-test-assertions-lift"
kind = "lift"
surface = "rust-test-assertions"
emit = "ir-document"

[solvers]
default = "z3"

[solvers.dispatch]
linear_arithmetic = "z3"
default = "z3"

[solvers.z3]
binary = "z3"
flags = ["-smt2", "-in"]

[platform_profile]
language = "rust"
library = "rust-std-coretests-scalar"
version = "$RUSTC_VERSION"
TOML

cat > "$PROJECT/.sugar/lift/rust-test-assertions/manifest.toml" <<TOML
name = "rust-test-assertions-lift"
version = "0.1.0"
protocol_version = "pep/1.7.0"
kind = "lift"
command = ["$ASSERT_RPC"]
working_dir = "."

[capabilities]
authoring_surfaces = ["rust-test-assertions"]
ir_version = "v1.1.0"
emits_signed_mementos = false
TOML

echo "== mint std/core selected source slice =="
(cd "$PROJECT" && "$SUGAR" mint --out .) >/dev/null

proof_path="$(
  python3 - "$PROJECT" <<'PY'
import glob
import os
import sys
matches = sorted(glob.glob(os.path.join(sys.argv[1], "blake3-512:*.proof")))
print(matches[0] if matches else "")
PY
)"
if [ -z "$proof_path" ]; then
  echo "mint produced no .proof" >&2
  exit 1
fi
echo "proof: $(basename "$proof_path")"

echo "== verify std/core selected source slice =="
(cd "$PROJECT" && "$SUGAR" verify --project . --json) > "$PROJECT/.verify.json" 2>&1 || true

python3 - "$PROJECT/.verify.json" <<'PY'
import json
import re
import sys

path = sys.argv[1]
text = open(path, encoding="utf-8").read()
text = re.sub(r"\x1b\[[0-9;]*m", "", text)
decoder = json.JSONDecoder()
receipt = None
for idx, ch in enumerate(text):
    if ch != "{":
        continue
    try:
        obj, _ = decoder.raw_decode(text[idx:])
    except Exception:
        continue
    if isinstance(obj, dict) and obj.get("kind") == "verification-receipt":
        receipt = obj
        break
if receipt is None:
    print("verify produced no verification-receipt", file=sys.stderr)
    print(text, file=sys.stderr)
    raise SystemExit(1)

rows = receipt.get("rows", [])
euf_rows = [r for r in rows if "#euf#" in (r.get("property") or "")]
failed = [r for r in euf_rows if r.get("status") != "discharged"]
needles = [
    "cmp::max_by#euf#c:callresult_cmp__max_by_a3(i:1,i:-1,v:f)::assertion",
    "size_of::<u8>#euf#c:callresult_size_of___u8__a0()::assertion",
    "size_of::<u16>#euf#c:callresult_size_of___u16__a0()::assertion",
    "align_of::<u8>#euf#c:callresult_align_of___u8__a0()::assertion",
    "align_of::<u16>#euf#c:callresult_align_of___u16__a0()::assertion",
    "method:to_string#euf#c:callresult_method_to_string_a1(v:a)::assertion",
    "method:div_duration_f32#euf#c:callresult_method_div_duration_f32_a2(v:Duration::ZERO,v:Duration::MAX)::assertion",
    "method:div_duration_f32#euf#c:callresult_method_div_duration_f32_a2(c:*(v:Duration::SECOND,i:2),v:Duration::SECOND)::assertion",
    "method:div_duration_f64#euf#c:callresult_method_div_duration_f64_a2(v:Duration::ZERO,v:Duration::MAX)::assertion",
    "method:div_duration_f64#euf#c:callresult_method_div_duration_f64_a2(c:*(v:Duration::SECOND,i:2),v:Duration::SECOND)::assertion",
]
missing = [
    needle for needle in needles
    if not any(needle in (r.get("property") or "") for r in euf_rows)
]

if not euf_rows:
    print("no #euf# consistency rows found", file=sys.stderr)
    raise SystemExit(1)
if missing:
    print("missing required claimed rows:", file=sys.stderr)
    for needle in missing:
        print(needle, file=sys.stderr)
    raise SystemExit(1)
if failed:
    print("non-discharged #euf# rows in claimed slice:", file=sys.stderr)
    for row in failed:
        print(f"{row.get('status')} {row.get('property')} {row.get('reason')}", file=sys.stderr)
    raise SystemExit(1)

print(f"claimed-euf-rows={len(euf_rows)} discharged={len(euf_rows)} failed=0")
for row in euf_rows:
    print(f"row: {row.get('property')} status={row.get('status')}")
PY

echo "== witness: rerun exact std/core vendor tests =="
(
  cd "$STDROOT/coretests"
  CARGO_TARGET_DIR="$WITNESS_TARGET" RUSTC_BOOTSTRAP=1 \
    cargo "+$STD_CORE_RUST_TOOLCHAIN" test --test coretests cmp::test_ord_min_max_by -- --exact --nocapture
)
(
  cd "$STDROOT/coretests"
  CARGO_TARGET_DIR="$WITNESS_TARGET" RUSTC_BOOTSTRAP=1 \
    cargo "+$STD_CORE_RUST_TOOLCHAIN" test --test coretests cmp::test_ord_min_max_by_key -- --exact --nocapture
)
(
  cd "$STDROOT/coretests"
  CARGO_TARGET_DIR="$WITNESS_TARGET" RUSTC_BOOTSTRAP=1 \
    cargo "+$STD_CORE_RUST_TOOLCHAIN" test --test coretests mem::size_of_basic -- --exact --nocapture
)
(
  cd "$STDROOT/coretests"
  CARGO_TARGET_DIR="$WITNESS_TARGET" RUSTC_BOOTSTRAP=1 \
    cargo "+$STD_CORE_RUST_TOOLCHAIN" test --test coretests mem::align_of_basic -- --exact --nocapture
)
(
  cd "$STDROOT/coretests"
  CARGO_TARGET_DIR="$WITNESS_TARGET" RUSTC_BOOTSTRAP=1 \
    cargo "+$STD_CORE_RUST_TOOLCHAIN" test --test coretests fmt::test_lifetime -- --exact --nocapture
)
(
  cd "$STDROOT/coretests"
  CARGO_TARGET_DIR="$WITNESS_TARGET" RUSTC_BOOTSTRAP=1 \
    cargo "+$STD_CORE_RUST_TOOLCHAIN" test --test coretests time::div_duration_f32 -- --exact --nocapture
)
(
  cd "$STDROOT/coretests"
  CARGO_TARGET_DIR="$WITNESS_TARGET" RUSTC_BOOTSTRAP=1 \
    cargo "+$STD_CORE_RUST_TOOLCHAIN" test --test coretests time::div_duration_f64 -- --exact --nocapture
)

echo "std/core showcase self-check passed"
echo "scope: scalar call-result equality rows from coretests/tests/{cmp.rs,mem.rs,time.rs,fmt/mod.rs} discharged; exact vendor tests reran."
echo "not-claimed: full std/coretests; macros/NaN-infinity-ordered-float-refinements/chars/cfg-sensitive generic width tests/complex terms remain gap census items."
echo "toolchain-detail: $RUSTC_VERBOSE"
