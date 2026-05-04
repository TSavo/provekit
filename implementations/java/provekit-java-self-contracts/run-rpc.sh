#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
#
# Launcher for the java-self-contracts lift surface. The lift manifest
# spawns this script (relative to implementations/java); we resolve a
# usable `java` binary in this order:
#
#   1. $JAVA   (explicit override)
#   2. $JAVA_HOME/bin/java
#   3. java on PATH (works in most CI / dev environments)
#   4. /usr/local/opt/openjdk/bin/java   (Homebrew x86_64 default)
#   5. /opt/homebrew/opt/openjdk/bin/java (Homebrew arm64 default)
#
# The macOS stub at /usr/bin/java (the JavaCommandLineTools shim) refuses
# to run when no JDK is registered in /Library/Java/JavaVirtualMachines/
# even when a usable openjdk is installed via Homebrew. We bypass it.

set -e

if [ -n "$JAVA" ] && [ -x "$JAVA" ]; then
    EXE="$JAVA"
elif [ -n "$JAVA_HOME" ] && [ -x "$JAVA_HOME/bin/java" ]; then
    EXE="$JAVA_HOME/bin/java"
elif command -v java >/dev/null 2>&1; then
    # Skip the macOS /usr/bin/java stub which exits non-zero with no JDK.
    candidate="$(command -v java)"
    if [ "$candidate" = "/usr/bin/java" ] && ! "$candidate" -version >/dev/null 2>&1; then
        :
    else
        EXE="$candidate"
    fi
fi

if [ -z "$EXE" ]; then
    for guess in /usr/local/opt/openjdk/bin/java /opt/homebrew/opt/openjdk/bin/java; do
        if [ -x "$guess" ]; then
            EXE="$guess"
            break
        fi
    done
fi

if [ -z "$EXE" ]; then
    echo "ERROR: no usable java runtime found. Install openjdk 17+ or set JAVA_HOME." >&2
    exit 1
fi

# The script lives in implementations/java/provekit-java-self-contracts/;
# the jar lands at target/provekit-java-self-contracts.jar relative to
# this script (resolve via $0 so the manifest's working_dir doesn't
# matter for jar discovery).
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
JAR="$SCRIPT_DIR/target/provekit-java-self-contracts.jar"

if [ ! -f "$JAR" ]; then
    echo "ERROR: jar not built at $JAR. Run \`make build-java-self-contracts\`." >&2
    exit 1
fi

exec "$EXE" -jar "$JAR" "$@"
