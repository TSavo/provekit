#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

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

rm -rf .classes
mkdir -p .classes
javac -d .classes ../library/src/main/java/zoo/UserDirectory.java src/main/java/zoo/UserDirectoryHarness.java
java -cp .classes zoo.UserDirectoryHarness
