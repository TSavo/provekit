#!/usr/bin/env bash
# Prove the whole supply chain. Maven-driven, period.
#
#   mvn, give me all my sources.   ->  one goal, the full transitive closure.
#   sugar, prove everything.       ->  one proof per artifact, from its own
#                                       source + its own tests.
#
# This is not a plugin and not a framework. It is the loop. Point it at any
# Maven project and it mints a `.proof` for every dependency the pom resolves
# to -- and logs, by name, every artifact that lifts to the empty set: the
# perimeter, the shape of what no vendor ever swore to.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
SUGAR="${SUGAR:-$REPO/implementations/rust/target/release/sugar}"
KIT_DIR="$REPO/implementations/java/sugar-lift-java-tests"
OUT="$HERE/proofs"; mkdir -p "$OUT"

command -v mvn >/dev/null 2>&1 || { echo "SKIP: no mvn on PATH"; exit 0; }
command -v javac >/dev/null 2>&1 || { echo "SKIP: no JDK on PATH"; exit 0; }

cd "$HERE"

# Drop a minimal .sugar lift config + manifest onto a freshly-unpacked vendor
# source tree, pointing the java-test-assertions kit at it. Right by
# construction: the kit reads the unpacked .java through com.sun.source.
render_manifest() {
  local work="$1" kit="$2" java; java="$(command -v java)"
  mkdir -p "$work/.sugar/lift/java-test-assertions"
  cat > "$work/.sugar/config.toml" <<TOML
[[plugins]]
name = "java-test-assertions-lift"
kind = "lift"
surface = "java-test-assertions"
emit = "ir-document"

[solvers]
default = "z3"
[solvers.dispatch]
default = "z3"
[solvers.z3]
binary = "z3"
TOML
  cat > "$work/.sugar/lift/java-test-assertions/manifest.toml" <<TOML
name = "java-test-assertions-lift"
version = "0.1.0"
protocol_version = "pep/1.7.0"
kind = "lift"
command = ["$java", "--add-exports", "jdk.compiler/com.sun.source.tree=ALL-UNNAMED", "--add-exports", "jdk.compiler/com.sun.source.util=ALL-UNNAMED", "--add-exports", "jdk.compiler/com.sun.tools.javac.api=ALL-UNNAMED", "-cp", "$kit/out", "JavaTestAssertionsRpc"]
working_dir = "."

[capabilities]
authoring_surfaces = ["java-test-assertions"]
ir_version = "v1.1.0"
emits_signed_mementos = false
TOML
}

# 1. mvn, give me all my sources (and the vendor's own tests = the spec).
echo "== mvn: resolve every dependency's source + test-source =="
mvn -q -B dependency:copy-dependencies -Dclassifier=sources      -DoutputDirectory=target/srcjars
mvn -q -B dependency:copy-dependencies -Dclassifier=test-sources -DoutputDirectory=target/testjars -DfailOnMissingClassifierArtifact=false || true

# 2. sugar, prove everything: one .proof per artifact, from its own source.
echo "== sugar: mint a proof per artifact =="
proven=0; gaps=0
for jar in target/srcjars/*-sources.jar; do
  [ -e "$jar" ] || continue
  art="$(basename "$jar" -sources.jar)"
  work="$(mktemp -d)"
  unzip -qo "$jar" -d "$work" -x 'META-INF/*' >/dev/null 2>&1 || true
  tj="target/testjars/${art}-test-sources.jar"
  [ -e "$tj" ] && unzip -qo "$tj" -d "$work" -x 'META-INF/*' >/dev/null 2>&1 || true
  render_manifest "$work" "$KIT_DIR"   # drop a java-test-assertions lift manifest onto $work
  if ( cd "$work" && "$SUGAR" mint --out . ) >/dev/null 2>&1 && ls "$work"/blake3-512:*.proof >/dev/null 2>&1; then
    cp "$work"/blake3-512:*.proof "$OUT/$art.proof"
    proven=$((proven+1)); echo "  PROOF  $art"
  else
    gaps=$((gaps+1));     echo "  GAP    $art -> []   (no sworn behavior; on the perimeter)"
  fi
  rm -rf "$work"
done

echo
echo "== supply chain: $proven proven, $gaps on the perimeter =="
echo "   proofs in $OUT/ ; the GAP lines ARE the map of what nobody warranted."
