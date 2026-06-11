#!/usr/bin/env bash
# java-panama-bridge showcase: P5b — Java-native Panama FFM call-edge bridge lifter.
#
# THE THESIS: the #euf# symbol-CID is the cross-language identity. No hub.
#
# A Java test calls a native Rust function via Panama FFM (java.lang.foreign).
# The bridge lifter reads the Java source via com.sun.source tree nodes (NO regex)
# and emits a call-edge declaration: Java callsite CID → native symbol CID.
# The verifier conjoins the Java consumer's claim with the Rust vendor's contract.
#
# SCOPE:
#   Rust vendor proof: base64 0.22.1 — assert_eq!(3, decoded_len_estimate(4))
#   Rust row: decoded_len_estimate#euf#c:callresult_decoded_len_estimate_a1(i:4)::assertion
#   Java bridge: assertEquals(N, decoded_len_estimate(4)) via Panama downcall
#   Bridge lifter reads: VariableTree(DECODED_LEN_ESTIMATE=Linker.downcallHandle(
#                             LOOKUP.find("decoded_len_estimate").orElseThrow(), ...))
#                    → wrapperMethod: decoded_len_estimate → DECODED_LEN_ESTIMATE
#                    → @Test assertEquals(N, decoded_len_estimate(4)) → call-edge
#   The call-edge targetSymbol = "rust-kit:decoded_len_estimate#euf#..." is resolved
#   from the imported base64 .proof via name_to_cid lookup in the verifier pool.
#
# GOOD suite:
#   assertEquals(3, decoded_len_estimate(4)) — consistent with Rust vendor row.
#   Java contract: =(result, 3). Rust contract: =(result, 3). Conjoined: SAT → discharged.
#
# BAD suite (the cross-language refutation):
#   assertEquals(4, decoded_len_estimate(4)) — CONTRADICTS the Rust vendor row.
#   Java contract: =(result, 4). Rust contract: =(result, 3). Conjoined: UNSAT → unsatisfied.
#
# The bad twin's refutation comes through the REAL sugar verify, parsed from the receipt.
# No fabrication. The bridge lifter emitted a call-edge; the verifier discharged it.
#
# Bridge lifter reads the Panama pattern from AST nodes (com.sun.source.tree.*):
#   VariableTree.initializer → MethodInvocationTree(downcallHandle)
#     → MethodInvocationTree(find) → LiteralTree("decoded_len_estimate")
#   NOT from regex patterns on Java source text.
set -euo pipefail

command -v javac   >/dev/null 2>&1 || { echo "SKIP: no JDK on PATH"; exit 0; }
command -v java    >/dev/null 2>&1 || { echo "SKIP: no java on PATH"; exit 0; }
command -v python3 >/dev/null 2>&1 || { echo "SKIP: no python3 on PATH"; exit 0; }

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
RUST="$REPO/implementations/rust"
KIT_DIR="$REPO/implementations/java/sugar-lift-java-tests"
BIN_DIR="$RUST/target/debug"
SUGAR="$BIN_DIR/sugar"
KIT_JAVA="$(which java)"

BASE64_VERSION="${BASE64_VERSION:-0.22.1}"
MAVEN_BASE="${MAVEN_BASE:-https://repo1.maven.org/maven2}"
JAR_DIR="${SUGAR_JAVA_PANAMA_JAR_DIR:-/tmp/sugar-java-panama-bridge}"
JUNIT_VERSION="${JUNIT_VERSION:-1.10.2}"
JUNIT_JAR="${SUGAR_JUNIT_CONSOLE_JAR:-$JAR_DIR/junit-platform-console-standalone-$JUNIT_VERSION.jar}"
WORK="$HERE/work"
BASE64_SRC="$WORK/base64-$BASE64_VERSION"
NATIVE_DIR="$HERE/native-shim"
JDK22_DIR="${SUGAR_JDK22_DIR:-/tmp/sugar-jdk-22}"
JDK22_URL="${SUGAR_JDK22_URL:-https://api.adoptium.net/v3/binary/latest/22/ga/linux/x64/jdk/hotspot/normal/eclipse?project=jdk}"

echo "SCOPE: P5b Java-native Panama FFM bridge lifter — cross-language correctness."
echo "SCOPE: Bridge reads Java source via com.sun.source tree nodes (no regex)."
echo "SCOPE: Rust vendor row: decoded_len_estimate#euf# assert_eq!(3, decoded_len_estimate(4))."
echo "SCOPE: GOOD: Java assertEquals(3, ...) — consistent → discharged."
echo "SCOPE: BAD: Java assertEquals(4, ...) — contradicts Rust row → unsatisfied (cross-language refutation)."

fetch_file() {
  local out="$1" url="$2"
  [ -f "$out" ] && return 0
  mkdir -p "$(dirname "$out")"
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url" -o "$out"
  elif command -v wget >/dev/null 2>&1; then
    wget -q "$url" -O "$out"
  else
    echo "neither curl nor wget available to fetch $url" >&2; exit 1
  fi
}

ensure_jdk22() {
  local major
  major="$(java -version 2>&1 | python3 -c 'import re,sys; t=sys.stdin.read(); m=re.search(r"version \"([0-9]+)",t); print(m.group(1) if m else "0")' || true)"
  if [ "${major:-0}" -ge 22 ] 2>/dev/null; then
    JDK_BIN="$(dirname "$(command -v java)")"
    export JDK_BIN
    return 0
  fi
  if [ ! -x "$JDK22_DIR/bin/java" ]; then
    echo "== fetch JDK 22 for Panama FFM =="
    rm -rf "$JDK22_DIR"; mkdir -p "$JDK22_DIR"
    local archive="$JDK22_DIR.tar.gz"
    fetch_file "$archive" "$JDK22_URL"
    tar -xzf "$archive" -C "$JDK22_DIR" --strip-components=1
  fi
  JDK_BIN="$JDK22_DIR/bin"
  export JDK_BIN PATH="$JDK_BIN:$PATH"
}

first_match() {
  python3 - "$1" <<'PY'
import glob, sys
m = sorted(glob.glob(sys.argv[1]))
print(m[0] if m else "")
PY
}

echo
echo "== build the sugar CLI =="
if [ "${JAVA_PANAMA_BRIDGE_SKIP_LOCAL_BUILD:-0}" != "1" ]; then
  cargo build --manifest-path "$RUST/Cargo.toml" \
    -p sugar-cli --bin sugar \
    -p sugar-lift-rust-tests --bin rust_test_assertions_rpc >/dev/null
fi
[ -x "$SUGAR" ] || { echo "FAIL: sugar binary not found at $SUGAR"; exit 1; }
RUST_ASSERT_RPC="$BIN_DIR/rust_test_assertions_rpc"
[ -x "$RUST_ASSERT_RPC" ] || { echo "FAIL: rust_test_assertions_rpc not found"; exit 1; }

echo
echo "== build the Java kit (JavaTestAssertionsRpc + JavaPanamaFfmRpc) =="
bash "$KIT_DIR/build.sh" "$KIT_DIR/out" >/dev/null 2>&1
[ -f "$KIT_DIR/out/JavaTestAssertionsRpc.class" ] || { echo "FAIL: JavaTestAssertionsRpc.class not built"; exit 1; }
[ -f "$KIT_DIR/out/JavaPanamaFfmRpc.class" ] || { echo "FAIL: JavaPanamaFfmRpc.class not built"; exit 1; }
echo "   JavaTestAssertionsRpc.class + JavaPanamaFfmRpc.class present"

echo
echo "== ensure JDK 22+ for Panama FFM runtime =="
ensure_jdk22
echo "JDK: $("${JDK_BIN}/java" -version 2>&1 | head -1)"

echo
echo "== fetch JUnit console jar =="
fetch_file "$JUNIT_JAR" \
  "$MAVEN_BASE/org/junit/platform/junit-platform-console-standalone/$JUNIT_VERSION/junit-platform-console-standalone-$JUNIT_VERSION.jar"
export SUGAR_JUNIT_CONSOLE_JAR="$JUNIT_JAR"
export SUGAR_JAVA_ASSERT_CLASSPATH="$JUNIT_JAR"
export JDK_JAVA_OPTIONS="${JDK_JAVA_OPTIONS:-} --enable-native-access=ALL-UNNAMED"

# ── Mint the Rust base64 vendor proof ────────────────────────────────────────

unpack_base64() {
  [ -d "$BASE64_SRC/src" ] && return 0
  echo "== fetch real rust crate base64 $BASE64_VERSION =="
  rm -rf "$BASE64_SRC"; mkdir -p "$WORK"
  local archive="$WORK/base64-$BASE64_VERSION.crate"
  fetch_file "$archive" "https://static.crates.io/crates/base64/base64-$BASE64_VERSION.crate"
  tar -xzf "$archive" -C "$WORK"
}

write_base64_manifest() {
  mkdir -p "$BASE64_SRC/.sugar/lift/rust-test-assertions"
  cat > "$BASE64_SRC/.sugar/config.toml" <<'TOML'
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
library = "base64"
TOML
  cat > "$BASE64_SRC/.sugar/lift/rust-test-assertions/manifest.toml" <<TOML
name = "rust-test-assertions-lift"
version = "0.1.0"
protocol_version = "pep/1.7.0"
kind = "lift"
command = ["$RUST_ASSERT_RPC"]
working_dir = "."

[capabilities]
authoring_surfaces = ["rust-test-assertions"]
ir_version = "v1.1.0"
emits_signed_mementos = false
TOML
}

mint_base64_proof() {
  unpack_base64
  write_base64_manifest
  rm -f "$BASE64_SRC"/blake3-512:*.proof "$BASE64_SRC/.prove.json"
  rm -rf "$BASE64_SRC/.sugar/runs" "$BASE64_SRC/target"
  echo "== mint base64 $BASE64_VERSION vendor proof from own tests =="
  echo "vendor-row: base64-$BASE64_VERSION/src/decode.rs assert_eq!(3, decoded_len_estimate(4))"
  (cd "$BASE64_SRC" && "$SUGAR" mint --out .) >/dev/null
  local proof
  proof="$(first_match "$BASE64_SRC/blake3-512:*.proof")"
  [ -n "$proof" ] || { echo "FAIL: base64 mint produced no proof" >&2; exit 1; }

  # Verify the expected row exists
  python3 - "$BASE64_SRC" <<'PY'
import glob, sys
dirp = sys.argv[1]
proofs = sorted(glob.glob(dirp + "/blake3-512:*.proof"))
if not proofs:
    print("FAIL: no proof found", file=sys.stderr); raise SystemExit(1)
needle = b"decoded_len_estimate#euf#c:callresult_decoded_len_estimate_a1(i:4)::assertion"
found = any(needle in open(p, "rb").read() for p in proofs)
if not found:
    print("FAIL: expected row not in proof", file=sys.stderr); raise SystemExit(1)
print(f"   rust-row: decoded_len_estimate#euf# found in {len(proofs)} proof(s)")
PY

  BASE64_PROOF="$proof"
  export BASE64_PROOF
}

# ── Build the native shim ─────────────────────────────────────────────────────

build_native_shim() {
  echo "== build native cdylib wrapper over real base64 crate =="
  cargo build --manifest-path "$NATIVE_DIR/Cargo.toml" --release >/dev/null
  case "$(uname -s)" in
    Darwin)      NATIVE_LIB="$NATIVE_DIR/target/release/libbase64_panama_demo.dylib" ;;
    MINGW*|MSYS*|CYGWIN*) NATIVE_LIB="$NATIVE_DIR/target/release/base64_panama_demo.dll" ;;
    *)            NATIVE_LIB="$NATIVE_DIR/target/release/libbase64_panama_demo.so" ;;
  esac
  [ -f "$NATIVE_LIB" ] || { echo "FAIL: missing native library: $NATIVE_LIB" >&2; exit 1; }
  NATIVE_LIB="$(python3 -c 'import os,sys; print(os.path.realpath(sys.argv[1]))' "$NATIVE_LIB")"
  export NATIVE_LIB
}

# ── Suite helpers ─────────────────────────────────────────────────────────────

render_consumer_source() {
  local suite="$1"
  python3 - "$HERE/$suite/src/test/java/demo/PanamaConsumerTest.java.in" \
            "$HERE/$suite/src/test/java/demo/PanamaConsumerTest.java" \
            "$NATIVE_LIB" <<'PY'
import sys
tmpl, out, lib = sys.argv[1:4]
open(out, "w", encoding="utf-8").write(open(tmpl, encoding="utf-8").read().replace("@LIB_PATH@", lib))
PY
}

render_manifests() {
  local suite="$1"
  local base="$HERE/$suite/.sugar/lift"
  for surface in java-test-assertions java-panama-ffm; do
    local mfin="$base/$surface/manifest.toml.in"
    local mf="$base/$surface/manifest.toml"
    sed "s#@KIT_JAVA@#${JDK_BIN}/java#g; s#@KIT_DIR@#${KIT_DIR}#g" "$mfin" > "$mf"
  done
}

clean_suite() {
  local suite="$1"
  local dir="$HERE/$suite"
  rm -f "$dir"/blake3-512:*.proof "$dir/.prove.json" "$dir/.verify.json"
  rm -f "$dir/java-panama-ffm.call-edges.json"
  rm -rf "$dir/.sugar/runs" "$dir/.sugar/witnesses" "$dir/target"
  mkdir -p "$dir/.sugar/imports"
  rm -f "$dir/.sugar/imports/"*.proof
  cp "$BASE64_PROOF" "$dir/.sugar/imports/"
}

edge_summary() {
  python3 - "$1" <<'PY'
import json, sys
path = sys.argv[1]
data = json.load(open(path, encoding="utf-8"))
edges = data.get("edges", [])
if not edges:
    print("MISSING"); raise SystemExit(0)
print(json.dumps(edges[0], sort_keys=True))
PY
}

TARGET_EUF="decoded_len_estimate#euf#c:callresult_decoded_len_estimate_a1(i:4)::assertion"

# verify_status: check the status of the specific decoded_len_estimate#euf# row.
# We only care about the bridge-specific row, not other rows from the base64 proof
# (some of those are refused for unrelated reasons — string/approx operations).
verify_status() {
  python3 - "$1" "$2" "$TARGET_EUF" <<'PY'
import json, sys
path, kind, target_euf = sys.argv[1:4]
try:
    data = json.load(open(path, encoding="utf-8"))
except Exception:
    print("MISSING"); raise SystemExit(0)
rows = data.get("rows") or data.get("claims") or []
bridge_statuses = []
for row in rows:
    prop = row.get("property") or row.get("predicate") or ""
    status = row.get("status") or row.get("result") or ""
    if "consistency:" in prop and target_euf in prop:
        bridge_statuses.append(status)
if not bridge_statuses:
    print("MISSING")
elif all(s == "discharged" for s in bridge_statuses):
    print("discharged")
else:
    print("refused")
PY
}

run_suite() {
  local suite="$1"
  local expect_consistency="$2"
  local dir="$HERE/$suite"

  render_consumer_source "$suite"
  render_manifests "$suite"
  clean_suite "$suite"

  echo
  echo "── suite: $suite (expect consistency: $expect_consistency) ──"

  echo "-- sugar mint $suite --"
  (cd "$dir" && "$SUGAR" mint --out .) >/dev/null
  local proof
  proof="$(first_match "$dir/blake3-512:*.proof")"
  [ -n "$proof" ] || { echo "FAIL[$suite]: mint produced no proof" >&2; exit 1; }
  echo "   proof: $(basename "$proof")"

  # Verify bridge lifter ran and emitted a call-edge
  local edge_file="$dir/java-panama-ffm.call-edges.json"
  [ -f "$edge_file" ] || { echo "FAIL[$suite]: bridge lifter did not emit call-edges sidecar" >&2; exit 1; }
  local edge
  edge="$(edge_summary "$edge_file")"
  [ "$edge" != "MISSING" ] || { echo "FAIL[$suite]: bridge lifter emitted no call edges" >&2; exit 1; }
  echo "   CallEdgeDecl: $edge"

  # Verify call-edge points to the right symbol
  python3 - "$edge" <<'PY'
import json, sys
edge = json.loads(sys.argv[1])
expected_sym = "rust-kit:decoded_len_estimate#euf#c:callresult_decoded_len_estimate_a1(i:4)::assertion"
if edge.get("targetSymbol") != expected_sym:
    print(f"FAIL: wrong targetSymbol: {edge.get('targetSymbol')}", file=sys.stderr)
    raise SystemExit(1)
print(f"   targetSymbol: {edge['targetSymbol']}")
print(f"   sourceContractCid: {edge['sourceContractCid']}")
print(f"   targetContractCid: {edge['targetContractCid']}")
print(f"   callSiteLocus: {edge['callSiteLocus']}")
PY

  echo "-- sugar verify $suite --"
  set +e
  (cd "$dir" && "$SUGAR" verify --project . --json 2>/dev/null) > "$dir/.verify.json"
  local verify_rc=$?
  set -e

  local got_consistency
  got_consistency="$(verify_status "$dir/.verify.json" consistency)"

  # Check only the bridge-specific row (decoded_len_estimate#euf#).
  # Other rows in the base64 proof (chunked_encode_str, encode_all_bytes_url) may
  # be refused for unrelated reasons (string-theory, approximate assertions) and
  # do not invalidate the bridge correctness check.
  if [ "$expect_consistency" = "discharged" ]; then
    if [ "$got_consistency" != "discharged" ]; then
      echo "FAIL[$suite]: bridge row expected discharged, got consistency=$got_consistency (rc=$verify_rc)" >&2
      python3 -c "
import json, sys
data = json.load(open('$dir/.verify.json', encoding='utf-8'))
rows = data.get('rows', [])
for r in rows:
    prop = r.get('property','')
    if 'decoded_len_estimate' in prop:
        print(prop[:100], '->', r.get('status',''))
" >&2 || true
      exit 1
    fi
    echo "   GOOD $suite: bridge-row consistency=$got_consistency (rc=$verify_rc)"
  else
    if [ "$got_consistency" = "discharged" ] || [ "$got_consistency" = "MISSING" ]; then
      echo "FAIL[$suite]: bridge row expected refusal, got consistency=$got_consistency (rc=$verify_rc)" >&2
      python3 -c "
import json
data = json.load(open('$dir/.verify.json', encoding='utf-8'))
rows = data.get('rows', [])
for r in rows:
    prop = r.get('property','')
    if 'decoded_len_estimate' in prop:
        print(prop[:100], '->', r.get('status',''))
" >&2 || true
      exit 1
    fi
    echo "   BAD $suite: bridge-row consistency=$got_consistency (rc=$verify_rc) — cross-language refutation confirmed"
  fi

  echo "$suite: proof=$(basename "$proof") consistency=$got_consistency"
}

# ── Main ──────────────────────────────────────────────────────────────────────

mint_base64_proof
build_native_shim

run_suite good discharged
run_suite bad refused

echo
echo "collision-pair: bad assertEquals(4, decoded_len_estimate(4)) vs rust assert_eq!(3, decoded_len_estimate(4))"
echo "java panama bridge showcase self-check passed"
