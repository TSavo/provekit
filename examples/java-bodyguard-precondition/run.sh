#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
RUST="$REPO/implementations/rust"
BIN_DIR="$RUST/target/debug"
SUGAR="$BIN_DIR/sugar"
CONTRACT_RPC="$BIN_DIR/java_jsr380_contracts_rpc"
IMPLICATION_RPC="$BIN_DIR/java_implications_rpc"
WITNESS_RPC="$BIN_DIR/java_junit_witness_rpc"
DISCHARGE_CLI="$BIN_DIR/java_junit_discharge_cli"
JUNIT_VERSION="${JUNIT_VERSION:-1.10.2}"
JUNIT_JAR="${SUGAR_JUNIT_CONSOLE_JAR:-/tmp/sugar-junit/junit-platform-console-standalone-${JUNIT_VERSION}.jar}"
JUNIT_URL="https://repo1.maven.org/maven2/org/junit/platform/junit-platform-console-standalone/${JUNIT_VERSION}/junit-platform-console-standalone-${JUNIT_VERSION}.jar"

echo "SCOPE: Java source body guard -> precondition predicate, zero Java annotations."
echo "SCOPE: chosen guard = Character.MIN_RADIX <= radix <= Character.MAX_RADIX."
echo "SCOPE: proof property = caller value facts discharge/refuse callee precondition at the method-call seam."
echo "SCOPE: throw is not modeled; throw is only the syntactic marker for the invalid-input branch."
echo "SCOPE: skipped residuals = non-flat guards, else branches, switch arms, loops, early-return reasoning, and exception semantics."

for suite in good bad; do
  if [ ! -d "$HERE/$suite" ]; then
    echo "missing suite directory: $HERE/$suite" >&2
    exit 1
  fi
done

if [ "${JAVA_BODYGUARD_SHOWCASE_ON_REMOTE:-0}" != "1" ] \
  && [ "${JAVA_BODYGUARD_SHOWCASE_USE_BCARGO:-1}" != "0" ] \
  && [ "$(uname -s)" != "Linux" ]; then
  echo "== run java bodyguard precondition showcase on battleaxe via bcargo =="
  "$REPO/bin/bcargo" build --manifest-path "$RUST/Cargo.toml" \
    -p sugar-cli --bin sugar \
    -p sugar-lift-java-tests --bin java_jsr380_contracts_rpc \
    -p sugar-lift-java-tests --bin java_implications_rpc \
    -p sugar-lift-java-tests --bin java_junit_witness_rpc \
    -p sugar-lift-java-tests --bin java_junit_discharge_cli >/dev/null

  remote_host="${BCARGO_REMOTE_HOST:-battleaxe}"
  remote_tag="$(printf '%s' "$(cd "$REPO" && pwd -P)" | shasum 2>/dev/null | cut -c1-12)"
  remote_tag="${remote_tag:-default}"
  remote_root="${BCARGO_REMOTE_ROOT:-/home/tsavo/remote/sugar-bcargo-${remote_tag}}"
  remote_repo="$remote_root/sugar"
  remote_cmd="cd $(printf '%q' "$remote_repo") && JAVA_BODYGUARD_SHOWCASE_ON_REMOTE=1 JAVA_BODYGUARD_SHOWCASE_SKIP_LOCAL_BUILD=1 examples/java-bodyguard-precondition/run.sh"
  ssh -o BatchMode=yes "$remote_host" "bash -lc $(printf '%q' "$remote_cmd")"
  exit $?
fi

if [ "${JAVA_BODYGUARD_SHOWCASE_SKIP_LOCAL_BUILD:-0}" != "1" ]; then
  echo "== build proof binaries =="
  cargo build --manifest-path "$RUST/Cargo.toml" \
    -p sugar-cli --bin sugar \
    -p sugar-lift-java-tests --bin java_jsr380_contracts_rpc \
    -p sugar-lift-java-tests --bin java_implications_rpc \
    -p sugar-lift-java-tests --bin java_junit_witness_rpc \
    -p sugar-lift-java-tests --bin java_junit_discharge_cli >/dev/null
fi

for bin in "$SUGAR" "$CONTRACT_RPC" "$IMPLICATION_RPC" "$WITNESS_RPC" "$DISCHARGE_CLI"; do
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

echo "== fetch pinned Java runtime jars =="
fetch_jar "$JUNIT_JAR" "$JUNIT_URL"
export SUGAR_JUNIT_CONSOLE_JAR="$JUNIT_JAR"

render_manifests() {
  local suite="$1"
  local base="$HERE/$suite/.sugar/lift"

  sed "s|@BIN_DIR@|$BIN_DIR|g" \
    "$base/java-jsr380-contracts/manifest.toml.in" \
    > "$base/java-jsr380-contracts/manifest.toml"

  sed "s|@BIN_DIR@|$BIN_DIR|g" \
    "$base/java-implications/manifest.toml.in" \
    > "$base/java-implications/manifest.toml"

  sed "s|@BIN_DIR@|$BIN_DIR|g" \
    "$base/java-junit-witness/manifest.toml.in" \
    > "$base/java-junit-witness/manifest.toml"
}

clean_suite() {
  local suite="$1"
  local dir="$HERE/$suite"
  rm -f "$dir"/blake3-512:*.proof "$dir/.prove.json" "$dir/.verify.json"
  rm -rf "$dir/.sugar/runs" "$dir/.sugar/witnesses" "$dir/target"
}

edge_status() {
  python3 - "$1" <<'PY'
import json
import re
import sys

text = open(sys.argv[1], "r", encoding="utf-8").read()
match = re.search(r"(?m)^\{", text)
if not match:
    print("MISSING")
    raise SystemExit(0)
data = json.loads(text[match.start():])
rows = data.get("rows") or data.get("claims") or data.get("obligations") or (data if isinstance(data, list) else [])
for row in rows:
    prop = row.get("property") or row.get("predicate") or ""
    if "witness-package" in prop:
        continue
    if row.get("bridge") == "digit" or row.get("callee") == "digit":
        print(row.get("status") or row.get("result") or "")
        raise SystemExit(0)
print("MISSING")
PY
}

witness_status() {
  python3 - "$1" <<'PY'
import json
import re
import sys

text = open(sys.argv[1], "r", encoding="utf-8").read()
match = re.search(r"(?m)^\{", text)
if not match:
    print("MISSING")
    raise SystemExit(0)
data = json.loads(text[match.start():])
rows = data.get("rows") or data.get("claims") or data.get("obligations") or (data if isinstance(data, list) else [])
for row in rows:
    prop = row.get("property") or row.get("predicate") or ""
    if "witness-package" in prop:
        print(row.get("status") or row.get("result") or "")
        raise SystemExit(0)
print("MISSING")
PY
}

run_suite() {
  local suite="$1"
  local expect_edge="$2"
  local expect_witness="$3"
  local dir="$HERE/$suite"

  render_manifests "$suite"
  clean_suite "$suite"

  echo "== mint $suite =="
  (cd "$dir" && "$SUGAR" mint --out .) >/dev/null

  local proof
  proof="$(find "$dir" -maxdepth 1 -name 'blake3-512:*.proof' -print -quit)"
  if [ -z "$proof" ]; then
    echo "$suite did not mint a proof" >&2
    exit 1
  fi

  echo "== verify durable proof+witness $suite =="
  set +e
  (cd "$dir" && "$SUGAR" verify --project . --json) > "$dir/.verify.json" 2>&1
  local verify_rc=$?
  set -e

  local got_edge got_witness
  got_edge="$(edge_status "$dir/.verify.json")"
  got_witness="$(witness_status "$dir/.verify.json")"

  if [ "$expect_edge" = "discharged" ]; then
    if [ "$verify_rc" -ne 0 ]; then
      echo "$suite durable verify expected exit 0, got $verify_rc" >&2
      cat "$dir/.verify.json" >&2
      exit 1
    fi
    if [ "$got_edge" != "discharged" ]; then
      echo "$suite java bodyguard edge expected discharged, got $got_edge" >&2
      cat "$dir/.verify.json" >&2
      exit 1
    fi
  else
    if [ "$verify_rc" -eq 0 ]; then
      echo "$suite durable verify expected refusal, but verify exited 0" >&2
      cat "$dir/.verify.json" >&2
      exit 1
    fi
    if [ "$got_edge" = "discharged" ] || [ "$got_edge" = "MISSING" ]; then
      echo "$suite java bodyguard edge expected refusal, got $got_edge" >&2
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

  echo "$suite java_bodyguard_edge=$got_edge witness=$got_witness verify_rc=$verify_rc verify_json=$dir/.verify.json"
}

run_suite good discharged discharged
run_suite bad refused discharged

echo "java bodyguard precondition showcase self-check passed"
