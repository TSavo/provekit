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

LIB_VERSION="${LIB_VERSION:-1.12.0}"
JUNIT_VERSION="${JUNIT_VERSION:-1.10.2}"
LANG3_VERSION="${LANG3_VERSION:-3.14.0}"
IO_VERSION="${IO_VERSION:-2.16.1}"
HAMCREST_VERSION="${HAMCREST_VERSION:-2.2}"
BYTEBUDDY_VERSION="${BYTEBUDDY_VERSION:-1.14.13}"
MOCKITO_VERSION="${MOCKITO_VERSION:-4.11.0}"
OBJENESIS_VERSION="${OBJENESIS_VERSION:-3.3}"
ASSERTJ_VERSION="${ASSERTJ_VERSION:-3.25.3}"
GRAAL_VERSION="${GRAAL_VERSION:-22.3.5}"
ICU4J_VERSION="${ICU4J_VERSION:-71.1}"
RNG_VERSION="${RNG_VERSION:-1.5}"
JMH_VERSION="${JMH_VERSION:-1.37}"
MAVEN_BASE="${MAVEN_BASE:-https://repo1.maven.org/maven2}"

WORK_ROOT="$HERE/work"
PROJECT="$WORK_ROOT/commons-text-$LIB_VERSION"
PROOF_SCOPE="$WORK_ROOT/proof-scope"
JAR_DIR="${SUGAR_JAVA_REAL_LIB_JAR_DIR:-/tmp/sugar-java-real-lib-commons-text}"
LIB_SOURCES_JAR="$JAR_DIR/commons-text-$LIB_VERSION-sources.jar"
LIB_TEST_SOURCES_JAR="$JAR_DIR/commons-text-$LIB_VERSION-test-sources.jar"
JUNIT_JAR="${SUGAR_JUNIT_CONSOLE_JAR:-$JAR_DIR/junit-platform-console-standalone-$JUNIT_VERSION.jar}"
LANG3_JAR="$JAR_DIR/commons-lang3-$LANG3_VERSION.jar"
IO_JAR="$JAR_DIR/commons-io-$IO_VERSION.jar"
HAMCREST_JAR="$JAR_DIR/hamcrest-$HAMCREST_VERSION.jar"
BYTEBUDDY_JAR="$JAR_DIR/byte-buddy-$BYTEBUDDY_VERSION.jar"
BYTEBUDDY_AGENT_JAR="$JAR_DIR/byte-buddy-agent-$BYTEBUDDY_VERSION.jar"
MOCKITO_INLINE_JAR="$JAR_DIR/mockito-inline-$MOCKITO_VERSION.jar"
MOCKITO_CORE_JAR="$JAR_DIR/mockito-core-$MOCKITO_VERSION.jar"
OBJENESIS_JAR="$JAR_DIR/objenesis-$OBJENESIS_VERSION.jar"
ASSERTJ_JAR="$JAR_DIR/assertj-core-$ASSERTJ_VERSION.jar"
GRAAL_JS_JAR="$JAR_DIR/js-$GRAAL_VERSION.jar"
GRAAL_JS_SCRIPTENGINE_JAR="$JAR_DIR/js-scriptengine-$GRAAL_VERSION.jar"
GRAAL_REGEX_JAR="$JAR_DIR/regex-$GRAAL_VERSION.jar"
GRAAL_TRUFFLE_API_JAR="$JAR_DIR/truffle-api-$GRAAL_VERSION.jar"
GRAAL_SDK_JAR="$JAR_DIR/graal-sdk-$GRAAL_VERSION.jar"
ICU4J_JAR="$JAR_DIR/icu4j-$ICU4J_VERSION.jar"
RNG_SIMPLE_JAR="$JAR_DIR/commons-rng-simple-$RNG_VERSION.jar"
RNG_CORE_JAR="$JAR_DIR/commons-rng-core-$RNG_VERSION.jar"
RNG_CLIENT_API_JAR="$JAR_DIR/commons-rng-client-api-$RNG_VERSION.jar"
JMH_CORE_JAR="$JAR_DIR/jmh-core-$JMH_VERSION.jar"

if [ "${JAVA_REAL_LIB_COMMONS_TEXT_ON_REMOTE:-0}" != "1" ] \
  && [ "${JAVA_REAL_LIB_COMMONS_TEXT_USE_BCARGO:-1}" != "0" ] \
  && [ "$(uname -s)" != "Linux" ]; then
  echo "== run java real-library Commons Text on battleaxe via bcargo =="
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
  remote_cmd="cd $(printf '%q' "$remote_repo") && JAVA_REAL_LIB_COMMONS_TEXT_ON_REMOTE=1 JAVA_REAL_LIB_COMMONS_TEXT_SKIP_LOCAL_BUILD=1 examples/java-real-lib-commons-text/run.sh"
  ssh -o BatchMode=yes "$remote_host" "bash -lc $(printf '%q' "$remote_cmd")"
  exit $?
fi

if [ "${JAVA_REAL_LIB_COMMONS_TEXT_SKIP_LOCAL_BUILD:-0}" != "1" ]; then
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

echo "== fetch pinned Apache Commons Text source/test artifacts =="
fetch_jar "$LIB_SOURCES_JAR" "$MAVEN_BASE/org/apache/commons/commons-text/$LIB_VERSION/commons-text-$LIB_VERSION-sources.jar"
fetch_jar "$LIB_TEST_SOURCES_JAR" "$MAVEN_BASE/org/apache/commons/commons-text/$LIB_VERSION/commons-text-$LIB_VERSION-test-sources.jar"
fetch_jar "$JUNIT_JAR" "$MAVEN_BASE/org/junit/platform/junit-platform-console-standalone/$JUNIT_VERSION/junit-platform-console-standalone-$JUNIT_VERSION.jar"
fetch_jar "$LANG3_JAR" "$MAVEN_BASE/org/apache/commons/commons-lang3/$LANG3_VERSION/commons-lang3-$LANG3_VERSION.jar"
fetch_jar "$IO_JAR" "$MAVEN_BASE/commons-io/commons-io/$IO_VERSION/commons-io-$IO_VERSION.jar"
fetch_jar "$HAMCREST_JAR" "$MAVEN_BASE/org/hamcrest/hamcrest/$HAMCREST_VERSION/hamcrest-$HAMCREST_VERSION.jar"
fetch_jar "$BYTEBUDDY_JAR" "$MAVEN_BASE/net/bytebuddy/byte-buddy/$BYTEBUDDY_VERSION/byte-buddy-$BYTEBUDDY_VERSION.jar"
fetch_jar "$BYTEBUDDY_AGENT_JAR" "$MAVEN_BASE/net/bytebuddy/byte-buddy-agent/$BYTEBUDDY_VERSION/byte-buddy-agent-$BYTEBUDDY_VERSION.jar"
fetch_jar "$MOCKITO_INLINE_JAR" "$MAVEN_BASE/org/mockito/mockito-inline/$MOCKITO_VERSION/mockito-inline-$MOCKITO_VERSION.jar"
fetch_jar "$MOCKITO_CORE_JAR" "$MAVEN_BASE/org/mockito/mockito-core/$MOCKITO_VERSION/mockito-core-$MOCKITO_VERSION.jar"
fetch_jar "$OBJENESIS_JAR" "$MAVEN_BASE/org/objenesis/objenesis/$OBJENESIS_VERSION/objenesis-$OBJENESIS_VERSION.jar"
fetch_jar "$ASSERTJ_JAR" "$MAVEN_BASE/org/assertj/assertj-core/$ASSERTJ_VERSION/assertj-core-$ASSERTJ_VERSION.jar"
fetch_jar "$GRAAL_JS_JAR" "$MAVEN_BASE/org/graalvm/js/js/$GRAAL_VERSION/js-$GRAAL_VERSION.jar"
fetch_jar "$GRAAL_JS_SCRIPTENGINE_JAR" "$MAVEN_BASE/org/graalvm/js/js-scriptengine/$GRAAL_VERSION/js-scriptengine-$GRAAL_VERSION.jar"
fetch_jar "$GRAAL_REGEX_JAR" "$MAVEN_BASE/org/graalvm/regex/regex/$GRAAL_VERSION/regex-$GRAAL_VERSION.jar"
fetch_jar "$GRAAL_TRUFFLE_API_JAR" "$MAVEN_BASE/org/graalvm/truffle/truffle-api/$GRAAL_VERSION/truffle-api-$GRAAL_VERSION.jar"
fetch_jar "$GRAAL_SDK_JAR" "$MAVEN_BASE/org/graalvm/sdk/graal-sdk/$GRAAL_VERSION/graal-sdk-$GRAAL_VERSION.jar"
fetch_jar "$ICU4J_JAR" "$MAVEN_BASE/com/ibm/icu/icu4j/$ICU4J_VERSION/icu4j-$ICU4J_VERSION.jar"
fetch_jar "$RNG_SIMPLE_JAR" "$MAVEN_BASE/org/apache/commons/commons-rng-simple/$RNG_VERSION/commons-rng-simple-$RNG_VERSION.jar"
fetch_jar "$RNG_CORE_JAR" "$MAVEN_BASE/org/apache/commons/commons-rng-core/$RNG_VERSION/commons-rng-core-$RNG_VERSION.jar"
fetch_jar "$RNG_CLIENT_API_JAR" "$MAVEN_BASE/org/apache/commons/commons-rng-client-api/$RNG_VERSION/commons-rng-client-api-$RNG_VERSION.jar"
fetch_jar "$JMH_CORE_JAR" "$MAVEN_BASE/org/openjdk/jmh/jmh-core/$JMH_VERSION/jmh-core-$JMH_VERSION.jar"

echo "== prepare real Apache Commons Text $LIB_VERSION sources =="
rm -rf "$PROJECT" "$PROOF_SCOPE"
SRC_EXTRACT="$WORK_ROOT/commons-text-$LIB_VERSION-source-extract"
TEST_EXTRACT="$WORK_ROOT/commons-text-$LIB_VERSION-test-extract"
rm -rf "$SRC_EXTRACT" "$TEST_EXTRACT"
mkdir -p "$PROJECT/src/main/java" "$PROJECT/src/test/java" "$SRC_EXTRACT" "$TEST_EXTRACT"
(cd "$SRC_EXTRACT" && jar xf "$LIB_SOURCES_JAR")
(cd "$TEST_EXTRACT" && jar xf "$LIB_TEST_SOURCES_JAR")
cp -R "$SRC_EXTRACT/org" "$PROJECT/src/main/java/"
cp -R "$TEST_EXTRACT/org" "$PROJECT/src/test/java/"
mkdir -p "$PROJECT/src/test/resources"
while IFS= read -r -d '' file; do
  rel="${file#$TEST_EXTRACT/}"
  mkdir -p "$PROJECT/src/test/resources/$(dirname "$rel")"
  cp "$file" "$PROJECT/src/test/resources/$rel"
done < <(find "$TEST_EXTRACT" -type f ! -name '*.java' -print0)
cp "$SRC_EXTRACT/META-INF/LICENSE.txt" "$PROJECT/LICENSE.txt" 2>/dev/null || true
cp "$SRC_EXTRACT/META-INF/NOTICE.txt" "$PROJECT/NOTICE.txt" 2>/dev/null || true
cp "$SRC_EXTRACT/META-INF/maven/org.apache.commons/commons-text/pom.xml" "$PROJECT/pom.xml" 2>/dev/null || true
mkdir -p "$PROJECT/src/site/xdoc"
mkdir -p "$PROOF_SCOPE/src/test/java/org/apache/commons/text/matcher" "$PROOF_SCOPE/.sugar/vocab-exceptions"
cp "$PROJECT/src/test/java/org/apache/commons/text/matcher/StringMatcherFactoryTest.java" \
  "$PROOF_SCOPE/src/test/java/org/apache/commons/text/matcher/StringMatcherFactoryTest.java"

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

cat > "$PROJECT/.sugar/vocab-exceptions/org.junit.jupiter.api.Assertions.json" <<'JSON'
{
  "overrides": {
    "equality": ["assertEquals"],
    "truth": ["assertTrue"]
  }
}
JSON
cp "$PROJECT/.sugar/vocab-exceptions/org.junit.jupiter.api.Assertions.json" \
  "$PROOF_SCOPE/.sugar/vocab-exceptions/org.junit.jupiter.api.Assertions.json"

EXTRA_CP="$LANG3_JAR:$IO_JAR:$HAMCREST_JAR:$BYTEBUDDY_JAR:$BYTEBUDDY_AGENT_JAR:$MOCKITO_INLINE_JAR:$MOCKITO_CORE_JAR:$OBJENESIS_JAR:$ASSERTJ_JAR:$GRAAL_JS_JAR:$GRAAL_JS_SCRIPTENGINE_JAR:$GRAAL_REGEX_JAR:$GRAAL_TRUFFLE_API_JAR:$GRAAL_SDK_JAR:$ICU4J_JAR:$RNG_SIMPLE_JAR:$RNG_CORE_JAR:$RNG_CLIENT_API_JAR:$JMH_CORE_JAR:$PROJECT/src/test/resources"
export SUGAR_JUNIT_CONSOLE_JAR="$JUNIT_JAR"
export SUGAR_JAVA_ASSERT_CLASSPATH="$JUNIT_JAR"
export SUGAR_JAVA_EXTRA_CLASSPATH="$EXTRA_CP"
export SUGAR_JAVA_JAVAC_ENCODING="ISO-8859-1"

java_count="$(find "$PROJECT" -name '*.java' | wc -l | tr -d ' ')"
test_count="$(find "$PROJECT" -name '*Test.java' | wc -l | tr -d ' ')"
jsr380_count="$({ grep -R -E '@(Min|Max|Size|NotNull)\b' "$PROJECT/src" --include='*.java' 2>/dev/null || true; } | wc -l | tr -d ' ')"

echo "SCOPE: proving Apache Commons Text $LIB_VERSION with zero source changes."
echo "SCOPE: consistency axis covers exact assertion rows from a real Commons Text test-source subcorpus (StringMatcherFactoryTest.java) learned from the real JUnit jar; unsupported/non-exact rows elsewhere are not claimed by this receipt."
echo "SCOPE: witness axis compiles and runs the real Commons Text JUnit suite; compiler facts and framework runtime behavior are not re-proven."
echo "real-lib=Apache Commons Text version=$LIB_VERSION java_files=$java_count test_files=$test_count jsr380_constraints=$jsr380_count"
if [ "$jsr380_count" = "0" ]; then
  echo "implication-edge stretch: skipped because this Commons Text artifact has no JSR-380 @Min/@Max/@Size/@NotNull method constraints."
fi

echo "== derive assertion vocabulary from real JUnit jar =="
javap -classpath "$JUNIT_JAR" -public org.junit.jupiter.api.Assertions \
  | grep -E 'assertEquals\(double, double, double|assertEquals\(int, int\)|assertEquals\(java.lang.Object, java.lang.Object\)|assertTrue\(boolean\)' \
  | sed 's/^/real-junit-signature: /'
echo "vocab override: .sugar/vocab-exceptions/org.junit.jupiter.api.Assertions.json declares equality/truth for the jar body gap; javap-signature tolerance overloads remain Approx"

echo "== mint Apache Commons Text real source/test suite =="
rm -f "$PROJECT"/blake3-512:*.proof "$PROJECT/.prove.json" "$PROJECT/.verify.json"
rm -rf "$PROJECT/.sugar/runs" "$PROJECT/.sugar/witnesses" "$PROJECT/target"
write_surface_manifests
require_surface_manifests
(cd "$PROJECT" && "$SUGAR" mint --out .) >/dev/null

proof="$(find "$PROJECT" -maxdepth 1 -name 'blake3-512:*.proof' -print -quit)"
if [ -z "$proof" ]; then
  echo "Apache Commons Text did not mint a proof" >&2
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

echo "java real-library Commons Text self-check passed"
