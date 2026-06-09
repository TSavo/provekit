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

JUNIT_VERSION="${JUNIT_VERSION:-1.10.2}"
CODEC_VERSION="${CODEC_VERSION:-1.17.1}"
IO_VERSION="${IO_VERSION:-2.16.1}"
TEXT_VERSION="${TEXT_VERSION:-1.12.0}"
GSON_VERSION="${GSON_VERSION:-2.10.1}"
LANG3_VERSION="${LANG3_VERSION:-3.14.0}"
MAVEN_BASE="${MAVEN_BASE:-https://repo1.maven.org/maven2}"
JAR_DIR="${SUGAR_JAVA_CONJOIN_JAR_DIR:-/tmp/sugar-java-consumer-conjoin}"
JUNIT_JAR="${SUGAR_JUNIT_CONSOLE_JAR:-$JAR_DIR/junit-platform-console-standalone-$JUNIT_VERSION.jar}"
CODEC_JAR="$JAR_DIR/commons-codec-$CODEC_VERSION.jar"
IO_JAR="$JAR_DIR/commons-io-$IO_VERSION.jar"
TEXT_JAR="$JAR_DIR/commons-text-$TEXT_VERSION.jar"
GSON_JAR="$JAR_DIR/gson-$GSON_VERSION.jar"
LANG3_JAR="$JAR_DIR/commons-lang3-$LANG3_VERSION.jar"

for suite in good bad; do
  if [ ! -d "$HERE/$suite" ]; then
    echo "missing suite directory: $HERE/$suite" >&2
    exit 1
  fi
done

if [ "${JAVA_CONJOIN_SHOWCASE_ON_REMOTE:-0}" != "1" ] \
  && [ "${JAVA_CONJOIN_SHOWCASE_USE_BCARGO:-1}" != "0" ] \
  && [ "$(uname -s)" != "Linux" ]; then
  echo "== run java consumer conjoin showcase on battleaxe via bcargo =="
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
  remote_cmd="cd $(printf '%q' "$remote_repo") && JAVA_CONJOIN_SHOWCASE_ON_REMOTE=1 JAVA_CONJOIN_SHOWCASE_SKIP_LOCAL_BUILD=1 examples/java-consumer-conjoin/run.sh"
  ssh -o BatchMode=yes "$remote_host" "bash -lc $(printf '%q' "$remote_cmd")"
  exit $?
fi

if [ "${JAVA_CONJOIN_SHOWCASE_SKIP_LOCAL_BUILD:-0}" != "1" ]; then
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

if ! command -v javac >/dev/null 2>&1 || ! command -v java >/dev/null 2>&1; then
  echo "missing JDK on this host; run this showcase on battleaxe/Linux" >&2
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

echo "== fetch pinned real-library jars =="
fetch_jar "$JUNIT_JAR" "$MAVEN_BASE/org/junit/platform/junit-platform-console-standalone/$JUNIT_VERSION/junit-platform-console-standalone-$JUNIT_VERSION.jar"
fetch_jar "$CODEC_JAR" "$MAVEN_BASE/commons-codec/commons-codec/$CODEC_VERSION/commons-codec-$CODEC_VERSION.jar"
fetch_jar "$IO_JAR" "$MAVEN_BASE/commons-io/commons-io/$IO_VERSION/commons-io-$IO_VERSION.jar"
fetch_jar "$TEXT_JAR" "$MAVEN_BASE/org/apache/commons/commons-text/$TEXT_VERSION/commons-text-$TEXT_VERSION.jar"
fetch_jar "$GSON_JAR" "$MAVEN_BASE/com/google/code/gson/gson/$GSON_VERSION/gson-$GSON_VERSION.jar"
fetch_jar "$LANG3_JAR" "$MAVEN_BASE/org/apache/commons/commons-lang3/$LANG3_VERSION/commons-lang3-$LANG3_VERSION.jar"

export SUGAR_JUNIT_CONSOLE_JAR="$JUNIT_JAR"
export SUGAR_JAVA_ASSERT_CLASSPATH="$JUNIT_JAR"
export SUGAR_JAVA_EXTRA_CLASSPATH="$CODEC_JAR:$IO_JAR:$TEXT_JAR:$GSON_JAR:$LANG3_JAR"

echo "== derive assertion vocabulary from real JUnit jar =="
javap -classpath "$JUNIT_JAR" -public org.junit.jupiter.api.Assertions \
  | grep -E 'assertEquals\(double, double, double|assertEquals\(int, int\)|assertTrue\(boolean\)' \
  | sed 's/^/real-junit-signature: /'
echo "vocab override: .sugar/vocab-exceptions/org.junit.jupiter.api.Assertions.json declares equality/truth for the jar body gap; javap-signature tolerance overloads remain Approx"

render_manifests() {
  local suite="$1"
  local base="$HERE/$suite/.sugar/lift"

  sed "s|@BIN_DIR@|$BIN_DIR|g" \
    "$base/java-test-assertions/manifest.toml.in" \
    > "$base/java-test-assertions/manifest.toml"

  sed "s|@BIN_DIR@|$BIN_DIR|g" \
    "$base/java-junit-witness/manifest.toml.in" \
    > "$base/java-junit-witness/manifest.toml"
}

clean_suite() {
  local suite="$1"
  local dir="$HERE/$suite"
  rm -f "$dir"/blake3-512:*.proof "$dir/.prove.json" "$dir/.verify.json" "$dir/.verify_recompute.json"
  rm -rf "$dir/.sugar/runs" "$dir/.sugar/witnesses" "$dir/target"
  clean_staged_library_sources "$dir"
}

verify_status() {
  python3 - "$1" "$2" <<'PY'
import json
import re
import sys

path, kind = sys.argv[1:3]
text = open(path, "r", encoding="utf-8").read()
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

run_suite() {
  local suite="$1"
  local expect_consistency="$2"
  local expect_witness="$3"
  local dir="$HERE/$suite"

  render_manifests "$suite"
  clean_suite "$suite"
  stage_imports_for_consumer "$suite"

  echo "== sugar mint $suite =="
  echo "cmd: (cd $dir && $SUGAR mint --out .)"
  (cd "$dir" && "$SUGAR" mint --out .) >/dev/null

  local proof
  proof="$(find "$dir" -maxdepth 1 -name 'blake3-512:*.proof' -print -quit)"
  if [ -z "$proof" ]; then
    echo "$suite did not mint a proof" >&2
    exit 1
  fi

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
    if [ "$verify_rc" -ne 0 ]; then
      echo "$suite durable verify expected exit 0, got $verify_rc" >&2
      cat "$dir/.verify.json" >&2
      exit 1
    fi
    if [ "$got_consistency" != "discharged" ]; then
      echo "$suite consistency expected discharged, got $got_consistency" >&2
      cat "$dir/.verify.json" >&2
      exit 1
    fi
  else
    if [ "$verify_rc" -eq 0 ]; then
      echo "$suite durable verify expected refusal, but verify exited 0" >&2
      cat "$dir/.verify.json" >&2
      exit 1
    fi
    if [ "$got_consistency" = "discharged" ] || [ "$got_consistency" = "MISSING" ]; then
      echo "$suite consistency expected refusal, got $got_consistency" >&2
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

ensure_real_library_proof() {
  local name="$1"
  local dir="$2"
  local env_prefix="$3"
  local proof
  proof="$(find "$dir/work" -maxdepth 5 -name 'blake3-512:*.proof' -print -quit 2>/dev/null || true)"
  if [ -z "$proof" ]; then
    echo "== mint real library proof: $name ==" >&2
    (cd "$REPO" && env "${env_prefix}_ON_REMOTE=1" "${env_prefix}_SKIP_LOCAL_BUILD=1" "$dir/run.sh") >&2
    proof="$(find "$dir/work" -maxdepth 5 -name 'blake3-512:*.proof' -print -quit 2>/dev/null || true)"
  else
    echo "== reuse real library proof: $name $(basename "$proof") ==" >&2
  fi
  if [ -z "$proof" ]; then
    echo "missing real library proof for $name after running $dir/run.sh" >&2
    exit 1
  fi
  printf '%s\n' "$proof"
}

stage_imports_for_consumer() {
  local consumer="$1"
  mkdir -p "$HERE/$consumer/.sugar/imports"
  mkdir -p "$HERE/$consumer/.sugar/witnesses"
  rm -f "$HERE/$consumer/.sugar/imports/"*.proof
  rm -f "$HERE/$consumer/.sugar/witnesses/"*.witness
  cp "$CODEC_PROOF" "$HERE/$consumer/.sugar/imports/"
  cp "$IO_PROOF" "$HERE/$consumer/.sugar/imports/"
  cp "$TEXT_PROOF" "$HERE/$consumer/.sugar/imports/"
  cp "$GSON_PROOF" "$HERE/$consumer/.sugar/imports/"
  stage_witness_for_proof "$CODEC_PROOF" "$HERE/$consumer/.sugar/witnesses"
  stage_witness_for_proof "$IO_PROOF" "$HERE/$consumer/.sugar/witnesses"
  stage_witness_for_proof "$TEXT_PROOF" "$HERE/$consumer/.sugar/witnesses"
  stage_witness_for_proof "$GSON_PROOF" "$HERE/$consumer/.sugar/witnesses"
  echo "$consumer imports:"
  printf '  %s\n' "$(basename "$CODEC_PROOF")" "$(basename "$IO_PROOF")" "$(basename "$TEXT_PROOF")" "$(basename "$GSON_PROOF")"
  echo "$consumer witness bundles:"
  find "$HERE/$consumer/.sugar/witnesses" -maxdepth 1 -name '*.witness' -type f -print \
    | sort \
    | while IFS= read -r witness; do
        printf '  %s\n' "$(basename "$witness")"
      done
}

clean_staged_library_sources() {
  local dir="$1"
  rm -rf "$dir/org" "$dir/com"
  rm -rf "$dir/src/main"
  rm -rf "$dir/src/test/java/org" "$dir/src/test/java/com"
  rm -rf "$dir/src/main/resources" "$dir/src/test/resources"
  rm -f "$dir/pom.xml" "$dir/build.gradle" "$dir/settings.gradle"
}

stage_witness_for_proof() {
  local proof="$1"
  local dest="$2"
  local source_dir
  local count=0
  source_dir="$(dirname "$proof")/.sugar/witnesses"
  if [ -d "$source_dir" ]; then
    while IFS= read -r witness; do
      cp "$witness" "$dest/"
      count=$((count + 1))
    done < <(find "$source_dir" -maxdepth 1 -name '*.witness' -type f | sort)
  fi
  if [ "$count" -eq 0 ]; then
    echo "missing durable witness package for imported proof $proof" >&2
    exit 1
  fi
}

CODEC_PROOF="$(ensure_real_library_proof "Apache Commons Codec $CODEC_VERSION" "$REPO/examples/java-real-lib-capstone" "JAVA_REAL_LIB_CAPSTONE")"
IO_PROOF="$(ensure_real_library_proof "Apache Commons IO $IO_VERSION" "$REPO/examples/java-real-lib-commons-io" "JAVA_REAL_LIB_COMMONS_IO")"
TEXT_PROOF="$(ensure_real_library_proof "Apache Commons Text $TEXT_VERSION" "$REPO/examples/java-real-lib-commons-text" "JAVA_REAL_LIB_COMMONS_TEXT")"
GSON_PROOF="$(ensure_real_library_proof "Gson $GSON_VERSION" "$REPO/examples/java-real-lib-gson" "JAVA_REAL_LIB_GSON")"

echo "SCOPE: real libraries imported = Commons Codec $CODEC_VERSION, Commons IO $IO_VERSION, Commons Text $TEXT_VERSION, Gson $GSON_VERSION"
echo "SCOPE: proven Codec row mirrored = org.apache.commons.codec.binary.Base64Test.java:282 assertEquals(\"K/fMJwH+Q5e0nr7tWsxwkA==\", Base64.encodeBase64String(b4), ...), b4=Hex.decodeHex(\"2bf7cc2701fe4397b49ebeed5acc7090\")"
echo "SCOPE: this turns a runtime error into a compile-time contract: the BAD url-safe assumption fails as a JUnit witness and is also refused statically by the conjoined proof against Codec's real Base64Test row."

run_suite good discharged discharged
run_suite bad refused refused

echo "collision-pair: bad app assertEquals(\"K_fMJwH-Q5e0nr7tWsxwkA\", Base64.encodeBase64String(b4)) vs Codec Base64Test assertEquals(\"K/fMJwH+Q5e0nr7tWsxwkA==\", Base64.encodeBase64String(b4))"
echo "java consumer conjoin showcase self-check passed"
