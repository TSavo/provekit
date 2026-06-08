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
JUNIT_JAR="${SUGAR_JUNIT_CONSOLE_JAR:-/tmp/sugar-junit/junit-platform-console-standalone-${JUNIT_VERSION}.jar}"
JUNIT_URL="https://repo1.maven.org/maven2/org/junit/platform/junit-platform-console-standalone/${JUNIT_VERSION}/junit-platform-console-standalone-${JUNIT_VERSION}.jar"

for suite in good bad; do
  if [ ! -d "$HERE/$suite" ]; then
    echo "missing suite directory: $HERE/$suite" >&2
    exit 1
  fi
done

if [ "${JAVA_ASSERT_SHOWCASE_ON_REMOTE:-0}" != "1" ] \
  && [ "${JAVA_ASSERT_SHOWCASE_USE_BCARGO:-1}" != "0" ] \
  && [ "$(uname -s)" != "Linux" ]; then
  echo "== run java assertion showcase on battleaxe via bcargo =="
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
  remote_cmd="cd $(printf '%q' "$remote_repo") && JAVA_ASSERT_SHOWCASE_ON_REMOTE=1 JAVA_ASSERT_SHOWCASE_SKIP_LOCAL_BUILD=1 examples/java-test-assertion-consistency/run.sh"
  ssh -o BatchMode=yes "$remote_host" "bash -lc $(printf '%q' "$remote_cmd")"
  exit $?
fi

if [ "${JAVA_ASSERT_SHOWCASE_SKIP_LOCAL_BUILD:-0}" != "1" ]; then
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

if [ ! -f "$JUNIT_JAR" ]; then
  echo "== fetch pinned JUnit console jar =="
  mkdir -p "$(dirname "$JUNIT_JAR")"
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$JUNIT_URL" -o "$JUNIT_JAR"
  elif command -v wget >/dev/null 2>&1; then
    wget -q "$JUNIT_URL" -O "$JUNIT_JAR"
  else
    echo "neither curl nor wget is available to fetch $JUNIT_URL" >&2
    exit 1
  fi
fi
export SUGAR_JUNIT_CONSOLE_JAR="$JUNIT_JAR"

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
}

json_status() {
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
rows = data.get("rows") or data.get("obligations") or (data if isinstance(data, list) else [])
for row in rows:
    prop = row.get("property") or row.get("predicate") or ""
    if kind == "consistency" and prop.startswith("consistency:") and "witness-package" not in prop:
        print(row.get("status") or row.get("result") or "")
        raise SystemExit(0)
    if kind == "witness" and "witness-package" in prop:
        print(row.get("status") or row.get("result") or "")
        raise SystemExit(0)
print("MISSING")
PY
}

run_suite() {
  local suite="$1"
  local expect_consistency="$2"
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

  echo "== prove $suite =="
  set +e
  (cd "$dir" && "$SUGAR" prove . --json) > "$dir/.prove.json" 2>&1
  local prove_rc=$?
  set -e

  local got_consistency got_witness
  got_consistency="$(json_status "$dir/.prove.json" consistency)"
  got_witness="$(json_status "$dir/.prove.json" witness)"

  if [ "$expect_consistency" = "discharged" ]; then
    if [ "$got_consistency" != "discharged" ]; then
      echo "$suite consistency expected discharged, got $got_consistency" >&2
      cat "$dir/.prove.json" >&2
      exit 1
    fi
  else
    if [ "$got_consistency" = "discharged" ] || [ "$got_consistency" = "MISSING" ]; then
      echo "$suite consistency expected refusal, got $got_consistency" >&2
      cat "$dir/.prove.json" >&2
      exit 1
    fi
  fi

  if [ "$expect_witness" = "discharged" ]; then
    if [ "$got_witness" != "discharged" ]; then
      echo "$suite witness expected discharged, got $got_witness" >&2
      cat "$dir/.prove.json" >&2
      exit 1
    fi
    : "$prove_rc"
  else
    if [ "$got_witness" = "discharged" ] || [ "$got_witness" = "MISSING" ]; then
      echo "$suite witness expected refusal, got $got_witness" >&2
      cat "$dir/.prove.json" >&2
      exit 1
    fi
  fi

  echo "$suite consistency=$got_consistency witness=$got_witness"
}

run_suite good discharged discharged
run_suite bad refused refused

echo "java learned-assertion showcase self-check passed"
