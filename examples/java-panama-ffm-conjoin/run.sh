#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
RUST="$REPO/implementations/rust"
BIN_DIR="$RUST/target/debug"
SUGAR="$BIN_DIR/sugar"
RUST_ASSERT_RPC="$BIN_DIR/rust_test_assertions_rpc"
JAVA_ASSERT_RPC="$BIN_DIR/java_test_assertions_rpc"
WITNESS_RPC="$BIN_DIR/java_junit_witness_rpc"
DISCHARGE_CLI="$BIN_DIR/java_junit_discharge_cli"
PANAMA_SRC="$REPO/implementations/java/sugar-lift-java-panama-ffm/src/PanamaFfmLiftRpc.java"

BASE64_VERSION="${BASE64_VERSION:-0.22.1}"
JUNIT_VERSION="${JUNIT_VERSION:-1.10.2}"
MAVEN_BASE="${MAVEN_BASE:-https://repo1.maven.org/maven2}"
JAR_DIR="${SUGAR_JAVA_PANAMA_JAR_DIR:-/tmp/sugar-java-panama-ffm}"
JUNIT_JAR="${SUGAR_JUNIT_CONSOLE_JAR:-$JAR_DIR/junit-platform-console-standalone-$JUNIT_VERSION.jar}"
JDK22_URL="${SUGAR_JDK22_URL:-https://api.adoptium.net/v3/binary/latest/22/ga/linux/x64/jdk/hotspot/normal/eclipse?project=jdk}"
JDK22_DIR="${SUGAR_JDK22_DIR:-/tmp/sugar-jdk-22}"
WORK="$HERE/work"
BASE64_SRC="$WORK/base64-$BASE64_VERSION"
PANAMA_CLASSES="$WORK/panama-lifter-classes"
NATIVE_DIR="$HERE/native-shim"

if [ "${JAVA_PANAMA_SHOWCASE_ON_REMOTE:-0}" != "1" ] \
  && [ "${JAVA_PANAMA_SHOWCASE_USE_BCARGO:-1}" != "0" ] \
  && [ "$(uname -s)" != "Linux" ]; then
  echo "== run java panama FFM conjoin showcase on battleaxe via bcargo =="
  "$REPO/bin/bcargo" build --manifest-path "$RUST/Cargo.toml" \
    -p sugar-cli --bin sugar \
    -p sugar-lift-rust-tests --bin rust_test_assertions_rpc \
    -p sugar-lift-java-tests --bin java_test_assertions_rpc \
    -p sugar-lift-java-tests --bin java_junit_witness_rpc \
    -p sugar-lift-java-tests --bin java_junit_discharge_cli >/dev/null

  remote_host="${BCARGO_REMOTE_HOST:-battleaxe}"
  remote_tag="$(printf '%s' "$(cd "$REPO" && pwd -P)" | shasum 2>/dev/null | cut -c1-12)"
  remote_tag="${remote_tag:-default}"
  remote_root="${BCARGO_REMOTE_ROOT:-/home/tsavo/remote/sugar-bcargo-${remote_tag}}"
  remote_repo="$remote_root/sugar"
  remote_cmd="cd $(printf '%q' "$remote_repo") && JAVA_PANAMA_SHOWCASE_ON_REMOTE=1 JAVA_PANAMA_SHOWCASE_SKIP_LOCAL_BUILD=1 examples/java-panama-ffm-conjoin/run.sh"
  ssh -o BatchMode=yes "$remote_host" "bash -lc $(printf '%q' "$remote_cmd")"
  exit $?
fi

fetch_file() {
  local out="$1"
  local url="$2"
  if [ -f "$out" ]; then
    return 0
  fi
  mkdir -p "$(dirname "$out")"
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url" -o "$out"
  elif command -v wget >/dev/null 2>&1; then
    wget -q "$url" -O "$out"
  else
    echo "neither curl nor wget is available to fetch $url" >&2
    exit 1
  fi
}

ensure_jdk22() {
  local major
  major="$(java -version 2>&1 | python3 -c 'import re,sys; text=sys.stdin.read(); m=re.search(r"version \"([0-9]+)", text); print(m.group(1) if m else "0")' || true)"
  if [ "${major:-0}" -ge 22 ] 2>/dev/null; then
    JDK_BIN="$(dirname "$(command -v java)")"
    export JDK_BIN
    return 0
  fi
  if [ ! -x "$JDK22_DIR/bin/java" ] || [ ! -x "$JDK22_DIR/bin/javac" ]; then
    echo "== fetch JDK 22 for Panama FFM =="
    rm -rf "$JDK22_DIR"
    mkdir -p "$JDK22_DIR"
    local archive="$JDK22_DIR.tar.gz"
    fetch_file "$archive" "$JDK22_URL"
    tar -xzf "$archive" -C "$JDK22_DIR" --strip-components=1
  fi
  JDK_BIN="$JDK22_DIR/bin"
  export JDK_BIN
  export PATH="$JDK_BIN:$PATH"
}

if [ "${JAVA_PANAMA_SHOWCASE_SKIP_LOCAL_BUILD:-0}" != "1" ]; then
  echo "== build targeted proof binaries =="
  cargo build --manifest-path "$RUST/Cargo.toml" \
    -p sugar-cli --bin sugar \
    -p sugar-lift-rust-tests --bin rust_test_assertions_rpc \
    -p sugar-lift-java-tests --bin java_test_assertions_rpc \
    -p sugar-lift-java-tests --bin java_junit_witness_rpc \
    -p sugar-lift-java-tests --bin java_junit_discharge_cli >/dev/null
fi

for bin in "$SUGAR" "$RUST_ASSERT_RPC" "$JAVA_ASSERT_RPC" "$WITNESS_RPC" "$DISCHARGE_CLI"; do
  if [ ! -x "$bin" ]; then
    echo "missing executable: $bin" >&2
    exit 1
  fi
done
if [ ! -f "$PANAMA_SRC" ]; then
  echo "missing Java Panama lifter source: $PANAMA_SRC" >&2
  exit 1
fi

ensure_jdk22
echo "JDK: $("${JDK_BIN}/java" -version 2>&1 | head -1)"

fetch_file "$JUNIT_JAR" "$MAVEN_BASE/org/junit/platform/junit-platform-console-standalone/$JUNIT_VERSION/junit-platform-console-standalone-$JUNIT_VERSION.jar"
export SUGAR_JUNIT_CONSOLE_JAR="$JUNIT_JAR"
export SUGAR_JAVA_ASSERT_CLASSPATH="$JUNIT_JAR"
export JDK_JAVA_OPTIONS="${JDK_JAVA_OPTIONS:-} --enable-native-access=ALL-UNNAMED"

echo "== derive assertion vocabulary from real JUnit jar =="
"$JDK_BIN/javap" -classpath "$JUNIT_JAR" -public org.junit.jupiter.api.Assertions \
  | grep -E 'assertEquals\(double, double, double|assertEquals\(int, int\)|assertTrue\(boolean\)' \
  | sed 's/^/real-junit-signature: /'

echo "== compile Java Panama FFM lifter =="
rm -rf "$PANAMA_CLASSES"
mkdir -p "$PANAMA_CLASSES"
"$JDK_BIN/javac" -d "$PANAMA_CLASSES" "$PANAMA_SRC"

unpack_base64() {
  if [ -d "$BASE64_SRC/src" ]; then
    return 0
  fi
  echo "== fetch real rust crate base64 $BASE64_VERSION =="
  rm -rf "$BASE64_SRC"
  mkdir -p "$WORK"
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

first_match() {
  python3 - "$1" <<'PY'
import glob
import sys
matches = sorted(glob.glob(sys.argv[1]))
print(matches[0] if matches else "")
PY
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
  if [ -z "$proof" ]; then
    echo "base64 mint produced no proof" >&2
    exit 1
  fi
  (cd "$BASE64_SRC" && "$SUGAR" prove . --json) > "$BASE64_SRC/.prove.json" 2>&1 || true
  python3 - "$BASE64_SRC/.prove.json" <<'PY'
import json
import re
import sys
text = open(sys.argv[1], encoding="utf-8").read()
match = re.search(r"(?m)^\{", text)
data = json.loads(text[match.start():]) if match else {}
needle = "decoded_len_estimate#euf#c:callresult_decoded_len_estimate_a1(i:4)::assertion"
rows = data.get("rows", [])
matched = [r for r in rows if needle in json.dumps(r, sort_keys=True)]
if not matched:
    print(f"missing expected rust #euf row: {needle}", file=sys.stderr)
    raise SystemExit(1)
status = matched[0].get("status") or matched[0].get("result")
print(f"rust-row: {needle} status={status}")
PY
  BASE64_PROOF="$proof"
  export BASE64_PROOF
}

build_native_shim() {
  echo "== build native cdylib wrapper over real base64 crate =="
  cargo build --manifest-path "$NATIVE_DIR/Cargo.toml" --release >/dev/null
  case "$(uname -s)" in
    Darwin) NATIVE_LIB="$NATIVE_DIR/target/release/libbase64_panama_demo.dylib" ;;
    MINGW*|MSYS*|CYGWIN*) NATIVE_LIB="$NATIVE_DIR/target/release/base64_panama_demo.dll" ;;
    *) NATIVE_LIB="$NATIVE_DIR/target/release/libbase64_panama_demo.so" ;;
  esac
  if [ ! -f "$NATIVE_LIB" ]; then
    echo "missing native library: $NATIVE_LIB" >&2
    exit 1
  fi
  NATIVE_LIB="$(python3 -c 'import os,sys; print(os.path.realpath(sys.argv[1]))' "$NATIVE_LIB")"
  export NATIVE_LIB
}

render_file() {
  python3 - "$1" "$2" "$3" "$4" "$5" "${6:-}" <<'PY'
import sys
src, dst, bin_dir, jdk_bin, panama_classes = sys.argv[1:6]
text = open(src, encoding="utf-8").read()
text = text.replace("@BIN_DIR@", bin_dir)
text = text.replace("@JDK_BIN@", jdk_bin)
text = text.replace("@PANAMA_CLASSES@", panama_classes)
text = text.replace("@LIB_PATH@", sys.argv[6] if len(sys.argv) > 6 else "")
open(dst, "w", encoding="utf-8").write(text)
PY
}

render_consumer_source() {
  local suite="$1"
  local dir="$HERE/$suite"
  python3 - "$dir/src/test/java/demo/PanamaConsumerTest.java.in" \
    "$dir/src/test/java/demo/PanamaConsumerTest.java" \
    "$NATIVE_LIB" <<'PY'
import sys
template, out, lib = sys.argv[1:4]
text = open(template, encoding="utf-8").read().replace("@LIB_PATH@", lib)
open(out, "w", encoding="utf-8").write(text)
PY
}

render_manifests() {
  local suite="$1"
  local base="$HERE/$suite/.sugar/lift"
  render_file "$base/java-test-assertions/manifest.toml.in" "$base/java-test-assertions/manifest.toml" "$BIN_DIR" "$JDK_BIN" "$PANAMA_CLASSES" "$NATIVE_LIB"
  render_file "$base/java-junit-witness/manifest.toml.in" "$base/java-junit-witness/manifest.toml" "$BIN_DIR" "$JDK_BIN" "$PANAMA_CLASSES" "$NATIVE_LIB"
  render_file "$base/java-panama-ffm/manifest.toml.in" "$base/java-panama-ffm/manifest.toml" "$BIN_DIR" "$JDK_BIN" "$PANAMA_CLASSES" "$NATIVE_LIB"
}

clean_suite() {
  local suite="$1"
  local dir="$HERE/$suite"
  rm -f "$dir"/blake3-512:*.proof "$dir/.prove.json" "$dir/.verify.json" "$dir/.verify_recompute.json" "$dir/java-panama-ffm.call-edges.json"
  rm -rf "$dir/.sugar/runs" "$dir/.sugar/witnesses" "$dir/target"
  mkdir -p "$dir/.sugar/imports" "$dir/.sugar/witnesses"
  rm -f "$dir/.sugar/imports/"*.proof "$dir/.sugar/witnesses/"*.witness
  cp "$BASE64_PROOF" "$dir/.sugar/imports/"
}

verify_status() {
  python3 - "$1" "$2" <<'PY'
import json
import re
import sys
path, kind = sys.argv[1:3]
text = open(path, encoding="utf-8").read()
match = re.search(r"(?m)^\{", text)
if not match:
    print("MISSING")
    raise SystemExit(0)
data = json.loads(text[match.start():])
rows = data.get("rows") or data.get("claims") or data.get("obligations") or (data if isinstance(data, list) else [])
statuses = []
for row in rows:
    prop = row.get("property") or row.get("predicate") or ""
    if kind == "consistency" and prop.startswith("consistency:") and "witness-package" not in prop:
        statuses.append(row.get("status") or row.get("result") or "")
    if kind == "witness" and "witness-package" in prop:
        statuses.append(row.get("status") or row.get("result") or "")
if not statuses:
    print("MISSING")
elif all(status == "discharged" for status in statuses):
    print("discharged")
else:
    print("refused")
PY
}

edge_summary() {
  python3 - "$1" <<'PY'
import json
import sys
path = sys.argv[1]
data = json.load(open(path, encoding="utf-8"))
edges = data.get("edges", [])
if not edges:
    print("MISSING")
    raise SystemExit(0)
edge = edges[0]
print(json.dumps(edge, sort_keys=True))
PY
}

run_suite() {
  local suite="$1"
  local expect_consistency="$2"
  local expect_witness="$3"
  local dir="$HERE/$suite"
  render_consumer_source "$suite"
  render_manifests "$suite"
  clean_suite "$suite"

  echo "== sugar mint $suite =="
  echo "cmd: (cd $dir && $SUGAR mint --out .)"
  (cd "$dir" && "$SUGAR" mint --out .) >/dev/null

  local proof
  proof="$(first_match "$dir/blake3-512:*.proof")"
  if [ -z "$proof" ]; then
    echo "$suite did not mint a proof" >&2
    exit 1
  fi

  local edge_file="$dir/java-panama-ffm.call-edges.json"
  if [ ! -f "$edge_file" ]; then
    echo "$suite did not emit java-panama-ffm.call-edges.json" >&2
    exit 1
  fi
  local edge
  edge="$(edge_summary "$edge_file")"
  if [ "$edge" = "MISSING" ]; then
    echo "$suite emitted no call edge" >&2
    exit 1
  fi
  echo "$suite CallEdgeDecl: $edge"

  echo "== sugar verify durable proof+witness $suite =="
  echo "cmd: (cd $dir && $SUGAR verify --project . --json)"
  set +e
  (cd "$dir" && "$SUGAR" verify --project . --json) > "$dir/.verify.json" 2>&1
  local verify_rc=$?
  set -e

  local got_consistency got_witness
  got_consistency="$(verify_status "$dir/.verify.json" consistency)"
  got_witness="$(verify_status "$dir/.verify.json" witness)"

  if [ "$expect_consistency" = "discharged" ]; then
    if [ "$verify_rc" -ne 0 ] || [ "$got_consistency" != "discharged" ]; then
      echo "$suite consistency expected discharged, got rc=$verify_rc consistency=$got_consistency" >&2
      cat "$dir/.verify.json" >&2
      exit 1
    fi
  else
    if [ "$verify_rc" -eq 0 ] || [ "$got_consistency" = "discharged" ] || [ "$got_consistency" = "MISSING" ]; then
      echo "$suite consistency expected refusal, got rc=$verify_rc consistency=$got_consistency" >&2
      cat "$dir/.verify.json" >&2
      exit 1
    fi
  fi

  if [ "$expect_witness" = "discharged" ]; then
    if [ "$got_witness" != "discharged" ]; then
      echo "$suite witness expected discharged, got $got_witness" >&2
      cat "$dir/.verify.json" >&2
      exit 1
    fi
  else
    if [ "$got_witness" = "discharged" ] || [ "$got_witness" = "MISSING" ]; then
      echo "$suite witness expected refusal, got $got_witness" >&2
      cat "$dir/.verify.json" >&2
      exit 1
    fi
  fi

  echo "$suite consistency=$got_consistency witness=$got_witness proof=$(basename "$proof")"
}

mint_base64_proof
build_native_shim

echo "SCOPE: rust vendor proof = real base64 $BASE64_VERSION tests, zero source changes."
echo "SCOPE: rust row = decoded_len_estimate#euf#c:callresult_decoded_len_estimate_a1(i:4)::assertion from assert_eq!(3, decoded_len_estimate(4))."
echo "SCOPE: Java Panama lifter emits targetSymbol=rust-kit:decoded_len_estimate#euf#c:callresult_decoded_len_estimate_a1(i:4)::assertion."

run_suite good discharged discharged
run_suite bad refused refused

echo "collision-pair: bad Java FFM callsite assertEquals(4, decoded_len_estimate(4)) vs rust base64 vendor row assert_eq!(3, decoded_len_estimate(4))"
echo "java panama FFM conjoin showcase self-check passed"
