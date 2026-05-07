#!/usr/bin/env bash
set -euo pipefail
script_dir="$(cd "$(dirname "$0")" && pwd)"
repo_root="$(cd "$script_dir/../../../../../.." && pwd)"
java_root="$repo_root/implementations/java"
realizer_jar="$java_root/provekit-realize-java-core/target/provekit-realize-java.jar"

ensure_jdk() {
  if java -version >/dev/null 2>&1 && javac -version >/dev/null 2>&1; then
    return 0
  fi

  for candidate in /usr/local/opt/openjdk/bin /opt/homebrew/opt/openjdk/bin; do
    if [[ -x "$candidate/java" && -x "$candidate/javac" ]]; then
      export PATH="$candidate:$PATH"
      if java -version >/dev/null 2>&1 && javac -version >/dev/null 2>&1; then
        return 0
      fi
    fi
  done

  echo "Unable to locate a usable Java runtime/compiler pair." >&2
  echo "Install OpenJDK or expose java and javac on PATH." >&2
  echo "Checked: PATH, /usr/local/opt/openjdk/bin, /opt/homebrew/opt/openjdk/bin" >&2
  exit 1
}

ensure_jdk

mvn -f "$java_root/pom.xml" -pl provekit-realize-java-core -am package -DskipTests >&2

if [[ ! -f "$realizer_jar" ]]; then
  echo "Missing required Java realizer jar: $realizer_jar" >&2
  echo "Try: mvn -f \"$java_root/pom.xml\" -pl provekit-realize-java-core -am package -DskipTests" >&2
  exit 1
fi

java -jar "$realizer_jar" --rpc
