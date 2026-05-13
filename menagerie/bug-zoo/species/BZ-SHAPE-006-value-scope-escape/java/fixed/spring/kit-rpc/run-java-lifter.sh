#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "$0")" && pwd)"
repo_root="$(cd "$script_dir/../../../../../../../.." && pwd)"
java_root="$repo_root/implementations/java"
core_jar="$java_root/provekit-lift-java-core/target/provekit-lsp-java.jar"
bean_jar="$java_root/provekit-lift-java-bean-validation/target/provekit-lift-java-bean-validation-0.1.0.jar"
spring_jar="$java_root/provekit-lift-java-spring-web/target/provekit-lift-java-spring-web-0.1.0.jar"

mvn -q -f "$java_root/pom.xml" -pl provekit-lift-java-core,provekit-lift-java-bean-validation,provekit-lift-java-spring-web -am package -DskipTests >&2

java -cp "$core_jar:$bean_jar:$spring_jar" com.provekit.lift.Main --rpc
