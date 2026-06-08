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

CODEC_VERSION="${CODEC_VERSION:-1.17.1}"
JUNIT_VERSION="${JUNIT_VERSION:-1.10.2}"
LANG3_VERSION="${LANG3_VERSION:-3.14.0}"
IO_VERSION="${IO_VERSION:-2.16.1}"
HAMCREST_VERSION="${HAMCREST_VERSION:-2.2}"
MAVEN_BASE="${MAVEN_BASE:-https://repo1.maven.org/maven2}"

WORK_ROOT="$HERE/work"
PROJECT="$WORK_ROOT/commons-codec-$CODEC_VERSION"
JAR_DIR="${SUGAR_JAVA_REAL_LIB_JAR_DIR:-/tmp/sugar-java-real-lib-capstone}"
CODEC_SOURCES_JAR="$JAR_DIR/commons-codec-$CODEC_VERSION-sources.jar"
CODEC_TEST_SOURCES_JAR="$JAR_DIR/commons-codec-$CODEC_VERSION-test-sources.jar"
JUNIT_JAR="${SUGAR_JUNIT_CONSOLE_JAR:-$JAR_DIR/junit-platform-console-standalone-$JUNIT_VERSION.jar}"
LANG3_JAR="$JAR_DIR/commons-lang3-$LANG3_VERSION.jar"
IO_JAR="$JAR_DIR/commons-io-$IO_VERSION.jar"
HAMCREST_JAR="$JAR_DIR/hamcrest-$HAMCREST_VERSION.jar"

if [ "${JAVA_REAL_LIB_CAPSTONE_ON_REMOTE:-0}" != "1" ] \
  && [ "${JAVA_REAL_LIB_CAPSTONE_USE_BCARGO:-1}" != "0" ] \
  && [ "$(uname -s)" != "Linux" ]; then
  echo "== run java real-library capstone on battleaxe via bcargo =="
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
  remote_cmd="cd $(printf '%q' "$remote_repo") && JAVA_REAL_LIB_CAPSTONE_ON_REMOTE=1 JAVA_REAL_LIB_CAPSTONE_SKIP_LOCAL_BUILD=1 examples/java-real-lib-capstone/run.sh"
  ssh -o BatchMode=yes "$remote_host" "bash -lc $(printf '%q' "$remote_cmd")"
  exit $?
fi

if [ "${JAVA_REAL_LIB_CAPSTONE_SKIP_LOCAL_BUILD:-0}" != "1" ]; then
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

if ! command -v javac >/dev/null 2>&1 || ! command -v java >/dev/null 2>&1 || ! command -v jar >/dev/null 2>&1; then
  echo "missing JDK tools on this host; run this showcase on battleaxe/Linux" >&2
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

echo "== fetch pinned Apache Commons Codec source/test artifacts =="
fetch_jar "$CODEC_SOURCES_JAR" "$MAVEN_BASE/commons-codec/commons-codec/$CODEC_VERSION/commons-codec-$CODEC_VERSION-sources.jar"
fetch_jar "$CODEC_TEST_SOURCES_JAR" "$MAVEN_BASE/commons-codec/commons-codec/$CODEC_VERSION/commons-codec-$CODEC_VERSION-test-sources.jar"
fetch_jar "$JUNIT_JAR" "$MAVEN_BASE/org/junit/platform/junit-platform-console-standalone/$JUNIT_VERSION/junit-platform-console-standalone-$JUNIT_VERSION.jar"
fetch_jar "$LANG3_JAR" "$MAVEN_BASE/org/apache/commons/commons-lang3/$LANG3_VERSION/commons-lang3-$LANG3_VERSION.jar"
fetch_jar "$IO_JAR" "$MAVEN_BASE/commons-io/commons-io/$IO_VERSION/commons-io-$IO_VERSION.jar"
fetch_jar "$HAMCREST_JAR" "$MAVEN_BASE/org/hamcrest/hamcrest/$HAMCREST_VERSION/hamcrest-$HAMCREST_VERSION.jar"

echo "== prepare real Apache Commons Codec $CODEC_VERSION sources =="
rm -rf "$PROJECT"
mkdir -p "$PROJECT"
(cd "$PROJECT" && jar xf "$CODEC_SOURCES_JAR" && jar xf "$CODEC_TEST_SOURCES_JAR")
mkdir -p "$PROJECT/src/test/resources"
while IFS= read -r -d '' file; do
  rel="${file#$PROJECT/}"
  mkdir -p "$PROJECT/src/test/resources/$(dirname "$rel")"
  cp "$file" "$PROJECT/src/test/resources/$rel"
done < <(find "$PROJECT/org" -type f ! -name '*.java' -print0)

mkdir -p "$PROJECT/.sugar/lift/java-test-assertions" \
  "$PROJECT/.sugar/lift/java-junit-witness" \
  "$PROJECT/.sugar/vocab-exceptions"

cat > "$PROJECT/.sugar/config.toml" <<'TOML'
[[plugins]]
name = "java-test-assertions-lift"
kind = "lift"
surface = "java-test-assertions"
emit = "ir-document"

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

cat > "$PROJECT/.sugar/vocab-exceptions/org.junit.jupiter.api.Assertions.json" <<'JSON'
{
  "overrides": {
    "equality": ["assertEquals"],
    "truth": ["assertTrue"]
  }
}
JSON

EXTRA_CP="$LANG3_JAR:$IO_JAR:$HAMCREST_JAR:$PROJECT/src/test/resources"
export SUGAR_JUNIT_CONSOLE_JAR="$JUNIT_JAR"
export SUGAR_JAVA_ASSERT_CLASSPATH="$JUNIT_JAR"
export SUGAR_JAVA_EXTRA_CLASSPATH="$EXTRA_CP"

java_count="$(find "$PROJECT" -name '*.java' | wc -l | tr -d ' ')"
test_count="$(find "$PROJECT" -name '*Test.java' | wc -l | tr -d ' ')"
jsr380_count="$({ grep -R -E '@(Min|Max|Size|NotNull)\b' "$PROJECT/org" --include='*.java' 2>/dev/null || true; } | wc -l | tr -d ' ')"

echo "SCOPE: proving Apache Commons Codec $CODEC_VERSION with zero source changes."
echo "SCOPE: consistency axis covers exact assertion rows learned from the real JUnit jar; unsupported/non-exact assertion forms are not claimed by this receipt."
echo "SCOPE: witness axis compiles and runs the real Commons Codec JUnit suite; compiler facts and framework runtime behavior are not re-proven."
echo "real-lib=Apache Commons Codec version=$CODEC_VERSION java_files=$java_count test_files=$test_count jsr380_constraints=$jsr380_count"
if [ "$jsr380_count" = "0" ]; then
  echo "implication-edge stretch: skipped because this Commons Codec artifact has no JSR-380 @Min/@Max/@Size/@NotNull method constraints."
fi

echo "== derive assertion vocabulary from real JUnit jar =="
javap -classpath "$JUNIT_JAR" -public org.junit.jupiter.api.Assertions \
  | grep -E 'assertEquals\(double, double, double|assertEquals\(int, int\)|assertEquals\(java.lang.Object, java.lang.Object\)|assertTrue\(boolean\)' \
  | sed 's/^/real-junit-signature: /'
echo "vocab override: .sugar/vocab-exceptions/org.junit.jupiter.api.Assertions.json declares equality/truth for the jar body gap; javap-signature tolerance overloads remain Approx"

echo "== mint Apache Commons Codec real source/test suite =="
rm -f "$PROJECT"/blake3-512:*.proof "$PROJECT/.prove.json"
rm -rf "$PROJECT/.sugar/runs" "$PROJECT/.sugar/witnesses" "$PROJECT/target"
write_surface_manifests
require_surface_manifests
(cd "$PROJECT" && "$SUGAR" mint --out .) >/dev/null

proof="$(find "$PROJECT" -maxdepth 1 -name 'blake3-512:*.proof' -print -quit)"
if [ -z "$proof" ]; then
  echo "Apache Commons Codec did not mint a proof" >&2
  exit 1
fi

echo "== prove consistency + witness =="
set +e
(cd "$PROJECT" && "$SUGAR" prove . --json) > "$PROJECT/.prove.json" 2>&1
prove_rc=$?
set -e
: "$prove_rc"

summary="$(
  python3 - "$PROJECT/.prove.json" "$PROJECT/target/sugar-java-junit/reports" <<'PY'
import json
import re
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

prove_path, reports_dir = sys.argv[1:3]
text = Path(prove_path).read_text(encoding="utf-8")
match = re.search(r"(?m)^\{", text)
if not match:
    raise SystemExit("missing JSON prove report")
data = json.loads(text[match.start():])
rows = data.get("rows") or data.get("obligations") or (data if isinstance(data, list) else [])
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
  cat "$PROJECT/.prove.json" >&2
  exit 1
fi

echo "java real-library capstone self-check passed"
