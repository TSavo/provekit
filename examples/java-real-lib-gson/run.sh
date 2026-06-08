#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
RUST="$REPO/implementations/rust"
BIN_DIR="$RUST/target/debug"
SUGAR="$BIN_DIR/sugar"
ASSERT_RPC="$BIN_DIR/java_test_assertions_rpc"
WITNESS_RPC="$BIN_DIR/java_junit_witness_rpc"
DISCHARGE_CLI="$BIN_DIR/java_junit_discharge_cli"

LIB_VERSION="${LIB_VERSION:-2.10.1}"
JUNIT_VERSION="${JUNIT_VERSION:-1.10.2}"
MAVEN_BASE="${MAVEN_BASE:-https://repo1.maven.org/maven2}"

WORK_ROOT="$HERE/work"
PROJECT="$WORK_ROOT/gson-$LIB_VERSION"
PROOF_SCOPE="$WORK_ROOT/proof-scope"
JAR_DIR="${SUGAR_JAVA_REAL_LIB_JAR_DIR:-/tmp/sugar-java-real-lib-gson}"
UPSTREAM_ZIP="$JAR_DIR/gson-parent-$LIB_VERSION.zip"
JUNIT_JAR="${SUGAR_JUNIT_CONSOLE_JAR:-$JAR_DIR/junit-platform-console-standalone-$JUNIT_VERSION.jar}"

if [ "${JAVA_REAL_LIB_GSON_ON_REMOTE:-0}" != "1" ] \
  && [ "${JAVA_REAL_LIB_GSON_USE_BCARGO:-1}" != "0" ] \
  && [ "$(uname -s)" != "Linux" ]; then
  echo "== run java real-library Gson on battleaxe via bcargo =="
  "$REPO/bin/bcargo" build --manifest-path "$RUST/Cargo.toml" \
    -p sugar-cli --bin sugar \
    -p sugar-lift-java-tests --bin java_test_assertions_rpc \
    -p sugar-lift-java-tests --bin java_junit_witness_rpc \
    -p sugar-lift-java-tests --bin java_junit_discharge_cli >/dev/null

  remote_host="${BCARGO_REMOTE_HOST:-battleaxe}"
  remote_tag="$(printf '%s' "$(cd "$REPO" && pwd -P)" | shasum 2>/dev/null | cut -c1-12)"
  remote_tag="${remote_tag:-default}"
  remote_root="${BCARGO_REMOTE_ROOT:-/home/tsavo/remote/sugar-bcargo-${remote_tag}}"
  remote_repo="$remote_root/sugar"
  remote_cmd="cd $(printf '%q' "$remote_repo") && JAVA_REAL_LIB_GSON_ON_REMOTE=1 JAVA_REAL_LIB_GSON_SKIP_LOCAL_BUILD=1 examples/java-real-lib-gson/run.sh"
  ssh -o BatchMode=yes "$remote_host" "bash -lc $(printf '%q' "$remote_cmd")"
  exit $?
fi

if [ "${JAVA_REAL_LIB_GSON_SKIP_LOCAL_BUILD:-0}" != "1" ]; then
  echo "== build proof binaries =="
  cargo build --manifest-path "$RUST/Cargo.toml" \
    -p sugar-cli --bin sugar \
    -p sugar-lift-java-tests --bin java_test_assertions_rpc \
    -p sugar-lift-java-tests --bin java_junit_witness_rpc \
    -p sugar-lift-java-tests --bin java_junit_discharge_cli >/dev/null
fi

for bin in "$SUGAR" "$ASSERT_RPC" "$WITNESS_RPC" "$DISCHARGE_CLI"; do
  if [ ! -x "$bin" ]; then
    echo "missing executable: $bin" >&2
    exit 1
  fi
done

if ! command -v javac >/dev/null 2>&1 || ! command -v java >/dev/null 2>&1 || ! command -v unzip >/dev/null 2>&1; then
  echo "missing JDK/unzip tools on this host; run this showcase on battleaxe/Linux" >&2
  exit 1
fi

fetch_jar() {
  local jar="$1"
  local url="$2"
  if [ -f "$jar" ]; then
    return 0
  fi
  mkdir -p "$(dirname "$jar")"
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url" -o "$jar"
  elif command -v wget >/dev/null 2>&1; then
    wget -q "$url" -O "$jar"
  else
    echo "neither curl nor wget is available to fetch $url" >&2
    exit 1
  fi
}

write_surface_manifests() {
  mkdir -p "$PROJECT/.sugar/lift/java-test-assertions" \
    "$PROJECT/.sugar/lift/java-junit-witness"

  cat > "$PROJECT/.sugar/lift/java-test-assertions/manifest.toml" <<TOML
name = "java-test-assertions-lift"
version = "0.1.0"
protocol_version = "pep/1.7.0"
kind = "lift"
command = ["$ASSERT_RPC"]
working_dir = "."

[capabilities]
authoring_surfaces = ["java-test-assertions"]
ir_version = "v1.1.0"
emits_signed_mementos = false
TOML

  cat > "$PROJECT/.sugar/lift/java-junit-witness/manifest.toml" <<TOML
name = "java-junit-witness-lift"
version = "0.1.0"
protocol_version = "pep/1.7.0"
kind = "lift"
command = ["$WITNESS_RPC"]
discharge_command = ["$DISCHARGE_CLI"]
witness_tool = "junit"
resolve_witness_command = ["$WITNESS_RPC"]
resolve_witness_method = "sugar.plugin.resolve_witness"
working_dir = "."

[capabilities]
authoring_surfaces = ["java-junit-witness"]
TOML
}

require_surface_manifests() {
  local manifest
  for manifest in \
    "$PROJECT/.sugar/lift/java-test-assertions/manifest.toml" \
    "$PROJECT/.sugar/lift/java-junit-witness/manifest.toml"; do
    if [ ! -s "$manifest" ]; then
      echo "missing generated plugin manifest before mint: $manifest" >&2
      exit 1
    fi
  done
}

echo "== fetch pinned Gson source/test artifacts =="
fetch_jar "$UPSTREAM_ZIP" "https://codeload.github.com/google/gson/zip/refs/tags/gson-parent-$LIB_VERSION"
fetch_jar "$JUNIT_JAR" "$MAVEN_BASE/org/junit/platform/junit-platform-console-standalone/$JUNIT_VERSION/junit-platform-console-standalone-$JUNIT_VERSION.jar"

echo "== prepare real Gson $LIB_VERSION sources =="
rm -rf "$PROJECT" "$PROOF_SCOPE"
UNZIP_ROOT="$WORK_ROOT/gson-parent-$LIB_VERSION-extract"
rm -rf "$UNZIP_ROOT"
mkdir -p "$UNZIP_ROOT" "$PROJECT/src/main/java" "$PROJECT/src/test/java/com/google/gson" \
  "$PROOF_SCOPE/src/test/java/com/google/gson" "$PROOF_SCOPE/.sugar/vocab-exceptions"
unzip -q "$UPSTREAM_ZIP" -d "$UNZIP_ROOT"
UPSTREAM="$UNZIP_ROOT/gson-gson-parent-$LIB_VERSION"
cp -R "$UPSTREAM/gson/src/main/java/com" "$PROJECT/src/main/java/"
cp -R "$UPSTREAM/gson/src/main/java-templates/com" "$PROJECT/src/main/java/"
cp "$UPSTREAM/gson/src/test/java/com/google/gson/JsonStreamParserTest.java" \
  "$PROJECT/src/test/java/com/google/gson/JsonStreamParserTest.java"
cp "$PROJECT/src/test/java/com/google/gson/JsonStreamParserTest.java" \
  "$PROOF_SCOPE/src/test/java/com/google/gson/JsonStreamParserTest.java"
cp "$UPSTREAM/LICENSE" "$PROJECT/LICENSE" 2>/dev/null || true
cp "$UPSTREAM/README.md" "$PROJECT/README.md" 2>/dev/null || true

mkdir -p "$PROJECT/.sugar/lift/java-test-assertions" \
  "$PROJECT/.sugar/lift/java-junit-witness" \
  "$PROJECT/.sugar/vocab-exceptions"

cat > "$PROJECT/.sugar/config.toml" <<'TOML'
[[plugins]]
name = "java-test-assertions-lift"
kind = "lift"
surface = "java-test-assertions"
emit = "ir-document"
workspace_override = "../proof-scope"

[[plugins]]
name = "java-junit-witness-lift"
kind = "lift"
surface = "java-junit-witness"

[solvers]
default = "z3"

[solvers.dispatch]
linear_arithmetic = "z3"
default = "z3"

[solvers.z3]
binary = "z3"
flags = ["-smt2", "-in"]
TOML

write_surface_manifests

cat > "$PROJECT/.sugar/vocab-exceptions/org.junit.Assert.json" <<'JSON'
{
  "overrides": {
    "equality": ["assertEquals"]
  }
}
JSON
cp "$PROJECT/.sugar/vocab-exceptions/org.junit.Assert.json" \
  "$PROOF_SCOPE/.sugar/vocab-exceptions/org.junit.Assert.json"

export SUGAR_JUNIT_CONSOLE_JAR="$JUNIT_JAR"
export SUGAR_JAVA_ASSERT_CLASSPATH="$JUNIT_JAR"
export SUGAR_JAVA_EXTRA_CLASSPATH="$PROJECT/src/test/resources"
export SUGAR_JAVA_JUNIT_SELECT_CLASS="com.google.gson.JsonStreamParserTest"

java_count="$(find "$PROJECT" -name '*.java' | wc -l | tr -d ' ')"
test_count="$(find "$PROJECT" -name '*Test.java' | wc -l | tr -d ' ')"
jsr380_count="$({ grep -R -E '@(Min|Max|Size|NotNull)\b' "$PROJECT/src" --include='*.java' 2>/dev/null || true; } | wc -l | tr -d ' ')"

echo "SCOPE: proving Gson $LIB_VERSION with zero source changes from the official upstream gson-parent-$LIB_VERSION tag."
echo "SCOPE: consistency axis covers exact assertion rows from a real Gson test-source subcorpus (JsonStreamParserTest.java) learned from the real JUnit jar; unsupported/non-exact rows elsewhere are not claimed by this receipt."
echo "SCOPE: witness axis compiles the real Gson core source plus real JsonStreamParserTest JUnit class; compiler facts and framework runtime behavior are not re-proven."
echo "real-lib=Gson version=$LIB_VERSION java_files=$java_count test_files=$test_count jsr380_constraints=$jsr380_count"
if [ "$jsr380_count" = "0" ]; then
  echo "implication-edge stretch: skipped because this Gson artifact has no JSR-380 @Min/@Max/@Size/@NotNull method constraints in the checked scope."
fi

echo "== derive assertion vocabulary from real JUnit jar =="
javap -classpath "$JUNIT_JAR" -public org.junit.Assert \
  | grep -E 'assertEquals\(java.lang.Object, java.lang.Object\)|assertEquals\(java.lang.String, java.lang.Object, java.lang.Object\)|assertTrue\(boolean\)|assertEquals\(double, double, double\)' \
  | sed 's/^/real-junit4-signature: /'
echo "vocab override: .sugar/vocab-exceptions/org.junit.Assert.json declares exact equality for the jar body gap; javap-signature tolerance overloads remain Approx"

echo "== mint Gson real source/test suite =="
rm -f "$PROJECT"/blake3-512:*.proof "$PROJECT/.prove.json" "$PROJECT/.verify.json"
rm -rf "$PROJECT/.sugar/runs" "$PROJECT/.sugar/witnesses" "$PROJECT/target"
write_surface_manifests
require_surface_manifests
(cd "$PROJECT" && "$SUGAR" mint --out .) >/dev/null

proof="$(find "$PROJECT" -maxdepth 1 -name 'blake3-512:*.proof' -print -quit)"
if [ -z "$proof" ]; then
  echo "Gson did not mint a proof" >&2
  exit 1
fi

echo "== verify durable proof+witness =="
set +e
(cd "$PROJECT" && "$SUGAR" verify --project . --json) > "$PROJECT/.verify.json" 2>&1
verify_rc=$?
set -e

summary="$(
  python3 - "$PROJECT/.verify.json" "$PROJECT/target/sugar-java-junit/reports" "$verify_rc" <<'PY'
import json
import re
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

verify_path, reports_dir, verify_rc = sys.argv[1:4]
if int(verify_rc) != 0:
    raise SystemExit(f"durable verify expected exit 0, got {verify_rc}")
text = Path(verify_path).read_text(encoding="utf-8")
match = re.search(r"(?m)^\{", text)
if not match:
    raise SystemExit("missing JSON verify report")
data = json.loads(text[match.start():])
rows = data.get("rows") or data.get("claims") or data.get("obligations") or (data if isinstance(data, list) else [])
consistency = []
witness = []
for row in rows:
    prop = row.get("property") or row.get("predicate") or ""
    if "witness-package" in prop:
        witness.append(row)
    elif prop.startswith("consistency:"):
        consistency.append(row)

consistency_statuses = [row.get("status") or row.get("result") or "" for row in consistency]
witness_statuses = [row.get("status") or row.get("result") or "" for row in witness]
if not consistency:
    raise SystemExit("no exact assertion consistency rows were lifted")
bad_consistency = [s for s in consistency_statuses if s != "discharged"]
if bad_consistency:
    raise SystemExit(f"non-discharged consistency rows: {bad_consistency}")
if witness_statuses != ["discharged"]:
    raise SystemExit(f"witness status expected [discharged], got {witness_statuses}")

testcases = failures = skipped = 0
for path in Path(reports_dir).glob("*.xml"):
    root = ET.parse(path).getroot()
    for case in root.iter("testcase"):
        testcases += 1
        if any(child.tag.endswith("failure") or child.tag.endswith("error") for child in case):
            failures += 1
        if any(child.tag.endswith("skipped") for child in case):
            skipped += 1
print(
    f"consistency_rows={len(consistency)} consistency=discharged "
    f"witness=discharged junit_testcases={testcases} junit_failures={failures} junit_skipped={skipped}"
)
PY
)"

echo "$summary"
if ! grep -q 'junit_failures=0' <<<"$summary"; then
  echo "real JUnit witness had failures" >&2
  cat "$PROJECT/.verify.json" >&2
  exit 1
fi

echo "java real-library Gson self-check passed"
