#!/usr/bin/env bash
set -euo pipefail
script_dir="$(cd "$(dirname "$0")" && pwd)"
repo_root="$(cd "$script_dir/../../../../../../.." && pwd)"
java_root="$repo_root/implementations/java"
core_jar="$java_root/provekit-lift-java-core/target/provekit-lsp-java.jar"
adapter_jar="$java_root/provekit-lift-java-spring-web/target/provekit-lift-java-spring-web-0.1.0.jar"

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

mvn -f "$java_root/pom.xml" -pl provekit-lift-java-core,provekit-lift-java-spring-web -am package -DskipTests >&2

for jar in "$core_jar" "$adapter_jar"; do
  if [[ ! -f "$jar" ]]; then
    echo "Missing required Java lifter jar: $jar" >&2
    echo "Try: mvn -f \"$java_root/pom.xml\" -pl provekit-lift-java-core,provekit-lift-java-spring-web -am package -DskipTests" >&2
    exit 1
  fi
done

java -cp "$core_jar:$adapter_jar" \
  com.provekit.lift.Main --rpc
