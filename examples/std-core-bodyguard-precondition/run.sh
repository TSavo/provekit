#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
RUST="$REPO/implementations/rust"
BIN_DIR="$RUST/target/debug"
SUGAR="$BIN_DIR/sugar"
WALK_RPC="$BIN_DIR/sugar-walk-rpc"
WITNESS_RPC="$BIN_DIR/witness_rpc"
DISCHARGE_CLI="$BIN_DIR/discharge_cli"
WORK="${STD_CORE_BODYGUARD_WORK:-$HERE/.work}"
STD_CORE_RUST_TOOLCHAIN="${STD_CORE_RUST_TOOLCHAIN:-1.96.0}"

echo "SCOPE: Rust std/core body guard -> precondition predicate, zero std source changes."
echo "SCOPE: chosen guard = char::to_digit radix range from pinned rust-src $STD_CORE_RUST_TOOLCHAIN."
echo "SCOPE: proof property = caller value facts discharge/refuse callee precondition at the method-call seam."
echo "SCOPE: no panic semantics are modeled; panic is only the syntactic marker for the invalid-input branch."
echo "SCOPE: skipped residuals = non-flat guards, hidden expect/unwrap, loops, match arms, if-then-else strengthening, and early-return reasoning."

ensure_rust_src() {
  local sysroot stdroot
  if command -v rustup >/dev/null 2>&1; then
    echo "== install rust-src for pinned toolchain $STD_CORE_RUST_TOOLCHAIN ==" >&2
    rustup toolchain install "$STD_CORE_RUST_TOOLCHAIN" --profile minimal --component rust-src >/dev/null
  fi
  sysroot="$(rustc "+$STD_CORE_RUST_TOOLCHAIN" --print sysroot)"
  stdroot="$sysroot/lib/rustlib/src/rust/library"
  if [ ! -f "$stdroot/core/src/char/methods.rs" ]; then
    echo "missing pinned rust-src core/src/char/methods.rs under $stdroot" >&2
    exit 1
  fi
  if [ ! -f "$stdroot/coretests/tests/char.rs" ]; then
    echo "missing pinned rust-src coretests/tests/char.rs under $stdroot" >&2
    exit 1
  fi
  printf '%s\n' "$stdroot"
}

if [ "${STD_CORE_BODYGUARD_SKIP_LOCAL_BUILD:-0}" != "1" ]; then
  echo "== build local proof binaries =="
  cargo build --manifest-path "$RUST/Cargo.toml" \
    -p sugar-cli --bin sugar \
    -p sugar-walk --bin sugar-walk-rpc \
    -p sugar-lift-rust-cargo-test-witness --bin witness_rpc \
    -p sugar-lift-rust-cargo-test-witness --bin discharge_cli >/dev/null
fi

for bin in "$SUGAR" "$WALK_RPC" "$WITNESS_RPC" "$DISCHARGE_CLI"; do
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
mkdir -p "$WORK"

write_suite() {
  local suite="$1"
  local package="$2"
  local radix="$3"
  local test_attr="$4"
  local expected_body="$5"
  local dir="$WORK/$suite"

  mkdir -p "$dir/src" \
    "$dir/.sugar/lift/rust-fn-contracts" \
    "$dir/.sugar/lift/rust-implications" \
    "$dir/.sugar/lift/rust-cargo-test-witness"
  ln -s "$STDROOT/core/src/char/methods.rs" "$dir/src/core_char_methods.rs"

  cat > "$dir/Cargo.toml" <<TOML
[package]
name = "$package"
version = "0.1.0"
edition = "2021"
TOML

  cat > "$dir/src/lib.rs" <<RS
pub fn bodyguard_edge() -> Option<u32> {
    let ch = 'a';
    ch.to_digit($radix)
}

#[cfg(test)]
mod tests {
    use super::bodyguard_edge;

    #[test]
$test_attr
    fn to_digit_bodyguard_witness() {
$expected_body
    }
}
RS

  cat > "$dir/.sugar/config.toml" <<TOML
[[plugins]]
name = "rust-fn-contracts"
surface = "rust-fn-contracts"
emit = "ir-document"

[[plugins]]
name = "rust-implications"
surface = "rust-implications"

[[plugins]]
name = "rust-cargo-test-witness"
surface = "rust-cargo-test-witness"

[platform_profile]
language = "rust"
family = "concept:family:sugar"
library = "$package"
version = "0.1.0"

[solvers]
mode = "first-wins"
portfolio = ["z3"]

[solvers.z3]
binary = "z3"
ir_compiler = "smt-lib-v2.6"
flags = ["-smt2", "-in"]
timeout_seconds = 30
version = "4.x"
TOML

  cat > "$dir/.sugar/lift/rust-fn-contracts/manifest.toml" <<TOML
name = "rust-fn-contracts-lift"
command = ["$WALK_RPC", "--rpc"]
working_dir = "."
TOML

  cat > "$dir/.sugar/lift/rust-implications/manifest.toml" <<TOML
name = "rust-implications-lift"
command = ["$WALK_RPC", "--rpc"]
working_dir = "."
method = "sugar.plugin.lift_implications"
phase = "consumer"
TOML

  cat > "$dir/.sugar/lift/rust-cargo-test-witness/manifest.toml" <<TOML
name = "rust-cargo-test-witness-lift"
version = "0.1.0-draft"
protocol_version = "pep/1.7.0"
kind = "lift"
command = ["$WITNESS_RPC"]
discharge_command = ["$DISCHARGE_CLI"]
witness_tool = "cargo-test"
resolve_witness_command = ["$WITNESS_RPC"]
resolve_witness_method = "sugar.plugin.resolve_witness"
working_dir = "."

[capabilities]
authoring_surfaces = ["rust-cargo-test-witness"]
TOML
}

write_suite \
  good \
  std-core-bodyguard-good \
  16 \
  "" \
  "        assert_eq!(bodyguard_edge(), Some(10));"
write_suite \
  bad \
  std-core-bodyguard-bad \
  1 \
  "    #[should_panic(expected = \"to_digit: invalid radix\")]" \
  "        let _ = bodyguard_edge();"

json_receipt() {
  python3 - "$1" <<'PY'
import json
import re
import sys

text = open(sys.argv[1], encoding="utf-8").read()
text = re.sub(r"\x1b\[[0-9;]*m", "", text)
decoder = json.JSONDecoder()
for idx, ch in enumerate(text):
    if ch != "{":
        continue
    try:
        obj, _ = decoder.raw_decode(text[idx:])
    except Exception:
        continue
    if isinstance(obj, dict):
        print(json.dumps(obj))
        raise SystemExit(0)
raise SystemExit(1)
PY
}

edge_status() {
  python3 - "$1" "$2" <<'PY'
import json
import re
import sys

text = open(sys.argv[1], encoding="utf-8").read()
target_property_cid = sys.argv[2]
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
    if isinstance(obj, dict):
        receipt = obj
        break
if receipt is None:
    print("MISSING")
    raise SystemExit(0)
rows = receipt.get("rows") or receipt.get("obligations") or []
for row in rows:
    prop = row.get("property") or row.get("predicate") or ""
    if "witness-package" in prop:
        continue
    haystack = json.dumps(row, sort_keys=True)
    if row.get("propertyCid") == target_property_cid and "method:to_digit" in haystack:
        print(row.get("status") or row.get("result") or row.get("verdict") or "MISSING")
        raise SystemExit(0)
print("MISSING")
PY
}

edge_pair() {
  python3 - "$1" "$2" <<'PY'
import json
import re
import sys

text = open(sys.argv[1], encoding="utf-8").read()
target_property_cid = sys.argv[2]
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
    if isinstance(obj, dict):
        receipt = obj
        break
if receipt is None:
    print("MISSING")
    raise SystemExit(0)
rows = receipt.get("rows") or receipt.get("obligations") or []
for row in rows:
    haystack = json.dumps(row, sort_keys=True)
    if row.get("propertyCid") == target_property_cid and "method:to_digit" in haystack:
        print(row.get("property") or row.get("predicate") or row.get("bridge") or haystack[:240])
        raise SystemExit(0)
print("MISSING")
PY
}

bodyguard_contract_cid() {
  local dump_file="$WORK/proof-dump.json"
  "$SUGAR" dump "$1" --json > "$dump_file"
  python3 - "$dump_file" <<'PY'
import json
import sys

doc = json.load(open(sys.argv[1], encoding="utf-8"))
for cid, member in (doc.get("members") or {}).items():
    body = member.get("header") or (member.get("evidence") or {}).get("body") or {}
    kind = body.get("kind") or (member.get("evidence") or {}).get("kind")
    name = body.get("name") or body.get("contractName")
    if kind == "contract" and name == "bodyguard_edge":
        print(cid)
        raise SystemExit(0)
raise SystemExit("bodyguard_edge contract not found in proof")
PY
}

witness_status() {
  python3 - "$1" <<'PY'
import json
import re
import sys

text = open(sys.argv[1], encoding="utf-8").read()
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
    if isinstance(obj, dict):
        receipt = obj
        break
if receipt is None:
    print("MISSING")
    raise SystemExit(0)
rows = receipt.get("rows") or receipt.get("obligations") or []
for row in rows:
    prop = row.get("property") or row.get("predicate") or ""
    if "witness-package" in prop:
        print(row.get("status") or row.get("result") or "MISSING")
        raise SystemExit(0)
print("MISSING")
PY
}

witness_verdict() {
  python3 - "$1" <<'PY'
import json
import re
import sys

text = open(sys.argv[1], encoding="utf-8").read()
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
    if isinstance(obj, dict):
        receipt = obj
        break
if receipt is None:
    print("MISSING")
    raise SystemExit(0)
for witness in receipt.get("witnessDimension", {}).get("witnesses", []):
    verdict = witness.get("verdict")
    if verdict:
        print(verdict)
        raise SystemExit(0)
print("MISSING")
PY
}

recompute_strategy() {
  python3 - "$1" <<'PY'
import json
import re
import sys

text = open(sys.argv[1], encoding="utf-8").read()
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
    if isinstance(obj, dict):
        receipt = obj
        break
if receipt is None:
    print("MISSING")
    raise SystemExit(0)
for witness in receipt.get("witnessDimension", {}).get("witnesses", []):
    if "content-address:recompute" in (witness.get("checks") or []):
        print("content-address:recompute")
        raise SystemExit(0)
print("MISSING")
PY
}

run_suite() {
  local suite="$1"
  local expect_edge="$2"
  local dir="$WORK/$suite"

  rm -f "$dir"/blake3-512:*.proof "$dir/.prove.json" "$dir/.verify.json" "$dir/.verify_recompute.json"
  rm -rf "$dir/.sugar/runs" "$dir/.sugar/witnesses" "$dir/target"

  echo "== mint $suite =="
  (cd "$dir" && "$SUGAR" mint --out .) >/dev/null
  proof_path="$(
    python3 - "$dir" <<'PY'
import glob
import os
import sys
matches = sorted(glob.glob(os.path.join(sys.argv[1], "blake3-512:*.proof")))
print(matches[0] if matches else "")
PY
  )"
  if [ -z "$proof_path" ]; then
    echo "$suite did not mint a proof" >&2
    exit 1
  fi
  echo "$suite proof: $(basename "$proof_path")"
  local bodyguard_cid
  bodyguard_cid="$(bodyguard_contract_cid "$proof_path")"
  echo "$suite bodyguard contract: $bodyguard_cid"

  echo "== prove $suite =="
  set +e
  (cd "$dir" && PATH="$BIN_DIR:$PATH" "$SUGAR" prove . --json) > "$dir/.prove.json" 2>&1
  set -e

  echo "== verify $suite =="
  set +e
  (cd "$dir" && PATH="$BIN_DIR:$PATH" "$SUGAR" verify --project . --json) > "$dir/.verify.json" 2>&1
  set -e

  local got_edge got_witness pair
  got_edge="$(edge_status "$dir/.verify.json" "$bodyguard_cid")"
  got_witness="$(witness_status "$dir/.verify.json")"
  pair="$(edge_pair "$dir/.verify.json" "$bodyguard_cid")"

  if [ "$expect_edge" = "discharged" ]; then
    if [ "$got_edge" != "discharged" ]; then
      echo "$suite bodyguard edge expected discharged, got $got_edge" >&2
      cat "$dir/.verify.json" >&2
      exit 1
    fi
  else
    if [ "$got_edge" = "discharged" ] || [ "$got_edge" = "MISSING" ]; then
      echo "$suite bodyguard edge expected refusal, got $got_edge" >&2
      cat "$dir/.verify.json" >&2
      exit 1
    fi
  fi
  if [ "$got_witness" != "discharged" ]; then
    echo "$suite cargo-test witness expected discharged, got $got_witness" >&2
    cat "$dir/.verify.json" >&2
    exit 1
  fi

  rm -rf "$dir/.sugar/witnesses"
  set +e
  (cd "$dir" && PATH="$BIN_DIR:$PATH" "$SUGAR" verify --project . --json) > "$dir/.verify_recompute.json" 2>&1
  set -e
  local recompute
  recompute="$(recompute_strategy "$dir/.verify_recompute.json")"
  if [ "$recompute" != "content-address:recompute" ]; then
    echo "$suite witness recompute expected content-address:recompute, got $recompute" >&2
    cat "$dir/.verify_recompute.json" >&2
    exit 1
  fi
  local verified
  verified="$(witness_verdict "$dir/.verify_recompute.json")"
  if [ "$verified" != "verified" ]; then
    echo "$suite witness recompute expected verified, got $verified" >&2
    cat "$dir/.verify_recompute.json" >&2
    exit 1
  fi

  echo "$suite bodyguard_edge=$got_edge witness=$got_witness recompute=$recompute pair=$pair"
}

run_suite good discharged
run_suite bad refused

echo "== witness: rerun exact std/core vendor should_panic tests =="
(
  cd "$STDROOT/coretests"
  CARGO_TARGET_DIR="$WORK/coretests-target" RUSTC_BOOTSTRAP=1 \
    cargo "+$STD_CORE_RUST_TOOLCHAIN" test --test coretests char::test_to_digit_radix_too_low -- --exact --nocapture
)
(
  cd "$STDROOT/coretests"
  CARGO_TARGET_DIR="$WORK/coretests-target" RUSTC_BOOTSTRAP=1 \
    cargo "+$STD_CORE_RUST_TOOLCHAIN" test --test coretests char::test_to_digit_radix_too_high -- --exact --nocapture
)

echo "std/core bodyguard precondition showcase self-check passed"
echo "effects-free: lifted only radix >= 2 && radix <= 36 as FOL over inputs; no panic/divergence/effect/control-flow semantics introduced."
echo "toolchain-detail: $RUSTC_VERBOSE"
