#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "$0")" && pwd)"
repo_root="$(cd "$script_dir/../../../../../../../.." && pwd)"
java_root="$repo_root/implementations/java"
core_jar="$java_root/provekit-lift-java-core/target/provekit-lsp-java.jar"
junit_jar="$java_root/provekit-lift-java-junit/target/provekit-lift-java-junit-0.1.0.jar"

mvn -q -f "$java_root/pom.xml" -pl provekit-lift-java-core,provekit-lift-java-junit -am package -DskipTests >&2

java -cp "$core_jar:$junit_jar" com.provekit.lift.Main --rpc
