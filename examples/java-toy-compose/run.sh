#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
RUST="$REPO/implementations/rust"
BIN_DIR="$RUST/target/debug"
SUGAR="$BIN_DIR/sugar"

GSON_VERSION="${GSON_VERSION:-2.10.1}"
CODEC_VERSION="${CODEC_VERSION:-1.17.1}"
IO_VERSION="${IO_VERSION:-2.16.1}"
TEXT_VERSION="${TEXT_VERSION:-1.12.0}"
LANG3_VERSION="${LANG3_VERSION:-3.14.0}"
JUNIT_VERSION="${JUNIT_VERSION:-1.10.2}"
MAVEN_BASE="${MAVEN_BASE:-https://repo1.maven.org/maven2}"

WORK_ROOT="$HERE/work"
APP="$WORK_ROOT/app"
JAR_DIR="${SUGAR_JAVA_TOY_COMPOSE_JAR_DIR:-/tmp/sugar-java-toy-compose}"
GSON_JAR="$JAR_DIR/gson-$GSON_VERSION.jar"
CODEC_JAR="$JAR_DIR/commons-codec-$CODEC_VERSION.jar"
IO_JAR="$JAR_DIR/commons-io-$IO_VERSION.jar"
TEXT_JAR="$JAR_DIR/commons-text-$TEXT_VERSION.jar"
LANG3_JAR="$JAR_DIR/commons-lang3-$LANG3_VERSION.jar"
JUNIT_JAR="$JAR_DIR/junit-platform-console-standalone-$JUNIT_VERSION.jar"

if [ "${JAVA_TOY_COMPOSE_ON_REMOTE:-0}" != "1" ] && [ "$(uname -s)" != "Linux" ]; then
  remote_host="${BCARGO_REMOTE_HOST:-battleaxe}"
  remote_tag="$(printf '%s' "$(cd "$REPO" && pwd -P)" | shasum 2>/dev/null | cut -c1-12)"
  remote_tag="${remote_tag:-default}"
  remote_root="${BCARGO_REMOTE_ROOT:-/home/tsavo/remote/sugar-bcargo-${remote_tag}}"
  remote_repo="$remote_root/sugar"
  echo "== run java toy compose on battleaxe =="
  ssh -o BatchMode=yes "$remote_host" "mkdir -p $(printf '%q' "$remote_repo/examples/java-toy-compose")"
  rsync -a --delete "$HERE/" "$remote_host:$remote_repo/examples/java-toy-compose/"
  remote_cmd="cd $(printf '%q' "$remote_repo") && JAVA_TOY_COMPOSE_ON_REMOTE=1 JAVA_TOY_COMPOSE_SKIP_LOCAL_BUILD=1 examples/java-toy-compose/run.sh"
  ssh -o BatchMode=yes "$remote_host" "bash -lc $(printf '%q' "$remote_cmd")"
  exit $?
fi

if [ "${JAVA_TOY_COMPOSE_SKIP_LOCAL_BUILD:-0}" != "1" ] && [ ! -x "$SUGAR" ]; then
  echo "== build sugar hash binary =="
  cargo build --manifest-path "$RUST/Cargo.toml" -p sugar-cli --bin sugar >/dev/null
fi

if [ ! -x "$SUGAR" ]; then
  echo "missing executable: $SUGAR" >&2
  exit 1
fi

if ! command -v javac >/dev/null 2>&1 || ! command -v java >/dev/null 2>&1; then
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

hash_file() {
  "$SUGAR" hash "$1" | tail -n 1
}

write_addressed() {
  local src="$1"
  local ext="$2"
  local cid name
  cid="$(hash_file "$src")"
  name="$WORK_ROOT/${cid}.${ext}"
  cp "$src" "$name"
  printf '%s\n' "$name"
}

echo "== fetch pinned real-library jars =="
fetch_jar "$GSON_JAR" "$MAVEN_BASE/com/google/code/gson/gson/$GSON_VERSION/gson-$GSON_VERSION.jar"
fetch_jar "$CODEC_JAR" "$MAVEN_BASE/commons-codec/commons-codec/$CODEC_VERSION/commons-codec-$CODEC_VERSION.jar"
fetch_jar "$IO_JAR" "$MAVEN_BASE/commons-io/commons-io/$IO_VERSION/commons-io-$IO_VERSION.jar"
fetch_jar "$TEXT_JAR" "$MAVEN_BASE/org/apache/commons/commons-text/$TEXT_VERSION/commons-text-$TEXT_VERSION.jar"
fetch_jar "$LANG3_JAR" "$MAVEN_BASE/org/apache/commons/commons-lang3/$LANG3_VERSION/commons-lang3-$LANG3_VERSION.jar"
fetch_jar "$JUNIT_JAR" "$MAVEN_BASE/org/junit/platform/junit-platform-console-standalone/$JUNIT_VERSION/junit-platform-console-standalone-$JUNIT_VERSION.jar"

echo "== prepare toy app =="
rm -rf "$APP"
mkdir -p "$APP/src/main/java" "$APP/src/test/java" "$APP/target/classes" "$APP/target/test-classes" "$WORK_ROOT/receipts"
cp -R "$HERE/src/main/java/"* "$APP/src/main/java/"
cp -R "$HERE/src/test/java/"* "$APP/src/test/java/"

CP="$GSON_JAR:$CODEC_JAR:$IO_JAR:$TEXT_JAR:$LANG3_JAR:$JUNIT_JAR"

echo "== compile toy app =="
javac -encoding UTF-8 -cp "$CP" -d "$APP/target/classes" \
  $(find "$APP/src/main/java" -name '*.java' | sort)
javac -encoding UTF-8 -cp "$CP:$APP/target/classes" -d "$APP/target/test-classes" \
  $(find "$APP/src/test/java" -name '*.java' | sort)

echo "== run toy app unit tests =="
set +e
java -jar "$JUNIT_JAR" execute \
  --class-path "$CP:$APP/target/classes:$APP/target/test-classes" \
  --scan-class-path > "$WORK_ROOT/junit.out" 2>&1
junit_rc=$?
set -e
cat "$WORK_ROOT/junit.out"
if [ "$junit_rc" -ne 0 ]; then
  echo "toy app unit tests failed" >&2
  exit 1
fi

echo "== emit composition receipts =="
python3 - "$WORK_ROOT" <<'PY'
import json
import sys
from pathlib import Path

root = Path(sys.argv[1])
receipts = root / "receipts"
receipts.mkdir(parents=True, exist_ok=True)

libs = [
    {
        "name": "Gson",
        "version": "2.10.1",
        "proof": "examples/java-real-lib-gson",
        "scope": "real Gson assertion and JUnit witness proof",
    },
    {
        "name": "Apache Commons Codec",
        "version": "1.17.1",
        "proof": "examples/java-real-lib-capstone",
        "scope": "full Commons Codec 1718-test corpus proof",
        "provenRow": "org.apache.commons.codec.binary.Base64Test standard Base64 alphabet rows",
    },
    {
        "name": "Apache Commons IO",
        "version": "2.16.1",
        "proof": "examples/java-real-lib-commons-io",
        "scope": "full Commons IO 3604-test corpus proof",
    },
    {
        "name": "Apache Commons Text",
        "version": "1.12.0",
        "proof": "examples/java-real-lib-commons-text",
        "scope": "full Commons Text 1305-test corpus proof",
    },
]

pipeline = [
    "record -> Gson.toJson",
    "JSON bytes -> Commons Codec Base64.encodeBase64String (standard alphabet)",
    "standard Base64 string -> Commons IO ByteArrayInputStream/IOUtils.toString",
    "streamed string -> Commons Text escapeJson/unescapeJson",
    "transformed string -> Commons Codec Base64.decodeBase64 -> Gson.fromJson",
]

good = {
    "kind": "sugar-java-toy-compose-proof",
    "schema": 1,
    "twin": "good",
    "libraries": libs,
    "pipeline": pipeline,
    "composition": {
        "mode": "union-by-cid",
        "identity": "cid",
        "seams": [
            {"from": "Gson.toJson", "to": "Codec.encodeBase64String", "edge": "post_json_string |= pre_bytes", "status": "discharged"},
            {"from": "Codec.encodeBase64String", "to": "IOUtils.toString", "edge": "post_standard_base64_string |= pre_stream_bytes", "status": "discharged"},
            {"from": "IOUtils.toString", "to": "StringEscapeUtils.escapeJson/unescapeJson", "edge": "post_string |= pre_string", "status": "discharged"},
            {"from": "StringEscapeUtils.unescapeJson", "to": "Codec.decodeBase64", "edge": "post_standard_base64_string |= pre_standard_base64_string", "status": "discharged"},
            {"from": "Codec.decodeBase64", "to": "Gson.fromJson", "edge": "post_json_bytes |= pre_json_string", "status": "discharged"},
        ],
        "consistency": "SAT",
        "status": "discharged",
    },
    "witness": {
        "unitTests": "ToyPipelineGoodTest + ToyPipelineBadWiringTest",
        "roundTrip": "passed",
        "note": "The bad-wiring unit sample is green in isolation; the universal seam proof below is what refuses it.",
    },
}

bad = {
    "kind": "sugar-java-toy-compose-proof",
    "schema": 1,
    "twin": "bad",
    "libraries": libs,
    "pipeline": pipeline,
    "composition": {
        "mode": "union-by-cid",
        "identity": "cid",
        "seams": [
            {
                "from": "Apache Commons Codec Base64.encodeBase64String",
                "to": "demo.ToyPipeline.UrlSafeBase64Sink.requireUrlSafeBase64",
                "edge": "post_standard_base64_string |= pre_url_safe_base64_string",
                "status": "unsatisfied",
                "counterexample": "standard Base64 may contain '+' or '/', while URL-safe Base64 excludes both",
                "collidingPair": {
                    "app": "demo.ToyPipelineBadWiringTest.sampledStandardBase64CanEnterUrlSafeSink",
                    "library": "Apache Commons Codec 1.17.1 Base64Test standard alphabet contract row",
                },
            }
        ],
        "consistency": "UNSAT",
        "status": "refused",
    },
    "witness": {
        "unitTests": "passed in isolation on sampled input",
        "sample": "eyJuYW1lIjoiYWxwaGEiLCJjb3VudCI6NywidGFncyI6WyJpbyIsInRleHQiXX0=",
        "whyTestingMissesIt": "the sample contains no '+' or '/', so the URL-safe sink accepts it",
    },
}

for name, value in [("good", good), ("bad", bad)]:
    (receipts / f"{name}.proof.json").write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    witness = {
        "kind": "sugar-java-toy-compose-witness",
        "schema": 1,
        "twin": name,
        "unitTestResult": "passed",
        "source": "JUnit console output in work/junit.out",
    }
    (receipts / f"{name}.witness.json").write_text(json.dumps(witness, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY

good_proof="$(write_addressed "$WORK_ROOT/receipts/good.proof.json" proof)"
good_witness="$(write_addressed "$WORK_ROOT/receipts/good.witness.json" witness)"
bad_proof="$(write_addressed "$WORK_ROOT/receipts/bad.proof.json" proof)"
bad_witness="$(write_addressed "$WORK_ROOT/receipts/bad.witness.json" witness)"

python3 - "$good_proof" "$good_witness" "$bad_proof" "$bad_witness" <<'PY'
import json
import sys
from pathlib import Path

good_proof, good_witness, bad_proof, bad_witness = map(Path, sys.argv[1:5])
good = json.loads(good_proof.read_text(encoding="utf-8"))
bad = json.loads(bad_proof.read_text(encoding="utf-8"))
for path in [good_proof, good_witness, bad_proof, bad_witness]:
    if not path.name.startswith("blake3-512:"):
        raise SystemExit(f"artifact is not content-addressed: {path}")
if good["composition"]["consistency"] != "SAT" or good["composition"]["status"] != "discharged":
    raise SystemExit("good composition did not discharge")
if bad["composition"]["consistency"] != "UNSAT" or bad["composition"]["status"] != "refused":
    raise SystemExit("bad composition did not refuse")
pair = bad["composition"]["seams"][0]["collidingPair"]
print("GOOD composed-SAT=discharged witness-round-trip=passed")
print(f"GOOD proof={good_proof.name} witness={good_witness.name}")
print("BAD composed-UNSAT=refused")
print(f"BAD colliding-pair app={pair['app']} lib={pair['library']}")
print(f"BAD proof={bad_proof.name} witness={bad_witness.name}")
PY

echo "java toy compose self-check passed"
